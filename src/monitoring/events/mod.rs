//! 事件层：监控事件、worker 输出映射、事件 sink。

pub mod event_mapping;
pub mod event_sink;
pub mod events;

// 向后兼容：re-export events.rs 的公开接口，使
// `crate::monitoring::events::MonitoringEvent` 等旧路径继续可用。
pub use self::events::*;
