//! 数值底层与推理：Jacobi SVD、DLT PnP、ONNX 人脸检测。

#[cfg(feature = "onnx-detector")]
mod linalg3;
#[cfg(feature = "onnx-detector")]
pub mod onnx_detector;
#[cfg(feature = "onnx-detector")]
pub mod solve_pnp;
