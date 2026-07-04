use eyes_lib::domain::event_log::{AppEventKind, EventLog};
use serde_json::Value;

#[test]
fn appending_events_creates_jsonl_file_and_preserves_order() {
    let temp = tempfile::tempdir().unwrap();
    let log = EventLog::new(temp.path());

    log.append(AppEventKind::CameraUnavailable, serde_json::json!({}))
        .unwrap();
    log.append(AppEventKind::CameraResumed, serde_json::json!({}))
        .unwrap();

    let content = std::fs::read_to_string(temp.path().join("events.jsonl")).unwrap();
    let lines: Vec<_> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("CAMERA_UNAVAILABLE"));
    assert!(lines[1].contains("CAMERA_RESUMED"));
}

#[test]
fn serializes_every_event_kind() {
    let temp = tempfile::tempdir().unwrap();
    let log = EventLog::new(temp.path());

    for kind in [
        AppEventKind::StateChange,
        AppEventKind::PromptFired,
        AppEventKind::CameraUnavailable,
        AppEventKind::CameraResumed,
        AppEventKind::SnoozeStart,
        AppEventKind::SnoozeEnd,
        AppEventKind::WarningLevelChanged,
    ] {
        log.append(kind, serde_json::json!({ "state": "VISIBLE" }))
            .unwrap();
    }

    let content = std::fs::read_to_string(temp.path().join("events.jsonl")).unwrap();
    for expected in [
        "STATE_CHANGE",
        "PROMPT_FIRED",
        "CAMERA_UNAVAILABLE",
        "CAMERA_RESUMED",
        "SNOOZE_START",
        "SNOOZE_END",
        "WARNING_LEVEL_CHANGED",
    ] {
        assert!(content.contains(expected));
    }
}

#[test]
fn event_payload_is_jsonl_and_contains_no_biometric_data_unless_caller_adds_it() {
    let temp = tempfile::tempdir().unwrap();
    let log = EventLog::new(temp.path());

    log.append(
        AppEventKind::StateChange,
        serde_json::json!({ "state": "OFF_AXIS_LEFT" }),
    )
    .unwrap();
    log.append(
        AppEventKind::PromptFired,
        serde_json::json!({ "prompt": "adjust", "direction": "LEFT" }),
    )
    .unwrap();

    let content = std::fs::read_to_string(temp.path().join("events.jsonl")).unwrap();
    for line in content.lines() {
        let value: Value = serde_json::from_str(line).unwrap();
        assert!(value.get("ts").is_some());
        assert!(value.get("kind").is_some());
    }
    let lower = content.to_lowercase();
    for forbidden in [
        "frame", "landmark", "face", "image", "pixel", "yaw", "roll", "angle",
    ] {
        assert!(
            !lower.contains(forbidden),
            "biometric field {forbidden} should not be logged"
        );
    }
}

#[test]
fn reads_back_valid_events_and_skips_malformed_lines() {
    let temp = tempfile::tempdir().unwrap();
    let events_file = temp.path().join("events.jsonl");
    std::fs::write(
        &events_file,
        "{\"ts\":\"2025-01-01T00:00:00+00:00\",\"kind\":\"CAMERA_UNAVAILABLE\"}\nnot json\n{\"ts\":\"2025-01-02T00:00:00+00:00\",\"kind\":\"STATE_CHANGE\",\"state\":\"FACING\"}\n{\"ts\":\"2025-01-03T00:00:00+00:00\"}\n",
    ).unwrap();

    let log = EventLog::new(temp.path());
    let events = log.events().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, AppEventKind::CameraUnavailable);
    assert_eq!(events[1].kind, AppEventKind::StateChange);
}
