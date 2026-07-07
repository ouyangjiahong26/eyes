use eyes_lib::domain::config::ConfigStore;
use eyes_lib::domain::paths::app_config_dir;

#[test]
fn config_store_uses_eyes_subdirectory() {
    let temp = tempfile::tempdir().unwrap();
    let dir = app_config_dir(Some(temp.path().to_path_buf()));

    let store = ConfigStore::new(&dir);
    store.load().unwrap();

    assert!(
        dir.ends_with("eyes"),
        "application directory should end with 'eyes': {dir:?}"
    );
    assert!(dir.join("config.yaml").exists(), "config.yaml should be under eyes dir");
}
