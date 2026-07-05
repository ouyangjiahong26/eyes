use super::classifier::PoseState;
use super::thresholds::TimingThresholds;

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

/// 单轴 off-axis 状态机。
///
/// 单一时间轴 `continuous_seconds` 同时驱动 WarningLevel 升级与 Correction
/// 触发节奏，二者从同一时刻序列派生（issue #32、ADR 0009）。
///
/// ## 字段
///
/// - `axis`：本轴身份（Yaw / Pitch），用于过滤另一轴状态。
/// - `warning_level`：Normal / Warning / Severe / Corrected FSM 当前所处状态。
/// - `continuous_seconds`：自进入本轴 off-axis 起累计的时长（秒）。进入 off-axis
///   那一刻从 0 开始；切回 `FacingScreen` 或 `NoFace` 时被重置为 0。
/// - `corrected_remaining_seconds`：Corrected 缓冲剩余时长（硬编码 2.0s，不参数化）。
/// - `next_correction_threshold`：下一次 Correction 触发的 `continuous_seconds`
///   阈值；离开 off-axis 时清为 `None`，进入 off-axis 后在首次 Correction
///   触发时被初始化为 `Some(continuous_seconds + repeat_interval)`。
///
/// ## 不对称重置语义（有意为之）
///
/// `NoFace` 与 `FacingScreen` 的重置语义**不对称**，是产品决策（ADR 0009 决策 9）：
///
/// - `NoFace` 直接走 `reset_warning()` 清零 WarningLevel 与 `continuous_seconds`，
///   **不**走 Corrected 流程。因为人已经离开屏幕，再发"已纠正"的提示没有对象。
/// - `FacingScreen` 走 Warning/Severe → Corrected 缓冲 2s → Normal；进入 Corrected
///   时把 `continuous_seconds` 清零。给主动纠正一个正向反馈缓冲。
#[derive(Debug, Clone)]
struct OffAxisState {
    axis: Axis,
    warning_level: WarningLevel,
    continuous_seconds: f64,
    corrected_remaining_seconds: f64,
    next_correction_threshold: Option<f64>,
}

impl OffAxisState {
    fn new(axis: Axis) -> Self {
        Self {
            axis,
            warning_level: WarningLevel::Normal,
            continuous_seconds: 0.0,
            corrected_remaining_seconds: 0.0,
            next_correction_threshold: None,
        }
    }

    fn new_yaw() -> Self {
        Self::new(Axis::Yaw)
    }

    fn new_pitch() -> Self {
        Self::new(Axis::Pitch)
    }

    /// 清空 WarningLevel FSM 与连续时长累加，但**不**触碰
    /// `next_correction_threshold`——后者由调用方根据上下文（离开 off-axis 时）
    /// 显式清空，避免在 Normal 状态保持 None 的语义被误改写。
    fn reset_warning(&mut self) {
        self.warning_level = WarningLevel::Normal;
        self.continuous_seconds = 0.0;
        self.corrected_remaining_seconds = 0.0;
    }

    /// 判断状态是否属于本轴的 off-axis。
    /// yaw 只关心 OffAxisLeft/OffAxisRight，pitch 只关心 HeadUp/HeadDown，
    /// 避免把另一轴的偏离状态计入本轴的 `continuous_seconds` 或警告升级。
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

    /// 越过 `next_correction_threshold` 时发出一次 Correction 并把阈值
    /// 递增一个 `repeat_interval`。Warning 与 Severe 两态共享此逻辑：
    /// Warning 态下可能与 Severe 升级同帧触发（第二次 Correction），
    /// Severe 态下为后续重复提醒。
    fn emit_due_correction(
        &mut self,
        events: &mut Vec<SenseEvent>,
        state: PoseState,
        repeat_interval: f64,
    ) {
        if let Some(due) = self.next_correction_threshold {
            if self.continuous_seconds >= due {
                events.push(SenseEvent::Correction { direction: state });
                self.next_correction_threshold = Some(self.continuous_seconds + repeat_interval);
            }
        }
    }

    /// 单轴完整 tick：所有 off-axis 派生事件从 `continuous_seconds` 单一时间轴派生。
    ///
    /// 触发条件（ADR 0009 决策 3-6）：
    ///
    /// - Normal/Corrected → Warning：`is_off_for_self(state) && continuous_seconds >= streak_threshold`
    /// - Warning → Severe：`continuous_seconds >= severe_threshold`
    /// - Correction：依次越过 `streak_threshold`、`repeat_interval`、`2*repeat_interval`...
    ///   首次与 Warning 升级同步；第二次与 Severe 升级同步（即下一次 Correction
    ///   在 `continuous_seconds >= next_correction_threshold` 时触发）。
    /// - Corrected → Normal：硬编码 2.0s 缓冲。
    fn tick(
        &mut self,
        state: PoseState,
        dt: f64,
        streak_threshold: f64,
        severe_threshold: f64,
        repeat_interval: f64,
    ) -> Vec<SenseEvent> {
        let mut events = Vec::new();
        // 不属于本轴且非通用状态 → 静默忽略
        if !self.is_applicable(state) {
            return events;
        }

        let direction = direction_label(state);

        if self.is_off_for_self(state) {
            self.continuous_seconds += dt;

            match self.warning_level {
                WarningLevel::Normal | WarningLevel::Corrected => {
                    // Normal/Corrected → Warning；首次 Correction 同步发出
                    if self.continuous_seconds >= streak_threshold {
                        self.warning_level = WarningLevel::Warning;
                        events.push(SenseEvent::WarningLevelChanged {
                            level: WarningLevel::Warning,
                            direction: Some(direction.to_string()),
                        });
                        events.push(SenseEvent::Correction { direction: state });
                        self.next_correction_threshold =
                            Some(self.continuous_seconds + repeat_interval);
                    }
                }
                WarningLevel::Warning => {
                    // Warning → Severe
                    if self.continuous_seconds >= severe_threshold {
                        self.warning_level = WarningLevel::Severe;
                        events.push(SenseEvent::WarningLevelChanged {
                            level: WarningLevel::Severe,
                            direction: Some(direction.to_string()),
                        });
                    }
                    // Correction：可能与 Severe 升级同帧触发（第二次 Correction）
                    self.emit_due_correction(&mut events, state, repeat_interval);
                }
                WarningLevel::Severe => {
                    // Severe 状态下的后续 Correction
                    self.emit_due_correction(&mut events, state, repeat_interval);
                }
            }
        } else if state == PoseState::FacingScreen {
            match self.warning_level {
                // Warning/Severe → Corrected：进入 Corrected 时清零 continuous_seconds
                // 并清空 next_correction_threshold（离开 off-axis）。
                WarningLevel::Warning | WarningLevel::Severe => {
                    self.warning_level = WarningLevel::Corrected;
                    self.corrected_remaining_seconds = 2.0;
                    self.continuous_seconds = 0.0;
                    self.next_correction_threshold = None;
                    events.push(SenseEvent::WarningLevelChanged {
                        level: WarningLevel::Corrected,
                        direction: None,
                    });
                }
                // Corrected → Normal：硬编码 2.0s 缓冲（不参数化）
                WarningLevel::Corrected => {
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
                WarningLevel::Normal => {}
            }
        } else if state == PoseState::NoFace {
            // NoFace：直接走 reset_warning，不走 Corrected 流程。
            // 故意与 FacingScreen 的重置语义不对称（ADR 0009 决策 9）。
            if self.warning_level != WarningLevel::Normal {
                self.reset_warning();
                self.next_correction_threshold = None;
                events.push(SenseEvent::WarningLevelChanged {
                    level: WarningLevel::Normal,
                    direction: None,
                });
            }
        }

        events
    }
}

#[derive(Debug, Clone)]
pub struct PostureTickEngine {
    off_axis_streak_threshold: f64,
    off_axis_repeat_interval: f64,
    off_axis_severe_threshold: f64,
    facing_threshold: f64,
    eyest_threshold: f64,
    facing_seconds: f64,
    presence_seconds: f64,
    yaw_oa: OffAxisState,
    pitch_oa: OffAxisState,
}

impl Default for PostureTickEngine {
    fn default() -> Self {
        Self::new(TimingThresholds::default())
    }
}

impl PostureTickEngine {
    pub fn new(timing: TimingThresholds) -> Self {
        Self {
            off_axis_streak_threshold: timing.off_axis_streak_threshold_seconds,
            off_axis_repeat_interval: timing.off_axis_repeat_interval_seconds,
            off_axis_severe_threshold: timing.off_axis_severe_threshold_seconds,
            facing_threshold: timing.facing_threshold_seconds,
            eyest_threshold: timing.eyest_threshold_seconds,
            facing_seconds: 0.0,
            presence_seconds: 0.0,
            yaw_oa: OffAxisState::new_yaw(),
            pitch_oa: OffAxisState::new_pitch(),
        }
    }

    /// 更新时机相关阈值，保留所有累加状态。
    pub fn update_timing(&mut self, timing: TimingThresholds) {
        self.off_axis_streak_threshold = timing.off_axis_streak_threshold_seconds;
        self.off_axis_repeat_interval = timing.off_axis_repeat_interval_seconds;
        self.off_axis_severe_threshold = timing.off_axis_severe_threshold_seconds;
        self.facing_threshold = timing.facing_threshold_seconds;
        self.eyest_threshold = timing.eyest_threshold_seconds;
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

        // 各轴独立处理 off-axis 警告升级与 Correction
        events.extend(self.yaw_oa.tick(
            yaw_state,
            dt,
            self.off_axis_streak_threshold,
            self.off_axis_severe_threshold,
            self.off_axis_repeat_interval,
        ));
        events.extend(self.pitch_oa.tick(
            pitch_state,
            dt,
            self.off_axis_streak_threshold,
            self.off_axis_severe_threshold,
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
        let mut engine = PostureTickEngine::new(TimingThresholds {
            off_axis_streak_threshold_seconds: 0.3,
            off_axis_repeat_interval_seconds: 10.0,
            off_axis_severe_threshold_seconds: 15.0,
            facing_threshold_seconds: 300.0,
            eyest_threshold_seconds: 900.0,
        });

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
        engine.update_timing(TimingThresholds {
            off_axis_streak_threshold_seconds: 1.0,
            off_axis_repeat_interval_seconds: 30.0,
            off_axis_severe_threshold_seconds: 20.0,
            facing_threshold_seconds: 600.0,
            eyest_threshold_seconds: 1800.0,
        });

        // 累加状态不变
        assert_eq!(engine.facing_seconds, prev_facing);
        assert_eq!(engine.presence_seconds, prev_presence);

        // 新阈值已生效（通过后续 tick 行为验证）
        assert_eq!(engine.off_axis_streak_threshold, 1.0);
        assert_eq!(engine.off_axis_repeat_interval, 30.0);
        assert_eq!(engine.off_axis_severe_threshold, 20.0);
        assert_eq!(engine.facing_threshold, 600.0);
        assert_eq!(engine.eyest_threshold, 1800.0);
    }

    #[test]
    fn update_timing_does_not_reset_warning_state() {
        let mut engine = PostureTickEngine::new(TimingThresholds {
            off_axis_streak_threshold_seconds: 0.1,
            off_axis_repeat_interval_seconds: 10.0,
            off_axis_severe_threshold_seconds: 5.0,
            facing_threshold_seconds: 300.0,
            eyest_threshold_seconds: 900.0,
        });

        // 触发 Warning 状态
        for _ in 0..5 {
            engine.tick(PoseState::OffAxisRight, PoseState::FacingScreen, 0.1);
        }
        assert_eq!(engine.warning_level(), WarningLevel::Warning);

        // 更新阈值
        engine.update_timing(TimingThresholds {
            off_axis_streak_threshold_seconds: 0.5,
            off_axis_repeat_interval_seconds: 5.0,
            off_axis_severe_threshold_seconds: 20.0,
            facing_threshold_seconds: 100.0,
            eyest_threshold_seconds: 500.0,
        });

        // 警告状态不变
        assert_eq!(engine.warning_level(), WarningLevel::Warning);
    }

    #[test]
    fn yaw_tracker_ignores_pitch_only_states() {
        // yaw 跟踪器不应把 pitch 轴的 HeadUp/HeadDown 当作本轴偏离。
        let mut yaw = OffAxisState::new_yaw();
        let dt = 0.1;
        let threshold = 0.3;
        let severe = 10.0;
        let repeat = 10.0;

        for _ in 0..100 {
            let events = yaw.tick(PoseState::HeadUp, dt, threshold, severe, repeat);
            assert!(events.is_empty(), "yaw tracker should not emit events for HeadUp");
        }

        assert_eq!(yaw.continuous_seconds, 0.0);
        assert_eq!(yaw.warning_level, WarningLevel::Normal);
    }

    #[test]
    fn pitch_tracker_ignores_yaw_only_states() {
        // pitch 跟踪器不应把 yaw 轴的 OffAxisLeft/OffAxisRight 当作本轴偏离。
        let mut pitch = OffAxisState::new_pitch();
        let dt = 0.1;
        let threshold = 0.3;
        let severe = 10.0;
        let repeat = 10.0;

        for _ in 0..100 {
            let events = pitch.tick(PoseState::OffAxisRight, dt, threshold, severe, repeat);
            assert!(
                events.is_empty(),
                "pitch tracker should not emit events for OffAxisRight"
            );
        }

        assert_eq!(pitch.continuous_seconds, 0.0);
        assert_eq!(pitch.warning_level, WarningLevel::Normal);
    }

    #[test]
    fn yaw_tracker_responds_to_yaw_states() {
        // 轴专属过滤后，yaw 仍应正常响应本轴偏离。
        let mut yaw = OffAxisState::new_yaw();
        let events = yaw.tick(PoseState::OffAxisRight, 0.5, 0.3, 10.0, 10.0);
        assert!(!events.is_empty(), "yaw tracker should emit Correction for OffAxisRight");
        assert_eq!(yaw.warning_level, WarningLevel::Warning);
    }

    #[test]
    fn pitch_tracker_responds_to_pitch_states() {
        // 轴专属过滤后，pitch 仍应正常响应本轴偏离。
        let mut pitch = OffAxisState::new_pitch();
        let events = pitch.tick(PoseState::HeadUp, 0.5, 0.3, 10.0, 10.0);
        assert!(!events.is_empty(), "pitch tracker should emit Correction for HeadUp");
        assert_eq!(pitch.warning_level, WarningLevel::Warning);
    }
}
