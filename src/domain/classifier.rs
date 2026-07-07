#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadPose {
    pub yaw: f64,
    pub pitch: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeutralPose {
    pub yaw: f64,
    pub pitch: f64,
}

impl Default for NeutralPose {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub yaw_deg: f64,
    pub yaw_hysteresis_deg: f64,
    pub pitch_deg: f64,
    pub pitch_hysteresis_deg: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        use super::defaults;
        Self {
            yaw_deg: defaults::YAW_DEG,
            yaw_hysteresis_deg: defaults::YAW_HYSTERESIS_DEG,
            pitch_deg: defaults::PITCH_DEG,
            pitch_hysteresis_deg: defaults::PITCH_HYSTERESIS_DEG,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoseState {
    FacingScreen,
    OffAxisLeft,
    OffAxisRight,
    HeadUp,
    HeadDown,
    NoFace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoseClassification {
    pub yaw_state: PoseState,
    pub pitch_state: PoseState,
}

impl Default for PoseClassification {
    fn default() -> Self {
        Self {
            yaw_state: PoseState::NoFace,
            pitch_state: PoseState::NoFace,
        }
    }
}

fn classify_axis(
    dev: f64,
    threshold: f64,
    hysteresis: f64,
    prev_state: PoseState,
    negative_state: PoseState,
    positive_state: PoseState,
) -> PoseState {
    let abs_dev = dev.abs();
    // 轴专属过滤：was_off_axis 只应判断本轴的 off-axis 状态。
    // 否则 pitch 的 HeadUp/HeadDown 会误让 yaw 使用 hysteresis，反之亦然。
    let was_off_axis = prev_state == negative_state || prev_state == positive_state;

    let outside = if was_off_axis {
        abs_dev > hysteresis
    } else {
        abs_dev > threshold
    };

    if !outside {
        return PoseState::FacingScreen;
    }

    if dev < 0.0 {
        negative_state
    } else {
        positive_state
    }
}

pub fn classify(
    pose: Option<HeadPose>,
    neutral: Option<NeutralPose>,
    thresholds: Option<Thresholds>,
    prev_classification: Option<PoseClassification>,
) -> PoseClassification {
    let Some(pose) = pose else {
        return PoseClassification {
            yaw_state: PoseState::NoFace,
            pitch_state: PoseState::NoFace,
        };
    };

    let neutral = neutral.unwrap_or_default();
    let thresholds = thresholds.unwrap_or_default();
    let prev = prev_classification.unwrap_or_default();

    let yaw_dev = pose.yaw - neutral.yaw;
    let pitch_dev = pose.pitch - neutral.pitch;

    let yaw_state = classify_axis(
        yaw_dev,
        thresholds.yaw_deg,
        thresholds.yaw_hysteresis_deg,
        prev.yaw_state,
        PoseState::OffAxisLeft,
        PoseState::OffAxisRight,
    );

    let pitch_state = classify_axis(
        pitch_dev,
        thresholds.pitch_deg,
        thresholds.pitch_hysteresis_deg,
        prev.pitch_state,
        PoseState::HeadDown,
        PoseState::HeadUp,
    );

    PoseClassification {
        yaw_state,
        pitch_state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaw_classify_ignores_pitch_prev_state_for_hysteresis() {
        // yaw 偏差落在 threshold 与 hysteresis 之间时，
        // 若 prev_state 是 pitch 轴的 HeadUp，本不应使用 hysteresis。
        let pose = HeadPose {
            yaw: 2.0,
            pitch: 0.0,
        };
        let thresholds = Thresholds {
            yaw_deg: 3.0,
            yaw_hysteresis_deg: 1.0,
            pitch_deg: 3.0,
            pitch_hysteresis_deg: 1.0,
        };
        let prev = PoseClassification {
            yaw_state: PoseState::FacingScreen,
            pitch_state: PoseState::HeadUp,
        };

        let result = classify(Some(pose), None, Some(thresholds), Some(prev));

        // 2.0 > threshold 3.0 不成立，应判定为 FacingScreen
        assert_eq!(result.yaw_state, PoseState::FacingScreen);
    }

    #[test]
    fn pitch_classify_ignores_yaw_prev_state_for_hysteresis() {
        // pitch 偏差落在 threshold 与 hysteresis 之间时，
        // 若 prev_state 是 yaw 轴的 OffAxisRight，本不应使用 hysteresis。
        let pose = HeadPose {
            yaw: 0.0,
            pitch: 2.0,
        };
        let thresholds = Thresholds {
            yaw_deg: 3.0,
            yaw_hysteresis_deg: 1.0,
            pitch_deg: 3.0,
            pitch_hysteresis_deg: 1.0,
        };
        let prev = PoseClassification {
            yaw_state: PoseState::OffAxisRight,
            pitch_state: PoseState::FacingScreen,
        };

        let result = classify(Some(pose), None, Some(thresholds), Some(prev));

        // 2.0 > threshold 3.0 不成立，应判定为 FacingScreen
        assert_eq!(result.pitch_state, PoseState::FacingScreen);
    }
}
