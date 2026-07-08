//! 设置面板的草稿数据结构及其与 AppConfig 的转换。

use bevy::prelude::*;

use crate::domain::config::AppConfig;
use crate::monitoring::camera::camera_enumerator::CameraDevice;

// ── 资源 ───────────────────────────────────────────────────────

/// 设置面板正在编辑的草稿值。打开面板时从 `ConfigState` 初始化。
#[derive(Resource, Clone)]
pub struct SettingsDraft {
    pub yaw_threshold: f64,
    pub pitch_threshold: f64,
    pub yaw_hysteresis: f64,
    pub pitch_hysteresis: f64,
    pub camera_index: u32,
    pub sound_enabled: bool,
    pub autostart_enabled: bool,
    pub language: String,
    pub off_axis_streak_threshold: f64,
    pub off_axis_repeat_interval: f64,
    pub off_axis_severe_threshold: f64,
    pub facing_threshold: f64,
    pub eyerest_threshold: f64,
    pub camera_list: Vec<CameraDevice>,
    pub advanced_visible: bool,
}

impl SettingsDraft {
    pub fn from_config(config: &AppConfig, camera_list: Vec<CameraDevice>) -> Self {
        Self {
            yaw_threshold: config.yaw_threshold,
            pitch_threshold: config.pitch_threshold,
            yaw_hysteresis: config.yaw_hysteresis,
            pitch_hysteresis: config.pitch_hysteresis,
            camera_index: config.camera_index,
            sound_enabled: config.sound_enabled,
            autostart_enabled: config.autostart_enabled,
            language: config.language.clone(),
            off_axis_streak_threshold: config.timing.off_axis_streak_threshold_seconds,
            off_axis_repeat_interval: config.timing.off_axis_repeat_interval_seconds,
            off_axis_severe_threshold: config.timing.off_axis_severe_threshold_seconds,
            facing_threshold: config.timing.facing_threshold_seconds,
            eyerest_threshold: config.timing.eyerest_threshold_seconds,
            camera_list,
            advanced_visible: false,
        }
    }
}

/// 把 draft 转成 AppConfig。
pub(crate) fn draft_to_config(draft: &SettingsDraft, base: &AppConfig) -> AppConfig {
    let mut cfg = base.clone();
    cfg.yaw_threshold = draft.yaw_threshold;
    cfg.pitch_threshold = draft.pitch_threshold;
    cfg.yaw_hysteresis = draft.yaw_hysteresis;
    cfg.pitch_hysteresis = draft.pitch_hysteresis;
    cfg.camera_index = draft.camera_index;
    cfg.sound_enabled = draft.sound_enabled;
    cfg.autostart_enabled = draft.autostart_enabled;
    cfg.language = draft.language.clone();
    cfg.timing.off_axis_streak_threshold_seconds = draft.off_axis_streak_threshold;
    cfg.timing.off_axis_repeat_interval_seconds = draft.off_axis_repeat_interval;
    cfg.timing.off_axis_severe_threshold_seconds = draft.off_axis_severe_threshold;
    cfg.timing.facing_threshold_seconds = draft.facing_threshold;
    cfg.timing.eyerest_threshold_seconds = draft.eyerest_threshold;
    cfg
}

/// 把 [0,1] 映射到 [min,max] 并按 step 对齐。
pub(crate) fn snap(frac: f64, min: f64, max: f64, step: f64) -> f64 {
    let raw = min + frac * (max - min);
    let snapped = ((raw - min) / step).round() * step + min;
    snapped.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_aligns_to_step() {
        // frac 0.0 → min
        assert!((snap(0.0, 1.0, 30.0, 0.5) - 1.0).abs() < 1e-9);
        // frac 1.0 → max
        assert!((snap(1.0, 1.0, 30.0, 0.5) - 30.0).abs() < 1e-9);
        // frac 0.5 → 约 15.5
        assert!((snap(0.5, 1.0, 30.0, 0.5) - 15.5).abs() < 1e-9);
    }

    #[test]
    fn snap_clamps() {
        assert!((snap(-1.0, 1.0, 30.0, 0.5) - 1.0).abs() < 1e-9);
        assert!((snap(2.0, 1.0, 30.0, 0.5) - 30.0).abs() < 1e-9);
    }

    #[test]
    fn draft_from_config_copies_fields() {
        let mut config = AppConfig::default();
        config.yaw_threshold = 12.0;
        config.language = "en".to_string();
        let draft = SettingsDraft::from_config(&config, vec![]);
        assert_eq!(draft.yaw_threshold, 12.0);
        assert_eq!(draft.language, "en");
    }

    #[test]
    fn draft_to_config_preserves_unedited_fields() {
        let mut base = AppConfig::default();
        base.neutral_yaw = 3.0;
        base.timing.facing_threshold_seconds = 600.0;
        let draft = SettingsDraft::from_config(&base, vec![]);
        let result = draft_to_config(&draft, &base);
        assert_eq!(result.neutral_yaw, 3.0);
        assert_eq!(result.timing.facing_threshold_seconds, 600.0);
    }
}
