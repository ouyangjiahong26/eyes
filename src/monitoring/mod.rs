//! 监控子系统。
//!
//! 沿抽象层级分为五个子模块：
//! - [`vision`]：数值底层（SVD、PnP）与 ONNX 推理
//! - [`camera`]：摄像头采集与平台 FFI
//! - [`pipeline`]：worker、orchestrator、控制通道、Detector trait
//! - [`events`]：监控事件、worker→事件映射
//! - [`preview`]：预览帧编码
//!
//! 真实子模块路径示例：
//! - `crate::monitoring::pipeline::worker::MonitoringWorker`
//! - `crate::monitoring::pipeline::channel::WorkerCommand`
//! - `crate::monitoring::camera::camera_enumerator::list_cameras`
//! - `crate::monitoring::events::MonitoringEvent`

pub mod camera;
pub mod events;
pub mod pipeline;
pub mod preview;
pub mod vision;
