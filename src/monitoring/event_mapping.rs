//! WorkerOutput / SenseEvent → MonitoringEvent 显式映射。
//!
//! 这些函数实现的是业务逻辑——决定一次纠正该发出哪些事件——
//! 独立于类型转换，便于单独测试。

use crate::domain::classifier::PoseState;
use crate::domain::event_log::AppEventKind;
use crate::domain::posture_tick_engine::SenseEvent;
use crate::monitoring::events::MonitoringEvent;
use crate::monitoring::worker::WorkerOutput;

/// 将单次 tick 的输出转为一组监控事件。
pub fn from_worker_output(output: &WorkerOutput) -> Vec<MonitoringEvent> {
    let mut events = Vec::new();

    if let Some(ref preview) = output.preview {
        events.push(MonitoringEvent::PreviewFrame(preview.clone()));
    }

    if !output.camera_ok {
        events.push(MonitoringEvent::CameraStateChanged {
            state: "unavailable".into(),
        });
        events.push(MonitoringEvent::LogEvent {
            kind: AppEventKind::CameraUnavailable,
            data: serde_json::json!({}),
        });
        return events;
    }

    let pose_str = format!("{:?}", output.pose_state);
    events.push(MonitoringEvent::PoseUpdated {
        yaw: output.yaw,
        pitch: output.pitch,
        pose_state: pose_str,
    });

    for event in &output.sense_events {
        events.extend(from_sense_event(event));
    }

    events
}

/// 将单个 SenseEvent 映射为一组监控事件。
///
/// 这不是纯粹的"转换"——它决定一次纠正该发出哪些事件
/// （警告级别变化、声音提示、日志记录）。
pub fn from_sense_event(event: &SenseEvent) -> Vec<MonitoringEvent> {
    match event {
        SenseEvent::Correction { direction } => {
            let dir = match *direction {
                PoseState::OffAxisLeft => "left",
                PoseState::OffAxisRight => "right",
                PoseState::HeadUp => "up",
                PoseState::HeadDown => "down",
                _ => "unknown",
            };
            vec![
                MonitoringEvent::WarningLevelChanged {
                    level: "correction".into(),
                    direction: Some(dir.into()),
                },
                MonitoringEvent::SoundAlert {
                    alert_type: "posture".into(),
                },
                MonitoringEvent::LogEvent {
                    kind: AppEventKind::PromptFired,
                    data: serde_json::json!({ "prompt": "correction", "direction": dir }),
                },
            ]
        }
        SenseEvent::GoodPosture => {
            vec![
                MonitoringEvent::WarningLevelChanged {
                    level: "good_posture".into(),
                    direction: None,
                },
                MonitoringEvent::LogEvent {
                    kind: AppEventKind::PromptFired,
                    data: serde_json::json!({ "prompt": "good_posture" }),
                },
            ]
        }
        SenseEvent::EyeRest => {
            vec![
                MonitoringEvent::WarningLevelChanged {
                    level: "eye_rest".into(),
                    direction: None,
                },
                MonitoringEvent::SoundAlert {
                    alert_type: "eyerest".into(),
                },
                MonitoringEvent::LogEvent {
                    kind: AppEventKind::PromptFired,
                    data: serde_json::json!({ "prompt": "eye_rest" }),
                },
            ]
        }
        SenseEvent::WarningLevelChanged { level, direction } => {
            let level_str = format!("{:?}", level);
            vec![
                MonitoringEvent::WarningLevelChanged {
                    level: level_str.clone(),
                    direction: direction.clone(),
                },
                MonitoringEvent::LogEvent {
                    kind: AppEventKind::WarningLevelChanged,
                    data: serde_json::json!({
                        "level": level_str,
                        "direction": direction,
                    }),
                },
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::posture_tick_engine::WarningLevel;
    use crate::monitoring::worker::WorkerOutput;

    fn good_output() -> WorkerOutput {
        WorkerOutput {
            preview: None,
            camera_ok: true,
            pose_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
            yaw: Some(0.0),
            pitch: Some(0.0),
            warning_level: WarningLevel::Normal,
            sense_events: Vec::new(),
        }
    }

    fn camera_fail_output() -> WorkerOutput {
        WorkerOutput {
            preview: None,
            camera_ok: false,
            pose_state: PoseState::NoFace,
            pitch_state: PoseState::NoFace,
            yaw: None,
            pitch: None,
            warning_level: WarningLevel::Normal,
            sense_events: Vec::new(),
        }
    }

    #[test]
    fn pose_only() {
        let events = from_worker_output(&good_output());
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            MonitoringEvent::PoseUpdated {
                pose_state: ref ps,
                ..
            }
            if ps == "FacingScreen"
        ));
    }

    #[test]
    fn camera_unavailable() {
        let events = from_worker_output(&camera_fail_output());
        assert!(events.iter().any(|e| matches!(
            e,
            MonitoringEvent::CameraStateChanged { ref state } if state == "unavailable"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            MonitoringEvent::LogEvent { ref kind, .. } if *kind == AppEventKind::CameraUnavailable
        )));
    }

    #[test]
    fn correction_direction() {
        let events = from_sense_event(&SenseEvent::Correction {
            direction: PoseState::OffAxisLeft,
        });
        assert!(events.iter().any(|e| matches!(
            e,
            MonitoringEvent::WarningLevelChanged { ref level, ref direction }
            if level == "correction" && direction.as_deref() == Some("left")
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            MonitoringEvent::SoundAlert { ref alert_type } if alert_type == "posture"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            MonitoringEvent::LogEvent { ref kind, .. } if *kind == AppEventKind::PromptFired
        )));
    }

    #[test]
    fn correction_all_directions() {
        for (pose, dir_str) in [
            (PoseState::OffAxisLeft, "left"),
            (PoseState::OffAxisRight, "right"),
            (PoseState::HeadUp, "up"),
            (PoseState::HeadDown, "down"),
        ] {
            let events = from_sense_event(&SenseEvent::Correction { direction: pose });
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    MonitoringEvent::WarningLevelChanged { ref direction, .. }
                    if direction.as_deref() == Some(dir_str)
                )),
                "expected direction {dir_str} for {pose:?}"
            );
        }
    }

    #[test]
    fn good_posture_events() {
        let events = from_sense_event(&SenseEvent::GoodPosture);
        assert!(events.iter().any(|e| matches!(
            e,
            MonitoringEvent::WarningLevelChanged { ref level, direction }
            if level == "good_posture" && direction.is_none()
        )));
    }

    #[test]
    fn eye_rest_events() {
        let events = from_sense_event(&SenseEvent::EyeRest);
        assert!(events.iter().any(|e| matches!(
            e,
            MonitoringEvent::SoundAlert { ref alert_type } if alert_type == "eyerest"
        )));
    }

    #[test]
    fn warning_level_changed_events() {
        let events = from_sense_event(&SenseEvent::WarningLevelChanged {
            level: WarningLevel::Normal,
            direction: None,
        });
        assert!(events.iter().any(|e| matches!(
            e,
            MonitoringEvent::WarningLevelChanged { ref level, .. }
            if level == "Normal"
        )));
    }

    #[test]
    fn output_with_sense_events() {
        let output = WorkerOutput {
            preview: None,
            camera_ok: true,
            pose_state: PoseState::OffAxisLeft,
            pitch_state: PoseState::FacingScreen,
            yaw: Some(10.0),
            pitch: Some(0.0),
            warning_level: WarningLevel::Normal,
            sense_events: vec![SenseEvent::Correction {
                direction: PoseState::OffAxisLeft,
            }],
        };
        let events = from_worker_output(&output);
        // 1 PoseUpdated + 3 from Correction
        assert_eq!(events.len(), 4);
    }
}
