//! DLT solvePnP 管道：2D+3D 对应点 → 旋转矩阵 → yaw/pitch。
//!
//! 依赖 `linalg3` 做 3×3 矩阵运算，依赖 `nalgebra` 做 12×12 SVD。

use super::linalg3;

// ── 常量 ───────────────────────────────────────────────────────

/// solvePnP 使用的 3D 模型点总数（含下巴）
const NUM_MODEL_POINTS: usize = 6;

// ── 3D 模型点（鼻尖为原点） ───────────────────────────────────
// 前 5 个对应 YuNet 的 5 关键点：左眼、右眼、鼻尖、左嘴角、右嘴角
// 第 6 个是下巴，由边界框底部估算
pub(crate) const MODEL_3D: [[f64; 3]; NUM_MODEL_POINTS] = [
    [-34.0, 32.0, -30.0],  // 左眼中心
    [34.0, 32.0, -30.0],   // 右眼中心
    [0.0, 0.0, 0.0],       // 鼻尖（原点）
    [-29.0, -29.0, -25.0], // 左嘴角
    [29.0, -29.0, -25.0],  // 右嘴角
    [0.0, -75.0, -12.0],   // 下巴
];

// ── solvePnP（DLT + nalgebra SVD） ─────────────────────────────

/// Direct Linear Transform solvePnP。
pub(crate) fn solve_pnp(
    points_2d: &[[f64; 2]; 6],
    points_3d: &[[f64; 3]; 6],
    camera_matrix: &[[f64; 3]; 3],
) -> Option<[[f64; 3]; 3]> {
    let a_data = build_dlt_matrix(points_2d, points_3d, camera_matrix);
    let p_vec = dlt_null_space(&a_data)?;
    let r_raw = extract_rotation_raw(&p_vec);
    orthogonalize_rotation(&r_raw)
}

/// 从旋转矩阵提取 yaw 和 pitch（度数）。
///
/// 与 Python 版 `head_pose_geometry.rotation_to_yaw_pitch` 行为一致：
/// - yaw = atan2(R[0][2], R[2][2])（绕 Y 轴）
/// - pitch = atan2(-R[2][1], R[2][2])（绕 X 轴，取反使 positive = 仰头）
///
/// 符号约定：positive pitch = 仰头，negative pitch = 低头。
/// 取反原因：camera Y 轴向下，头部上仰时 R[2][1] = -sin(θ)，
/// atan2 直接得 -θ，取反后 positive = 仰头，与约定一致。
pub fn rotation_to_yaw_pitch(rotation: &[[f64; 3]; 3]) -> (f64, f64) {
    let yaw = rotation[0][2].atan2(rotation[2][2]);
    let pitch = -rotation[2][1].atan2(rotation[2][2]);
    (yaw.to_degrees(), pitch.to_degrees())
}

// ── 私有辅助 ───────────────────────────────────────────────────

/// 构建 DLT A 矩阵（12×12），行优先存储。
fn build_dlt_matrix(
    points_2d: &[[f64; 2]; 6],
    points_3d: &[[f64; 3]; 6],
    camera_matrix: &[[f64; 3]; 3],
) -> [f64; 144] {
    let fx = camera_matrix[0][0];
    let fy = camera_matrix[1][1];
    let cx = camera_matrix[0][2];
    let cy = camera_matrix[1][2];
    let mut a = [0.0f64; 144];
    for i in 0..6 {
        let [x, y, z] = points_3d[i];
        let u = (points_2d[i][0] - cx) / fx;
        let v = (points_2d[i][1] - cy) / fy;
        let r0 = 2 * i;
        let r1 = r0 + 1;
        a[r0 * 12..r0 * 12 + 12]
            .copy_from_slice(&[x, y, z, 1.0, 0.0, 0.0, 0.0, 0.0, -u * x, -u * y, -u * z, -u]);
        a[r1 * 12..r1 * 12 + 12]
            .copy_from_slice(&[0.0, 0.0, 0.0, 0.0, x, y, z, 1.0, -v * x, -v * y, -v * z, -v]);
    }
    a
}

/// 用 nalgebra SVD 求 DLT 矩阵的零空间向量。
fn dlt_null_space(a_data: &[f64; 144]) -> Option<Vec<f64>> {
    use nalgebra::SVD;
    let a_mat = nalgebra::DMatrix::from_row_slice(12, 12, a_data);
    let svd = SVD::new(a_mat, true, true);
    let v_t = svd.v_t.as_ref()?;
    Some(v_t.row(11).iter().copied().collect())
}

/// 从 DLT 解向量提取原始旋转矩阵。
fn extract_rotation_raw(p_vec: &[f64]) -> [[f64; 3]; 3] {
    let mut r = [[0.0f64; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = p_vec[i * 4 + j];
        }
    }
    r
}

/// SVD 正交化为 SO(3)，确保 det(R) = +1。
fn orthogonalize_rotation(r_raw: &[[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let (u, _s, vt) = linalg3::svd3x3(r_raw)?;
    let mut r = linalg3::mat_mul_3x3(&u, &vt);
    if linalg3::det3x3(&r) < 0.0 {
        for row in &mut r {
            for x in row.iter_mut() {
                *x = -*x;
            }
        }
    }
    Some(r)
}

// ── 测试 ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── rotation_to_yaw_pitch ──────────────────────────────────

    #[test]
    fn identity_rotation_gives_zero() {
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let (yaw, pitch) = rotation_to_yaw_pitch(&identity);
        assert!((yaw).abs() < 0.01);
        assert!((pitch).abs() < 0.01);
    }

    #[test]
    fn detects_yaw_rotation() {
        let angle = 30.0_f64.to_radians();
        let rotation = [
            [angle.cos(), 0.0, angle.sin()],
            [0.0, 1.0, 0.0],
            [-angle.sin(), 0.0, angle.cos()],
        ];
        let (yaw, pitch) = rotation_to_yaw_pitch(&rotation);
        assert!((yaw - 30.0).abs() < 0.1, "yaw={yaw}");
        assert!((pitch).abs() < 0.1, "pitch={pitch}");
    }

    // ── solve_pnp ──────────────────────────────────────────────

    fn project_points(r: &[[f64; 3]; 3], tz: f64, f: f64, cx: f64, cy: f64) -> [[f64; 2]; 6] {
        std::array::from_fn(|i| {
            let [x, y, z] = MODEL_3D[i];
            let rx = r[0][0] * x + r[0][1] * y + r[0][2] * z;
            let ry = r[1][0] * x + r[1][1] * y + r[1][2] * z;
            let rz = r[2][0] * x + r[2][1] * y + r[2][2] * z;
            [f * rx / (rz + tz) + cx, f * ry / (rz + tz) + cy]
        })
    }

    #[test]
    fn solve_pnp_identity_projection() {
        let cam = [[1000.0, 0.0, 160.0], [0.0, 1000.0, 120.0], [0.0, 0.0, 1.0]];
        let pts = project_points(&identity_matrix(), 500.0, 1000.0, 160.0, 120.0);
        let r = solve_pnp(&pts, &MODEL_3D, &cam).unwrap();
        let (yaw, pitch) = rotation_to_yaw_pitch(&r);
        assert!(yaw.abs() < 5.0, "yaw={yaw}");
        assert!(pitch.abs() < 5.0, "pitch={pitch}");
    }

    #[test]
    fn solve_pnp_detects_left_turn() {
        let cam = [[1000.0, 0.0, 160.0], [0.0, 1000.0, 120.0], [0.0, 0.0, 1.0]];
        let r_true = yaw_rotation_matrix(-25.0);
        let pts = project_points(&r_true, 500.0, 1000.0, 160.0, 120.0);
        let r = solve_pnp(&pts, &MODEL_3D, &cam).unwrap();
        let (yaw, pitch) = rotation_to_yaw_pitch(&r);
        assert!((yaw - (-25.0)).abs() < 5.0, "yaw={yaw}");
        assert!(pitch.abs() < 5.0, "pitch={pitch}");
    }

    #[test]
    fn solve_pnp_detects_right_turn() {
        let cam = [[1000.0, 0.0, 160.0], [0.0, 1000.0, 120.0], [0.0, 0.0, 1.0]];
        let r_true = yaw_rotation_matrix(35.0);
        let pts = project_points(&r_true, 500.0, 1000.0, 160.0, 120.0);
        let r = solve_pnp(&pts, &MODEL_3D, &cam).unwrap();
        let (yaw, pitch) = rotation_to_yaw_pitch(&r);
        assert!((yaw - 35.0).abs() < 5.0, "yaw={yaw}");
        assert!(pitch.abs() < 5.0, "pitch={pitch}");
    }

    #[test]
    fn solve_pnp_detects_pitch_up() {
        let cam = [[1000.0, 0.0, 160.0], [0.0, 1000.0, 120.0], [0.0, 0.0, 1.0]];
        // 负向绕 X 轴旋转 = 仰头
        let r_true = pitch_rotation_matrix(-20.0);
        let pts = project_points(&r_true, 500.0, 1000.0, 160.0, 120.0);
        let r = solve_pnp(&pts, &MODEL_3D, &cam).unwrap();
        let (yaw, pitch) = rotation_to_yaw_pitch(&r);
        assert!(yaw.abs() < 5.0, "yaw={yaw}");
        assert!(pitch > 5.0, "仰头应为正 pitch, got {pitch}");
    }

    #[test]
    fn solve_pnp_detects_pitch_down() {
        let cam = [[1000.0, 0.0, 160.0], [0.0, 1000.0, 120.0], [0.0, 0.0, 1.0]];
        // 正向绕 X 轴旋转 = 低头
        let r_true = pitch_rotation_matrix(15.0);
        let pts = project_points(&r_true, 500.0, 1000.0, 160.0, 120.0);
        let r = solve_pnp(&pts, &MODEL_3D, &cam).unwrap();
        let (yaw, pitch) = rotation_to_yaw_pitch(&r);
        assert!(yaw.abs() < 5.0, "yaw={yaw}");
        assert!(pitch < -5.0, "低头应为负 pitch, got {pitch}");
    }

    // ── 测试辅助 ───────────────────────────────────────────────

    fn identity_matrix() -> [[f64; 3]; 3] {
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    }

    fn yaw_rotation_matrix(deg: f64) -> [[f64; 3]; 3] {
        let r = deg.to_radians();
        [[r.cos(), 0.0, r.sin()], [0.0, 1.0, 0.0], [-r.sin(), 0.0, r.cos()]]
    }

    fn pitch_rotation_matrix(deg: f64) -> [[f64; 3]; 3] {
        let r = deg.to_radians();
        [[1.0, 0.0, 0.0], [0.0, r.cos(), -r.sin()], [0.0, r.sin(), r.cos()]]
    }
}
