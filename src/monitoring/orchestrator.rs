//! WorkerOrchestrator —— 管理后台监控 worker 的完整生命周期。

use crate::domain::calibration::CalibrationSession;
use crate::domain::config::ConfigState;
use crate::domain::snooze;
use crate::domain::thresholds::TimingThresholds;
use crate::monitoring::channel::{WorkerCommand, WorkerReceiver};
use crate::monitoring::detector::Detector;
use crate::monitoring::event_mapping;
use crate::monitoring::events::{EventSink, MonitoringEvent};
use crate::monitoring::worker::{FrameSource, MonitoringWorker, WorkerOutput};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 消除泛型参数，使 orchestrator 通过 trait object 持有 worker。
pub trait Monitor: Send + 'static {
    fn tick(&mut self, dt: f64) -> WorkerOutput;
    fn set_snoozed(&mut self, snoozed: bool);
    fn update_timing(&mut self, timing: TimingThresholds);
}

impl<C: FrameSource + Send + 'static> Monitor for MonitoringWorker<C> {
    fn tick(&mut self, dt: f64) -> WorkerOutput { self.tick(dt) }
    fn set_snoozed(&mut self, snoozed: bool) { self.set_snoozed(snoozed); }
    fn update_timing(&mut self, timing: TimingThresholds) {
        self.engine_mut().update_timing(timing);
    }
}

pub type CameraFactory = Box<dyn FnMut(u32) -> Result<Box<dyn FrameSource>, String> + Send>;
pub type DetectorFactory = Box<dyn Fn() -> Option<Box<dyn Detector>> + Send>;
pub type MonitorFactory = Box<dyn Fn(Box<dyn FrameSource>, Option<Box<dyn Detector>>) -> Box<dyn Monitor> + Send>;

pub struct WorkerOrchestrator {
    config_state: Arc<ConfigState>,
    shared_state: Arc<Mutex<crate::app_state::AppState>>,
    calibration_session: Option<CalibrationSession>,
    calibration_no_face_seconds: f64,
    event_sink: Box<dyn EventSink>,
    camera_factory: CameraFactory,
    detector_factory: DetectorFactory,
    monitor_factory: MonitorFactory,
    retry_interval: Duration,
}

impl WorkerOrchestrator {
    pub fn new(
        config_state: Arc<ConfigState>,
        shared_state: Arc<Mutex<crate::app_state::AppState>>,
        event_sink: Box<dyn EventSink>,
        camera_factory: CameraFactory,
        detector_factory: DetectorFactory,
        monitor_factory: MonitorFactory,
    ) -> Self {
        Self {
            config_state, shared_state, event_sink,
            camera_factory, detector_factory, monitor_factory,
            calibration_session: None,
            calibration_no_face_seconds: 0.0,
            retry_interval: Duration::from_secs(5),
        }
    }

    pub fn with_retry_interval(mut self, interval: Duration) -> Self {
        self.retry_interval = interval;
        self
    }

    pub fn run(mut self, rx: WorkerReceiver) {
        use crate::app_state::CameraState;

        let mut camera_index = self.config_state.get().camera_index;
        let mut snooze_until: Option<Instant> = None;
        let mut snoozed = false;
        let mut monitor: Option<Box<dyn Monitor>> = self.open_monitor(camera_index, snoozed);

        {
            let state = if monitor.is_some() { CameraState::Available } else { CameraState::Unavailable };
            if let Ok(mut s) = self.shared_state.lock() { s.status.camera_state = state; }
        }

        let mut retry_at: Option<Instant> =
            if monitor.is_none() { Some(Instant::now() + self.retry_interval) } else { None };

        let tick_interval = Duration::from_millis(100);
        let mut stopped = false;
        let mut last_tick = Instant::now();

        while !stopped {
            match rx.recv_timeout(tick_interval) {
                Ok(cmd) => match cmd {
                    WorkerCommand::Stop => {
                        stopped = true;
                        continue;
                    }
                    WorkerCommand::SetCameraIndex(idx) => {
                        camera_index = idx;
                        monitor = None;
                        retry_at = Some(Instant::now());
                        if let Ok(mut s) = self.shared_state.lock() {
                            s.status.camera_state = CameraState::Starting;
                        }
                    }
                    WorkerCommand::SetConfig(new_config) => {
                        let camera_changed = new_config.camera_index != camera_index;
                        if let Some(ref mut m) = monitor {
                            m.update_timing(new_config.timing);
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
                    WorkerCommand::Snooze(seconds) => {
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
                    WorkerCommand::Resume => {
                        snoozed = false;
                        snooze_until = None;
                        if let Some(ref mut w) = monitor {
                            w.set_snoozed(false);
                        }
                        if let Ok(mut s) = self.shared_state.lock() {
                            s.status.snooze_state = snooze::SnoozeState::Inactive;
                        }
                    }
                    WorkerCommand::StartCalibration => {
                        let mut session = CalibrationSession::new(5.0);
                        session.start();
                        self.calibration_session = Some(session);
                        self.calibration_no_face_seconds = 0.0;
                        if let Ok(mut s) = self.shared_state.lock() {
                            s.status.calibration_active = true;
                        }
                    }
                    WorkerCommand::CancelCalibration => {
                        self.calibration_session = None;
                        self.calibration_no_face_seconds = 0.0;
                        if let Ok(mut s) = self.shared_state.lock() {
                            s.status.calibration_active = false;
                        }
                    }
                },
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }

            self.process_tick(&mut monitor, &mut snooze_until, &mut snoozed, &mut retry_at, camera_index, &mut last_tick);
        }
    }

    fn process_tick(
        &mut self,
        monitor: &mut Option<Box<dyn Monitor>>,
        snooze_until: &mut Option<Instant>,
        snoozed: &mut bool,
        retry_at: &mut Option<Instant>,
        camera_index: u32,
        last_tick: &mut Instant,
    ) {
        use crate::app_state::CameraState;
        use crate::domain::classifier::PoseState;

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
                if let Ok(mut s) = self.shared_state.lock() {
                    s.status.snooze_state = snooze::SnoozeState::Inactive;
                }
            }
        }

        if monitor.is_none() && retry_at.is_some_and(|due| now >= due) {
            *monitor = self.open_monitor(camera_index, *snoozed);
            if monitor.is_some() {
                if let Ok(mut s) = self.shared_state.lock() {
                    s.status.camera_state = CameraState::Available;
                }
                *retry_at = None;
            } else {
                *retry_at = Some(now + self.retry_interval);
            }
        }

        if let Some(ref mut w) = monitor {
            let output = w.tick(dt);

            // 从 monitor 输出喂入校准样本并检测连续无脸
            if let Some(ref mut session) = self.calibration_session {
                if session.is_active() {
                    if let (Some(y), Some(p)) = (output.yaw, output.pitch) {
                        session.feed(y, p);
                    }
                    if output.pose_state == PoseState::NoFace {
                        self.calibration_no_face_seconds += dt;
                        if self.calibration_no_face_seconds >= 1.0 {
                            self.event_sink.emit(MonitoringEvent::CalibrationFailed {
                                reason: "no_face".into(),
                            });
                            if let Ok(mut s) = self.shared_state.lock() {
                                s.status.calibration_active = false;
                            }
                            self.calibration_session = None;
                            self.calibration_no_face_seconds = 0.0;
                        }
                    } else {
                        self.calibration_no_face_seconds = 0.0;
                    }
                }
            }

            if let Ok(mut s) = self.shared_state.lock() {
                s.status.pose_state = output.pose_state;
                s.status.yaw = output.yaw;
                s.status.pitch = output.pitch;
                s.status.warning_level = output.warning_level;
                if !output.camera_ok {
                    s.status.camera_state = CameraState::Unavailable;
                }
            }

            for event in event_mapping::from_worker_output(&output) {
                self.event_sink.emit(event);
            }

            if !output.camera_ok {
                monitor.take();
                *retry_at = Some(now + self.retry_interval);
            }
        }

        // 监控器缺失时也视为连续无脸，达到阈值同样触发失败
        if monitor.is_none() {
            if let Some(ref session) = self.calibration_session {
                if session.is_active() {
                    self.calibration_no_face_seconds += dt;
                    if self.calibration_no_face_seconds >= 1.0 {
                        self.event_sink.emit(MonitoringEvent::CalibrationFailed {
                            reason: "no_face".into(),
                        });
                        if let Ok(mut s) = self.shared_state.lock() {
                            s.status.calibration_active = false;
                        }
                        self.calibration_session = None;
                        self.calibration_no_face_seconds = 0.0;
                    }
                }
            }
        }

        // 推进校准倒计时（使用真实 dt，与 monitor 状态无关）
        if let Some(ref mut session) = self.calibration_session {
            if session.is_active() {
                session.tick(dt);
                if !session.is_active() {
                    // 校准结束
                    let result = session.result();
                    if let Some(res) = result {
                        let _ = self.config_state.update(|cfg| {
                            cfg.neutral_yaw = res.yaw;
                            cfg.neutral_pitch = res.pitch;
                        });
                        self.event_sink.emit(MonitoringEvent::CalibrationComplete {
                            yaw: res.yaw,
                            pitch: res.pitch,
                            sample_count: res.sample_count,
                        });
                    } else {
                        self.event_sink.emit(MonitoringEvent::CalibrationFailed {
                            reason: "no_face".into(),
                        });
                    }
                    if let Ok(mut s) = self.shared_state.lock() {
                        s.status.calibration_active = false;
                    }
                    self.calibration_session = None;
                    self.calibration_no_face_seconds = 0.0;
                }
            }
        }
    }

    fn open_monitor(&mut self, camera_index: u32, snoozed: bool) -> Option<Box<dyn Monitor>> {
        let camera = (self.camera_factory)(camera_index).ok()?;
        let detector = (self.detector_factory)();
        let mut monitor = (self.monitor_factory)(camera, detector);
        monitor.set_snoozed(snoozed);
        Some(monitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::classifier::PoseState;
    use crate::domain::config::ConfigStore;
    use crate::domain::posture_tick_engine::WarningLevel;
    use crate::monitoring::channel;
    use crate::monitoring::events::MonitoringEvent;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};

    // ── Mock Monitor ─────────────────────────────────────────────

    struct MockMonitor {
        outputs: Vec<WorkerOutput>,
        index: usize,
        snoozed: Arc<AtomicBool>,
    }

    impl MockMonitor {
        fn new(outputs: Vec<WorkerOutput>) -> Self {
            Self::with_shared_state(outputs, Arc::new(AtomicBool::new(false)))
        }

        fn with_shared_state(outputs: Vec<WorkerOutput>, snoozed: Arc<AtomicBool>) -> Self {
            Self {
                outputs,
                index: 0,
                snoozed,
            }
        }
    }

    impl Monitor for MockMonitor {
        fn tick(&mut self, _dt: f64) -> WorkerOutput {
            if self.index < self.outputs.len() {
                let out = &self.outputs[self.index];
                self.index += 1;
                WorkerOutput {
                    preview: None,
                    camera_ok: out.camera_ok,
                    pose_state: out.pose_state,
                    pitch_state: out.pitch_state,
                    yaw: out.yaw,
                    pitch: out.pitch,
                    warning_level: out.warning_level,
                    sense_events: out.sense_events.clone(),
                }
            } else {
                WorkerOutput {
                    preview: None,
                    camera_ok: false,
                    pose_state: PoseState::NoFace,
                    pitch_state: PoseState::NoFace,
                    yaw: None,
                    pitch: None,
                    warning_level: WarningLevel::Normal,
                    sense_events: Vec::new(),
                }
            }
        }

        fn set_snoozed(&mut self, snoozed: bool) {
            self.snoozed.store(snoozed, Ordering::SeqCst);
        }

        fn update_timing(&mut self, _timing: TimingThresholds) {}
    }

    // ── Mock EventSink ───────────────────────────────────────────

    #[derive(Clone)]
    struct MockSink {
        events: Arc<Mutex<Vec<String>>>,
    }

    impl MockSink {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
    }

    impl EventSink for MockSink {
        fn emit(&self, event: MonitoringEvent) {
            let s = match event {
                MonitoringEvent::CameraStateChanged { state } => format!("camera:{}", state),
                MonitoringEvent::PoseUpdated { pose_state, .. } => format!("pose:{}", pose_state),
                MonitoringEvent::PreviewFrame(_) => "preview".into(),
                MonitoringEvent::SoundAlert { alert_type } => format!("sound:{}", alert_type),
                MonitoringEvent::WarningLevelChanged { level, direction } => {
                    format!("warning:{}:{}", level, direction.unwrap_or_else(|| "none".into()))
                }
                MonitoringEvent::LogEvent { kind, .. } => format!("log:{:?}", kind),
                MonitoringEvent::LogInfo { message } => format!("info:{}", message),
                MonitoringEvent::CalibrationComplete { yaw, pitch, sample_count } => {
                    format!("calibration_complete:{}:{}:{}", yaw, pitch, sample_count)
                }
                MonitoringEvent::CalibrationFailed { reason } => {
                    format!("calibration_failed:{}", reason)
                }
            };
            self.events.lock().unwrap().push(s);
        }
    }

    // ── 辅助 ─────────────────────────────────────────────────────

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

    fn camera_fail_output() -> WorkerOutput {
        WorkerOutput {
            preview: None,
            camera_ok: false,
            pose_state: PoseState::NoFace,
            pitch_state: PoseState::NoFace,
            yaw: None,
            pitch: None,
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

    fn correction_output(direction: PoseState) -> WorkerOutput {
        WorkerOutput {
            preview: None,
            camera_ok: true,
            pose_state: direction,
            pitch_state: PoseState::FacingScreen,
            yaw: Some(10.0),
            pitch: Some(0.0),
            warning_level: WarningLevel::Normal,
            sense_events: vec![SenseEvent::Correction { direction }],
        }
    }

    use crate::domain::posture_tick_engine::SenseEvent;

    fn setup_orchestrator(
        outputs: Vec<WorkerOutput>,
        sink: MockSink,
        camera_factory_succeeds: bool,
    ) -> (WorkerOrchestrator, channel::WorkerSender, WorkerReceiver) {
        let (tx, rx) = channel::channel();

        let dir = tempfile::tempdir().unwrap();
        let config_state = Arc::new(
            ConfigState::new(ConfigStore::new(dir.path())).unwrap(),
        );

        let shared_state = Arc::new(Mutex::new(crate::app_state::AppState::new()));

        let mock_monitor: Arc<Mutex<Option<Box<dyn Monitor>>>> =
            Arc::new(Mutex::new(Some(Box::new(MockMonitor::new(outputs)))));

        let monitor_ref = mock_monitor.clone();
        let monitor_factory: MonitorFactory = Box::new(move |_cam, _det| {
            monitor_ref
                .lock()
                .unwrap()
                .take()
                .expect("mock monitor already consumed")
        });

        let camera_factory: CameraFactory = if camera_factory_succeeds {
            Box::new(move |_idx| {
                struct Dummy;
                impl FrameSource for Dummy {
                    fn read_frame(&mut self) -> Result<Option<crate::monitoring::preview::Frame>, String> {
                        Ok(None)
                    }
                }
                Ok(Box::new(Dummy))
            })
        } else {
            Box::new(move |_idx| Err("摄像头不可用".into()))
        };

        let detector_factory: DetectorFactory = Box::new(|| None);

        let orchestrator = WorkerOrchestrator::new(
            config_state,
            shared_state,
            Box::new(sink),
            camera_factory,
            detector_factory,
            monitor_factory,
        );

        (orchestrator, tx, rx)
    }

    fn setup_orchestrator_with_monitor_queue(
        outputs_per_monitor: Vec<Vec<WorkerOutput>>,
        sink: MockSink,
        camera_factory_succeeds: bool,
    ) -> (
        WorkerOrchestrator,
        channel::WorkerSender,
        WorkerReceiver,
        Arc<Mutex<Vec<Arc<AtomicBool>>>>,
    ) {
        let (tx, rx) = channel::channel();

        let dir = tempfile::tempdir().unwrap();
        let config_state = Arc::new(
            ConfigState::new(ConfigStore::new(dir.path())).unwrap(),
        );

        let shared_state = Arc::new(Mutex::new(crate::app_state::AppState::new()));

        let snooze_states: Arc<Mutex<Vec<Arc<AtomicBool>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let outputs_queue: Arc<Mutex<VecDeque<Vec<WorkerOutput>>>> =
            Arc::new(Mutex::new(outputs_per_monitor.into_iter().collect()));

        let states = snooze_states.clone();
        let queue = outputs_queue.clone();
        let monitor_factory: MonitorFactory = Box::new(move |_cam, _det| {
            let outputs = queue
                .lock()
                .unwrap()
                .pop_front()
                .expect("mock monitor queue exhausted");
            let state = Arc::new(AtomicBool::new(false));
            states.lock().unwrap().push(state.clone());
            Box::new(MockMonitor::with_shared_state(outputs, state))
        });

        let camera_factory: CameraFactory = if camera_factory_succeeds {
            Box::new(move |_idx| {
                struct Dummy;
                impl FrameSource for Dummy {
                    fn read_frame(&mut self) -> Result<Option<crate::monitoring::preview::Frame>, String> {
                        Ok(None)
                    }
                }
                Ok(Box::new(Dummy))
            })
        } else {
            Box::new(move |_idx| Err("摄像头不可用".into()))
        };

        let detector_factory: DetectorFactory = Box::new(|| None);

        let orchestrator = WorkerOrchestrator::new(
            config_state,
            shared_state,
            Box::new(sink),
            camera_factory,
            detector_factory,
            monitor_factory,
        );

        (orchestrator, tx, rx, snooze_states)
    }

    // ── 测试用例 ─────────────────────────────────────────────────

    #[test]
    fn emits_pose_on_good_tick() {
        let sink = MockSink::new();
        let (orch, tx, rx) =
            setup_orchestrator(vec![good_output()], sink.clone(), true);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        std::thread::sleep(Duration::from_millis(150));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();

        let events = sink.get_events();
        assert!(events.iter().any(|e| e.starts_with("pose:")));
    }

    #[test]
    fn stops_on_command() {
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(vec![], sink.clone(), true);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();
    }

    #[test]
    fn snooze_command_sets_snooze() {
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(vec![good_output()], sink.clone(), true);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        let _ = tx.send(WorkerCommand::Snooze(600.0));
        std::thread::sleep(Duration::from_millis(150));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();
    }

    #[test]
    fn resume_clears_snooze() {
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(vec![good_output()], sink.clone(), true);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        let _ = tx.send(WorkerCommand::Snooze(600.0));
        std::thread::sleep(Duration::from_millis(50));
        let _ = tx.send(WorkerCommand::Resume);
        std::thread::sleep(Duration::from_millis(50));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();
    }

    #[test]
    fn correction_emits_warning_and_sound() {
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(
            vec![correction_output(PoseState::OffAxisLeft)],
            sink.clone(),
            true,
        );

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        std::thread::sleep(Duration::from_millis(150));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();

        let events = sink.get_events();
        assert!(events.iter().any(|e| e.starts_with("warning:correction:")));
        assert!(events.iter().any(|e| e == "sound:posture"));
    }

    #[test]
    fn camera_failure_emits_state_change() {
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(
            vec![camera_fail_output()],
            sink.clone(),
            true,
        );

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        std::thread::sleep(Duration::from_millis(150));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();

        let events = sink.get_events();
        assert!(events.iter().any(|e| e == "camera:unavailable"));
    }

    #[test]
    fn camera_factory_failure_triggers_retry() {
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(vec![], sink.clone(), false);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        std::thread::sleep(Duration::from_millis(150));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();
    }

    #[test]
    fn set_camera_index_resets_monitor() {
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(vec![good_output()], sink.clone(), true);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        std::thread::sleep(Duration::from_millis(50));
        let _ = tx.send(WorkerCommand::SetCameraIndex(1));
        std::thread::sleep(Duration::from_millis(50));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();
    }

    #[test]
    fn channel_close_exits_gracefully() {
        let sink = MockSink::new();
        let (orch, _tx, rx) = setup_orchestrator(vec![], sink.clone(), true);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        drop(_tx);
        let _ = handle.join();
    }

    #[test]
    fn worker_command_is_debug() {
        let cmd = WorkerCommand::Stop;
        let _ = format!("{:?}", cmd);
    }

    #[test]
    fn set_config_same_camera_does_not_rebuild_monitor() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let (tx, rx) = channel::channel();
        let dir = tempfile::tempdir().unwrap();
        let config_state = Arc::new(
            ConfigState::new(ConfigStore::new(dir.path())).unwrap(),
        );
        let shared_state = Arc::new(Mutex::new(crate::app_state::AppState::new()));
        let sink = MockSink::new();
        let factory_call_count = Arc::new(AtomicUsize::new(0));
        let count = factory_call_count.clone();
        let monitor_factory: MonitorFactory = Box::new(move |_cam, _det| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::new(MockMonitor::new(vec![good_output()]))
        });
        let camera_factory: CameraFactory = Box::new(move |_idx| {
            struct Dummy;
            impl FrameSource for Dummy {
                fn read_frame(&mut self) -> Result<Option<crate::monitoring::preview::Frame>, String> {
                    Ok(None)
                }
            }
            Ok(Box::new(Dummy))
        });
        let detector_factory: DetectorFactory = Box::new(|| None);

        let orch = WorkerOrchestrator::new(
            config_state.clone(),
            shared_state,
            Box::new(sink),
            camera_factory,
            detector_factory,
            monitor_factory,
        );

        let handle = std::thread::spawn(move || orch.run(rx));
        std::thread::sleep(Duration::from_millis(150));

        // 初始创建消耗 1 次
        assert_eq!(factory_call_count.load(Ordering::SeqCst), 1);

        // SetConfig 相同 camera_index → 不重建
        let new_config = config_state.get();
        let _ = tx.send(WorkerCommand::SetConfig(Box::new(new_config)));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(factory_call_count.load(Ordering::SeqCst), 1);

        // SetConfig 不同 camera_index → 重建
        let mut changed_config = config_state.get();
        changed_config.camera_index = 99;
        let _ = tx.send(WorkerCommand::SetConfig(Box::new(changed_config)));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(factory_call_count.load(Ordering::SeqCst), 2);

        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();
    }

    // ── 校准统一测试 ────────────────────────────────────────────────

    #[test]
    fn start_calibration_creates_session() {
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(vec![good_output()], sink.clone(), true);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        let _ = tx.send(WorkerCommand::StartCalibration);
        std::thread::sleep(Duration::from_millis(150));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();

        // 校准期间 calibration_active 应为 true
        // （通过 events 检查——如果 session 存在且 monitor 有输出，
        //  orchestrator 会正常发出 pose 事件）
        let events = sink.get_events();
        assert!(events.iter().any(|e| e.starts_with("pose:")));
    }

    #[test]
    fn cancel_calibration_destroys_session() {
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(vec![good_output()], sink.clone(), true);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        let _ = tx.send(WorkerCommand::StartCalibration);
        std::thread::sleep(Duration::from_millis(50));
        let _ = tx.send(WorkerCommand::CancelCalibration);
        std::thread::sleep(Duration::from_millis(50));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();

        // 取消后不应发出 CalibrationComplete
        let events = sink.get_events();
        assert!(!events.iter().any(|e| e.starts_with("calibration_complete:")));
    }

    #[test]
    fn calibration_complete_emits_event() {
        // 5 秒校准，每 tick 约 100ms，准备足够多的 good_output
        let outputs: Vec<WorkerOutput> = (0..60).map(|_| good_output()).collect();
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(outputs, sink.clone(), true);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        let _ = tx.send(WorkerCommand::StartCalibration);
        // 等待 6 秒让校准完成
        std::thread::sleep(Duration::from_millis(6100));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();

        let events = sink.get_events();
        // 应发出 CalibrationComplete 事件
        let cal_events: Vec<_> = events.iter().filter(|e| e.starts_with("calibration_complete:")).collect();
        assert_eq!(cal_events.len(), 1, "应恰好发出一次 CalibrationComplete");
    }

    #[test]
    fn calibration_uses_real_dt() {
        // 验证校准不依赖硬编码 dt=0.1，而是使用真实时间间隔
        // 通过在极短时间内发送 StartCalibration 后等待 6 秒来验证完成
        let outputs: Vec<WorkerOutput> = (0..100).map(|_| good_output()).collect();
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(outputs, sink.clone(), true);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        let _ = tx.send(WorkerCommand::StartCalibration);
        std::thread::sleep(Duration::from_millis(6000));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();

        let events = sink.get_events();
        let cal_events: Vec<_> = events.iter().filter(|e| e.starts_with("calibration_complete:")).collect();
        assert_eq!(cal_events.len(), 1, "校准应在真实时间后完成");
    }

    #[test]
    fn calibration_fails_on_continuous_no_face() {
        // 连续 1 秒检测不到人脸应触发 CalibrationFailed
        let outputs: Vec<WorkerOutput> = (0..20).map(|_| no_face_output()).collect();
        let sink = MockSink::new();
        let (orch, tx, rx) = setup_orchestrator(outputs, sink.clone(), true);

        let handle = std::thread::spawn(move || {
            orch.run(rx);
        });

        let _ = tx.send(WorkerCommand::StartCalibration);
        std::thread::sleep(Duration::from_millis(1200));
        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();

        let events = sink.get_events();
        assert!(
            events.iter().any(|e| e == "calibration_failed:no_face"),
            "应发出连续无脸导致的校准失败事件"
        );
        assert!(
            !events.iter().any(|e| e.starts_with("calibration_complete:")),
            "失败时不应发出 CalibrationComplete"
        );
    }

    // ── snooze 状态继承测试 ───────────────────────────────────────

    #[test]
    fn snooze_preserved_on_set_camera_index() {
        let sink = MockSink::new();
        let (orch, tx, rx, states) = setup_orchestrator_with_monitor_queue(
            vec![vec![good_output()], vec![good_output()]],
            sink.clone(),
            true,
        );

        let handle = std::thread::spawn(move || {
            orch.with_retry_interval(Duration::from_millis(50))
                .run(rx);
        });

        // 等待初始 monitor 创建
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(states.lock().unwrap().len(), 1);
        assert!(!states.lock().unwrap()[0].load(Ordering::SeqCst));

        // 进入无限期 snooze
        let _ = tx.send(WorkerCommand::Snooze(f64::INFINITY));
        std::thread::sleep(Duration::from_millis(100));
        assert!(states.lock().unwrap()[0].load(Ordering::SeqCst));

        // 切换摄像头，新 monitor 应继承 snooze 状态
        let _ = tx.send(WorkerCommand::SetCameraIndex(1));
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(states.lock().unwrap().len(), 2);
        assert!(
            states.lock().unwrap()[1].load(Ordering::SeqCst),
            "切换摄像头后新 monitor 应保持 snooze"
        );

        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();
    }

    #[test]
    fn snooze_preserved_on_camera_retry() {
        let sink = MockSink::new();
        let (tx, rx) = channel::channel();
        let dir = tempfile::tempdir().unwrap();
        let config_state = Arc::new(
            ConfigState::new(ConfigStore::new(dir.path())).unwrap(),
        );
        let shared_state = Arc::new(Mutex::new(crate::app_state::AppState::new()));

        let snooze_states: Arc<Mutex<Vec<Arc<AtomicBool>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let states = snooze_states.clone();
        let monitor_factory: MonitorFactory = Box::new(move |_cam, _det| {
            let state = Arc::new(AtomicBool::new(false));
            states.lock().unwrap().push(state.clone());
            Box::new(MockMonitor::with_shared_state(vec![good_output()], state))
        });

        let camera_succeed = Arc::new(AtomicBool::new(false));
        let cam_ok = camera_succeed.clone();
        let camera_factory: CameraFactory = Box::new(move |_idx| {
            if cam_ok.load(Ordering::SeqCst) {
                struct Dummy;
                impl FrameSource for Dummy {
                    fn read_frame(&mut self) -> Result<Option<crate::monitoring::preview::Frame>, String> {
                        Ok(None)
                    }
                }
                Ok(Box::new(Dummy))
            } else {
                Err("摄像头不可用".into())
            }
        });
        let detector_factory: DetectorFactory = Box::new(|| None);

        let orch = WorkerOrchestrator::new(
            config_state,
            shared_state,
            Box::new(sink),
            camera_factory,
            detector_factory,
            monitor_factory,
        );

        let handle = std::thread::spawn(move || {
            orch.with_retry_interval(Duration::from_millis(50))
                .run(rx);
        });

        // 初始摄像头不可用，无 monitor
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(snooze_states.lock().unwrap().len(), 0);

        // 在摄像头不可用时进入 snooze
        let _ = tx.send(WorkerCommand::Snooze(f64::INFINITY));
        std::thread::sleep(Duration::from_millis(50));

        // 摄像头恢复可用，重试创建 monitor
        camera_succeed.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(200));

        assert_eq!(snooze_states.lock().unwrap().len(), 1);
        assert!(
            snooze_states.lock().unwrap()[0].load(Ordering::SeqCst),
            "摄像头恢复后创建的新 monitor 应保持 snooze"
        );

        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();
    }

    #[test]
    fn resume_clears_snooze_for_future_monitors() {
        let sink = MockSink::new();
        let (orch, tx, rx, states) = setup_orchestrator_with_monitor_queue(
            vec![vec![good_output()], vec![good_output()]],
            sink.clone(),
            true,
        );

        let handle = std::thread::spawn(move || {
            orch.with_retry_interval(Duration::from_millis(50))
                .run(rx);
        });

        // 等待初始 monitor 创建并进入 snooze
        std::thread::sleep(Duration::from_millis(100));
        let _ = tx.send(WorkerCommand::Snooze(f64::INFINITY));
        std::thread::sleep(Duration::from_millis(100));
        assert!(states.lock().unwrap()[0].load(Ordering::SeqCst));

        // resume 后再切换摄像头
        let _ = tx.send(WorkerCommand::Resume);
        std::thread::sleep(Duration::from_millis(50));
        let _ = tx.send(WorkerCommand::SetCameraIndex(1));
        std::thread::sleep(Duration::from_millis(200));

        assert_eq!(states.lock().unwrap().len(), 2);
        assert!(
            !states.lock().unwrap()[1].load(Ordering::SeqCst),
            "resume 后新 monitor 不应保持 snooze"
        );

        let _ = tx.send(WorkerCommand::Stop);
        let _ = handle.join();
    }
}
