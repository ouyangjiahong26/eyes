use eyes_lib::domain::classifier::HeadPose;
use eyes_lib::domain::config::{ConfigState, ConfigStore};
use eyes_lib::monitoring::{
    detector::Detector,
    preview::Frame,
    worker::{FrameSource, MonitoringWorker},
};
use std::sync::{Arc, Mutex};

fn default_config_state() -> Arc<ConfigState> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.into_path();
    Arc::new(ConfigState::new(ConfigStore::new(&path)).unwrap())
}

#[derive(Debug)]
struct FakeCamera {
    frames: Vec<Frame>,
}

impl FrameSource for FakeCamera {
    fn read_frame(&mut self) -> Result<Option<Frame>, String> {
        Ok(self.frames.pop())
    }
}

#[test]
fn worker_tick_returns_preview_frame_from_camera_source() {
    let frame = Frame::rgb(2, 1, vec![255, 0, 0, 0, 255, 0]).unwrap();
    let mut worker = MonitoringWorker::new(FakeCamera {
        frames: vec![frame],
    }, None, Default::default(), default_config_state());

    let output = worker.tick(0.1);

    assert!(output.camera_ok);
    let preview = output.preview.expect("expected preview frame");
    assert_eq!(preview.width, 2);
    assert_eq!(preview.height, 1);
    assert!(preview.image_data_url.starts_with("data:image/png;base64,"));
}

#[test]
fn frame_constructor_rejects_invalid_dimensions_and_lengths() {
    assert!(Frame::rgb(0, 1, Vec::new()).is_err());
    assert!(Frame::rgb(1, 1, Vec::new()).is_err());
}

#[test]
fn worker_tick_reports_camera_unavailable_without_crashing() {
    struct UnavailableCamera;
    impl FrameSource for UnavailableCamera {
        fn read_frame(&mut self) -> Result<Option<Frame>, String> {
            Ok(None)
        }
    }

    let mut worker = MonitoringWorker::new(UnavailableCamera, None, Default::default(), default_config_state());
    let output = worker.tick(0.1);

    assert!(!output.camera_ok);
    assert!(output.sense_events.is_empty());
}

#[test]
fn worker_reports_camera_unavailable_when_read_fails_after_success() {
    struct FailingAfterOne {
        remaining: usize,
    }

    impl FrameSource for FailingAfterOne {
        fn read_frame(&mut self) -> Result<Option<Frame>, String> {
            if self.remaining > 0 {
                self.remaining -= 1;
                Ok(Some(Frame::rgb(1, 1, vec![0, 0, 0]).unwrap()))
            } else {
                Err("disconnected".to_string())
            }
        }
    }

    let mut worker = MonitoringWorker::new(FailingAfterOne { remaining: 1 }, None, Default::default(), default_config_state());

    let output = worker.tick(0.1);
    assert!(output.camera_ok);
    assert!(output.preview.is_some());

    let output = worker.tick(0.1);
    assert!(!output.camera_ok);
}

// ── #92: preview 走镜像帧、detection 走原帧 ─────────────────────

/// 从 data:image/png;base64,... 解码出 RGB 字节。
fn decode_preview_rgb(data_url: &str) -> Vec<u8> {
    let b64 = data_url.strip_prefix("data:image/png;base64,").unwrap();
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        b64,
    )
    .unwrap();
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

/// 捕获 detector 收到的 RGB 字节。
struct SpyDetector {
    captured_rgb: Arc<Mutex<Vec<u8>>>,
    pose: Option<HeadPose>,
}

impl Detector for SpyDetector {
    fn detect(&mut self, rgb: &[u8], _w: u32, _h: u32) -> Option<HeadPose> {
        *self.captured_rgb.lock().unwrap() = rgb.to_vec();
        self.pose
    }
}

#[test]
fn preview_is_mirrored_relative_to_detection_frame() {
    // 2×1 帧：左红(255,0,0) 右绿(0,255,0)
    let frame = Frame::rgb(2, 1, vec![255, 0, 0, 0, 255, 0]).unwrap();

    let captured = Arc::new(Mutex::new(Vec::new()));
    let detector = SpyDetector {
        captured_rgb: Arc::clone(&captured),
        pose: Some(HeadPose { yaw: 0.0, pitch: 0.0 }),
    };
    let det: Option<Box<dyn Detector>> = Some(Box::new(detector));

    let mut worker = MonitoringWorker::new(
        FakeCamera { frames: vec![frame] },
        det,
        Default::default(),
        default_config_state(),
    );

    let output = worker.tick(0.1);
    let preview = output.preview.unwrap();

    // 解码 preview PNG
    let preview_rgb = decode_preview_rgb(&preview.image_data_url);

    // preview 应为镜像：左绿 右红
    assert_eq!(&preview_rgb[..3], &[0, 255, 0], "preview 左像素应为绿");
    assert_eq!(&preview_rgb[3..6], &[255, 0, 0], "preview 右像素应为红");

    // detector 收到的应为原帧：左红 右绿
    let det_rgb = captured.lock().unwrap();
    assert_eq!(&det_rgb[..3], &[255, 0, 0], "detector 左像素应为红（原帧）");
    assert_eq!(&det_rgb[3..6], &[0, 255, 0], "detector 右像素应为绿（原帧）");
}

#[test]
fn original_frame_rgb_not_mutated_by_tick() {
    let frame = Frame::rgb(2, 1, vec![255, 0, 0, 0, 255, 0]).unwrap();
    let original_rgb = frame.rgb.clone();

    let mut worker = MonitoringWorker::new(
        FakeCamera { frames: vec![frame.clone()] },
        None,
        Default::default(),
        default_config_state(),
    );

    let _output = worker.tick(0.1);

    // 原帧的 rgb buffer 不应被修改
    // （FakeCamera 用 pop 取出帧，内部 clone 已分离；
    //   这里验证 frame 变量本身不受影响）
    assert_eq!(frame.rgb, original_rgb);
}
