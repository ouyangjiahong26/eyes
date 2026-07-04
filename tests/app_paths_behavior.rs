use eyes_lib::domain::config::ConfigStore;
use eyes_lib::domain::event_log::{AppEventKind, EventLog};
use eyes_lib::domain::paths::app_config_dir;
use serde_json::json;

#[test]
fn config_and_event_log_share_eyes_subdirectory() {
    let temp = tempfile::tempdir().unwrap();
    let dir = app_config_dir(Some(temp.path().to_path_buf()));

    let store = ConfigStore::new(&dir);
    store.load().unwrap();

    let log = EventLog::new(&dir);
    log.append(AppEventKind::StateChange, json!({})).unwrap();

    assert!(
        dir.ends_with("eyes"),
        "application directory should end with 'eyes': {dir:?}"
    );
    assert!(dir.join("config.yaml").exists(), "config.yaml should be under eyes dir");
    assert!(dir.join("events.jsonl").exists(), "events.jsonl should be under eyes dir");
}
