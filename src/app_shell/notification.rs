//! 系统通知：订阅 `WarningLevelChanged`，通过 `notify-rust` 发 toast。
//!
//! VS4：通知触发逻辑独立于设置面板（VS2），只订阅事件总线。
//! snooze 状态下（`SnoozeResource` 处于激活态）跳过通知，避免打扰。
//!
//! 通知内容映射（基于 worker 产出的 level 字符串）：
//! - `"correction"` → 偏头提醒："请调整头部方向" + 方向提示
//! - `"eye_rest"`   → 眼休提醒："请眺望远方休息一下"
//! - 其余（`"good_posture"` / `"Normal"` / `"Corrected"` 等）→ 不通知

use bevy::prelude::*;

use crate::monitoring::events::MonitoringEvent;

/// snooze 运行时状态。存在即表示处于暂停提醒状态。
///
/// 由 tray 菜单的 Pause / Resume 入口更新。
/// `Some(())` 表示暂停中（不通知）；`None` 表示正常工作。
/// 时间维度的到期由 worker 端 `WorkerCommand::Snooze(seconds)` 负责，
/// Bevy 侧只跟踪布尔语义。
#[derive(Resource, Default)]
pub struct SnoozeResource {
    pub paused: bool,
}

impl SnoozeResource {
    pub fn is_paused(&self) -> bool {
        self.paused
    }
}

/// 启动时检测系统通知能力。失败只记日志，不阻塞应用启动。
pub fn check_notification_capability() {
    // notify-rust 在 Windows 走 toast（tauri-winrt-notification），
    // Linux 走 D-Bus。show() 在 Windows 上返回一个 NotificationHandle（异步），
    // 在 Linux 上同步发送后立即返回。失败时只记日志。
    if let Err(e) = notify_rust::Notification::new()
        .summary("Eyes")
        .body("通知已启用")
        .show()
    {
        bevy::log::warn!("系统通知不可用: {e}");
    }
}

/// 订阅 `WarningLevelChanged`，按 level 发系统通知。
///
/// snooze 状态下跳过。`OffAxisReminder`（"correction"）和
/// `EyestReminder`（"eye_rest"）触发通知；`Good` 及恢复类不触发。
pub fn update_system_notification_system(
    mut reader: EventReader<MonitoringEvent>,
    snooze: Res<SnoozeResource>,
) {
    if snooze.is_paused() {
        // 暂停期间消费掉事件但不通知。
        reader.clear();
        return;
    }

    for event in reader.read() {
        if let MonitoringEvent::WarningLevelChanged { level, direction } = event {
            if let Some((title, body)) = notification_text(level, direction.as_deref()) {
                // notify-rust 的发送在大多数平台上是异步的，不阻塞 Bevy 主循环。
                if let Err(e) = notify_rust::Notification::new()
                    .summary(title)
                    .body(&body)
                    .show()
                {
                    bevy::log::warn!("系统通知发送失败: {e}");
                }
            }
        }
    }
}

/// 根据 level 字符串决定通知文案。返回 `None` 表示不发通知。
fn notification_text(level: &str, direction: Option<&str>) -> Option<(&'static str, String)> {
    match level {
        "correction" => {
            let dir_hint = direction.map(direction_hint).unwrap_or("");
            let body = if dir_hint.is_empty() {
                "请调整头部方向".to_string()
            } else {
                format!("请调整头部方向（{dir_hint}）")
            };
            Some(("Eyes", body))
        }
        "eye_rest" => Some(("Eyes", "请眺望远方休息一下".to_string())),
        // good_posture / Normal / Corrected 等正面或恢复状态不通知
        _ => None,
    }
}

/// 把方向字符串转为中文提示。
fn direction_hint(dir: &str) -> &'static str {
    match dir {
        "left" => "向左偏",
        "right" => "向右偏",
        "up" => "抬头",
        "down" => "低头",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_level_produces_notification_with_direction() {
        let (title, body) = notification_text("correction", Some("left")).unwrap();
        assert_eq!(title, "Eyes");
        assert!(body.contains("向左偏"));
    }

    #[test]
    fn correction_level_without_direction_still_notifies() {
        let (_, body) = notification_text("correction", None).unwrap();
        assert!(body.contains("请调整头部方向"));
        assert!(!body.contains("（"));
    }

    #[test]
    fn eye_rest_level_produces_notification() {
        let (_, body) = notification_text("eye_rest", None).unwrap();
        assert!(body.contains("眺望"));
    }

    #[test]
    fn good_posture_level_does_not_notify() {
        assert!(notification_text("good_posture", None).is_none());
    }

    #[test]
    fn normal_level_does_not_notify() {
        assert!(notification_text("Normal", None).is_none());
    }

    #[test]
    fn corrected_level_does_not_notify() {
        assert!(notification_text("Corrected", None).is_none());
    }

    #[test]
    fn snooze_resource_defaults_to_not_paused() {
        let snooze = SnoozeResource::default();
        assert!(!snooze.is_paused());
    }
}
