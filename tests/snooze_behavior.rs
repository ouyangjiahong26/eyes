use eyes_lib::domain::snooze::{evaluate_snooze, SnoozeState};

const NOW: &str = "2026-01-01T12:00:00+00:00";

#[test]
fn evaluates_absent_indefinite_and_malformed_persisted_snooze_values() {
    assert_eq!(evaluate_snooze(None, NOW), SnoozeState::Inactive);
    assert_eq!(
        evaluate_snooze(Some("indefinite"), NOW),
        SnoozeState::Indefinite
    );
    assert_eq!(
        evaluate_snooze(Some("not-a-valid-timestamp"), NOW),
        SnoozeState::Malformed
    );
    assert_eq!(evaluate_snooze(Some(""), NOW), SnoozeState::Malformed);
}

#[test]
fn evaluates_future_past_and_boundary_timed_snoozes() {
    assert_eq!(
        evaluate_snooze(Some("2026-01-01T13:00:00+00:00"), NOW),
        SnoozeState::Active {
            until_iso: "2026-01-01T13:00:00+00:00".to_string()
        },
    );
    assert_eq!(
        evaluate_snooze(Some("2026-01-01T11:59:50+00:00"), NOW),
        SnoozeState::Expired
    );
    assert_eq!(evaluate_snooze(Some(NOW), NOW), SnoozeState::Expired);
}

#[test]
fn treats_naive_timestamp_as_utc() {
    assert_eq!(
        evaluate_snooze(Some("2026-01-01T13:00:00"), NOW),
        SnoozeState::Active {
            until_iso: "2026-01-01T13:00:00+00:00".to_string()
        },
    );
}
