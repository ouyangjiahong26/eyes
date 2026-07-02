use super::classifier::PoseState;
use super::defaults;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum WarningLevel {
    Normal,
    Warning,
    Severe,
    Corrected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SenseEvent {
    Correction {
        direction: PoseState,
    },
    GoodPosture,
    EyeRest,
    WarningLevelChanged {
        level: WarningLevel,
        direction: Option<String>,
    },
}

// 默认值已统一到 domain::defaults

fn direction_label(state: PoseState) -> &'static str {
    match state {
        PoseState::OffAxisLeft => "left",
        PoseState::OffAxisRight => "right",
        PoseState::HeadUp => "up",
        PoseState::HeadDown => "down",
        _ => "",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Yaw,
    Pitch,
}

/// 单轴 off-axis 跟踪状态：streak 计时、重复提醒、警告升级 FSM。
#[derive(Debug, Clone)]
struct OffAxisState {
    axis: Axis,
    streak: f64,
    repeat_due_at: Option<f64>,
    last_emit_at: Option<f64>,
    warning_level: WarningLevel,
    continuous_seconds: f64,
    corrected_remaining_seconds: f64,
}

impl OffAxisState {
    fn new(axis: Axis) -> Self {
        Self {
            axis,
            streak: 0.0,
            repeat_due_at: None,
            last_emit_at: None,
            warning_level: WarningLevel::Normal,
            continuous_seconds: 0.0,
            corrected_remaining_seconds: 0.0,
        }
    }

    fn new_yaw() -> Self {
        Self::new(Axis::Yaw)
    }

    fn new_pitch() -> Self {
        Self::new(Axis::Pitch)
    }

    fn reset_streak(&mut self) {
        self.streak = 0.0;
        self.repeat_due_at = None;
        self.last_emit_at = None;
    }

    fn reset_warning(&mut self) {
        self.warning_level = WarningLevel::Normal;
        self.continuous_seconds = 0.0;
        self.corrected_remaining_seconds = 0.0;
    }

    /// 偏离连续时长追踪。
    fn update_streak(
        &mut self,
        state: PoseState,
        dt: f64,
        streak_threshold: f64,
        repeat_interval: f64,
        events: &mut Vec<SenseEvent>,
    ) {
        if self.is_off_for_self(state) {
            self.streak += dt;
            if self.streak >= streak_threshold {
                if self.last_emit_at.is_none() {
                    self.last_emit_at = Some(self.streak);
                    self.repeat_due_at = Some(self.streak + repeat_interval);
                    events.push(SenseEvent::Correction { direction: state });
                } else if self
                    .repeat_due_at
                    .is_some_and(|due_at| self.streak >= due_at)
                {
                    self.repeat_due_at = Some(self.streak + repeat_interval);
                    events.push(SenseEvent::Correction { direction: state });
                }
            }
        } else {
            self.reset_streak();
        }
    }

    /// Warning 级别 FSM：Normal → Warning → Severe → Corrected → Normal。
    fn update_warning_level(
        &mut self,
        state: PoseState,
        dt: f64,
        repeat_interval: f64,
        events: &mut Vec<SenseEvent>,
    ) {
        let direction = direction_label(state);
        match state {
            _ if self.is_off_for_self(state) => {
                if matches!(
                    self.warning_level,
                    WarningLevel::Normal | WarningLevel::Corrected
                ) {
                    self.warning_level = WarningLevel::Warning;
                    self.continuous_seconds = dt;
                    events.push(SenseEvent::WarningLevelChanged {
                        level: WarningLevel::Warning,
                        direction: Some(direction.to_string()),
                    });
                } else {
                    self.continuous_seconds += dt;
                    if self.warning_level == WarningLevel::Warning
                        && self.continuous_seconds >= repeat_interval
                    {
                        self.warning_level = WarningLevel::Severe;
                        events.push(SenseEvent::WarningLevelChanged {
                            level: WarningLevel::Severe,
                            direction: Some(direction.to_string()),
                        });
                    }
                }
            }
            PoseState::FacingScreen => {
                if matches!(
                    self.warning_level,
                    WarningLevel::Warning | WarningLevel::Severe
                ) {
                    self.warning_level = WarningLevel::Corrected;
                    self.corrected_remaining_seconds = 2.0;
                    self.continuous_seconds = 0.0;
                    events.push(SenseEvent::WarningLevelChanged {
                        level: WarningLevel::Corrected,
                        direction: None,
                    });
                } else if self.warning_level == WarningLevel::Corrected {
                    self.corrected_remaining_seconds -= dt;
                    if self.corrected_remaining_seconds <= 0.0 {
                        self.warning_level = WarningLevel::Normal;
                        self.corrected_remaining_seconds = 0.0;
                        events.push(SenseEvent::WarningLevelChanged {
                            level: WarningLevel::Normal,
                            direction: None,
                        });
                    }
                }
            }
            PoseState::NoFace => {
                if matches!(
                    self.warning_level,
                    WarningLevel::Warning | WarningLevel::Severe | WarningLevel::Corrected
                ) {
                    self.reset_warning();
                    events.push(SenseEvent::WarningLevelChanged {
                        level: WarningLevel::Normal,
                        direction: None,
                    });
                }
            }
            _ => {}
        }
    }

    /// 判断状态是否属于本轴的 off-axis。
    /// yaw 只关心 OffAxisLeft/OffAxisRight，pitch 只关心 HeadUp/HeadDown，
    /// 避免把另一轴的偏离状态计入本轴的 streak 或警告升级。
    fn is_off_for_self(&self, state: PoseState) -> bool {
        match self.axis {
            Axis::Yaw => matches!(state, PoseState::OffAxisLeft | PoseState::OffAxisRight),
            Axis::Pitch => matches!(state, PoseState::HeadUp | PoseState::HeadDown),
        }
    }

    /// 通用状态（两轴共享）：FacingScreen、NoFace。
    fn is_applicable(&self, state: PoseState) -> bool {
        self.is_off_for_self(state) || matches!(state, PoseState::FacingScreen | PoseState::NoFace)
    }

    /// 处理单轴完整 tick：streak + warning。
    fn tick(
        &mut self,
        state: PoseState,
        dt: f64,
        streak_threshold: f64,
        repeat_interval: f64,
    ) -> Vec<SenseEvent> {
        let mut events = Vec::new();
        // 不属于本轴且非通用状态 → 静默忽略
        if !self.is_applicable(state) {
            return events;
        }
        self.update_streak(state, dt, streak_threshold, repeat_interval, &mut events);
        self.update_warning_level(state, dt, repeat_interval, &mut events);
        events
    }
}

#[derive(Debug, Clone)]
pub struct PostureTickEngine {
    off_axis_streak_threshold: f64,
    off_axis_repeat_interval: f64,
    facing_threshold: f64,
    eyest_threshold: f64,
    facing_seconds: f64,
    presence_seconds: f64,
    snoozed: bool,
    yaw_oa: OffAxisState,
    pitch_oa: OffAxisState,
}

impl Default for PostureTickEngine {
    fn default() -> Self {
        Self::new(None, None, None, None)
    }
}

impl PostureTickEngine {
    pub fn new(
        off_axis_streak_threshold_seconds: Option<f64>,
        off_axis_repeat_interval_seconds: Option<f64>,
        facing_threshold_seconds: Option<f64>,
        eyest_threshold_seconds: Option<f64>,
    ) -> Self {
        Self {
            off_axis_streak_threshold: off_axis_streak_threshold_seconds
                .unwrap_or(defaults::OFF_AXIS_STREAK_THRESHOLD),
            off_axis_repeat_interval: off_axis_repeat_interval_seconds
                .unwrap_or(defaults::OFF_AXIS_REPEAT_INTERVAL),
            facing_threshold: facing_threshold_seconds.unwrap_or(defaults::FACING_THRESHOLD),
            eyest_threshold: eyest_threshold_seconds.unwrap_or(defaults::EYEREST_THRESHOLD),
            facing_seconds: 0.0,
            presence_seconds: 0.0,
            snoozed: false,
            yaw_oa: OffAxisState::new_yaw(),
            pitch_oa: OffAxisState::new_pitch(),
        }
    }

    pub fn is_snoozed(&self) -> bool {
        self.snoozed
    }

    pub fn snooze(&mut self) {
        self.snoozed = true;
    }

    pub fn resume(&mut self) {
        self.snoozed = false;
    }

    /// 更新时机相关阈值，保留所有累加状态。
    pub fn update_timing(
        &mut self,
        off_axis_streak_threshold: f64,
        off_axis_repeat_interval: f64,
        facing_threshold: f64,
        eyest_threshold: f64,
    ) {
        self.off_axis_streak_threshold = off_axis_streak_threshold;
        self.off_axis_repeat_interval = off_axis_repeat_interval;
        self.facing_threshold = facing_threshold;
        self.eyest_threshold = eyest_threshold;
    }

    /// 当前综合警告级别（取两轴中更严重者）。
    pub fn warning_level(&self) -> WarningLevel {
        match (self.yaw_oa.warning_level, self.pitch_oa.warning_level) {
            (WarningLevel::Severe, _) | (_, WarningLevel::Severe) => WarningLevel::Severe,
            (WarningLevel::Warning, _) | (_, WarningLevel::Warning) => WarningLevel::Warning,
            (WarningLevel::Corrected, _) | (_, WarningLevel::Corrected) => WarningLevel::Corrected,
            _ => WarningLevel::Normal,
        }
    }

    /// 双轴 tick。
    ///
    /// - yaw_state / pitch_state 各自独立生成 Correction 和 WarningLevelChanged 事件。
    /// - GoodPosture 仅在两轴均 FacingScreen 时累积。
    /// - EyeRest 基于有脸时间（两轴均非 NoFace）。
    pub fn tick(
        &mut self,
        yaw_state: PoseState,
        pitch_state: PoseState,
        dt: f64,
    ) -> Vec<SenseEvent> {
        let mut events = Vec::new();

        if self.snoozed {
            return events;
        }

        // 各轴独立处理 off-axis streak + 警告升级
        events.extend(self.yaw_oa.tick(
            yaw_state,
            dt,
            self.off_axis_streak_threshold,
            self.off_axis_repeat_interval,
        ));
        events.extend(self.pitch_oa.tick(
            pitch_state,
            dt,
            self.off_axis_streak_threshold,
            self.off_axis_repeat_interval,
        ));

        // GoodPosture：两轴均 FacingScreen 才累积，其他情况暂停（不重置）
        let both_facing =
            yaw_state == PoseState::FacingScreen && pitch_state == PoseState::FacingScreen;
        if both_facing {
            self.facing_seconds += dt;
            if self.facing_seconds >= self.facing_threshold {
                self.facing_seconds = 0.0;
                events.push(SenseEvent::GoodPosture);
            }
        }

        // EyeRest：两轴均有脸即累积
        let any_face = yaw_state != PoseState::NoFace && pitch_state != PoseState::NoFace;
        if any_face {
            self.presence_seconds += dt;
            if self.presence_seconds >= self.eyest_threshold {
                self.presence_seconds = 0.0;
                events.push(SenseEvent::EyeRest);
            }
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_timing_preserves_accumulated_state() {
        let mut engine = PostureTickEngine::new(Some(0.3), Some(10.0), Some(300.0), Some(900.0));

        // 累积一些 facing_seconds
        for _ in 0..10 {
            engine.tick(PoseState::FacingScreen, PoseState::FacingScreen, 0.1);
        }
        // facing_seconds ≈ 1.0
        assert!(engine.facing_seconds > 0.5);

        // 累积一些 presence_seconds
        for _ in 0..5 {
            engine.tick(PoseState::OffAxisRight, PoseState::FacingScreen, 0.1);
        }
        // presence_seconds ≈ 1.0 + 前面的
        assert!(engine.presence_seconds > 0.5);

        let prev_facing = engine.facing_seconds;
        let prev_presence = engine.presence_seconds;

        // 更新阈值
        engine.update_timing(1.0, 30.0, 600.0, 1800.0);

        // 累加状态不变
        assert_eq!(engine.facing_seconds, prev_facing);
        assert_eq!(engine.presence_seconds, prev_presence);

        // 新阈值已生效（通过后续 tick 行为验证）
        assert_eq!(engine.off_axis_streak_threshold, 1.0);
        assert_eq!(engine.off_axis_repeat_interval, 30.0);
        assert_eq!(engine.facing_threshold, 600.0);
        assert_eq!(engine.eyest_threshold, 1800.0);
    }

    #[test]
    fn update_timing_does_not_reset_warning_state() {
        let mut engine = PostureTickEngine::new(Some(0.1), Some(10.0), Some(300.0), Some(900.0));

        // 触发 Warning 状态
        for _ in 0..5 {
            engine.tick(PoseState::OffAxisRight, PoseState::FacingScreen, 0.1);
        }
        assert_eq!(engine.warning_level(), WarningLevel::Warning);

        // 更新阈值
        engine.update_timing(0.5, 5.0, 100.0, 500.0);

        // 警告状态不变
        assert_eq!(engine.warning_level(), WarningLevel::Warning);
    }

    #[test]
    fn yaw_tracker_ignores_pitch_only_states() {
        // yaw 跟踪器不应把 pitch 轴的 HeadUp/HeadDown 当作本轴偏离。
        let mut yaw = OffAxisState::new_yaw();
        let dt = 0.1;
        let threshold = 0.3;
        let repeat = 10.0;

        for _ in 0..100 {
            let events = yaw.tick(PoseState::HeadUp, dt, threshold, repeat);
            assert!(events.is_empty(), "yaw tracker should not emit events for HeadUp");
        }

        assert_eq!(yaw.streak, 0.0);
        assert_eq!(yaw.warning_level, WarningLevel::Normal);
    }

    #[test]
    fn pitch_tracker_ignores_yaw_only_states() {
        // pitch 跟踪器不应把 yaw 轴的 OffAxisLeft/OffAxisRight 当作本轴偏离。
        let mut pitch = OffAxisState::new_pitch();
        let dt = 0.1;
        let threshold = 0.3;
        let repeat = 10.0;

        for _ in 0..100 {
            let events = pitch.tick(PoseState::OffAxisRight, dt, threshold, repeat);
            assert!(
                events.is_empty(),
                "pitch tracker should not emit events for OffAxisRight"
            );
        }

        assert_eq!(pitch.streak, 0.0);
        assert_eq!(pitch.warning_level, WarningLevel::Normal);
    }

    #[test]
    fn yaw_tracker_responds_to_yaw_states() {
        // 轴专属过滤后，yaw 仍应正常响应本轴偏离。
        let mut yaw = OffAxisState::new_yaw();
        let events = yaw.tick(PoseState::OffAxisRight, 0.5, 0.3, 10.0);
        assert!(!events.is_empty(), "yaw tracker should emit Correction for OffAxisRight");
        assert_eq!(yaw.warning_level, WarningLevel::Warning);
    }

    #[test]
    fn pitch_tracker_responds_to_pitch_states() {
        // 轴专属过滤后，pitch 仍应正常响应本轴偏离。
        let mut pitch = OffAxisState::new_pitch();
        let events = pitch.tick(PoseState::HeadUp, 0.5, 0.3, 10.0);
        assert!(!events.is_empty(), "pitch tracker should emit Correction for HeadUp");
        assert_eq!(pitch.warning_level, WarningLevel::Warning);
    }
}
