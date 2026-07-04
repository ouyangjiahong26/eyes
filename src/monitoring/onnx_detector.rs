//! ONNX 检测器：YuNet 人脸检测 + 5 关键点 solvePnP。
//!
//! 工作流程：
//! 1. YuNet 检测人脸，输出边界框 + 5 个关键点
//! 2. 5 个 2D 关键点 + 第 6 点（下巴，由 bbox 估算）→ DLT solvePnP → 旋转矩阵
//! 3. 从旋转矩阵提取 yaw 和 pitch
//!
//! 模型输出 12 个张量（3 个尺度 × cls/obj/bbox/kps），anchor-free 解码。

use crate::domain::classifier::HeadPose;
use crate::monitoring::detector::Detector;
use super::solve_pnp;

// ── 常量 ───────────────────────────────────────────────────────

/// YuNet 输入尺寸（正方形）
const INPUT_SIZE: u32 = 640;

/// 三个特征图步长
const STRIDES: [u32; 3] = [8, 16, 32];

/// 关键点数量（YuNet 输出）
const NUM_KEYPOINTS: usize = 5;

/// 最低置信度阈值
const MIN_CONFIDENCE: f32 = 0.5;

// ── YuNet 检测器 ──────────────────────────────────────────────

/// YuNet 检测器。
///
/// 单模型方案：YuNet 输出 5 关键点，下巴从边界框底部估算，
/// 用 6 个对应点做 solvePnP 计算头部姿态。
pub struct YuNetDetector {
    session: ort::session::Session,
}

/// YuNet 单次检测结果（在原始图像坐标系中）。
struct Detection {
    landmarks_2d: [[f64; 2]; NUM_KEYPOINTS],
    bbox_xyxy: [f64; 4],
}

impl YuNetDetector {
    /// 从 ONNX 模型文件创建检测器。
    pub fn new(model_path: &str) -> Result<Self, String> {
        let session = ort::session::Session::builder()
            .map_err(|e| format!("创建 session builder 失败: {e}"))?
            .commit_from_file(model_path)
            .map_err(|e| format!("加载模型失败: {e}"))?;
        Ok(Self { session })
    }
}

impl Detector for YuNetDetector {
    fn detect(&mut self, rgb: &[u8], width: u32, height: u32) -> Option<HeadPose> {
        let input_data = preprocess_rgb(rgb, width, height);
        let tensor = ort::value::Tensor::from_array((
            [1usize, 3, INPUT_SIZE as usize, INPUT_SIZE as usize],
            input_data,
        ))
        .ok()?;

        let input_name = self.session.inputs()[0].name().to_string();
        let outputs = self
            .session
            .run(ort::inputs![input_name.as_str() => tensor])
            .ok()?;

        // 提取 12 个输出张量：每个尺度有 obj/bbox/kps（cls 是单类，与 obj 等价）
        // 输出索引约定（按 stride 8, 16, 32 分组）：
        //   cls_8(0), cls_16(1), cls_32(2), obj_8(3), obj_16(4), obj_32(5),
        //   bbox_8(6), bbox_16(7), bbox_32(8), kps_8(9), kps_16(10), kps_32(11)
        let mut all_objs: Vec<(Vec<f32>, u32)> = Vec::new();
        let mut all_bboxes: Vec<Vec<f32>> = Vec::new();
        let mut all_kps: Vec<Vec<f32>> = Vec::new();

        for (si, &stride) in STRIDES.iter().enumerate() {
            let obj_idx = 3 + si; // obj_8=3, obj_16=4, obj_32=5
            let bbox_idx = 6 + si;
            let kps_idx = 9 + si;

            let obj = extract_flat(&outputs[obj_idx])?;
            let bbox = extract_flat(&outputs[bbox_idx])?;
            let kps = extract_flat(&outputs[kps_idx])?;

            let num = obj.len(); // obj shape [1, N, 1]，N = grid_h * grid_w
            let grid = INPUT_SIZE / stride;
            debug_assert_eq!(num, (grid * grid) as usize);

            all_objs.push((obj, stride));
            all_bboxes.push(bbox);
            all_kps.push(kps);
        }

        // 在所有 anchor 中找置信度最高的
        let scale = INPUT_SIZE as f64;
        let scale_x = width as f64 / scale;
        let scale_y = height as f64 / scale;

        let mut best_conf = MIN_CONFIDENCE;
        let mut best: Option<(usize, usize, u32)> = None; // (flat_idx, group, stride)

        for (group, (obj, stride)) in all_objs.iter().enumerate() {
            for (i, &conf) in obj.iter().enumerate() {
                if conf > best_conf {
                    best_conf = conf;
                    best = Some((i, group, *stride));
                }
            }
        }

        let (flat_idx, group, stride) = best?;
        let bbox_data = &all_bboxes[group];
        let kps_data = &all_kps[group];

        let grid = INPUT_SIZE / stride;
        let gx = flat_idx % grid as usize;
        let gy = flat_idx / grid as usize;

        // anchor 中心（在 640×640 坐标系中）
        let cx = gx as f64 * stride as f64;
        let cy = gy as f64 * stride as f64;

        // 解码 bbox：[dl, dt, dr, db] → xyxy
        let b = flat_idx * 4;
        let x1 = (cx - bbox_data[b] as f64) * scale_x;
        let y1 = (cy - bbox_data[b + 1] as f64) * scale_y;
        let x2 = (cx + bbox_data[b + 2] as f64) * scale_x;
        let y2 = (cy + bbox_data[b + 3] as f64) * scale_y;

        // 解码 5 关键点：每个 (dx, dy) 相对 anchor 中心
        let mut landmarks_2d = [[0.0_f64; 2]; NUM_KEYPOINTS];
        let k = flat_idx * 10;
        for j in 0..NUM_KEYPOINTS {
            landmarks_2d[j] = [
                (cx + kps_data[k + j * 2] as f64) * scale_x,
                (cy + kps_data[k + j * 2 + 1] as f64) * scale_y,
            ];
        }

        let det = Detection {
            landmarks_2d,
            bbox_xyxy: [x1, y1, x2, y2],
        };

        // 下巴由 bbox 底部中心估算
        let chin_x = (det.bbox_xyxy[0] + det.bbox_xyxy[2]) / 2.0;
        let chin_y = det.bbox_xyxy[3];

        let mut points_2d = [[0.0_f64; 2]; 6];
        points_2d[..NUM_KEYPOINTS].copy_from_slice(&det.landmarks_2d);
        points_2d[5] = [chin_x, chin_y];

        let camera_matrix = estimate_camera_matrix(width, height);
        let rotation = solve_pnp::solve_pnp(&points_2d, &solve_pnp::MODEL_3D, &camera_matrix)?;
        let (yaw, pitch) = solve_pnp::rotation_to_yaw_pitch(&rotation);
        Some(HeadPose { yaw, pitch })
    }
}

/// 从 ort 输出值中提取扁平化的 f32 Vec。
fn extract_flat(value: &ort::value::Value) -> Option<Vec<f32>> {
    let arr = value.try_extract_array::<f32>().ok()?;
    Some(arr.as_slice().unwrap_or(&[]).to_vec())
}

// ── 预处理 ─────────────────────────────────────────────────────

/// 双线性缩放到 640×640 + 转 NCHW float32（归一化到 0-1）。
fn preprocess_rgb(rgb: &[u8], width: u32, height: u32) -> Vec<f32> {
    let out_size = INPUT_SIZE as usize;
    let mut buf = vec![0.0f32; 3 * out_size * out_size];

    let sw = width as f64 / INPUT_SIZE as f64;
    let sh = height as f64 / INPUT_SIZE as f64;

    for oy in 0..out_size {
        let sy = ((oy as f64 + 0.5) * sh - 0.5).round().max(0.0) as u32;
        let sy = sy.min(height - 1);
        for ox in 0..out_size {
            let sx = ((ox as f64 + 0.5) * sw - 0.5).round().max(0.0) as u32;
            let sx = sx.min(width - 1);
            let src_idx = ((sy * width + sx) * 3) as usize;
            let dst_base = oy * out_size + ox;
            buf[dst_base] = rgb[src_idx] as f32;
            buf[out_size * out_size + dst_base] = rgb[src_idx + 1] as f32;
            buf[2 * out_size * out_size + dst_base] = rgb[src_idx + 2] as f32;
        }
    }
    buf
}

// ── 相机内参估计 ───────────────────────────────────────────────

/// 从图像尺寸估算相机内参矩阵。假设主点在中心，焦距 = max(w, h)。
fn estimate_camera_matrix(width: u32, height: u32) -> [[f64; 3]; 3] {
    let f = width.max(height) as f64;
    [[f, 0.0, width as f64 / 2.0], [0.0, f, height as f64 / 2.0], [0.0, 0.0, 1.0]]
}

// ── 测试 ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::detector::Detector;

    #[test]
    fn camera_matrix_centered() {
        let cam = estimate_camera_matrix(640, 480);
        assert!((cam[0][0] - 640.0).abs() < 0.01);
        assert!((cam[0][2] - 320.0).abs() < 0.01);
        assert!((cam[1][2] - 240.0).abs() < 0.01);
    }

    #[test]
    fn preprocess_output_size() {
        let rgb = vec![0u8; 640 * 480 * 3];
        let out = preprocess_rgb(&rgb, 640, 480);
        assert_eq!(out.len(), 3 * 640 * 640);
    }

    #[test]
    fn model_3d_nose_at_origin() {
        assert_eq!(solve_pnp::MODEL_3D[2], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn latency_benchmark() {
        let model_path = "../models/face_detection_yunet_2023mar.onnx";
        if !std::path::Path::new(model_path).exists() {
            eprintln!("latency_benchmark: 模型文件不存在，跳过 ({model_path})");
            return;
        }
        let mut detector = YuNetDetector::new(model_path).expect("加载模型失败");
        let frame = vec![0u8; 640 * 480 * 3];
        let n = 30usize;
        let mut durations: Vec<f64> = (0..n)
            .map(|_| {
                let t = std::time::Instant::now();
                let _ = detector.detect(&frame, 640, 480);
                t.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        durations.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = durations[n * 50 / 100];
        let p95 = durations[(n * 95 / 100).min(n - 1)];
        let p99 = durations[(n * 99 / 100).min(n - 1)];
        eprintln!("YuNet 延迟 (N={n}):");
        eprintln!("  P50: {p50:.1} ms");
        eprintln!("  P95: {p95:.1} ms");
        eprintln!("  P99: {p99:.1} ms");
    }
}
