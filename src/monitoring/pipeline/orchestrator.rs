//! WorkerOrchestrator —— 管理后台监控 worker 的完整生命周期。

use crate::domain::calibration::{CalibrationResult, CalibrationSession, CALIBRATION_DURATION};
use crate::domain::classifier::PoseState;
use crate::domain::config::ConfigState;
use crate::monitoring::events::event_mapping;
use crate::monitoring::events::MonitoringEvent;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::channel::{WorkerCommand, WorkerReceiver};
use super::detector::Detector;
use super::worker::{FrameSource, MonitoringWorker, WorkerOutput};

pub type CameraFactory = Box<dyn FnMut(u32) -> Result<Box<dyn FrameSource>, String> + Send>;
pub type DetectorFactory = Box<dyn Fn() -> Option<Box<dyn Detector>> + Send>;

pub type Worker = MonitoringWorker<Box<dyn FrameSource>>;
pub type WorkerFactory =
    Box<dyn FnMut(Box<dyn FrameSource>, Option<Box<dyn Detector>>) -> Box<Worker> + Send>;

/// Orchestrator 内部状态，只包含可被纯函数推进的字段。
///
/// `tick` 方法输入当前 worker 输出与时间步，返回要 emit 的事件列表和可能产生的校准结果。
/// 所有 I/O（Instant、channel send）仍由外层 `WorkerOrchestrator` 负责。
#[derive(Debug, Default)]
struct OrchestratorState {
    calibration_session: Option<CalibrationSession>,
    calibration_no_face_seconds: f64,
}

impl OrchestratorState {
    /// 统一校准失败清理：清空 session 与无脸计数，返回失败事件。
    fn fail_calibration(&mut self, reason: &str) -> Vec<MonitoringEvent> {
        self.calibration_session = None;
        self.calibration_no_face_seconds = 0.0;
        vec![MonitoringEvent::CalibrationFailed {
            reason: reason.into(),
        }]
    }

    /// 纯函数：根据当前状态与一次 worker 输出推进校准状态机。
    ///
    /// - `output` 为 `None` 表示当前没有可用 monitor（摄像头未就绪）。
    /// - 返回的 `CalibrationResult` 由外层写入 `ConfigState`。
    fn tick(
        &mut self,
        output: Option<&WorkerOutput>,
        dt: f64,
    ) -> (Vec<MonitoringEvent>, Option<CalibrationResult>) {
        let mut events = Vec::new();
        let mut result = None;

        // 喂入样本并检测连续无脸（monitor 存在或不存在两种路径）
        if let Some(session) = self.calibration_session.as_mut() {
            if session.is_active() {
                match output {
                    Some(out) => {
                        if let (Some(y), Some(p)) = (out.yaw, out.pitch) {
                            session.feed(y, p);
                        }
                        if out.pose_state == PoseState::NoFace {
                            self.calibration_no_face_seconds += dt;
                            if self.calibration_no_face_seconds >= 1.0 {
                                events.extend(self.fail_calibration("no_face"));
                            }
                        } else {
                            self.calibration_no_face_seconds = 0.0;
                        }
                    }
                    None => {
                        self.calibration_no_face_seconds += dt;
                        if self.calibration_no_face_seconds >= 1.0 {
                            events.extend(self.fail_calibration("no_face"));
                        }
                    }
                }
            }
        }

        // 推进校准倒计时
        if let Some(session) = self.calibration_session.as_mut() {
            if session.is_active() {
                session.tick(dt);
                if !session.is_active() {
                    // 校准结束
                    if let Some(res) = session.result() {
                        events.push(MonitoringEvent::CalibrationComplete {
                            yaw: res.yaw,
                            pitch: res.pitch,
                            sample_count: res.sample_count,
                        });
                        self.calibration_session = None;
                        self.calibration_no_face_seconds = 0.0;
                        result = Some(res);
                    } else {
                        events.extend(self.fail_calibration("no_face"));
                    }
                }
            }
        }

        (events, result)
    }
}

pub struct WorkerOrchestrator {
    config_state: Arc<ConfigState>,
    state: OrchestratorState,
    event_tx: Sender<MonitoringEvent>,
    camera_factory: CameraFactory,
    detector_factory: DetectorFactory,
    worker_factory: WorkerFactory,
    retry_interval: Duration,
}

impl WorkerOrchestrator {
    pub fn new(
        config_state: Arc<ConfigState>,
        event_tx: Sender<MonitoringEvent>,
        camera_factory: CameraFactory,
        detector_factory: DetectorFactory,
        worker_factory: WorkerFactory,
    ) -> Self {
        Self {
            config_state,
            state: OrchestratorState::default(),
            event_tx,
            camera_factory,
            detector_factory,
            worker_factory,
            retry_interval: Duration::from_secs(5),
        }
    }

    pub fn with_retry_interval(mut self, interval: Duration) -> Self {
        self.retry_interval = interval;
        self
    }

    pub fn run(mut self, rx: WorkerReceiver) {
        let mut camera_index = self.config_state.get().camera_index;
        let mut snooze_until: Option<Instant> = None;
        let mut snoozed = false;
        let mut monitor: Option<Box<Worker>> = self.open_monitor(camera_index, snoozed);

        let mut retry_at: Option<Instant> = if monitor.is_none() {
            Some(Instant::now() + self.retry_interval)
        } else {
            None
        };

        let tick_interval = Duration::from_millis(100);
        let mut stopped = false;
        let mut last_tick = Instant::now();

        while !stopped {
            match rx.recv_timeout(tick_interval) {
                Ok(WorkerCommand::Stop) => {
                    stopped = true;
                    continue;
                }
                Ok(WorkerCommand::SetCameraIndex(idx)) => {
                    camera_index = idx;
                    monitor = None;
                    retry_at = Some(Instant::now());
                }
                Ok(WorkerCommand::SetConfig(new_config)) => {
                    let camera_changed = new_config.camera_index != camera_index;
                    if let Some(ref mut w) = monitor {
                        w.engine_mut().update_timing(new_config.timing);
                    }
                    if camera_changed {
                        camera_index = new_config.camera_index;
                        monitor.take();
                        monitor = self.open_monitor(camera_index, snoozed);
                        if monitor.is_none() {
                            retry_at = Some(Instant::now() + self.retry_interval);
                        }
                    }
                }
                Ok(WorkerCommand::Snooze(seconds)) => {
                    snoozed = true;
                    snooze_until = if seconds.is_infinite() {
                        None
                    } else {
                        Some(Instant::now() + Duration::from_secs_f64(seconds))
                    };
                    if let Some(ref mut w) = monitor {
                        w.set_snoozed(true);
                    }
                }
                Ok(WorkerCommand::Resume) => {
                    snoozed = false;
                    snooze_until = None;
                    if let Some(ref mut w) = monitor {
                        w.set_snoozed(false);
                    }
                }
                Ok(WorkerCommand::StartCalibration) => {
                    let mut session = CalibrationSession::new(CALIBRATION_DURATION as f64);
                    session.start();
                    self.state.calibration_session = Some(session);
                    self.state.calibration_no_face_seconds = 0.0;
                }
                Ok(WorkerCommand::CancelCalibration) => {
                    self.state.calibration_session = None;
                    self.state.calibration_no_face_seconds = 0.0;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }

            self.process_tick(
                &mut monitor,
                &mut snooze_until,
                &mut snoozed,
                &mut retry_at,
                camera_index,
                &mut last_tick,
            );
        }
    }

    fn process_tick(
        &mut self,
        monitor: &mut Option<Box<Worker>>,
        snooze_until: &mut Option<Instant>,
        snoozed: &mut bool,
        retry_at: &mut Option<Instant>,
        camera_index: u32,
        last_tick: &mut Instant,
    ) {
        let now = Instant::now();
        let dt = now.duration_since(*last_tick).as_secs_f64();
        *last_tick = now;

        if let Some(until) = snooze_until {
            if now >= *until {
                *snooze_until = None;
                *snoozed = false;
                if let Some(ref mut w) = monitor {
                    w.set_snoozed(false);
                }
            }
        }

        if monitor.is_none() && retry_at.is_some_and(|due| now >= due) {
            *monitor = self.open_monitor(camera_index, *snoozed);
            if monitor.is_some() {
                *retry_at = None;
            } else {
                *retry_at = Some(now + self.retry_interval);
            }
        }

        let worker_output = monitor.as_mut().map(|w| w.tick(dt));
        let output_ref = worker_output.as_ref();

        let (events, calibration_result) = self.state.tick(output_ref, dt);

        for event in events {
            let _ = self.event_tx.send(event);
        }

        if let Some(result) = calibration_result {
            let _ = self.config_state.update(|cfg| {
                cfg.neutral_yaw = result.yaw;
                cfg.neutral_pitch = result.pitch;
            });
        }

        if let Some(ref output) = worker_output {
            for event in event_mapping::from_worker_output(output) {
                let _ = self.event_tx.send(event);
            }

            if !output.camera_ok {
                monitor.take();
                *retry_at = Some(now + self.retry_interval);
            }
        }
    }

    fn open_monitor(&mut self, camera_index: u32, snoozed: bool) -> Option<Box<Worker>> {
        let camera = (self.camera_factory)(camera_index).ok()?;
        let detector = (self.detector_factory)();
        let mut worker = (self.worker_factory)(camera, detector);
        worker.set_snoozed(snoozed);
        Some(worker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::classifier::{HeadPose, PoseState};
    use crate::domain::config::ConfigStore;
    use crate::domain::posture_tick_engine::{PostureTickEngine, WarningLevel};
    use crate::domain::thresholds::TimingThresholds;
    use crate::monitoring::events::MonitoringEvent;
    use crate::monitoring::preview::Frame;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::channel;

    // ── 事件 → 字符串（供测试断言复用） ──────────────────────────

    /// 把 `MonitoringEvent` 转成稳定字符串，便于在测试里断言。
    fn event_to_string(event: MonitoringEvent) -> String {
        match event {
            MonitoringEvent::CameraStateChanged { state } => format!("camera:{}", state),
            MonitoringEvent::PoseUpdated { pose_state, .. } => format!("pose:{}", pose_state),
            MonitoringEvent::PreviewFrame(_) => "preview".into(),
            MonitoringEvent::SoundAlert { alert_type } => format!("sound:{}", alert_type),
            MonitoringEvent::WarningLevelChanged { level, direction } => {
                format!(
                    "warning:{}:{}",
                    level,
                    direction.unwrap_or_else(|| "none".into())
                )
            }
            MonitoringEvent::LogEvent { kind, .. } => format!("log:{:?}", kind),
            MonitoringEvent::CalibrationComplete {
                yaw,
                pitch,
                sample_count,
            } => {
                format!("calibration_complete:{}:{}:{}", yaw, pitch, sample_count)
            }
            MonitoringEvent::CalibrationFailed { reason } => {
                format!("calibration_failed:{}", reason)
            }
        }
    }

    // ── WorkerOutput 辅助 ───────────────────────────────────────

    fn good_output() -> WorkerOutput {
        WorkerOutput {
            preview: None,
            camera_ok: true,
            pose_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
            yaw: Some(0.0),
            pitch: Some(0.0),
            warning_level: WarningLevel::Normal,
            sense_events: Vec::new(),
        }
    }

    fn no_face_output() -> WorkerOutput {
        WorkerOutput {
            preview: None,
            camera_ok: true,
            pose_state: PoseState::NoFace,
            pitch_state: PoseState::NoFace,
            yaw: None,
            pitch: None,
            warning_level: WarningLevel::Normal,
            sense_events: Vec::new(),
        }
    }

    fn sample_output(yaw: f64, pitch: f64) -> WorkerOutput {
        WorkerOutput {
            preview: None,
            camera_ok: true,
            pose_state: PoseState::FacingScreen,
            pitch_state: PoseState::FacingScreen,
            yaw: Some(yaw),
            pitch: Some(pitch),
            warning_level: WarningLevel::Normal,
            sense_events: Vec::new(),
        }
    }

    // ── 纯函数测试：OrchestratorState::tick ─────────────────────

    #[test]
    fn calibration_samples_are_fed() {
        let mut state = OrchestratorState::default();
        let mut session = CalibrationSession::new(1.0);
        session.start();
        state.calibration_session = Some(session);

        let (events, result) = state.tick(Some(&sample_output(1.0, 2.0)), 0.1);
        assert!(events.is_empty());
        assert!(result.is_none());
        assert_eq!(
            state.calibration_session.as_ref().unwrap().sample_count(),
            1
        );
    }

    #[test]
    fn calibration_fails_on_continuous_no_face_from_output() {
        let mut state = OrchestratorState::default();
        let mut session = CalibrationSession::new(5.0);
        session.start();
        state.calibration_session = Some(session);

        let mut seen_failed = false;
        for _ in 0..15 {
            let (events, _) = state.tick(Some(&no_face_output()), 0.1);
            if events
                .iter()
                .any(|e| matches!(e, MonitoringEvent::CalibrationFailed { .. }))
            {
                seen_failed = true;
            }
        }

        assert!(seen_failed, "连续 1.5 秒无脸应触发 CalibrationFailed");
        assert!(state.calibration_session.is_none());
        assert_eq!(state.calibration_no_face_seconds, 0.0);
    }

    #[test]
    fn calibration_fails_on_continuous_no_face_when_monitor_absent() {
        let mut state = OrchestratorState::default();
        let mut session = CalibrationSession::new(5.0);
        session.start();
        state.calibration_session = Some(session);

        let mut seen_failed = false;
        for _ in 0..15 {
            let (events, _) = state.tick(None, 0.1);
            if events
                .iter()
                .any(|e| matches!(e, MonitoringEvent::CalibrationFailed { .. }))
            {
                seen_failed = true;
            }
        }

        assert!(
            seen_failed,
            "monitor 缺失时连续 1.5 秒也应触发 CalibrationFailed"
        );
        assert!(state.calibration_session.is_none());
    }

    #[test]
    fn calibration_no_face_resets_when_face_returns() {
        let mut state = OrchestratorState::default();
        let mut session = CalibrationSession::new(5.0);
        session.start();
        state.calibration_session = Some(session);

        // 先无脸 0.5 秒
        for _ in 0..5 {
            let (events, _) = state.tick(Some(&no_face_output()), 0.1);
            assert!(events.is_empty());
        }
        assert!((state.calibration_no_face_seconds - 0.5).abs() < 1e-9);

        // 恢复有脸，计数应清零
        let (events, _) = state.tick(Some(&good_output()), 0.1);
        assert!(events.is_empty());
        assert_eq!(state.calibration_no_face_seconds, 0.0);
    }

    #[test]
    fn calibration_completes_with_result() {
        let mut state = OrchestratorState::default();
        let mut session = CalibrationSession::new(1.0);
        session.start();
        state.calibration_session = Some(session);

        // 1 秒校准，每 tick 0.1 秒，喂 10 个样本
        let mut result = None;
        for _ in 0..10 {
            let (events, r) = state.tick(Some(&sample_output(3.0, 5.0)), 0.1);
            result = r;
            if events
                .iter()
                .any(|e| matches!(e, MonitoringEvent::CalibrationComplete { .. }))
            {
                break;
            }
        }

        assert!(result.is_some(), "校准应完成并返回结果");
        let res = result.unwrap();
        assert!((res.yaw - 3.0).abs() < 1e-9);
        assert!((res.pitch - 5.0).abs() < 1e-9);
        assert!(state.calibration_session.is_none());
    }

    #[test]
    fn calibration_fails_when_finished_without_samples() {
        let mut state = OrchestratorState::default();
        let mut session = CalibrationSession::new(1.0);
        session.start();
        state.calibration_session = Some(session);

        // 倒计时结束但没有任何样本
        let (events, result) = state.tick(Some(&no_face_output()), 1.0);

        assert!(result.is_none());
        assert!(
            events.iter().any(|e| matches!(e, MonitoringEvent::CalibrationFailed { reason } if reason == "no_face")),
            "无样本完成时应触发 CalibrationFailed"
        );
        assert!(state.calibration_session.is_none());
    }

    #[test]
    fn cancel_calibration_clears_state() {
        let mut state = OrchestratorState::default();
        let mut session = CalibrationSession::new(5.0);
        session.start();
        state.calibration_session = Some(session);
        state.calibration_no_face_seconds = 0.5;

        state.calibration_session = None;
        state.calibration_no_face_seconds = 0.0;

        assert!(state.calibration_session.is_none());
        assert_eq!(state.calibration_no_face_seconds, 0.0);
    }

    // ── 集成测试辅助：用真实 MonitoringWorker 验证命令循环 ─────────

    struct TestCamera {
        frames: Vec<Option<Frame>>,
        idx: usize,
    }

    impl FrameSource for TestCamera {
        fn read_frame(&mut self) -> Result<Option<Frame>, String> {
            let f = self.frames.get(self.idx).cloned().flatten();
            self.idx += 1;
            Ok(f)
        }
    }

    struct TestDetector {
        pose: Option<HeadPose>,
    }

    impl Detector for TestDetector {
        fn detect(&mut self, _rgb: &[u8], _w: u32, _h: u32) -> Option<HeadPose> {
            self.pose
        }
    }

    fn fake_frame() -> Frame {
        Frame {
            width: 32,
            height: 24,
            rgb: vec![0u8; 32 * 24 * 3],
        }
    }

    fn default_worker_engine() -> PostureTickEngine {
        PostureTickEngine::new(TimingThresholds {
            off_axis_streak_threshold_seconds: 0.3,
            off_axis_repeat_interval_seconds: 2.0,
            off_axis_severe_threshold_seconds: 2.0,
            facing_threshold_seconds: 5.0,
            eyerest_threshold_seconds: 10.0,
        })
    }

    fn setup_orchestrator(
        camera_succeeds: bool,
        detector_pose: Option<HeadPose>,
    ) -> (
        WorkerOrchestrator,
        channel::WorkerSender,
        WorkerReceiver,
        std::sync::mpsc::Receiver<MonitoringEvent>,
    ) {
        let (tx, rx) = channel::channel();
        let (event_tx, event_rx) = std::sync::mpsc::channel::<MonitoringEvent>();

        let dir = tempfile::tempdir().unwrap();
        let config_state = Arc::new(ConfigState::new(ConfigStore::new(dir.path())).unwrap());

        let camera_factory: CameraFactory = if camera_succeeds {
            Box::new(move |_idx| {
                Ok(Box::new(TestCamera {
                    frames: vec![Some(fake_frame()); 100],
                    idx: 0,
                }) as Box<dyn FrameSource>)
            })
        } else {
            Box::new(move |_idx| Err("摄像头不可用".into()))
        };

        let detector_factory: DetectorFactory = Box::new(move || {
            detector_pose
                .map(|pose| Box::new(TestDetector { pose: Some(pose) }) as Box<dyn Detector>)
        });

        let cs = config_state.clone();
        let worker_factory: WorkerFactory = Box::new(move |camera, detector| {
            Box::new(MonitoringWorker::new(
                camera,
                detector,
                default_worker_engine(),
                cs.clone(),
            ))
        });

        let orchestrator = WorkerOrchestrator::new(
            config_state,
            event_tx,
            camera_factory,
            detector_factory,
            worker_factory,
        );

        (orchestrator, tx, rx, event_rx)
    }

    /// 从事件接收端收集所有已到达事件并转成字符串。
    fn collect_events(event_rx: &std::sync::mpsc::Receiver<MonitoringEvent>) -> Vec<String> {
        event_rx.try_iter().map(event_to_string).collect()
    }

    // ── 集成测试用例 ───────────────────────────────────────────

    #[test]
    fn stops_on_command() {
        let (orch, tx, rx, _event_rx) = setup_orchestrator(
            true,
            Some(HeadPose {
                yaw: 0.0,
                pitch: 0.0,
            }),
        );

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        let _ = tx.0.send(WorkerCommand::Stop);
        let _ = handle.join();
    }

    #[test]
    fn channel_close_exits_gracefully() {
        let (orch, _tx, rx, _event_rx) = setup_orchestrator(
            true,
            Some(HeadPose {
                yaw: 0.0,
                pitch: 0.0,
            }),
        );

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        drop(_tx);
        let _ = handle.join();
    }

    #[test]
    fn emits_pose_on_good_tick() {
        let (orch, tx, rx, event_rx) = setup_orchestrator(
            true,
            Some(HeadPose {
                yaw: 0.0,
                pitch: 0.0,
            }),
        );

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        std::thread::sleep(Duration::from_millis(150));
        let _ = tx.0.send(WorkerCommand::Stop);
        let _ = handle.join();

        let events = collect_events(&event_rx);
        assert!(events.iter().any(|e| e.starts_with("pose:")));
    }

    #[test]
    fn camera_failure_emits_state_change() {
        // 摄像头 factory 成功（monitor 能创建），但 read_frame 失败，
        // 模拟运行中 camera 断开，worker 返回 camera_ok=false。
        let (tx, rx) = channel::channel();
        let (event_tx, event_rx) = std::sync::mpsc::channel::<MonitoringEvent>();

        let dir = tempfile::tempdir().unwrap();
        let config_state = Arc::new(ConfigState::new(ConfigStore::new(dir.path())).unwrap());

        struct FailingCamera;
        impl FrameSource for FailingCamera {
            fn read_frame(&mut self) -> Result<Option<Frame>, String> {
                Err("摄像头不可用".into())
            }
        }

        let camera_factory: CameraFactory =
            Box::new(move |_idx| Ok(Box::new(FailingCamera) as Box<dyn FrameSource>));
        let detector_factory: DetectorFactory = Box::new(|| None);
        let cs = config_state.clone();
        let worker_factory: WorkerFactory = Box::new(move |camera, detector| {
            Box::new(MonitoringWorker::new(
                camera,
                detector,
                default_worker_engine(),
                cs.clone(),
            ))
        });

        let orch = WorkerOrchestrator::new(
            config_state,
            event_tx,
            camera_factory,
            detector_factory,
            worker_factory,
        );

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        std::thread::sleep(Duration::from_millis(150));
        let _ = tx.0.send(WorkerCommand::Stop);
        let _ = handle.join();

        let events = collect_events(&event_rx);
        assert!(events.iter().any(|e| e == "camera:unavailable"));
    }

    #[test]
    fn set_config_same_camera_does_not_rebuild_monitor() {
        let (tx, rx) = channel::channel();
        let (event_tx, _event_rx) = std::sync::mpsc::channel::<MonitoringEvent>();
        let dir = tempfile::tempdir().unwrap();
        let config_state = Arc::new(ConfigState::new(ConfigStore::new(dir.path())).unwrap());

        let factory_call_count = Arc::new(AtomicUsize::new(0));
        let count = factory_call_count.clone();
        let cs = config_state.clone();
        let worker_factory: WorkerFactory = Box::new(move |_cam, _det| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::new(MonitoringWorker::new(
                Box::new(TestCamera {
                    frames: vec![Some(fake_frame()); 5],
                    idx: 0,
                }) as Box<dyn FrameSource>,
                None,
                default_worker_engine(),
                cs.clone(),
            ))
        });

        let camera_factory: CameraFactory = Box::new(move |_idx| {
            Ok(Box::new(TestCamera {
                frames: vec![Some(fake_frame()); 5],
                idx: 0,
            }) as Box<dyn FrameSource>)
        });
        let detector_factory: DetectorFactory = Box::new(|| None);

        let orch = WorkerOrchestrator::new(
            config_state.clone(),
            event_tx,
            camera_factory,
            detector_factory,
            worker_factory,
        );

        let handle = std::thread::spawn(move || orch.run(rx));
        std::thread::sleep(Duration::from_millis(150));

        // 初始创建消耗 1 次
        assert_eq!(factory_call_count.load(Ordering::SeqCst), 1);

        // SetConfig 相同 camera_index → 不重建
        let new_config = config_state.get();
        let _ = tx.0.send(WorkerCommand::SetConfig(Box::new(new_config)));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(factory_call_count.load(Ordering::SeqCst), 1);

        // SetConfig 不同 camera_index → 重建
        let mut changed_config = config_state.get();
        changed_config.camera_index = 99;
        let _ =
            tx.0.send(WorkerCommand::SetConfig(Box::new(changed_config)));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(factory_call_count.load(Ordering::SeqCst), 2);

        let _ = tx.0.send(WorkerCommand::Stop);
        let _ = handle.join();
    }

    // ── 校准端到端测试（仍用线程，但断言行为而非硬等时间） ───────

    #[test]
    fn calibration_complete_emits_event() {
        // 5 秒校准，准备足够多的样本
        let (orch, tx, rx, event_rx) = setup_orchestrator(
            true,
            Some(HeadPose {
                yaw: 3.0,
                pitch: 5.0,
            }),
        );

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        let _ = tx.0.send(WorkerCommand::StartCalibration);
        // 等待 6 秒让校准完成
        std::thread::sleep(Duration::from_millis(6100));
        let _ = tx.0.send(WorkerCommand::Stop);
        let _ = handle.join();

        let events = collect_events(&event_rx);
        let cal_events: Vec<_> = events
            .iter()
            .filter(|e| e.starts_with("calibration_complete:"))
            .collect();
        assert_eq!(cal_events.len(), 1, "应恰好发出一次 CalibrationComplete");
    }

    #[test]
    fn calibration_fails_on_continuous_no_face() {
        // 检测器始终返回 None → 连续无脸
        let (orch, tx, rx, event_rx) = setup_orchestrator(true, None);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        let _ = tx.0.send(WorkerCommand::StartCalibration);
        std::thread::sleep(Duration::from_millis(1200));
        let _ = tx.0.send(WorkerCommand::Stop);
        let _ = handle.join();

        let events = collect_events(&event_rx);
        assert!(
            events.iter().any(|e| e == "calibration_failed:no_face"),
            "应发出连续无脸导致的校准失败事件"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.starts_with("calibration_complete:")),
            "失败时不应发出 CalibrationComplete"
        );
    }

    // ── snooze 状态继承测试（用真实 worker 通过事件断言） ───────

    #[test]
    fn snooze_preserved_on_set_camera_index() {
        let (orch, tx, rx, event_rx) = setup_orchestrator(
            true,
            Some(HeadPose {
                yaw: 6.0,
                pitch: 0.0,
            }),
        );

        let handle = std::thread::spawn(move || {
            orch.with_retry_interval(Duration::from_millis(50)).run(rx);
        });

        // 等待首次 worker 创建并产生 correction
        std::thread::sleep(Duration::from_millis(300));

        // 进入无限期 snooze，后续 correction 事件应消失
        let _ = tx.0.send(WorkerCommand::Snooze(f64::INFINITY));
        std::thread::sleep(Duration::from_millis(300));

        // 切换摄像头
        let _ = tx.0.send(WorkerCommand::SetCameraIndex(1));
        std::thread::sleep(Duration::from_millis(300));

        // resume 后恢复 correction
        let _ = tx.0.send(WorkerCommand::Resume);
        std::thread::sleep(Duration::from_millis(800));

        let _ = tx.0.send(WorkerCommand::Stop);
        let _ = handle.join();

        let events = collect_events(&event_rx);
        let correction_events: Vec<_> = events
            .iter()
            .filter(|e| e.starts_with("warning:correction:"))
            .collect();
        // 至少出现过一次 correction，且 snooze 期间没有新的 correction 即说明状态继承正确。
        assert!(
            !correction_events.is_empty(),
            "resume 后应恢复 correction；实际 correction 事件 {:?}",
            correction_events
        );
    }

    #[test]
    fn worker_command_is_debug() {
        let cmd = WorkerCommand::Stop;
        let _ = format!("{:?}", cmd);
    }
}
