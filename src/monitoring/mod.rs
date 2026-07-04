pub mod camera_enumerator;
pub mod detector;
pub mod event_sink;
#[cfg(feature = "onnx-detector")]
mod linalg3;
#[cfg(feature = "opencv-camera")]
pub mod opencv_camera;
#[cfg(feature = "onnx-detector")]
pub mod onnx_detector;
pub mod preview;
#[cfg(feature = "onnx-detector")]
pub mod solve_pnp;
#[cfg(target_os = "windows")]
pub mod win32;
pub mod worker;
pub mod channel;
pub mod event_mapping;
pub mod events;
pub mod orchestrator;
// 向后兼容：re-export 全部公开接口
pub use channel::{channel, WorkerCommand, WorkerReceiver, WorkerSender};
pub use events::{EventSink, MonitoringEvent};
pub use orchestrator::{CameraFactory, DetectorFactory, Monitor, MonitorFactory, WorkerOrchestrator};
