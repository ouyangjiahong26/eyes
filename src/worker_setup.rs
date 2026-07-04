//! 后台 worker 启动。
//!
//! VS1：用 `BevyEventSink` 把监控事件通过 mpsc channel 推到 Bevy 主线程。
//! 调用方（lib.rs setup system）先建好 channel，把 `Sender` 传进来。

use std::sync::{mpsc::Sender, Arc};

#[cfg(all(feature = "opencv-camera", feature = "onnx-detector"))]
use std::path::PathBuf;
#[cfg(all(feature = "opencv-camera", feature = "onnx-detector"))]
use crate::app_shell::platform::install_dir;

use crate::app_state::{AppState, SharedAppState};
use crate::domain::config::ConfigState;
use crate::domain::paths;
use crate::domain::posture_tick_engine::PostureTickEngine;
use crate::monitoring::channel::{self, WorkerSender};
use crate::monitoring::event_sink::BevyEventSink;
use crate::monitoring::events::MonitoringEvent;
use crate::monitoring::orchestrator::{
    CameraFactory, DetectorFactory, MonitorFactory, WorkerOrchestrator,
};
use crate::monitoring::worker::MonitoringWorker;

/// 启动后台监控 worker，返回命令发送端。
///
/// `event_tx` 是 mpsc 的发送端，worker 产出的 `MonitoringEvent` 会通过
/// `BevyEventSink` 推到这个 channel；Bevy 主线程用对应的 `Receiver`
/// 在 Update 系统里消费。
pub fn spawn_worker(
    config_state: Arc<ConfigState>,
    event_tx: Sender<MonitoringEvent>,
) -> WorkerSender {
    let (tx, rx) = channel::channel();

    let shared_state: SharedAppState = Arc::new(std::sync::Mutex::new(AppState::new()));

    // 相机工厂（feature-gated）。默认 feature 下返回错误。
    let camera_factory: CameraFactory = Box::new(move |camera_index: u32| {
        #[cfg(feature = "opencv-camera")]
        {
            use crate::monitoring::opencv_camera::OpenCvCamera;
            let cam = OpenCvCamera::open(camera_index as i32)?;
            Ok(Box::new(cam))
        }

        #[cfg(not(feature = "opencv-camera"))]
        {
            let _ = camera_index;
            Err("No camera backend available".into())
        }
    });

    // 检测器工厂。
    let detector_factory: DetectorFactory = Box::new(|| {
        #[cfg(all(feature = "opencv-camera", feature = "onnx-detector"))]
        {
            load_onnx_detector()
        }
        #[cfg(not(all(feature = "opencv-camera", feature = "onnx-detector")))]
        {
            None
        }
    });

    // 监控器工厂
    let cs = config_state.clone();
    let monitor_factory: MonitorFactory = Box::new(move |camera, detector| {
        let engine = PostureTickEngine::default();
        Box::new(MonitoringWorker::new(camera, detector, engine, cs.clone()))
    });

    let orchestrator = WorkerOrchestrator::new(
        config_state,
        shared_state,
        Box::new(BevyEventSink::new(event_tx)),
        camera_factory,
        detector_factory,
        monitor_factory,
    );

    std::thread::Builder::new()
        .name("eyes-worker".into())
        .spawn(move || {
            orchestrator.run(rx);
        })
        .expect("spawn worker thread");

    tx
}

/// 加载用户配置目录，构造 `ConfigState`。
///
/// 失败时回退到临时目录（仅在配置目录不可写时）。
pub fn load_config_state() -> Arc<ConfigState> {
    let config_dir = paths::app_config_dir(dirs::config_dir());
    let store = crate::domain::config::ConfigStore::new(config_dir);
    Arc::new(ConfigState::new(store).expect("加载配置失败"))
}

#[cfg(all(feature = "opencv-camera", feature = "onnx-detector"))]
fn load_onnx_detector() -> Option<Box<dyn crate::monitoring::detector::Detector>> {
    use crate::monitoring::detector::Detector;
    use crate::monitoring::onnx_detector::YuNetDetector;

    let path = resolve_model_path()?;
    match YuNetDetector::new(path.to_str().unwrap_or("")) {
        Ok(detector) => Some(Box::new(detector) as Box<dyn Detector>),
        Err(e) => {
            eprintln!("[eyes] 加载 ONNX 检测器失败（路径={}）：{e}", path.display());
            None
        }
    }
}

#[cfg(all(feature = "opencv-camera", feature = "onnx-detector"))]
/// 定位 YuNet ONNX 模型文件。
///
/// 按以下顺序尝试：
/// 1. `<exe_dir>/models/face_detection_yunet_2023mar.onnx`（MSI 安装态）
/// 2. `<exe_dir>/../../models/face_detection_yunet_2023mar.onnx`（cargo run 开发态）
/// 3. `<cwd>/models/face_detection_yunet_2023mar.onnx`（任意工作目录兜底）
fn resolve_model_path() -> Option<PathBuf> {
    const MODEL_NAME: &str = "face_detection_yunet_2023mar.onnx";

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(dir) = install_dir() {
        candidates.push(dir.join("models").join(MODEL_NAME));
        if let Ok(dev_path) = dir.join("..").join("..").join("models").join(MODEL_NAME).canonicalize() {
            candidates.push(dev_path);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("models").join(MODEL_NAME));
    }

    for path in candidates {
        if path.exists() {
            return Some(path);
        }
    }

    eprintln!("[eyes] 找不到 ONNX 模型文件 {MODEL_NAME}；检测功能将不可用");
    None
}
