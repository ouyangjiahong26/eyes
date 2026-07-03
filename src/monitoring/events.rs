//! 监控事件类型与输出抽象。

use bevy::ecs::event::Event;

use crate::domain::event_log::AppEventKind;
use crate::monitoring::preview::PreviewFrame;

/// 监控 worker 向外部发射的事件。
#[derive(Debug, Clone, Event)]
pub enum MonitoringEvent {
    CameraStateChanged { state: String },
    PoseUpdated {
        yaw: Option<f64>,
        pitch: Option<f64>,
        pose_state: String,
    },
    PreviewFrame(PreviewFrame),
    SoundAlert { alert_type: String },
    WarningLevelChanged {
        level: String,
        direction: Option<String>,
    },
    LogEvent {
        kind: AppEventKind,
        data: serde_json::Value,
    },
    LogInfo {
        message: String,
    },
    CalibrationComplete {
        yaw: f64,
        pitch: f64,
        sample_count: usize,
    },
    CalibrationFailed {
        reason: String,
    },
}

pub trait EventSink: Send + 'static {
    fn emit(&self, event: MonitoringEvent);
}
