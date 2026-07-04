use std::sync::Arc;

use super::detector::Detector;
use super::preview::{encode_preview, Frame, PreviewFrame};
use crate::domain::classifier::{self, NeutralPose, PoseClassification, PoseState, Thresholds};
use crate::domain::config::ConfigState;
use crate::domain::posture_tick_engine::{PostureTickEngine, SenseEvent, WarningLevel};

/// 帧来源（摄像头）。
pub trait FrameSource: Send + 'static {
    fn read_frame(&mut self) -> Result<Option<Frame>, String>;
}

/// 允许 `Box<dyn FrameSource>` 作为 `FrameSource` 使用。
impl<S: FrameSource + ?Sized> FrameSource for Box<S> {
    fn read_frame(&mut self) -> Result<Option<Frame>, String> {
        (**self).read_frame()
    }
}

/// 监控 worker 单次 tick 的输出。
#[derive(Debug)]
pub struct WorkerOutput {
    pub preview: Option<PreviewFrame>,
    pub camera_ok: bool,
    pub pose_state: PoseState,
    pub pitch_state: PoseState,
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    pub warning_level: WarningLevel,
    pub sense_events: Vec<SenseEvent>,
}

struct WorkerState {
    prev_classification: PoseClassification,
}

/// 监控 worker：摄像头 → 检测 → 分类 → tick 引擎 → 事件。
///
/// 每次 `tick()` 消耗一帧，返回 `WorkerOutput`。
pub struct MonitoringWorker<C> {
    camera: C,
    detector: Option<Box<dyn Detector>>,
    engine: PostureTickEngine,
    config_state: Arc<ConfigState>,
    state: WorkerState,
    snoozed: bool,
}

impl<C: FrameSource> MonitoringWorker<C> {
    pub fn new(
        camera: C,
        detector: Option<Box<dyn Detector>>,
        engine: PostureTickEngine,
        config_state: Arc<ConfigState>,
    ) -> Self {
        Self {
            camera,
            detector,
            engine,
            config_state,
            state: WorkerState {
                prev_classification: PoseClassification::default(),
            },
            snoozed: false,
        }
    }

    pub fn engine_mut(&mut self) -> &mut PostureTickEngine {
        &mut self.engine
    }

    pub fn set_snoozed(&mut self, snoozed: bool) {
        self.snoozed = snoozed;
    }

    pub fn tick(&mut self, dt: f64) -> WorkerOutput {
        let frame = match self.camera.read_frame() {
            Ok(Some(f)) => f,
            Ok(None) | Err(_) => return self.no_frame_output(),
        };

        let (width, height) = (frame.width, frame.height);
        let preview = encode_preview(&frame.mirror_horizontal()).ok();

        // 检测
        let detected_pose = self
            .detector
            .as_mut()
            .and_then(|d| d.detect(&frame.rgb, width, height));

        // 分类
        let config = self.config_state.get();
        let thresholds = Thresholds {
            yaw_deg: config.yaw_threshold,
            yaw_hysteresis_deg: config.yaw_hysteresis,
            pitch_deg: config.pitch_threshold,
            pitch_hysteresis_deg: config.pitch_hysteresis,
        };
        let neutral = NeutralPose {
            yaw: config.neutral_yaw,
            pitch: config.neutral_pitch,
        };
        let classification = classifier::classify(
            detected_pose,
            Some(neutral),
            Some(thresholds),
            Some(self.state.prev_classification),
        );
        self.state.prev_classification = classification;

        // tick 引擎（snoozed 时跳过事件生成）
        let sense_events = if self.snoozed {
            Vec::new()
        } else {
            self.engine
                .tick(classification.yaw_state, classification.pitch_state, dt)
        };

        WorkerOutput {
            preview,
            camera_ok: true,
            pose_state: classification.yaw_state,
            pitch_state: classification.pitch_state,
            yaw: detected_pose.map(|p| p.yaw),
            pitch: detected_pose.map(|p| p.pitch),
            warning_level: self.engine.warning_level(),
            sense_events,
        }
    }

    fn no_frame_output(&self) -> WorkerOutput {
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

// ── 测试 ──────────────────────────────────────────────────────

#[cfg(test)]
pub struct FakeDetector {
    pub pose: Option<crate::domain::classifier::HeadPose>,
}

#[cfg(test)]
impl Detector for FakeDetector {
    fn detect(&mut self, _rgb: &[u8], _w: u32, _h: u32) -> Option<crate::domain::classifier::HeadPose> {
        self.pose
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::classifier::HeadPose;
    use crate::domain::config::ConfigStore;
    use crate::domain::posture_tick_engine::SenseEvent;

    struct FakeCamera {
        frames: Vec<Option<Frame>>,
        idx: usize,
    }

    impl FrameSource for FakeCamera {
        fn read_frame(&mut self) -> Result<Option<Frame>, String> {
            let f = self.frames.get(self.idx).cloned().flatten();
            self.idx += 1;
            Ok(f)
        }
    }

    fn fake_frame(w: u32, h: u32) -> Frame {
        Frame {
            rgb: vec![0u8; (w * h * 3) as usize],
            width: w,
            height: h,
        }
    }

    fn make_worker(
        frames: Vec<Option<Frame>>,
        pose: Option<HeadPose>,
    ) -> MonitoringWorker<FakeCamera> {
        make_worker_with_config(frames, pose, Arc::new(
            ConfigState::new(ConfigStore::new(tempfile::tempdir().unwrap().path())).unwrap(),
        ))
    }

    fn make_worker_with_config(
        frames: Vec<Option<Frame>>,
        pose: Option<HeadPose>,
        config_state: Arc<ConfigState>,
    ) -> MonitoringWorker<FakeCamera> {
        let camera = FakeCamera { frames, idx: 0 };
        let det: Option<Box<dyn Detector>> = pose.map(|p| {
            Box::new(FakeDetector { pose: Some(p) }) as Box<dyn Detector>
        });
        MonitoringWorker::new(
            camera,
            det,
            PostureTickEngine::default(),
            config_state,
        )
    }

    #[test]
    fn no_frame_returns_camera_not_ok() {
        let mut w = make_worker(vec![None], None);
        let out = w.tick(0.1);
        assert!(!out.camera_ok);
        assert_eq!(out.pose_state, PoseState::NoFace);
    }

    #[test]
    fn frame_without_detector_returns_no_face() {
        let mut w = make_worker(vec![Some(fake_frame(640, 480))], None);
        let out = w.tick(0.1);
        assert!(out.camera_ok);
        assert_eq!(out.pose_state, PoseState::NoFace);
    }

    #[test]
    fn facing_screen_detected() {
        let mut w = make_worker(
            vec![Some(fake_frame(640, 480))],
            Some(HeadPose { yaw: 0.0, pitch: 0.0 }),
        );
        let out = w.tick(0.1);
        assert!(out.camera_ok);
        assert_eq!(out.pose_state, PoseState::FacingScreen);
        assert_eq!(out.yaw, Some(0.0));
    }

    #[test]
    fn off_axis_right_triggers_correction() {
        let frames = vec![Some(fake_frame(640, 480)); 20];
        let mut w = make_worker(frames, Some(HeadPose { yaw: 6.0, pitch: 0.0 }));

        let mut got_correction = false;
        for _ in 0..20 {
            let out = w.tick(0.1);
            for ev in &out.sense_events {
                if matches!(ev, SenseEvent::Correction { .. }) {
                    got_correction = true;
                }
            }
        }
        assert!(got_correction, "should trigger correction after ~0.3s");
    }

    #[test]
    fn snooze_suppresses_sense_events() {
        let cam = FakeCamera {
            frames: vec![Some(fake_frame(640, 480))],
            idx: 0,
        };
        let det: Option<Box<dyn Detector>> =
            Some(Box::new(FakeDetector { pose: Some(HeadPose { yaw: 6.0, pitch: 0.0 }) }));
        let cs = Arc::new(
            ConfigState::new(ConfigStore::new(tempfile::tempdir().unwrap().path())).unwrap(),
        );
        let mut w = MonitoringWorker::new(cam, det, PostureTickEngine::default(), cs);
        w.set_snoozed(true);
        let out = w.tick(0.1);
        assert!(out.sense_events.is_empty());
        assert_eq!(out.pose_state, PoseState::OffAxisRight);
    }

    #[test]
    fn no_face_resets_classification() {
        let frames = vec![
            Some(fake_frame(640, 480)),
            Some(fake_frame(640, 480)),
        ];
        // 第一帧有脸，第二帧无脸
        let mut w = make_worker(frames, Some(HeadPose { yaw: 0.0, pitch: 0.0 }));
        let out1 = w.tick(0.1);
        assert_eq!(out1.pose_state, PoseState::FacingScreen);

        // 移除 detector → 模拟无脸
        w.detector = None;
        let out2 = w.tick(0.1);
        assert_eq!(out2.pose_state, PoseState::NoFace);
    }

    #[test]
    fn classify_uses_user_configured_thresholds() {
        let dir = tempfile::tempdir().unwrap();
        let cs = Arc::new(ConfigState::new(ConfigStore::new(dir.path())).unwrap());
        // 默认 yaw_threshold=5.0，yaw=6.0 会判为 OffAxisRight
        let mut w = make_worker_with_config(
            vec![Some(fake_frame(640, 480))],
            Some(HeadPose { yaw: 6.0, pitch: 0.0 }),
            cs.clone(),
        );
        let out = w.tick(0.1);
        assert_eq!(out.pose_state, PoseState::OffAxisRight);

        // 放宽阈值到 20.0，yaw=6.0 应判为 FacingScreen
        cs.update(|c| c.yaw_threshold = 20.0).unwrap();
        let mut w2 = make_worker_with_config(
            vec![Some(fake_frame(640, 480))],
            Some(HeadPose { yaw: 6.0, pitch: 0.0 }),
            cs.clone(),
        );
        let out2 = w2.tick(0.1);
        assert_eq!(out2.pose_state, PoseState::FacingScreen);
    }

    #[test]
    fn classify_uses_user_configured_neutral_pose() {
        let dir = tempfile::tempdir().unwrap();
        let cs = Arc::new(ConfigState::new(ConfigStore::new(dir.path())).unwrap());
        // 中性偏航=0，默认阈值=5.0，yaw=4.0 → FacingScreen
        let mut w = make_worker_with_config(
            vec![Some(fake_frame(640, 480))],
            Some(HeadPose { yaw: 4.0, pitch: 0.0 }),
            cs.clone(),
        );
        let out = w.tick(0.1);
        assert_eq!(out.pose_state, PoseState::FacingScreen);

        // 中性偏航=-5.0，yaw=4.0 → 偏离=9.0 > 5.0 → OffAxisRight
        cs.update(|c| c.neutral_yaw = -5.0).unwrap();
        let mut w2 = make_worker_with_config(
            vec![Some(fake_frame(640, 480))],
            Some(HeadPose { yaw: 4.0, pitch: 0.0 }),
            cs.clone(),
        );
        let out2 = w2.tick(0.1);
        assert_eq!(out2.pose_state, PoseState::OffAxisRight);
    }
}
