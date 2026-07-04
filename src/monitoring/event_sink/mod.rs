//! `EventSink` 的不同实现。
//!
//! `BevyEventSink` 把监控事件通过 mpsc channel 推到 Bevy 主线程（VS1 起接入）。
//! `NullSink` 丢弃所有事件，用于测试或不需事件输出的场景。

pub mod bevy_sink;

pub use bevy_sink::BevyEventSink;

use std::sync::Arc;

use crate::monitoring::events::{EventSink, MonitoringEvent};

/// 空事件 sink。所有事件直接丢弃。
///
/// 用于测试或不需要事件输出的场景。VS1 起 `WorkerOrchestrator`
/// 使用 `BevyEventSink` 替代本类型。
#[derive(Clone, Default)]
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: MonitoringEvent) {
        // 故意为空——VS0 不消费监控事件。
        let _ = Arc::new(());
    }
}
