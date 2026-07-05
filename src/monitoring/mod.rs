//! 监控子系统。
//!
//! 沿抽象层级分为五个子模块：
//! - [`vision`]：数值底层（SVD、PnP）与 ONNX 推理
//! - [`camera`]：摄像头采集与平台 FFI
//! - [`pipeline`]：worker、orchestrator、控制通道、Detector trait
//! - [`events`]：监控事件、worker→事件映射、事件 sink
//! - [`preview`]：预览帧编码
//!
//! 下方 `pub use` 兼容层保留旧路径（如 `crate::monitoring::worker::WorkerOutput`），
//! 外部引用无需改动。

pub mod camera;
pub mod events;
pub mod pipeline;
pub mod vision;
pub mod preview;

// 模块别名：旧路径 `crate::monitoring::worker` / `::channel` / `::orchestrator`
// / `::detector` / `::events` / `::event_mapping` / `::event_sink`
// / `::camera_enumerator` / `::opencv_camera` / `::onnx_detector` / `::solve_pnp`
// / `::win32` 继续可用。
pub use pipeline::{channel, detector, orchestrator, worker};
pub use events::{event_mapping, event_sink};
pub use camera::camera_enumerator;
#[cfg(feature = "opencv-camera")]
pub use camera::opencv_camera;
#[cfg(feature = "onnx-detector")]
pub use vision::{onnx_detector, solve_pnp};
#[cfg(target_os = "windows")]
pub use camera::win32;

// 向后兼容：re-export 全部公开接口
pub use channel::{channel, WorkerCommand, WorkerReceiver, WorkerSender};
pub use self::events::{EventSink, MonitoringEvent};
pub use orchestrator::{CameraFactory, DetectorFactory, Monitor, MonitorFactory, WorkerOrchestrator};
