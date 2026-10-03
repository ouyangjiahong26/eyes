// 5 个 off-axis / 累积时机阈值，集中承载以消除 Shotgun Surgery（issue #141）。
//
// 历史背景：issue #84 新增 `off_axis_severe_threshold_seconds` 时，一次改动穿透
// 6 文件 11 处。把这 5 个阈值收进同一结构体后，新增第 6 个阈值只需在此处加字段
// 并补默认值，`PostureTickEngine::new` / `update_timing` / `Monitor::update_timing`
// 的签名与所有调用点都不用动。
//
// 同时用于：
// - `PostureTickEngine::new` / `update_timing` 的入参；
// - `AppConfig` 经 `serde(flatten)` 嵌入，使 YAML 字段保持平铺（旧配置文件仍可读）。

use serde::{Deserialize, Serialize};

use super::defaults;

/// 5 个 off-axis / 累积时机阈值。
///
/// 字段名带 `_seconds` 后缀，与历史 YAML 配置文件一致；`AppConfig` 经
/// `serde(flatten)` 嵌入本结构体后，序列化产物与重构前字节兼容。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimingThresholds {
    /// 偏离连续时长阈值（秒）。
    pub off_axis_streak_threshold_seconds: f64,
    /// 偏离重复提醒间隔（秒）。
    pub off_axis_repeat_interval_seconds: f64,
    /// 偏离严重升级时长阈值（秒）。
    pub off_axis_severe_threshold_seconds: f64,
    /// 正面朝向累积时长阈值（秒）。
    pub facing_threshold_seconds: f64,
    /// 用眼休息累积时长阈值（秒）。
    #[serde(alias = "eyest_threshold_seconds")]
    pub eyerest_threshold_seconds: f64,
}

impl Default for TimingThresholds {
    fn default() -> Self {
        Self {
            off_axis_streak_threshold_seconds: defaults::OFF_AXIS_STREAK_THRESHOLD,
            off_axis_repeat_interval_seconds: defaults::OFF_AXIS_REPEAT_INTERVAL,
            off_axis_severe_threshold_seconds: defaults::OFF_AXIS_SEVERE_THRESHOLD,
            facing_threshold_seconds: defaults::FACING_THRESHOLD,
            eyerest_threshold_seconds: defaults::EYEREST_THRESHOLD,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_defaults_constants() {
        let t = TimingThresholds::default();
        assert_eq!(
            t.off_axis_streak_threshold_seconds,
            defaults::OFF_AXIS_STREAK_THRESHOLD
        );
        assert_eq!(
            t.off_axis_repeat_interval_seconds,
            defaults::OFF_AXIS_REPEAT_INTERVAL
        );
        assert_eq!(
            t.off_axis_severe_threshold_seconds,
            defaults::OFF_AXIS_SEVERE_THRESHOLD
        );
        assert_eq!(t.facing_threshold_seconds, defaults::FACING_THRESHOLD);
        assert_eq!(t.eyerest_threshold_seconds, defaults::EYEREST_THRESHOLD);
    }
}
