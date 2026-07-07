use eyes_lib::domain::{
    classifier::PoseState,
    posture_tick_engine::{PostureTickEngine, SenseEvent, WarningLevel},
    thresholds::TimingThresholds,
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
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_streak_threshold_seconds: 0.0,
        ..Default::default()
    });
    assert!(has_correction(&tick_yaw(&mut engine, PoseState::OffAxisLeft, 0.1)));
}

#[test]
fn non_off_axis_left_right_states_reset_or_skip_correction_streak() {
    // 新语义（ADR 0009 决策 7）：NoFace 在 warning_level == Normal 时**不重置**
    // `continuous_seconds`（off-axis 累积已在 NoFace 期间停止）。
    // 因此累积值跨 NoFace 保留，回到 OffAxisLeft 继续累积，
    // 累计到达 streak_threshold 时仍能触发 Correction。
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_streak_threshold_seconds: 5.0,
        ..Default::default()
    });

    // 累积 cont=1..3 (< streak_threshold=5)，无 Correction
    for _ in 0..3 {
        tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    }
    // NoFace 时 warning_level 仍 Normal，按新语义不重置 cont，cont 保留 = 3
    tick_yaw(&mut engine, PoseState::NoFace, 1.0);
    // 再 1.0s：cont=4 < 5，仍无 Correction
    assert!(!has_correction(&tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0)));
    // 再 1.0s：cont=5 ≥ streak_threshold，首次 Correction
    let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    assert!(has_correction(&events));
}

#[test]
fn facing_and_presence_accumulators_fire_and_reset_at_thresholds() {
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_streak_threshold_seconds: 5.0,
        facing_threshold_seconds: 2.0,
        eyerest_threshold_seconds: 3.0,
        ..Default::default()
    });

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
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_streak_threshold_seconds: 5.0,
        facing_threshold_seconds: 10.0,
        eyerest_threshold_seconds: 10.0,
        ..Default::default()
    });

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

// 注意：旧设计下 Normal→Warning 立即触发；新设计（ADR 0009）等
// streak_threshold（默认 0.3s）。本测试首 tick 1.0s > 0.3s，行为恰好仍命中，
// 仅语义已变化——首 Correction 现与 Warning 升级同帧。
#[test]
fn warning_level_lifecycle_matches_python_oracle() {
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_repeat_interval_seconds: 10.0,
        ..Default::default()
    });

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
        } if *direction == PoseState::OffAxisRight
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
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_repeat_interval_seconds: 10.0,
        ..Default::default()
    });

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
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_repeat_interval_seconds: 10.0,
        ..Default::default()
    });

    // 先在 yaw 轴触发 Warning
    tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);

    // pitch=HeadUp 不应影响 yaw 警告状态
    for _ in 0..20 {
        let events = engine.tick(PoseState::FacingScreen, PoseState::HeadUp, 1.0);
        for ev in &events {
            assert!(
                !matches!(ev, SenseEvent::WarningLevelChanged { direction: Some(d), .. } if *d == PoseState::OffAxisLeft || *d == PoseState::OffAxisRight),
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
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_repeat_interval_seconds: 10.0,
        ..Default::default()
    });

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
    let mut engine = PostureTickEngine::new(TimingThresholds {
        facing_threshold_seconds: 2.0,
        ..Default::default()
    });

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
    let mut engine = PostureTickEngine::new(TimingThresholds {
        facing_threshold_seconds: 5.0,
        ..Default::default()
    });

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
    let mut engine = PostureTickEngine::new(TimingThresholds {
        eyerest_threshold_seconds: 2.0,
        ..Default::default()
    });

    engine.tick(PoseState::FacingScreen, PoseState::FacingScreen, 1.0);
    engine.tick(PoseState::NoFace, PoseState::FacingScreen, 1.0);
    let events = engine.tick(PoseState::FacingScreen, PoseState::FacingScreen, 1.0);
    assert!(has_eye_rest(&events));
}

// ── 新设计回归测试（ADR 0009） ──────────────────────────────
// 以下 6 个测试针对合并 `streak` + `continuous_seconds` 后的状态机：
// 不变量是 Normal/Corrected → Warning 必须等 streak_threshold，
// Warning → Severe 由独立的 severe_threshold 控制，
// Correction 在同一时间轴上与 warning 升级（首次）/ severe 升级（第二次）同帧。

/// 旧设计 Normal→Warning 立即触发；新设计等 streak_threshold。
/// 设 streak=1.0s、dt=0.5s：第一帧 0.5s 不应触发 Warning；
/// 第二帧累积到 1.0s 才升级 Warning 与首次 Correction。
#[test]
fn warning_escalates_after_streak_threshold_not_immediately() {
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_streak_threshold_seconds: 1.0,
        ..Default::default()
    });

    // 第一帧：0.5s < streak_threshold (1.0s)，应没有 WarningLevelChanged 也不应有 Correction
    let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 0.5);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, SenseEvent::WarningLevelChanged { .. })),
        "在 streak_threshold (1.0s) 之前不应有 WarningLevelChanged"
    );
    assert!(!has_correction(&events));

    // 第二帧：累积到 1.0s 才升级 Warning 并发出首次 Correction
    let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 0.5);
    assert!(has_warning(&events, WarningLevel::Warning));
    assert!(has_correction(&events));
}

/// 首次 Correction 与首次 Warning 升级同帧触发——
/// 这是 ADR 0009 决策 5 的关键不变量。
#[test]
fn first_correction_and_warning_fire_on_same_tick() {
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_streak_threshold_seconds: 0.5,
        off_axis_repeat_interval_seconds: 10.0,
        ..Default::default()
    });

    // 累积到 streak_threshold (0.5s) 这一帧
    let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 0.5);

    assert!(
        has_correction_for(&events, PoseState::OffAxisLeft),
        "首次 Correction 应在 streak_threshold 那一帧发出"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SenseEvent::WarningLevelChanged {
                level: WarningLevel::Warning,
                ..
            })),
        "WarningLevelChanged {{ Warning }} 应与首次 Correction 同帧触发"
    );
}

/// NoFace 不走 Corrected 缓冲，直接归 Normal（ADR 0009 决策 7、9）——
/// 故意与 FacingScreen 的重置语义不对称：人离开屏幕再发"已纠正"无对象。
#[test]
fn no_face_resets_to_normal_directly_without_corrected() {
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_streak_threshold_seconds: 1.0,
        off_axis_repeat_interval_seconds: 10.0,
        ..Default::default()
    });

    // 进入 Warning：累积 1.0s 触发 Warning + 首次 Correction
    let _ = tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);

    // NoFace 重置
    let events = tick_yaw(&mut engine, PoseState::NoFace, 1.0);
    assert!(
        events.iter().any(|event| matches!(event, SenseEvent::WarningLevelChanged {
            level: WarningLevel::Normal,
            direction: None,
        })),
        "NoFace 应直接发出 WarningLevelChanged(Normal)"
    );
    assert!(
        !events.iter().any(|event| matches!(event, SenseEvent::WarningLevelChanged {
            level: WarningLevel::Corrected,
            ..
        })),
        "NoFace 重置不走 Corrected 流程"
    );
}

/// FacingScreen 之后走 Corrected 缓冲 2.0s（硬编码，非参数化，
/// 见 ADR 0009 决策 6）再到 Normal。本测试用 dt=1.0s：
/// 第 1 个 FacingScreen tick → Corrected；第 2 个仍 Corrected；
/// 第 3 个才 Normal。
#[test]
fn facing_screen_after_warning_routes_through_corrected_for_2s() {
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_streak_threshold_seconds: 1.0,
        ..Default::default()
    });

    // 进入 Warning
    let _ = tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);

    // 第一个 FacingScreen：Warning → Corrected，缓冲 2.0s
    let events = tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    assert!(has_warning(&events, WarningLevel::Corrected));
    assert!(!events.iter().any(|event| matches!(event, SenseEvent::WarningLevelChanged {
        level: WarningLevel::Normal,
        ..
    })));

    // 第二个 FacingScreen：缓冲 2.0 → 1.0，仍 Corrected，不应到 Normal
    let events = tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    assert!(
        !events.iter().any(|event| matches!(event, SenseEvent::WarningLevelChanged { .. })),
        "1s 后应仍在 Corrected 缓冲期，不应到 Normal"
    );

    // 第三个 FacingScreen：缓冲 0.0，到 Normal
    let events = tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    assert!(has_warning(&events, WarningLevel::Normal));
}

/// Corrected → Warning 也必须等 streak_threshold，不立即重新升 Warning。
/// 状态机对称原则（ADR 0009 决策 16）：
/// Corrected 与 Normal 升级到 Warning 的条件一致。
#[test]
fn corrected_to_warning_also_waits_for_streak_threshold() {
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_streak_threshold_seconds: 1.0,
        ..Default::default()
    });

    // Warning → Corrected 路径
    let _ = tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    let _ = tick_yaw(&mut engine, PoseState::FacingScreen, 1.0);
    assert_eq!(engine.warning_level(), WarningLevel::Corrected);

    // 从 Corrected 再 OffAxisLeft 0.5s：< streak_threshold，不应立即重新升 Warning
    let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 0.5);
    assert!(
        !events.iter().any(|event| matches!(event, SenseEvent::WarningLevelChanged { .. })),
        "Corrected → Warning 也应等 streak_threshold，不能立即重新升 Warning"
    );

    // 再累积 0.5s 到达 1.0s 才升 Warning 与 Correction
    let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 0.5);
    assert!(has_warning(&events, WarningLevel::Warning));
    assert!(has_correction(&events));
}

/// streak=0.5、repeat=10、severe=10 的组合：
/// 累积到 0.5s 首次 Correction；累积到 10.0s 时
/// 第二次 Correction + Severe 升级同帧触发（同为 10.5 这一刻同时满足
/// `next_correction_threshold=10.5` 与 `severe_threshold=10.0`）。
#[test]
fn second_correction_fires_at_severe_threshold_when_repeat_equals_severe() {
    let mut engine = PostureTickEngine::new(TimingThresholds {
        off_axis_streak_threshold_seconds: 0.5,
        off_axis_repeat_interval_seconds: 10.0,
        off_axis_severe_threshold_seconds: 10.0,
        ..Default::default()
    });

    // 首次 Correction 在 cont=0.5
    let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 0.5);
    assert!(has_correction(&events));
    assert!(has_warning(&events, WarningLevel::Warning));

    // 中间 9 个 1.0s tick（cont=1.5..9.5），都不应触发第二次 Correction 也不应升 Severe
    for _ in 0..9 {
        let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
        assert!(
            !has_correction(&events),
            "在第二次 Correction 阈值之前不应当有 Correction"
        );
        assert!(
            !has_warning(&events, WarningLevel::Severe),
            "在 severe_threshold 之前不应当升 Severe"
        );
    }

    // 累积到 10.5：10.5 >= 10.0 (severe) -> Severe；10.5 >= 10.5 (next_correction) -> 第二次 Correction
    let events = tick_yaw(&mut engine, PoseState::OffAxisLeft, 1.0);
    assert!(
        has_correction(&events),
        "第二次 Correction 应在 severe_threshold 同帧发出"
    );
    assert!(
        has_warning(&events, WarningLevel::Severe),
        "Severe 升级应与第二次 Correction 同帧发出"
    );
}
