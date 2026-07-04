use eyes_lib::domain::calibration::{compute_median_pose, CalibrationSession, PoseSample};

#[test]
fn computes_median_pose_by_sorting_samples_by_yaw() {
    assert_eq!(
        compute_median_pose(&[PoseSample {
            yaw: 5.0,
            pitch: 3.0
        }])
        .unwrap(),
        PoseSample {
            yaw: 5.0,
            pitch: 3.0
        },
    );
    assert_eq!(
        compute_median_pose(&[
            PoseSample {
                yaw: 2.0,
                pitch: 1.0
            },
            PoseSample {
                yaw: 4.0,
                pitch: 3.0
            },
        ])
        .unwrap(),
        PoseSample {
            yaw: 3.0,
            pitch: 2.0
        },
    );
    assert_eq!(
        compute_median_pose(&[
            PoseSample {
                yaw: 1.0,
                pitch: 0.0
            },
            PoseSample {
                yaw: 5.0,
                pitch: 10.0
            },
            PoseSample {
                yaw: 3.0,
                pitch: 5.0
            },
        ])
        .unwrap(),
        PoseSample {
            yaw: 3.0,
            pitch: 5.0
        },
    );
}

#[test]
fn empty_median_input_returns_error() {
    assert!(compute_median_pose(&[]).is_err());
}

#[test]
fn calibration_session_collects_only_while_active_and_returns_finished_median() {
    let mut session = CalibrationSession::new(1.0);
    assert!(!session.is_active());
    assert_eq!(session.sample_count(), 0);
    assert!(session.result().is_none());

    session.feed(99.0, 99.0);
    assert_eq!(session.sample_count(), 0);

    session.start();
    assert!(session.is_active());
    assert_eq!(session.countdown_seconds(), 1.0);
    session.feed(1.0, 0.0);
    session.feed(5.0, 10.0);
    session.feed(3.0, 5.0);
    assert_eq!(session.sample_count(), 3);
    assert!(session.result().is_none());

    for _ in 0..10 {
        session.tick(0.1);
    }

    assert!(!session.is_active());
    let result = session.result().unwrap();
    assert_eq!(result.yaw, 3.0);
    assert_eq!(result.pitch, 5.0);
    assert_eq!(result.sample_count, 3);
}

#[test]
fn restart_clears_previous_samples_and_no_sample_session_has_no_result() {
    let mut session = CalibrationSession::new(1.0);
    session.start();
    session.feed(42.0, 42.0);
    session.tick(1.0);
    assert!(session.result().is_some());

    session.start();
    assert!(session.is_active());
    assert_eq!(session.sample_count(), 0);
    assert_eq!(session.countdown_seconds(), 1.0);
    assert!(session.result().is_none());

    session.tick(1.0);
    assert!(!session.is_active());
    assert!(session.result().is_none());
}
