//! 后台 worker 启动。
//!
//! 调用方（lib.rs setup system）先建好 mpsc channel，把 `Sender` 传进来；
//! worker 产出的 `MonitoringEvent` 直接通过这个 channel 推到 Bevy 主线程，
//! 由 `forward_monitoring_events` 系统消费。

use std::sync::{mpsc::Sender, Arc};

use crate::domain::config::ConfigState;
use crate::domain::posture_tick_engine::PostureTickEngine;
use crate::monitoring::events::MonitoringEvent;
use crate::monitoring::pipeline::channel::{self, WorkerSender};
use crate::monitoring::pipeline::orchestrator::{
    CameraFactory, DetectorFactory, WorkerFactory, WorkerOrchestrator,
};
use crate::monitoring::pipeline::worker::MonitoringWorker;

/// 启动后台监控 worker，返回命令发送端。
///
/// `event_tx` 是 mpsc 的发送端，worker 产出的 `MonitoringEvent` 会推到
/// 这个 channel；Bevy 主线程用对应的 `Receiver` 在 Update 系统里消费。
pub fn spawn_worker(
    config_state: Arc<ConfigState>,
    event_tx: Sender<MonitoringEvent>,
) -> WorkerSender {
    let (tx, rx) = channel::channel();

    // 相机工厂（feature-gated）。默认 feature 下返回错误。
    let camera_factory: CameraFactory = Box::new(move |camera_index: u32| {
        #[cfg(feature = "opencv-camera")]
        {
            use crate::monitoring::camera::opencv_camera::OpenCvCamera;
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
            crate::monitoring::vision::onnx_detector::load_onnx_detector()
        }
        #[cfg(not(all(feature = "opencv-camera", feature = "onnx-detector")))]
        {
            None
        }
    });

    // worker 工厂
    let cs = config_state.clone();
    let worker_factory: WorkerFactory = Box::new(move |camera, detector| {
        let engine = PostureTickEngine::default();
        Box::new(MonitoringWorker::new(camera, detector, engine, cs.clone()))
    });

    let orchestrator = WorkerOrchestrator::new(
        config_state,
        event_tx,
        camera_factory,
        detector_factory,
        worker_factory,
    );

    std::thread::Builder::new()
        .name("eyes-worker".into())
        .spawn(move || {
            orchestrator.run(rx);
        })
        .expect("spawn worker thread");

    tx
}
