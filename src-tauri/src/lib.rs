pub mod app_shell;
pub mod app_state;
pub mod autostart;
pub mod commands;
pub mod domain;
pub mod monitoring;
pub mod worker_setup;

use std::sync::{Arc, Mutex};

use app_shell::desktop::{create_tray, handle_second_instance, handle_window_event};
use app_state::AppState;
use autostart::apply_autostart;
use commands::{
    cancel_calibration, feed_calibration, get_config, get_status, list_cameras, resume,
    set_camera_index, set_config, snooze, start_calibration,
};
use worker_setup::spawn_worker;
use domain::config::ConfigState;
use tauri::Manager;

/// 将 Tauri 资源目录加入 DLL 搜索路径。
///
/// Windows 默认搜索路径不包含 `resources/` 子目录。
/// 通过 `SetDllDirectory` 添加后，Windows loader 和 `dlopen2`
/// （`LoadLibraryW` 底层）都能找到 `onnxruntime.dll` 和 `opencv_world*.dll`。
/// 此函数在 setup 闭包中调用，此时为单线程环境。
#[cfg(target_os = "windows")]
fn add_resource_dll_dir(app_handle: &tauri::AppHandle) {
    #[link(name = "kernel32")]
    extern "system" {
        fn SetDllDirectoryW(lpPathName: *const u16) -> i32;
    }
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = OsStr::new(resource_dir.as_os_str())
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        unsafe { SetDllDirectoryW(wide.as_ptr()) };
    }
}

#[cfg(not(target_os = "windows"))]
fn add_resource_dll_dir(_app_handle: &tauri::AppHandle) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_dir = domain::paths::app_config_dir(dirs::config_dir());
    let config_state = Arc::new(
        ConfigState::new(domain::config::ConfigStore::new(config_dir))
            .expect("加载配置失败"),
    );
    let language = config_state.get().language.clone();
    let shared_state: app_state::SharedAppState = Arc::new(Mutex::new(AppState::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            handle_second_instance(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(shared_state.clone())
        .manage(config_state.clone())
        .setup(move |app| {
            add_resource_dll_dir(app.handle());
            create_tray(app, &language)?;

            let initial_autostart = config_state.get().autostart_enabled;
            apply_autostart(app.handle(), initial_autostart);

            let worker_tx = spawn_worker(
                app.handle().clone(),
                config_state,
                shared_state,
            );
            app.manage(Mutex::new(worker_tx));
            Ok(())
        })
        .on_window_event(handle_window_event)
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_config,
            set_config,
            list_cameras,
            set_camera_index,
            snooze,
            resume,
            start_calibration,
            feed_calibration,
            cancel_calibration,
        ])
        .run(tauri::generate_context!())
        .expect("运行 Eyes 时出错");
}
