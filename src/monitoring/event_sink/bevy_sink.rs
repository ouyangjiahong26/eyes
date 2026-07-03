//! `BevyEventSink` —— 把 `MonitoringEvent` 转发到 Bevy 事件总线的实现。
//!
//! VS0：类型已定义并实现 `EventSink`，但 `WorkerOrchestrator` 暂不构造它。
//! VS1+ 接入时，Bevy 端用 `mpsc::Receiver<MonitoringEvent>` 作为资源，
//! 配合一个 `Update` 系统把事件转成 Bevy `Events<MonitoringEvent>` 写入。
//!
//! 设计动机：`EventSink::emit(&self, ...)` 可能在后台线程被调用，
//! 而 Bevy 的 `EventWriter` 不是 `Send`。通过 `mpsc` 解耦可以保留
//! 线程安全。

use std::sync::mpsc::Sender;

use crate::monitoring::events::{EventSink, MonitoringEvent};

/// 把监控事件转发到 Bevy 主线程的 sink。
///
/// 内部持有一个 `mpsc::Sender`，由 VS1+ 的 Bevy 系统在主线程消费对应 receiver。
pub struct BevyEventSink {
    tx: Sender<MonitoringEvent>,
}

impl BevyEventSink {
    /// 用一个已建好的 `mpsc::Sender` 构造 sink。
    /// 通常由 `lib.rs` 在主线程创建 channel 后传入。
    pub fn new(tx: Sender<MonitoringEvent>) -> Self {
        Self { tx }
    }
}

impl EventSink for BevyEventSink {
    fn emit(&self, event: MonitoringEvent) {
        // 发送失败说明接收端已掉（应用退出），不必 panic。
        let _ = self.tx.send(event);
    }
}
