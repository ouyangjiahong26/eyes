use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::domain::classifier::PoseState;
use crate::domain::posture_tick_engine::WarningLevel;
use crate::domain::snooze::SnoozeState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CameraState {
    Starting,
    Available,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct StatusSnapshot {
    pub pose_state: PoseState,
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    pub warning_level: WarningLevel,
    pub camera_state: CameraState,
    pub snooze_state: SnoozeState,
    pub calibration_active: bool,
}

#[derive(Debug)]
pub struct AppState {
    pub status: StatusSnapshot,
}

/// 跨线程共享的 snooze 标志。
pub type SharedSnooze = Arc<AtomicBool>;

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            status: StatusSnapshot {
                pose_state: PoseState::NoFace,
                yaw: None,
                pitch: None,
                warning_level: WarningLevel::Normal,
                camera_state: CameraState::Starting,
                snooze_state: SnoozeState::Inactive,
                calibration_active: false,
            },
        }
    }
}

pub type SharedAppState = Arc<Mutex<AppState>>;
