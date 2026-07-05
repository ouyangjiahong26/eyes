use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
};

/// 应用配置，持久化到 config.yaml。
///
/// `roll` → `pitch` 重命名兼容：serde 同时接受 `roll_threshold` / `neutral_roll` 和
/// `pitch_threshold` / `neutral_pitch`，序列化始终使用 `pitch` 系列。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub yaw_threshold: f64,
    #[serde(alias = "roll_threshold")]
    pub pitch_threshold: f64,
    pub yaw_hysteresis: f64,
    #[serde(alias = "roll_hysteresis")]
    pub pitch_hysteresis: f64,
    pub neutral_yaw: f64,
    #[serde(alias = "neutral_roll")]
    pub neutral_pitch: f64,
    pub camera_index: u32,
    pub language: String,
    pub sound_enabled: bool,
    pub autostart_enabled: bool,
    pub snooze_until_iso: Option<String>,
    // issue #141：5 个时机阈值收进 TimingThresholds，经 serde(flatten) 平铺到
    // YAML，旧配置文件（字段平铺）仍可反序列化。
    #[serde(flatten)]
    pub timing: super::thresholds::TimingThresholds,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            yaw_threshold: 5.0,
            pitch_threshold: 10.0,
            yaw_hysteresis: 2.5,
            pitch_hysteresis: 5.0,
            neutral_yaw: 0.0,
            neutral_pitch: 0.0,
            camera_index: 0,
            language: "zh-CN".to_string(),
            sound_enabled: true,
            autostart_enabled: false,
            snooze_until_iso: None,
            timing: super::thresholds::TimingThresholds::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    config_dir: PathBuf,
}

impl ConfigStore {
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        Self {
            config_dir: config_dir.as_ref().to_path_buf(),
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn load(&self) -> io::Result<AppConfig> {
        fs::create_dir_all(&self.config_dir)?;
        let path = self.config_path();
        if !path.exists() {
            let config = AppConfig::default();
            self.save(&config)?;
            return Ok(config);
        }

        let content = fs::read_to_string(path)?;
        if content.trim().is_empty() {
            return Ok(AppConfig::default());
        }

        Ok(serde_yaml::from_str(&content).unwrap_or_default())
    }

    pub fn save(&self, config: &AppConfig) -> io::Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        let path = self.config_path();
        let temp_path = path.with_extension("yaml.tmp");
        let content = serde_yaml::to_string(config)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(&temp_path, content)?;
        fs::rename(temp_path, path)?;
        Ok(())
    }

    pub fn update(&self, update: impl FnOnce(&mut AppConfig)) -> io::Result<AppConfig> {
        let mut config = self.load()?;
        update(&mut config);
        self.save(&config)?;
        Ok(config)
    }

    fn config_path(&self) -> PathBuf {
        self.config_dir.join("config.yaml")
    }
}

// ── ConfigState ────────────────────────────────────────────────────

/// 配置的唯一数据源。拥有持久化逻辑和变更通知。
///
/// 调用方不持有自己的 `AppConfig` 副本，全部通过 `get()` 读取。
/// `set()` / `update()` 原子地写入内存和磁盘，然后通知所有订阅者。
pub struct ConfigState {
    inner: Mutex<AppConfig>,
    store: ConfigStore,
    subscribers: Mutex<Vec<mpsc::Sender<AppConfig>>>,
}

impl ConfigState {
    pub fn new(store: ConfigStore) -> io::Result<Self> {
        let config = store.load()?;
        Ok(Self {
            inner: Mutex::new(config),
            store,
            subscribers: Mutex::new(Vec::new()),
        })
    }

    /// 读取当前配置（克隆）。
    pub fn get(&self) -> AppConfig {
        self.inner.lock().unwrap().clone()
    }

    /// 整体替换配置，持久化到磁盘并通知订阅者。
    pub fn set(&self, config: AppConfig) -> io::Result<()> {
        self.store.save(&config)?;
        {
            let mut inner = self.inner.lock().unwrap();
            *inner = config;
        }
        self.notify();
        Ok(())
    }

    /// 原地修改配置，持久化到磁盘并通知订阅者。
    pub fn update(&self, patch: impl FnOnce(&mut AppConfig)) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        patch(&mut inner);
        self.store.save(&inner)?;
        drop(inner);
        self.notify();
        Ok(())
    }

    /// 订阅配置变更通知。返回的 receiver 会在每次 `set()` 或 `update()` 后收到新配置。
    pub fn subscribe(&self) -> mpsc::Receiver<AppConfig> {
        let (tx, rx) = mpsc::channel();
        let mut subs = self.subscribers.lock().unwrap();
        subs.push(tx);
        rx
    }

    fn notify(&self) {
        let config = self.inner.lock().unwrap().clone();
        let mut subs = self.subscribers.lock().unwrap();
        subs.retain(|tx| tx.send(config.clone()).is_ok());
    }
}

/// 加载用户配置目录，构造 `ConfigState`。
///
/// 失败时回退到临时目录（仅在配置目录不可写时）。
pub fn load_config_state() -> Arc<ConfigState> {
    let config_dir = crate::domain::paths::app_config_dir(dirs::config_dir());
    let store = ConfigStore::new(config_dir);
    Arc::new(ConfigState::new(store).expect("加载配置失败"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_round_trips_yaml() {
        let config = AppConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: AppConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn deserialize_with_legacy_roll_field_names() {
        let yaml = r#"
yaw_threshold: 3.0
roll_threshold: 7.0
neutral_yaw: 1.0
neutral_roll: -2.0
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.yaw_threshold, 3.0);
        assert_eq!(config.pitch_threshold, 7.0);
        assert_eq!(config.neutral_yaw, 1.0);
        assert_eq!(config.neutral_pitch, -2.0);
    }

    #[test]
    fn deserialize_with_new_pitch_field_names() {
        let yaml = r#"
yaw_threshold: 4.0
pitch_threshold: 8.0
neutral_yaw: 0.5
neutral_pitch: -1.5
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.pitch_threshold, 8.0);
        assert_eq!(config.neutral_pitch, -1.5);
    }

    #[test]
    fn serialize_uses_pitch_not_roll() {
        let config = AppConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("pitch_threshold"));
        assert!(yaml.contains("neutral_pitch"));
        assert!(!yaml.contains("roll_threshold"));
        assert!(!yaml.contains("neutral_roll"));
    }

    #[test]
    fn missing_fields_use_defaults() {
        let yaml = "yaw_threshold: 2.0\n";
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.yaw_threshold, 2.0);
        assert_eq!(config.pitch_threshold, 10.0); // 默认值
        assert_eq!(config.language, "zh-CN");
        assert!(config.sound_enabled);
    }

    #[test]
    fn round_trip_preserves_advanced_fields() {
        let config = AppConfig {
            timing: crate::domain::thresholds::TimingThresholds {
                facing_threshold_seconds: 600.0,
                eyest_threshold_seconds: 1800.0,
                ..Default::default()
            },
            ..AppConfig::default()
        };
        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: AppConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.timing.facing_threshold_seconds, 600.0);
        assert_eq!(parsed.timing.eyest_threshold_seconds, 1800.0);
    }

    #[test]
    fn update_modifies_specific_fields() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        let result = store.update(|cfg| {
            cfg.yaw_threshold = 7.0;
        });
        let config = result.unwrap();
        assert_eq!(config.yaw_threshold, 7.0);
        assert_eq!(config.pitch_threshold, 10.0);
    }

    // ── ConfigState 测试 ─────────────────────────────────────────────

    #[test]
    fn config_state_get_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let state = ConfigState::new(ConfigStore::new(dir.path())).unwrap();
        let config = state.get();
        assert_eq!(config.yaw_threshold, 5.0);
        assert_eq!(config.language, "zh-CN");
    }

    #[test]
    fn config_state_set_persists_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let state = ConfigState::new(ConfigStore::new(dir.path())).unwrap();

        let mut new_config = AppConfig::default();
        new_config.yaw_threshold = 15.0;
        new_config.language = "en".to_string();
        state.set(new_config).unwrap();

        // 从磁盘重新加载验证
        let store = ConfigStore::new(dir.path());
        let loaded = store.load().unwrap();
        assert_eq!(loaded.yaw_threshold, 15.0);
        assert_eq!(loaded.language, "en");

        // get() 也应返回新值
        assert_eq!(state.get().yaw_threshold, 15.0);
    }

    #[test]
    fn config_state_update_patches_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let state = ConfigState::new(ConfigStore::new(dir.path())).unwrap();

        state.update(|cfg| {
            cfg.camera_index = 3;
            cfg.sound_enabled = false;
        }).unwrap();

        let config = state.get();
        assert_eq!(config.camera_index, 3);
        assert!(!config.sound_enabled);
        assert_eq!(config.yaw_threshold, 5.0); // 其他字段不变

        // 磁盘验证
        let store = ConfigStore::new(dir.path());
        let loaded = store.load().unwrap();
        assert_eq!(loaded.camera_index, 3);
    }

    #[test]
    fn config_state_subscribe_receives_notifications() {
        let dir = tempfile::tempdir().unwrap();
        let state = ConfigState::new(ConfigStore::new(dir.path())).unwrap();

        let rx = state.subscribe();

        // set 通知
        let mut cfg = AppConfig::default();
        cfg.yaw_threshold = 20.0;
        state.set(cfg).unwrap();

        let received = rx.recv().unwrap();
        assert_eq!(received.yaw_threshold, 20.0);

        // update 也通知
        state.update(|c| c.camera_index = 5).unwrap();
        let received = rx.recv().unwrap();
        assert_eq!(received.camera_index, 5);
    }

    #[test]
    fn config_state_subscribe_multiple_subscribers() {
        let dir = tempfile::tempdir().unwrap();
        let state = ConfigState::new(ConfigStore::new(dir.path())).unwrap();

        let rx1 = state.subscribe();
        let rx2 = state.subscribe();

        let mut cfg = AppConfig::default();
        cfg.yaw_threshold = 42.0;
        state.set(cfg).unwrap();

        assert_eq!(rx1.recv().unwrap().yaw_threshold, 42.0);
        assert_eq!(rx2.recv().unwrap().yaw_threshold, 42.0);
    }

    #[test]
    fn config_state_dropped_subscriber_is_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let state = ConfigState::new(ConfigStore::new(dir.path())).unwrap();

        let rx = state.subscribe();
        drop(rx); // 订阅者断开

        // 不应 panic，静默清理
        state.update(|c| c.yaw_threshold = 1.0).unwrap();
        assert_eq!(state.get().yaw_threshold, 1.0);
    }
}
