use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::AutoLaunchManager;

/// 根据配置启用或禁用系统开机自启。
///
/// 非 Windows 平台若插件调用失败，仅记录错误，不向上传播，
/// 因此前端开关不会导致应用报错。
pub fn apply_autostart(app_handle: &AppHandle, enabled: bool) {
    let manager = app_handle.state::<AutoLaunchManager>();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };

    if let Err(e) = result {
        eprintln!("应用开机自启设置失败 (enabled={enabled}): {e}");
    }
}
