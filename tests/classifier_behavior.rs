use eyes_lib::domain::classifier::{
    classify, HeadPose, NeutralPose, PoseClassification, PoseState, Thresholds,
};

#[test]
fn classifies_missing_pose_as_no_face() {
    let result = classify(None, None, None, None);
    assert_eq!(result.yaw_state, PoseState::NoFace);
    assert_eq!(result.pitch_state, PoseState::NoFace);
}

#[test]
fn classifies_yaw_relative_to_neutral() {
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: 0.0,
            }),
            None,
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 1.0,
                pitch: 0.0,
            }),
            None,
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: -1.0,
                pitch: 0.0,
            }),
            None,
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 10.0,
                pitch: 0.0,
            }),
            None,
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::OffAxisRight,
            pitch_state: PoseState::FacingScreen,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: -10.0,
                pitch: 0.0,
            }),
            None,
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::OffAxisLeft,
            pitch_state: PoseState::FacingScreen,
        },
    );
}

#[test]
fn classifies_pitch_relative_to_neutral() {
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: 5.0,
            }),
            None,
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: 10.0,
            }),
            None,
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::HeadUp,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: -10.0,
            }),
            None,
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::HeadDown,
        },
    );
}

#[test]
fn yaw_and_pitch_can_deviate_simultaneously() {
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: -5.0,
                pitch: 10.0,
            }),
            None,
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::OffAxisLeft,
            pitch_state: PoseState::HeadUp,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 5.0,
                pitch: -10.0,
            }),
            None,
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::OffAxisRight,
            pitch_state: PoseState::HeadDown,
        },
    );
}

#[test]
fn classifies_relative_to_non_zero_neutral() {
    let neutral = NeutralPose {
        yaw: 10.0,
        pitch: 5.0,
    };
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 10.0,
                pitch: 5.0,
            }),
            Some(neutral),
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 15.0,
                pitch: 5.0,
            }),
            Some(neutral),
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::OffAxisRight,
            pitch_state: PoseState::FacingScreen,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 10.0,
                pitch: 12.0,
            }),
            Some(neutral),
            None,
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::HeadUp,
        },
    );
}

#[test]
fn custom_thresholds_change_boundaries() {
    let strict = Thresholds {
        yaw_deg: 0.5,
        yaw_hysteresis_deg: 0.5,
        pitch_deg: 5.0,
        pitch_hysteresis_deg: 2.5,
    };
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 1.0,
                pitch: 0.0,
            }),
            None,
            Some(strict),
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::OffAxisRight,
            pitch_state: PoseState::FacingScreen,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.3,
                pitch: 0.0,
            }),
            None,
            Some(strict),
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );

    let lenient = Thresholds {
        yaw_deg: 30.0,
        yaw_hysteresis_deg: 0.5,
        pitch_deg: 5.0,
        pitch_hysteresis_deg: 2.5,
    };
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 25.0,
                pitch: 0.0,
            }),
            None,
            Some(lenient),
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );
}

#[test]
fn custom_pitch_thresholds_change_boundaries() {
    let strict_pitch = Thresholds {
        yaw_deg: 1.0,
        yaw_hysteresis_deg: 0.5,
        pitch_deg: 3.0,
        pitch_hysteresis_deg: 2.5,
    };
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: 4.0,
            }),
            None,
            Some(strict_pitch),
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::HeadUp,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: 2.0,
            }),
            None,
            Some(strict_pitch),
            None,
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );
}

#[test]
fn keeps_yaw_off_axis_inside_hysteresis_zone() {
    let thresholds = Thresholds {
        yaw_deg: 1.0,
        yaw_hysteresis_deg: 0.5,
        pitch_deg: 5.0,
        pitch_hysteresis_deg: 2.5,
    };
    let prev = PoseClassification {
        yaw_state: PoseState::OffAxisRight,
        pitch_state: PoseState::FacingScreen,
    };

    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.7,
                pitch: 0.0,
            }),
            None,
            Some(thresholds),
            Some(prev),
        ),
        PoseClassification {
            yaw_state: PoseState::OffAxisRight,
            pitch_state: PoseState::FacingScreen,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.5,
                pitch: 0.0,
            }),
            None,
            Some(thresholds),
            Some(prev),
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );

    let prev_left = PoseClassification {
        yaw_state: PoseState::OffAxisLeft,
        pitch_state: PoseState::FacingScreen,
    };
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: -0.7,
                pitch: 0.0,
            }),
            None,
            Some(thresholds),
            Some(prev_left),
        ),
        PoseClassification {
            yaw_state: PoseState::OffAxisLeft,
            pitch_state: PoseState::FacingScreen,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: -0.5,
                pitch: 0.0,
            }),
            None,
            Some(thresholds),
            Some(prev_left),
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );
}

#[test]
fn keeps_pitch_off_axis_inside_hysteresis_zone() {
    let thresholds = Thresholds {
        yaw_deg: 1.0,
        yaw_hysteresis_deg: 0.5,
        pitch_deg: 5.0,
        pitch_hysteresis_deg: 2.5,
    };
    let prev_up = PoseClassification {
        yaw_state: PoseState::FacingScreen,
        pitch_state: PoseState::HeadUp,
    };

    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: 3.0,
            }),
            None,
            Some(thresholds),
            Some(prev_up),
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::HeadUp,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: 2.5,
            }),
            None,
            Some(thresholds),
            Some(prev_up),
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );

    let prev_down = PoseClassification {
        yaw_state: PoseState::FacingScreen,
        pitch_state: PoseState::HeadDown,
    };
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: -3.0,
            }),
            None,
            Some(thresholds),
            Some(prev_down),
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::HeadDown,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: -2.5,
            }),
            None,
            Some(thresholds),
            Some(prev_down),
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );
}

#[test]
fn no_face_previous_state_has_no_hysteresis() {
    let prev = PoseClassification {
        yaw_state: PoseState::NoFace,
        pitch_state: PoseState::NoFace,
    };
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.7,
                pitch: 3.0,
            }),
            None,
            None,
            Some(prev),
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );
}

#[test]
fn custom_pitch_hysteresis_changes_return_boundary() {
    let thresholds = Thresholds {
        yaw_deg: 1.0,
        yaw_hysteresis_deg: 0.5,
        pitch_deg: 5.0,
        pitch_hysteresis_deg: 1.0,
    };
    let prev_up = PoseClassification {
        yaw_state: PoseState::FacingScreen,
        pitch_state: PoseState::HeadUp,
    };

    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: 3.0,
            }),
            None,
            Some(thresholds),
            Some(prev_up),
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::HeadUp,
        },
    );
    assert_eq!(
        classify(
            Some(HeadPose {
                yaw: 0.0,
                pitch: 1.0,
            }),
            None,
            Some(thresholds),
            Some(prev_up),
        ),
        PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
        },
    );
}
