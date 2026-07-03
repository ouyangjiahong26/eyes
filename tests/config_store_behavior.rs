use eyes_lib::domain::config::{AppConfig, ConfigStore};

#[test]
fn first_run_creates_default_yaml_config() {
    let temp = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(temp.path());

    let config = store.load().unwrap();

    assert_eq!(config, AppConfig::default());
    let config_file = temp.path().join("config.yaml");
    assert!(config_file.exists());
    let yaml = std::fs::read_to_string(config_file).unwrap();
    assert!(yaml.contains("yaw_threshold: 5"));
    assert!(yaml.contains("language: zh-CN"));
}

#[test]
fn roundtrips_saved_config_and_uses_defaults_for_partial_yaml() {
    let temp = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(temp.path());
    let config = AppConfig {
        yaw_threshold: 20.0,
        pitch_threshold: 12.0,
        neutral_yaw: 3.0,
        neutral_pitch: -2.0,
        camera_index: 1,
        snooze_until_iso: Some("2026-05-11T12:00:00+08:00".to_string()),
        sound_enabled: true,
        autostart_enabled: true,
        language: "en-US".to_string(),
        ..AppConfig::default()
    };

    store.save(&config).unwrap();
    assert_eq!(store.load().unwrap(), config);

    std::fs::write(
        temp.path().join("config.yaml"),
        "language: ja-JP\nunknown_field: ignored\n",
    )
    .unwrap();
    assert_eq!(
        store.load().unwrap(),
        AppConfig {
            language: "ja-JP".to_string(),
            ..AppConfig::default()
        }
    );
}

#[test]
fn invalid_or_empty_yaml_falls_back_to_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(temp.path());
    store.load().unwrap();

    std::fs::write(temp.path().join("config.yaml"), "invalid: yaml: content: {").unwrap();
    assert_eq!(store.load().unwrap(), AppConfig::default());

    std::fs::write(temp.path().join("config.yaml"), "").unwrap();
    assert_eq!(store.load().unwrap(), AppConfig::default());
}

#[test]
fn partial_update_preserves_unspecified_fields_and_leaves_no_tmp_file() {
    let temp = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(temp.path());
    let original = store.load().unwrap();

    let updated = store
        .update(|config| {
            config.yaw_threshold = 25.0;
            config.language = "ja-JP".to_string();
        })
        .unwrap();

    assert_eq!(updated.yaw_threshold, 25.0);
    assert_eq!(updated.language, "ja-JP");
    assert_eq!(updated.pitch_threshold, original.pitch_threshold);
    assert_eq!(updated.camera_index, original.camera_index);
    assert!(std::fs::read_dir(temp.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".tmp")));
}
