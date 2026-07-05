# 0008 — onnx_detector 深度化：提取 3×3 线性代数和 solvePnP

`onnx_detector.rs` 是代码库中最大的文件（653 行），但不是深层模块——接口简单（`Detector::detect` 只接收 RGB 帧、输出 `HeadPose`），实现却混合了三层不相关的关注点。本 ADR 记录沿数据流边界拆为三个模块的决策。

## 考虑过的选项

- **保持单文件，只加注释分隔符** —— 否决：注释分隔不提供编译器强制的边界。删掉线性代数代码后编译不会报错，说明它不是一个真正的 seam。
- **把线性代数抽成 trait，用泛型注入** —— 否决：只有一个实现（手写 Jacobi SVD），trait 带来的间接层零收益。CLAUDE.md 明确要求"只有一个实现的接口"是死灵活性。
- **用 nalgebra 替换全部手写线性代数** —— 否决：当前只在 `dlt_null_space` 中用 nalgebra 做 12×12 SVD，手写 3×3 Jacobi SVD 不引入额外依赖、延迟可忽略（<0.01 ms）。替换为 nalgebra 全家桶是过度工程。
- **拆为三模块：linalg3 + solve_pnp + onnx_detector** —— 采纳：沿已有的数据流边界切割，每个模块可独立测试，不引入新抽象、不改接口。

## 决策

沿数据流的三条自然边界拆为三个模块：

### 1. `monitoring/linalg3.rs`

纯 3×3 线性代数工具，零外部依赖，纯函数。包含 `mat_mul_3x3`、`det3x3`、`transpose_3x3`、`svd3x3`（Jacobi SVD）、`jacobi_eigen_3x3`、`jacobi_rotate`、`eigenvalues_to_sigma`、`sort_descending`、`compute_u`、`orthogonalize_u`、`mat_transpose_times_self`。内部函数保持私有，只通过 `svd3x3` 对外暴露。

独立测试：SVD 正交性（`U^T U = I`）、奇异矩阵（rank-1 矩阵应有两个零奇异值）、单位矩阵。

### 2. `monitoring/solve_pnp.rs`

DLT solvePnP 管道，依赖 linalg3 和 `nalgebra::SVD`。包含 `build_dlt_matrix`、`dlt_null_space`、`extract_rotation_raw`、`orthogonalize_rotation`、`solve_pnp`。`rotation_to_yaw_pitch` 保持 `pub` 供外部使用。`MODEL_3D` 常量放在此模块（`pub(crate)`），因为它是 solvePnP 的 3D 输入模型。

独立测试：沿用现有 6 个投影点 roundtrip 测试 + 2 个 `rotation_to_yaw_pitch` 测试。

### 3. `monitoring/onnx_detector.rs`（缩减）

只保留 YuNet 检测器专属逻辑：`YuNetDetector`、`Detection` 结构体、`preprocess_rgb`、`find_best_detection`、`build_6_point_correspondence`、`estimate_camera_matrix`。`Detector::detect` 实现调用 `solve_pnp::solve_pnp`。

### 接口不变

- `Detector::detect(rgb, width, height) -> Option<HeadPose>` 签名不变
- `rotation_to_yaw_pitch` 保持 `pub`
- `YuNetDetector::new` 签名不变
- 三个文件都在 `#[cfg(feature = "onnx-detector")]` 下

### 测试辅助函数

`identity_matrix`、`yaw_rotation_matrix`、`pitch_rotation_matrix`、`diag_matrix`、`project_points` 是 1-3 行的简单构造器，复制到各模块的 `#[cfg(test)]` 块中，不抽取共享测试模块。

## 后果

- `onnx_detector.rs` 从 653 行缩减到约 200 行，只包含 YuNet 专属逻辑
- `linalg3.rs` 可被未来的其他模块复用（如果需要 3×3 矩阵运算）
- `solve_pnp.rs` 集中了 DLT 管道的所有步骤，便于整体替换（比如未来用 opencv::solve_pnp）
- `nalgebra` 依赖边界更清晰：只在 solve_pnp.rs 中出现
- 需要同步更新 `docs/legacy/migration-plan.md` 中的 `pose.rs` 命名为 `solve_pnp.rs`

## 关联

- Issue: #89
- ADR-0007: ONNX 检测器选型
- Blocked by: #86（编译错误修复，已通过 `cargo check` 验证）
