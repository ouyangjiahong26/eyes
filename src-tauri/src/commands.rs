use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

use crate::app_shell::desktop::rebuild_tray;
use crate::app_state::{CameraState, SharedAppState};
use crate::autostart::apply_autostart;
use crate::domain::config::{AppConfig, ConfigState};
use crate::domain::snooze::{self, SnoozeState};
use crate::monitoring::camera_enumerator;
use crate::monitoring::channel::{WorkerCommand, WorkerSender};

// ── Tauri 命令 ───────────────────────────────────────────────────

#[tauri::command]
pub fn get_status(state: State<'_, SharedAppState>) -> Result<serde_json::Value, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "pose_state": app.status.pose_state,
        "yaw": app.status.yaw,
        "pitch": app.status.pitch,
        "warning_level": app.status.warning_level,
        "camera_state": app.status.camera_state,
        "snooze_state": app.status.snooze_state,
        "calibration_active": app.status.calibration_active,
    }))
}

#[tauri::command]
pub fn get_config(config_state: State<'_, Arc<ConfigState>>) -> Result<AppConfig, String> {
    Ok(config_state.get())
}

#[tauri::command]
pub fn set_config(
    new_config: AppConfig,
    config_state: State<'_, Arc<ConfigState>>,
    worker_tx: State<'_, Mutex<WorkerSender>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let old_config = config_state.get();
    let autostart_changed =
        old_config.autostart_enabled != new_config.autostart_enabled;

    config_state.set(new_config.clone()).map_err(|e| e.to_string())?;

    if autostart_changed {
        apply_autostart(&app_handle, new_config.autostart_enabled);
    }

    if let Ok(tx) = worker_tx.lock() {
        let _ = tx.send(WorkerCommand::SetConfig(Box::new(new_config.clone())));
    }

    let _ = app_handle.emit("config-changed", ());
    rebuild_tray(&app_handle, &new_config.language);

    Ok(())
}

// ── 摄像头 ──────────────────────────────────────────────────────

#[tauri::command]
pub fn list_cameras() -> Result<Vec<camera_enumerator::CameraDevice>, String> {
    camera_enumerator::list_cameras()
}

#[tauri::command]
pub fn set_camera_index(
    index: u32,
    state: State<'_, SharedAppState>,
    config_state: State<'_, Arc<ConfigState>>,
    worker_tx: State<'_, Mutex<WorkerSender>>,
) -> Result<(), String> {
    config_state
        .update(|cfg| cfg.camera_index = index)
        .map_err(|e| e.to_string())?;
    {
        let mut app = state.lock().map_err(|e| e.to_string())?;
        app.status.camera_state = CameraState::Starting;
    }
    let tx = worker_tx.lock().map_err(|e| e.to_string())?;
    tx.send(WorkerCommand::SetCameraIndex(index))?;
    Ok(())
}

// ── 暂停/恢复 ───────────────────────────────────────────────────

#[tauri::command]
pub fn snooze(
    minutes: u32,
    state: State<'_, SharedAppState>,
    worker_tx: State<'_, Mutex<WorkerSender>>,
) -> Result<(), String> {
    {
        let mut app = state.lock().map_err(|e| e.to_string())?;
        app.status.snooze_state = snooze::compute_snooze_from_minutes(minutes);
    }
    let seconds = if minutes == 0 {
        f64::INFINITY
    } else {
        (minutes as f64) * 60.0
    };
    let tx = worker_tx.lock().map_err(|e| e.to_string())?;
    tx.send(WorkerCommand::Snooze(seconds))?;
    Ok(())
}

#[tauri::command]
pub fn resume(
    state: State<'_, SharedAppState>,
    worker_tx: State<'_, Mutex<WorkerSender>>,
) -> Result<(), String> {
    {
        let mut app = state.lock().map_err(|e| e.to_string())?;
        app.status.snooze_state = SnoozeState::Inactive;
    }
    let tx = worker_tx.lock().map_err(|e| e.to_string())?;
    tx.send(WorkerCommand::Resume)?;
    Ok(())
}

// ── 校准 ────────────────────────────────────────────────────────

#[tauri::command]
pub fn start_calibration(
    state: State<'_, SharedAppState>,
    worker_tx: State<'_, Mutex<WorkerSender>>,
) -> Result<(), String> {
    {
        let mut app = state.lock().map_err(|e| e.to_string())?;
        app.status.calibration_active = true;
    }
    let tx = worker_tx.lock().map_err(|e| e.to_string())?;
    tx.send(WorkerCommand::StartCalibration)?;
    Ok(())
}

#[tauri::command]
pub fn feed_calibration(
    _yaw: f64,
    _pitch: f64,
) -> Result<(), String> {
    // 校准样本由 orchestrator 在 tick 中自动喂入，前端调用保留为 no-op。
    Ok(())
}

#[tauri::command]
pub fn cancel_calibration(
    state: State<'_, SharedAppState>,
    worker_tx: State<'_, Mutex<WorkerSender>>,
) -> Result<(), String> {
    {
        let mut app = state.lock().map_err(|e| e.to_string())?;
        app.status.calibration_active = false;
    }
    let tx = worker_tx.lock().map_err(|e| e.to_string())?;
    tx.send(WorkerCommand::CancelCalibration)?;
    Ok(())
}
