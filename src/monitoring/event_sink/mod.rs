//! `EventSink` 的不同实现。
//!
//! VS0 只定义了 `BevyEventSink` 类型与一个空实现的 `NullSink`。
//! 后者让 `WorkerOrchestrator` 在没接任何 sink 时也能跑空 tick 循环；
//! 前者定义了 VS1+ 接入 Bevy 事件总线的形态，本切片暂不构造。

pub mod bevy_sink;

pub use bevy_sink::BevyEventSink;

use std::sync::Arc;

use crate::monitoring::events::{EventSink, MonitoringEvent};

/// 空事件 sink。所有事件直接丢弃。
///
/// 用途：VS0 让 orchestrator 跑空 tick 循环而无需 Bevy 介入。
/// 后续切片接 Bevy 事件总线时会被 `BevyEventSink` 替换。
#[derive(Clone, Default)]
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: MonitoringEvent) {
        // 故意为空——VS0 不消费监控事件。
        let _ = Arc::new(());
    }
}
