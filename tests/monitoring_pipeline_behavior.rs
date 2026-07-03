//! 监测管道集成测试：FakeDetector + FakeCamera → MonitoringWorker → SenseEvent 序列。
//!
//! 覆盖 4 条端到端行为轨道：
//! 1. 偏离 streak → Correction 事件
//! 2. 时间累加器（GoodPosture / EyeRest）
//! 3. Warning 级别流转
//! 4. Pitch 轴 Correction

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use eyes_lib::domain::classifier::{HeadPose, PoseState};
use eyes_lib::domain::config::{ConfigState, ConfigStore};
use eyes_lib::domain::posture_tick_engine::{PostureTickEngine, SenseEvent, WarningLevel};
use eyes_lib::monitoring::detector::Detector;
use eyes_lib::monitoring::preview::Frame;
use eyes_lib::monitoring::worker::{FrameSource, MonitoringWorker, WorkerOutput};

// ── Fake 实现 ──────────────────────────────────────────────────

/// 可在测试中途改变返回 pose 的 FakeDetector。
///
/// - 通过 `active` 标志控制是否返回 `Some(HeadPose)` 或 `None`
/// - 通过 `yaw` / `pitch` 原子值控制返回的头部姿态
///
/// `active` 是 `&'static AtomicBool`，由各测试函数提供。
struct FakeDetector {
    yaw: AtomicU64,
    pitch: AtomicU64,
    active: &'static AtomicBool,
}

impl FakeDetector {
    fn new(yaw: f64, pitch: f64, active: &'static AtomicBool) -> Self {
        Self {
            yaw: AtomicU64::new(yaw.to_bits()),
            pitch: AtomicU64::new(pitch.to_bits()),
            active,
        }
    }

    fn set_pose(&self, yaw: f64, pitch: f64) {
        self.yaw.store(yaw.to_bits(), Ordering::Relaxed);
        self.pitch.store(pitch.to_bits(), Ordering::Relaxed);
    }
}

impl Detector for FakeDetector {
    fn detect(&mut self, _rgb: &[u8], _w: u32, _h: u32) -> Option<HeadPose> {
        if !self.active.load(Ordering::Relaxed) {
            return None;
        }
        let yaw = f64::from_bits(self.yaw.load(Ordering::Relaxed));
        let pitch = f64::from_bits(self.pitch.load(Ordering::Relaxed));
        Some(HeadPose { yaw, pitch })
    }
}

/// 始终返回帧的 FakeCamera。
struct FakeCamera;

impl FrameSource for FakeCamera {
    fn read_frame(&mut self) -> Result<Option<Frame>, String> {
        Ok(Some(Frame {
            width: 640,
            height: 480,
            rgb: vec![0u8; 640 * 480 * 3],
        }))
    }
}

// ── 辅助函数 ───────────────────────────────────────────────────

fn has_correction(events: &[SenseEvent]) -> bool {
    events.iter().any(|e| matches!(e, SenseEvent::Correction { .. }))
}

fn has_correction_for(events: &[SenseEvent], state: PoseState) -> bool {
    events
        .iter()
        .any(|e| matches!(e, SenseEvent::Correction { direction } if *direction == state))
}

fn has_good_posture(events: &[SenseEvent]) -> bool {
    events.iter().any(|e| matches!(e, SenseEvent::GoodPosture))
}

fn has_eye_rest(events: &[SenseEvent]) -> bool {
    events.iter().any(|e| matches!(e, SenseEvent::EyeRest))
}

fn has_warning(events: &[SenseEvent], level: WarningLevel) -> bool {
    events.iter().any(|e| {
        matches!(e, SenseEvent::WarningLevelChanged { level: actual, .. } if *actual == level)
    })
}

fn collect_events(outputs: &[WorkerOutput]) -> Vec<SenseEvent> {
    outputs
        .iter()
        .flat_map(|o| o.sense_events.iter().cloned())
        .collect()
}

fn default_engine() -> PostureTickEngine {
    PostureTickEngine::new(
        Some(0.3),  // streak threshold
        Some(2.0),  // repeat interval
        Some(5.0),  // facing threshold
        Some(10.0), // eyest threshold
    )
}

fn default_config_state() -> Arc<ConfigState> {
    let dir = tempfile::tempdir().unwrap();
    // 为避免 tempdir 被 drop，泄漏它（测试进程退出时回收）
    let path = dir.into_path();
    Arc::new(ConfigState::new(ConfigStore::new(&path)).unwrap())
}

fn make_worker(yaw: f64, pitch: f64) -> MonitoringWorker<FakeCamera> {
    let active = Box::leak(Box::new(AtomicBool::new(true)));
    let detector = Box::new(FakeDetector::new(yaw, pitch, active));
    MonitoringWorker::new(FakeCamera, Some(detector as Box<dyn Detector>), default_engine(), default_config_state())
}

fn make_worker_with_engine(
    yaw: f64,
    pitch: f64,
    engine: PostureTickEngine,
) -> (MonitoringWorker<FakeCamera>, &'static AtomicBool) {
    let active = Box::leak(Box::new(AtomicBool::new(true)));
    let detector = Box::new(FakeDetector::new(yaw, pitch, active));
    let worker = MonitoringWorker::new(FakeCamera, Some(detector as Box<dyn Detector>), engine, default_config_state());
    (worker, active)
}

/// tick n 次，收集所有 WorkerOutput。
fn tick_n(worker: &mut MonitoringWorker<FakeCamera>, n: usize, dt: f64) -> Vec<WorkerOutput> {
    (0..n).map(|_| worker.tick(dt)).collect()
}

// ── Track 1：偏离 streak → Correction 事件 ─────────────────────

#[test]
fn off_axis_yields_first_correction() {
    let mut w = make_worker(6.0, 0.0);
    let outputs = tick_n(&mut w, 4, 0.1); // 0.4s > 0.3s threshold
    let events = collect_events(&outputs);
    assert!(
        events.iter().any(|e| matches!(e, SenseEvent::Correction { direction: PoseState::OffAxisRight })),
        "持续偏离 0.4s 应触发 Correction, events={events:?}"
    );
}

#[test]
fn repeated_off_axis_fires_repeat_correction() {
    let mut w = make_worker(6.0, 0.0);
    // 第一次 correction 在 0.3s
    tick_n(&mut w, 4, 0.1);
    // repeat_interval=2.0s，再 tick 2.1s
    let outputs = tick_n(&mut w, 21, 0.1);
    let events = collect_events(&outputs);
    assert!(
        has_correction(&events),
        "持续偏离应重复触发 Correction, events={events:?}"
    );
}

#[test]
fn streak_resets_after_returning_to_center() {
    let mut w = make_worker(6.0, 0.0);
    // 偏离 0.2s（未触发）
    tick_n(&mut w, 2, 0.1);
    // 回到正对 — 创建新 worker 重置 streak
    let mut w2 = make_worker(0.0, 0.0);
    tick_n(&mut w2, 5, 0.1); // 0.5s 正对
    // 再次偏离，需要重新计时
    let mut w3 = make_worker(6.0, 0.0);
    let outputs = tick_n(&mut w3, 2, 0.1); // 0.2s < 0.3s
    let events = collect_events(&outputs);
    assert!(
        !has_correction(&events),
        "回到正对后 streak 应清零，0.2s 不应触发 Correction"
    );
    // 再多 tick 2 次达到阈值
    let outputs = tick_n(&mut w3, 2, 0.1);
    let events = collect_events(&outputs);
    assert!(has_correction(&events), "重新偏离 0.4s 应触发 Correction");
}

// ── Track 2：时间累加器 ────────────────────────────────────────

#[test]
fn facing_screen_fires_good_posture() {
    let mut w = make_worker(0.0, 0.0);
    // facing_threshold=5.0s，dt=0.1，多 tick 几帧以避免浮点边界问题
    let outputs = tick_n(&mut w, 55, 0.1);
    let events = collect_events(&outputs);
    assert!(has_good_posture(&events), "持续正对 ~5s 应触发 GoodPosture");
}

#[test]
fn face_present_fires_eye_rest() {
    let mut w = make_worker(0.0, 0.0);
    // eyest_threshold=10.0s，dt=0.1，多 tick 几帧
    let outputs = tick_n(&mut w, 105, 0.1);
    let events = collect_events(&outputs);
    assert!(has_eye_rest(&events), "有脸 ~10s 应触发 EyeRest");
}

#[test]
fn presence_accumulator_pauses_on_face_loss() {
    // 有脸 8s，然后无脸 5s，再有脸 ~2s → 应在 ~10s 时触发 EyeRest
    let (mut w, active) = make_worker_with_engine(0.0, 0.0, default_engine());

    // 有脸 8s（80 帧 × 0.1s）
    tick_n(&mut w, 80, 0.1);

    // 关闭 detector → 模拟无脸（engine 仍在 tick，但 presence 不累积）
    active.store(false, Ordering::Relaxed);
    tick_n(&mut w, 50, 0.1); // 无脸 5s，presence_seconds 暂停在 ~8s

    // 重新开启 detector → 有脸
    active.store(true, Ordering::Relaxed);
    // 再 25 帧（~2.5s）→ presence 应超过 10s → 触发 EyeRest
    let outputs = tick_n(&mut w, 25, 0.1);
    let events = collect_events(&outputs);
    assert!(
        has_eye_rest(&events),
        "有脸 8s + 无脸 5s + 有脸 ~2.5s 应触发 EyeRest（累加器暂停不归零）"
    );
}

// ── Track 3：Warning 级别流转 ───────────────────────────────────

#[test]
fn warning_fires_on_first_off_axis() {
    let mut w = make_worker(6.0, 0.0);
    // 偏离 1s → 触发 Warning
    let outputs = tick_n(&mut w, 10, 0.1);
    let events = collect_events(&outputs);
    assert!(has_warning(&events, WarningLevel::Warning), "首次偏离应触发 Warning");
}

#[test]
fn severe_fires_after_sustained_off_axis() {
    let mut w = make_worker(6.0, 0.0);
    // 偏离 2s+ → Warning 再到 Severe
    let outputs = tick_n(&mut w, 21, 0.1);
    let events = collect_events(&outputs);
    assert!(has_warning(&events, WarningLevel::Severe), "偏离 2s+ 应触发 Severe");
}

#[test]
fn no_face_resets_warning_to_normal() {
    let (mut w, active) = make_worker_with_engine(6.0, 0.0, default_engine());
    // 偏离 1s → Warning
    tick_n(&mut w, 10, 0.1);

    // 关闭 detector → 模拟无脸 → 直接 Normal
    active.store(false, Ordering::Relaxed);
    let outputs = tick_n(&mut w, 1, 0.1);
    let events = collect_events(&outputs);
    assert!(
        has_warning(&events, WarningLevel::Normal),
        "无脸应直接回到 Normal, events={events:?}"
    );
}

// ── Track 4：Pitch 轴 ──────────────────────────────────────────

#[test]
fn pitch_down_triggers_correction() {
    let mut w = make_worker(0.0, -20.0); // yaw=0, pitch=-20 → HeadDown（低头）
    let outputs = tick_n(&mut w, 4, 0.1);
    let events = collect_events(&outputs);
    assert!(
        has_correction_for(&events, PoseState::HeadDown),
        "低头偏离应触发 HeadDown Correction, events={events:?}"
    );
}

#[test]
fn pitch_up_triggers_correction() {
    let mut w = make_worker(0.0, 20.0); // yaw=0, pitch=20 → HeadUp（仰头）
    let outputs = tick_n(&mut w, 4, 0.1);
    let events = collect_events(&outputs);
    assert!(
        has_correction_for(&events, PoseState::HeadUp),
        "仰头偏离应触发 HeadUp Correction, events={events:?}"
    );
}

#[test]
fn yaw_and_pitch_both_trigger_independent_corrections() {
    let mut w = make_worker(6.0, -20.0); // yaw=6→OffAxisRight, pitch=-20→HeadDown
    let outputs = tick_n(&mut w, 5, 0.1); // 0.5s > 0.3s threshold
    let events = collect_events(&outputs);
    assert!(
        has_correction_for(&events, PoseState::OffAxisRight),
        "yaw 偏离应触发 OffAxisRight Correction"
    );
    assert!(
        has_correction_for(&events, PoseState::HeadDown),
        "pitch 偏离应触发 HeadDown Correction"
    );
}
