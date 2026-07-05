//! `EventSink` 的不同实现。
//!
//! `BevyEventSink` 把监控事件通过 mpsc channel 推到 Bevy 主线程（VS1 起接入）。

pub mod bevy_sink;

pub use bevy_sink::BevyEventSink;
