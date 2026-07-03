//! 纯 3×3 线性代数工具，零外部依赖。
//!
//! 提供矩阵乘法、行列式、转置和 Jacobi SVD。
//! 仅用于 `solve_pnp` 模块的旋转矩阵正交化。

// ── 常量 ───────────────────────────────────────────────────────

/// Jacobi SVD 最大迭代次数
const JACOBI_MAX_ITER: usize = 100;

/// 数值零阈值
const EPSILON: f64 = 1e-10;
const EPSILON_OFF_DIAG: f64 = 1e-15;

// ── 公共 API ───────────────────────────────────────────────────

/// 3×3 矩阵乘法。
pub(crate) fn mat_mul_3x3(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0f64; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

/// 3×3 行列式。
pub(crate) fn det3x3(m: &[[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// 3×3 Jacobi SVD。
///
/// 返回 (U, sigma, V^T)，使得 A = U * diag(sigma) * V^T。
pub(crate) fn svd3x3(a: &[[f64; 3]; 3]) -> Option<([[f64; 3]; 3], [f64; 3], [[f64; 3]; 3])> {
    let ata = mat_transpose_times_self(a);
    let (mut v, s) = jacobi_eigen_3x3(&ata);
    let mut sigma = eigenvalues_to_sigma(&s);
    sort_descending(&mut sigma, &mut v);
    let u = compute_u(a, &v, &sigma);
    let u = orthogonalize_u(u, &sigma);
    let vt = transpose_3x3(&v);
    Some((u, sigma, vt))
}

// ── 私有辅助 ───────────────────────────────────────────────────

/// 计算 A^T A。
fn mat_transpose_times_self(a: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut ata = [[0.0f64; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            ata[i][j] = (0..3).map(|k| a[k][i] * a[k][j]).sum();
        }
    }
    ata
}

/// Jacobi 迭代求 3×3 对称矩阵的特征值和特征向量。
fn jacobi_eigen_3x3(matrix: &[[f64; 3]; 3]) -> ([[f64; 3]; 3], [[f64; 3]; 3]) {
    let mut v = [[0.0f64; 3]; 3];
    v[0][0] = 1.0;
    v[1][1] = 1.0;
    v[2][2] = 1.0;
    let mut s = *matrix;

    for _ in 0..JACOBI_MAX_ITER {
        let mut converged = true;
        for p in 0..3 {
            for q in (p + 1)..3 {
                if s[p][q].abs() < EPSILON_OFF_DIAG {
                    continue;
                }
                converged = false;
                jacobi_rotate(&mut s, &mut v, p, q);
            }
        }
        if converged {
            break;
        }
    }
    (v, s)
}

/// 对 (p, q) 位置执行一次 Jacobi 旋转。
fn jacobi_rotate(s: &mut [[f64; 3]; 3], v: &mut [[f64; 3]; 3], p: usize, q: usize) {
    let tau = (s[q][q] - s[p][p]) / (2.0 * s[p][q]);
    let t = if tau >= 0.0 {
        1.0 / (tau + (1.0 + tau * tau).sqrt())
    } else {
        -1.0 / (-tau + (1.0 + tau * tau).sqrt())
    };
    let c = 1.0 / (1.0 + t * t).sqrt();
    let st = t * c;

    let spq = s[p][q];
    s[p][q] = 0.0;
    s[q][p] = 0.0;
    let spp = s[p][p];
    let sqq = s[q][q];
    s[p][p] = spp - t * spq;
    s[q][q] = sqq + t * spq;
    for r in 0..3 {
        if r != p && r != q {
            let srp = s[r][p];
            let srq = s[r][q];
            s[r][p] = c * srp - st * srq;
            s[p][r] = s[r][p];
            s[r][q] = st * srp + c * srq;
            s[q][r] = s[r][q];
        }
    }
    for r in 0..3 {
        let vrp = v[r][p];
        let vrq = v[r][q];
        v[r][p] = c * vrp - st * vrq;
        v[r][q] = st * vrp + c * vrq;
    }
}

/// 从对称矩阵的特征值计算奇异值（sqrt of max(eigenvalue, 0)）。
fn eigenvalues_to_sigma(s: &[[f64; 3]; 3]) -> [f64; 3] {
    [s[0][0].max(0.0).sqrt(), s[1][1].max(0.0).sqrt(), s[2][2].max(0.0).sqrt()]
}

/// 按奇异值降序排列 sigma 和对应的 V 列。
fn sort_descending(sigma: &mut [f64; 3], v: &mut [[f64; 3]; 3]) {
    let mut indices = [0, 1, 2];
    indices.sort_by(|&i, &j| {
        sigma[j].partial_cmp(&sigma[i]).unwrap_or(std::cmp::Ordering::Equal)
    });
    let orig_sigma = *sigma;
    let orig_v = *v;
    for (new_i, &old_i) in indices.iter().enumerate() {
        sigma[new_i] = orig_sigma[old_i];
        for r in 0..3 {
            v[r][new_i] = orig_v[r][old_i];
        }
    }
}

/// U = A * V * Sigma^{-1}。
fn compute_u(a: &[[f64; 3]; 3], v: &[[f64; 3]; 3], sigma: &[f64; 3]) -> [[f64; 3]; 3] {
    let mut u = [[0.0f64; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            if sigma[j] > EPSILON {
                u[i][j] = (0..3).map(|k| a[i][k] * v[k][j]).sum::<f64>() / sigma[j];
            } else {
                u[i][j] = if i == j { 1.0 } else { 0.0 };
            }
        }
    }
    u
}

/// Gram-Schmidt 正交化 U 中对应零奇异值的列。
fn orthogonalize_u(mut u: [[f64; 3]; 3], sigma: &[f64; 3]) -> [[f64; 3]; 3] {
    for col in 0..3 {
        if sigma[col] < EPSILON {
            for prev in 0..col {
                let dot: f64 = (0..3).map(|i| u[i][col] * u[i][prev]).sum();
                for i in 0..3 {
                    u[i][col] -= dot * u[i][prev];
                }
            }
            let norm: f64 = (0..3).map(|i| u[i][col] * u[i][col]).sum::<f64>().sqrt();
            if norm > EPSILON {
                for i in 0..3 {
                    u[i][col] /= norm;
                }
            }
        }
    }
    u
}

/// 3×3 矩阵转置。
fn transpose_3x3(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut t = [[0.0f64; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            t[i][j] = m[j][i];
        }
    }
    t
}

// ── 测试 ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_matrix() -> [[f64; 3]; 3] {
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    }

    fn diag_matrix(d: &[f64; 3]) -> [[f64; 3]; 3] {
        [[d[0], 0.0, 0.0], [0.0, d[1], 0.0], [0.0, 0.0, d[2]]]
    }

    #[test]
    fn svd3x3_identity() {
        let (u, s, vt) = svd3x3(&identity_matrix()).unwrap();
        for i in 0..3 {
            assert!((s[i] - 1.0).abs() < 0.01, "sigma[{i}]={}", s[i]);
        }
        let recon = mat_mul_3x3(&mat_mul_3x3(&u, &diag_matrix(&s)), &vt);
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((recon[i][j] - expected).abs() < 0.01, "recon[{i}][{j}]={}", recon[i][j]);
            }
        }
    }

    #[test]
    fn svd3x3_rank1() {
        let a = [[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [3.0, 6.0, 9.0]];
        let (_u, s, _vt) = svd3x3(&a).unwrap();
        assert!(s[0] > 1.0, "sigma[0]={}", s[0]);
        assert!(s[1].abs() < 0.1, "sigma[1]={}", s[1]);
        assert!(s[2].abs() < 0.1, "sigma[2]={}", s[2]);
    }

    #[test]
    fn det3x3_identity() {
        let d = det3x3(&identity_matrix());
        assert!((d - 1.0).abs() < 1e-10);
    }

    #[test]
    fn mat_mul_3x3_identity() {
        let a = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
        let result = mat_mul_3x3(&a, &identity_matrix());
        for i in 0..3 {
            for j in 0..3 {
                assert!((result[i][j] - a[i][j]).abs() < 1e-10);
            }
        }
    }
}
