//! ONNX 检测器：YuNet 人脸检测 + 5 关键点 solvePnP。
//!
//! 工作流程：
//! 1. YuNet 检测人脸，输出边界框 + 5 个关键点
//! 2. 5 个 2D 关键点 + 第 6 点（下巴，由 bbox 估算）→ DLT solvePnP → 旋转矩阵
//! 3. 从旋转矩阵提取 yaw 和 pitch
//!
//! 下巴点由边界框底部中心估算，大角度（>45°）时精度会下降。

use crate::domain::classifier::HeadPose;
use crate::monitoring::detector::Detector;
use super::solve_pnp;

// ── 常量 ───────────────────────────────────────────────────────

/// YuNet 输入尺寸
const INPUT_W: u32 = 320;
const INPUT_H: u32 = 240;

/// YuNet 每个检测的输出宽度：[x, y, w, h, conf, 5×(lx, ly)]
const YUNET_DETECTION_STRIDE: usize = 15;

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

/// YuNet 单次检测结果。
struct Detection {
    landmarks_2d: [[f64; 2]; NUM_KEYPOINTS],
    bbox: [f32; 4], // x, y, w, h（输入图像坐标）
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
            [1usize, 3, INPUT_H as usize, INPUT_W as usize],
            input_data,
        ))
        .ok()?;

        let input_name = self.session.inputs()[0].name().to_string();
        let outputs = self
            .session
            .run(ort::inputs![input_name.as_str() => tensor])
            .ok()?;

        let output = outputs[0].try_extract_array::<f32>().ok()?;
        let det = find_best_detection(output.as_slice()?, output.shape(), width, height)?;

        let points_2d = build_6_point_correspondence(&det, width, height);
        let camera_matrix = estimate_camera_matrix(width, height);
        let rotation = solve_pnp::solve_pnp(&points_2d, &solve_pnp::MODEL_3D, &camera_matrix)?;
        let (yaw, pitch) = solve_pnp::rotation_to_yaw_pitch(&rotation);
        Some(HeadPose { yaw, pitch })
    }
}

/// 从 YuNet 输出中找置信度最高的人脸检测。
fn find_best_detection(
    data: &[f32],
    shape: &[usize],
    width: u32,
    height: u32,
) -> Option<Detection> {
    if shape.len() < 3 || shape[2] < YUNET_DETECTION_STRIDE {
        return None;
    }
    let num_detections = shape[1];
    let scale_x = width as f64 / INPUT_W as f64;
    let scale_y = height as f64 / INPUT_H as f64;

    let mut best_conf = MIN_CONFIDENCE;
    let mut best = None;

    for i in 0..num_detections {
        let base = i * YUNET_DETECTION_STRIDE;
        let conf = data[base + 4];
        if conf > best_conf {
            best_conf = conf;
            let mut landmarks_2d = [[0.0_f64; 2]; NUM_KEYPOINTS];
            for j in 0..NUM_KEYPOINTS {
                landmarks_2d[j] = [
                    data[base + 5 + j * 2] as f64 * scale_x,
                    data[base + 5 + j * 2 + 1] as f64 * scale_y,
                ];
            }
            best = Some(Detection {
                landmarks_2d,
                bbox: [data[base], data[base + 1], data[base + 2], data[base + 3]],
            });
        }
    }
    best
}

/// 将 5 关键点 + 下巴估算组合为 6 点对应关系。
///
/// 下巴 = 边界框底部中心。大角度时此估算有误差。
fn build_6_point_correspondence(det: &Detection, width: u32, height: u32) -> [[f64; 2]; 6] {
    let scale_x = width as f64 / INPUT_W as f64;
    let scale_y = height as f64 / INPUT_H as f64;
    let chin_x = (det.bbox[0] as f64 + det.bbox[2] as f64 / 2.0) * scale_x;
    let chin_y = (det.bbox[1] as f64 + det.bbox[3] as f64) * scale_y;
    let mut pts = [[0.0_f64; 2]; 6];
    pts[..NUM_KEYPOINTS].copy_from_slice(&det.landmarks_2d);
    pts[5] = [chin_x, chin_y];
    pts
}

// ── 预处理 ─────────────────────────────────────────────────────

/// 最近邻缩放 + 转 NCHW float32。
fn preprocess_rgb(rgb: &[u8], width: u32, height: u32) -> Vec<f32> {
    debug_assert!(
        rgb.len() >= (width * height * 3) as usize,
        "rgb buffer 太小: {} < {}",
        rgb.len(),
        width * height * 3
    );
    let out_w = INPUT_W as usize;
    let out_h = INPUT_H as usize;
    let mut buf = vec![0.0f32; 3 * out_h * out_w];

    for oy in 0..out_h {
        let sy = ((oy as f64 + 0.5) * height as f64 / out_h as f64 - 0.5).round() as u32;
        let sy = sy.min(height - 1);
        for ox in 0..out_w {
            let sx = ((ox as f64 + 0.5) * width as f64 / out_w as f64 - 0.5).round() as u32;
            let sx = sx.min(width - 1);
            let src_idx = ((sy * width + sx) * 3) as usize;
            let dst_base = oy * out_w + ox;
            buf[dst_base] = rgb[src_idx] as f32;
            buf[out_h * out_w + dst_base] = rgb[src_idx + 1] as f32;
            buf[2 * out_h * out_w + dst_base] = rgb[src_idx + 2] as f32;
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
        assert_eq!(out.len(), 3 * 240 * 320);
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
