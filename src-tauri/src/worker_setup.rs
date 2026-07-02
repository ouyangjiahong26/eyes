use std::sync::Arc;

use tauri::{AppHandle, Emitter};

use crate::app_state::SharedAppState;
use crate::domain::config::ConfigState;
use crate::domain::event_log::EventLog;
use crate::domain::posture_tick_engine::PostureTickEngine;
use crate::monitoring::worker::MonitoringWorker;
use crate::monitoring::channel::{WorkerSender, channel};
use crate::monitoring::events::{EventSink, MonitoringEvent};
use crate::monitoring::orchestrator::{
    CameraFactory, DetectorFactory, MonitorFactory, WorkerOrchestrator,
};

// ── Tauri EventSink ──────────────────────────────────────────────

struct TauriEventSink {
    app: AppHandle,
    event_log: Arc<EventLog>,
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: MonitoringEvent) {
        match event {
            MonitoringEvent::CameraStateChanged { state } => {
                let _ = self
                    .app
                    .emit("camera-state-changed", serde_json::json!({ "state": state }));
            }
            MonitoringEvent::PoseUpdated {
                yaw,
                pitch,
                pose_state,
            } => {
                let _ = self.app.emit(
                    "pose-updated",
                    serde_json::json!({
                        "pose_state": pose_state,
                        "yaw": yaw,
                        "pitch": pitch,
                    }),
                );
            }
            MonitoringEvent::PreviewFrame(preview) => {
                let _ = self.app.emit(
                    "preview-frame",
                    serde_json::json!({
                        "image_data_url": preview.image_data_url,
                        "width": preview.width,
                        "height": preview.height,
                    }),
                );
            }
            MonitoringEvent::SoundAlert { alert_type } => {
                let _ = self.app.emit("play-sound-alert", alert_type);
            }
            MonitoringEvent::WarningLevelChanged { level, direction } => {
                let _ = self.app.emit(
                    "warning-level-changed",
                    serde_json::json!({ "level": level, "direction": direction }),
                );
            }
            MonitoringEvent::LogEvent { kind, data } => {
                let _ = self.event_log.append(kind, data);
            }
            MonitoringEvent::LogInfo { message } => {
                eprintln!("[eyes] {}", message);
            }
            MonitoringEvent::CalibrationComplete { yaw, pitch, sample_count } => {
                let _ = self.app.emit(
                    "calibration-complete",
                    serde_json::json!({
                        "yaw": yaw,
                        "pitch": pitch,
                        "sample_count": sample_count,
                    }),
                );
            }
            MonitoringEvent::CalibrationFailed { reason } => {
                let _ = self.app.emit(
                    "calibration-failed",
                    serde_json::json!({ "reason": reason }),
                );
            }
        }
    }
}

// ── 后台 worker ─────────────────────────────────────────────────

/// 启动后台监控 worker。返回 `WorkerSender`，前端命令通过它向 worker 发指令。
pub fn spawn_worker(
    app_handle: AppHandle,
    config_state: Arc<ConfigState>,
    shared_state: SharedAppState,
) -> WorkerSender {
    let (tx, rx) = channel();

    let config_dir = crate::domain::paths::app_config_dir(dirs::config_dir());
    let event_log = Arc::new(EventLog::new(config_dir.clone()));

    // 相机工厂（feature-gated）
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

    // 检测器工厂
    let detector_factory: DetectorFactory = {
        let app_handle = app_handle.clone();
        Box::new(move || load_detector(&app_handle))
    };

    // 监控器工厂
    let cs = config_state.clone();
    let monitor_factory: MonitorFactory = Box::new(move |camera, detector| {
        let engine = PostureTickEngine::default();
        Box::new(MonitoringWorker::new(camera, detector, engine, cs.clone()))
    });

    let orchestrator = WorkerOrchestrator::new(
        config_state,
        shared_state,
        Box::new(TauriEventSink { app: app_handle, event_log: event_log.clone() }),
        camera_factory,
        detector_factory,
        monitor_factory,
    );

    std::thread::spawn(move || {
        orchestrator.run(rx);
    });

    tx
}

// ── 检测器加载（feature-gated） ──────────────────────────────────

#[cfg(all(feature = "opencv-camera", feature = "onnx-detector"))]
fn load_detector(app_handle: &AppHandle) -> Option<Box<dyn crate::monitoring::detector::Detector>> {
    let resource_dir = app_handle.path().resource_dir().ok()?;
    let model_path = resource_dir.join("face_detection_yunet_2023mar.onnx");
    if !model_path.exists() {
        return None;
    }
    let path_str = model_path.to_string_lossy().to_string();
    match crate::monitoring::onnx_detector::YuNetDetector::new(&path_str) {
        Ok(det) => Some(Box::new(det)),
        Err(_) => None,
    }
}

#[cfg(all(feature = "opencv-camera", not(feature = "onnx-detector")))]
fn load_detector(_app_handle: &AppHandle) -> Option<Box<dyn crate::monitoring::detector::Detector>> {
    None
}

#[cfg(not(feature = "opencv-camera"))]
fn load_detector(_app_handle: &AppHandle) -> Option<Box<dyn crate::monitoring::detector::Detector>> {
    None
}
