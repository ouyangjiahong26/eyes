//! 事件分类枚举。
//!
//! 这里只保留 `AppEventKind`——它被 `event_mapping` 用于事件去重，
//! 把 worker 输出的原始信息打上稳定的事件种类标签。
//! 历史上曾有一套把事件落盘到 `events.jsonl` 的 `EventLog`/`AppEvent`
//! 机制，生产代码零调用，已删除。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppEventKind {
    #[serde(rename = "STATE_CHANGE")]
    StateChange,
    #[serde(rename = "PROMPT_FIRED")]
    PromptFired,
    #[serde(rename = "CAMERA_UNAVAILABLE")]
    CameraUnavailable,
    #[serde(rename = "CAMERA_RESUMED")]
    CameraResumed,
    #[serde(rename = "SNOOZE_START")]
    SnoozeStart,
    #[serde(rename = "SNOOZE_END")]
    SnoozeEnd,
    #[serde(rename = "WARNING_LEVEL_CHANGED")]
    WarningLevelChanged,
}
