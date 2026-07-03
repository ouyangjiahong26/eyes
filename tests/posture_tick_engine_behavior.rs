use eyes_lib::domain::{
    classifier::PoseState,
    posture_tick_engine::{PostureTickEngine, SenseEvent, WarningLevel},
};

fn has_correction(events: &[SenseEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, SenseEvent::Correction { .. }))
}

fn has_correction_for(events: &[SenseEvent], state: PoseState) -> bool {
    events
        .iter()
        .any(|event| matches!(event, SenseEvent::Correction { direction } if *direction == state))
}

fn has_good_posture(events: &[SenseEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, SenseEvent::GoodPosture))
}

fn has_eye_rest(events: &[SenseEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, SenseEvent::EyeRest))
}

fn has_warning(events: &[SenseEvent], level: WarningLevel) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            SenseEvent::WarningLevelChanged {
                level: actual,
                ..
            } if *actual == level
        )
    })
}

/// 便捷：仅传 yaw，pitch 默认 FacingScreen。
fn tick_yaw(engine: &mut PostureTickEngine, state: PoseState, dt: f64) -> Vec<SenseEvent> {
    engine.tick(state, PoseState::FacingScreen, dt)
}

#[test]
fn off_axis_streak_fires_correction_at_threshold_and_repeats() {
    let mut engine = PostureTickEngine::default();

    assert!(!has_correction(&tick_yaw(&mut engine, PoseState::OffAxisLeft, 0.2)));
    assert!(has_correction(&tick_yaw(&mut engine, PoseState::OffAxisLeft, 0.1)));

    for _ in 0..9 {
        assert!(!has_correction(&tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0)));
    }
    assert!(has_correction(&tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0)));
}

#[test]
fn zero_streak_threshold_fires_immediately() {
    let mut engine = PostureTickEngine::new(Some(0.0), None, None, None);
    assert!(has_correction(&tick_yaw(&mut engine, PoseState::OffAxisLeft, 0.1)));
}

#[test]
fn non_off_axis_left_right_states_reset_or_skip_correction_streak() {
    let mut engine = PostureTickEngine::new(Some(5.0), None, None, None);
    for _ in 0..3 {
        tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    }
    tick_yaw(&mut engine, PoseState::NoFace, 1.0);
    for _ in 0..4 {
        assert!(!has_correction(&tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0)));
    }
    assert!(has_correction(&tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0)));
}

#[test]
fn facing_and_presence_accumulators_fire_and_reset_at_thresholds() {
    let mut engine = PostureTickEngine::new(Some(5.0), None, Some(2.0), Some(3.0));

    assert!(!has_good_posture(
        &tick_yaw(&mut engine, PoseState::FacingScreen, 1.0)
    ));
    let events = tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    assert!(has_good_posture(&events));
    assert!(!has_eye_rest(&events));

    let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    assert!(has_eye_rest(&events));
}

#[test]
fn non_facing_and_no_face_pause_accumulators_without_resetting() {
    let mut engine = PostureTickEngine::new(Some(5.0), None, Some(10.0), Some(10.0));

    for _ in 0..5 {
        tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    }
    tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    tick_yaw(&mut engine, PoseState::NoFace, 1.0);
    assert!(!has_good_posture(
        &tick_yaw(&mut engine, PoseState::FacingScreen, 1.0)
    ));

    for _ in 0..3 {
        tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    }
    let events = tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    assert!(has_good_posture(&events));
}

#[test]
fn warning_level_lifecycle_matches_python_oracle() {
    let mut engine = PostureTickEngine::new(None, Some(10.0), None, None);

    let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    assert!(has_warning(&events, WarningLevel::Warning));

    for _ in 0..8 {
        assert!(!has_warning(
            &tick_yaw(&mut engine, PoseState::OffAxisRight, 1.0),
            WarningLevel::Severe
        ));
    }
    let events = tick_yaw(&mut engine, PoseState::OffAxisRight, 1.0);
    assert!(events.iter().any(|event| matches!(
        event,
        SenseEvent::WarningLevelChanged {
            level: WarningLevel::Severe,
            direction: Some(direction)
        } if direction == "right"
    )));

    assert!(has_warning(
        &tick_yaw(&mut engine, PoseState::FacingScreen, 1.0),
        WarningLevel::Corrected
    ));
    assert!(!has_warning(
        &tick_yaw(&mut engine, PoseState::FacingScreen, 1.0),
        WarningLevel::Normal
    ));
    assert!(has_warning(
        &tick_yaw(&mut engine, PoseState::FacingScreen, 1.0),
        WarningLevel::Normal
    ));
}

#[test]
fn warning_does_not_escalate_below_threshold_and_no_face_starts_fresh_episode() {
    let mut engine = PostureTickEngine::new(None, Some(10.0), None, None);

    tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    for _ in 0..8 {
        assert!(!has_warning(
            &tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0),
            WarningLevel::Severe
        ));
    }

    let events = tick_yaw(&mut engine, PoseState::NoFace, 1.0);
    assert!(has_warning(&events, WarningLevel::Normal));
    let events = tick_yaw(&mut engine, PoseState::OffAxisRight, 1.0);
    assert!(has_warning(&events, WarningLevel::Warning));
}

#[test]
fn head_up_does_not_advance_yaw_warning_escalation() {
    // HeadUp 属于 pitch 轴，不应影响 yaw 轴的警告升级。
    let mut engine = PostureTickEngine::new(None, Some(10.0), None, None);

    // 先在 yaw 轴触发 Warning
    tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);

    // pitch=HeadUp 不应影响 yaw 警告状态
    for _ in 0..20 {
        let events = engine.tick(PoseState::FacingScreen, PoseState::HeadUp, 1.0);
        for ev in &events {
            assert!(
                !matches!(ev, SenseEvent::WarningLevelChanged { direction: Some(d), .. } if d == "left" || d == "right"),
                "yaw warning should not change during pitch-only off-axis"
            );
        }
    }
    // yaw 轴继续 OffAxisLeft，警告应正常升级
    for _ in 0..9 {
        assert!(!has_warning(
            &tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0),
            WarningLevel::Severe
        ));
    }
    assert!(has_warning(
        &tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0),
        WarningLevel::Severe
    ));
}

#[test]
fn snooze_freezes_all_accumulators_until_resume() {
    let mut engine = PostureTickEngine::new(Some(5.0), Some(10.0), Some(10.0), Some(10.0));
    for _ in 0..5 {
        tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    }
    for _ in 0..3 {
        tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    }

    engine.snooze();
    assert!(engine.is_snoozed());
    for _ in 0..20 {
        assert!(tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0).is_empty());
    }

    engine.resume();
    assert!(!engine.is_snoozed());
    assert!(!has_correction(&tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0)));
    let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    assert!(has_correction(&events));
    assert!(has_eye_rest(&events));
}

#[test]
fn snooze_freezes_facing_presence_and_warning_escalation_independently() {
    let mut engine = PostureTickEngine::new(Some(5.0), Some(10.0), Some(10.0), Some(10.0));
    for _ in 0..5 {
        tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    }
    tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);

    engine.snooze();
    for _ in 0..20 {
        assert!(tick_yaw(&mut engine, PoseState::FacingScreen, 1.0).is_empty());
    }
    engine.resume();

    for _ in 0..3 {
        let events = tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
        assert!(!has_good_posture(&events));
        assert!(!has_eye_rest(&events));
    }
    let events = tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    assert!(has_eye_rest(&events));
    assert!(!has_good_posture(&events));
    let events = tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    assert!(has_good_posture(&events));

    let mut engine = PostureTickEngine::new(None, Some(10.0), None, None);
    tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    engine.snooze();
    for _ in 0..20 {
        assert!(tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0).is_empty());
    }
    engine.resume();
    for _ in 0..8 {
        assert!(!has_warning(
            &tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0),
            WarningLevel::Severe
        ));
    }
    assert!(has_warning(
        &tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0),
        WarningLevel::Severe
    ));
}

// ── Pitch 轴测试 ──────────────────────────────────────────────

#[test]
fn pitch_off_axis_triggers_correction() {
    let mut engine = PostureTickEngine::default();

    assert!(!has_correction(
        &engine.tick(PoseState::FacingScreen, PoseState::HeadUp, 0.2)
    ));
    let events = engine.tick(PoseState::FacingScreen, PoseState::HeadUp, 0.1);
    assert!(has_correction_for(&events, PoseState::HeadUp));
}

#[test]
fn pitch_correction_repeats_at_interval() {
    let mut engine = PostureTickEngine::default();

    engine.tick(PoseState::FacingScreen, PoseState::HeadDown, 0.2);
    let events = engine.tick(PoseState::FacingScreen, PoseState::HeadDown, 0.1);
    assert!(has_correction_for(&events, PoseState::HeadDown));

    for _ in 0..9 {
        assert!(!has_correction(
            &engine.tick(PoseState::FacingScreen, PoseState::HeadDown, 1.0)
        ));
    }
    let events = engine.tick(PoseState::FacingScreen, PoseState::HeadDown, 1.0);
    assert!(has_correction_for(&events, PoseState::HeadDown));
}

#[test]
fn pitch_warning_level_escalates_independently() {
    let mut engine = PostureTickEngine::new(None, Some(10.0), None, None);

    let events = engine.tick(PoseState::FacingScreen, PoseState::HeadUp, 1.0);
    assert!(has_warning(&events, WarningLevel::Warning));

    for _ in 0..8 {
        assert!(!has_warning(
            &engine.tick(PoseState::FacingScreen, PoseState::HeadUp, 1.0),
            WarningLevel::Severe
        ));
    }
    let events = engine.tick(PoseState::FacingScreen, PoseState::HeadUp, 1.0);
    assert!(has_warning(&events, WarningLevel::Severe));

    let events = engine.tick(PoseState::FacingScreen, PoseState::FacingScreen, 1.0);
    assert!(has_warning(&events, WarningLevel::Corrected));
}

#[test]
fn both_axes_off_generate_independent_corrections() {
    let mut engine = PostureTickEngine::default();

    engine.tick(PoseState::OffAxisLeft, PoseState::HeadUp, 0.2);
    let events = engine.tick(PoseState::OffAxisLeft, PoseState::HeadUp, 0.1);

    assert!(has_correction_for(&events, PoseState::OffAxisLeft));
    assert!(has_correction_for(&events, PoseState::HeadUp));
}

#[test]
fn good_posture_requires_both_axes_facing() {
    let mut engine = PostureTickEngine::new(None, None, Some(2.0), None);

    for _ in 0..5 {
        let events = engine.tick(PoseState::FacingScreen, PoseState::HeadUp, 1.0);
        assert!(!has_good_posture(&events));
    }

    let events = engine.tick(PoseState::FacingScreen, PoseState::FacingScreen, 1.0);
    assert!(!has_good_posture(&events));
    let events = engine.tick(PoseState::FacingScreen, PoseState::FacingScreen, 1.0);
    assert!(has_good_posture(&events));
}

#[test]
fn good_posture_pauses_during_pitch_off_axis() {
    let mut engine = PostureTickEngine::new(None, None, Some(5.0), None);

    for _ in 0..3 {
        engine.tick(PoseState::FacingScreen, PoseState::FacingScreen, 1.0);
    }
    engine.tick(PoseState::FacingScreen, PoseState::HeadDown, 1.0);
    let events = engine.tick(PoseState::FacingScreen, PoseState::FacingScreen, 1.0);
    assert!(!has_good_posture(&events));
    let events = engine.tick(PoseState::FacingScreen, PoseState::FacingScreen, 1.0);
    assert!(has_good_posture(&events));
}

#[test]
fn eye_rest_requires_both_axes_have_face() {
    let mut engine = PostureTickEngine::new(None, None, None, Some(2.0));

    engine.tick(PoseState::FacingScreen, PoseState::FacingScreen, 1.0);
    engine.tick(PoseState::NoFace, PoseState::FacingScreen, 1.0);
    let events = engine.tick(PoseState::FacingScreen, PoseState::FacingScreen, 1.0);
    assert!(has_eye_rest(&events));
}
