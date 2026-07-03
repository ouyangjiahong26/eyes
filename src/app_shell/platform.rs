//! 平台相关的启动期基础设施。
//!
//! VS7（Windows 打包）需要两件事：
//!
//! 1. **DLL 搜索路径**：MSI 把 `onnxruntime.dll` / `opencv_world4100.dll`
//!    放在与 exe 同目录（安装目录）。Windows 默认搜索路径不一定包含该目录，
//!    用 `SetDllDirectoryW` 把它加进去，加载器（含 `dlopen2` / `LoadLibraryW`）
//!    才能找到这两个 DLL。
//! 2. **开机自启**：通过 `auto-launch` 在注册表
//!    `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 写入 / 删除项。
//!    设置面板（VS2）的开关调用这里的 `set_autostart`。
//!
//! 非 Windows 平台这些函数为 no-op，调用方无需条件编译。

use std::path::PathBuf;

/// 安装目录：exe 所在目录。
///
/// 开发态（`cargo run`）指向 `target/<profile>/`，此时 DLL / 模型通常不在
/// 该目录；MSI 安装态指向安装目录，DLL / 模型就在此处。
/// 拿不到 exe 路径时返回 `None`，调用方按失败处理。
pub fn install_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
}

/// 把安装目录加入 DLL 搜索路径（仅 Windows）。
///
/// 应在应用启动最早处调用——任何 `LoadLibrary` / 检测器初始化之前。
/// 调用失败只记日志，不阻塞启动：开发态没有 DLL 时也不应 panic。
#[cfg(target_os = "windows")]
pub fn add_resource_dll_dir() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetDllDirectoryW(lplibdirname: *const u16) -> i32;
    }

    let Some(dir) = install_dir() else {
        eprintln!("[eyes] 无法定位 exe 目录，跳过 DLL 搜索路径设置");
        return;
    };

    let wide: Vec<u16> = OsStr::new(&dir)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // 返回值非零表示成功；零表示失败（GetLastError 可查具体原因）。
    let ok = unsafe { SetDllDirectoryW(wide.as_ptr()) };
    if ok == 0 {
        eprintln!(
            "[eyes] SetDllDirectoryW 失败（目录={}），DLL 可能无法加载",
            dir.display()
        );
    }
}

#[cfg(not(target_os = "windows"))]
pub fn add_resource_dll_dir() {
    // 非 Windows 平台无需此步骤。
}

/// 开机自启设置结果。
#[derive(Debug)]
pub enum AutostartResult {
    /// 成功写入或删除注册表项。
    Ok,
    /// `auto-launch` 调用失败，附错误信息。
    Failed(String),
}

/// 设置 / 取消开机自启。
///
/// Windows 写注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`；
/// 其他平台由 `auto-launch` 按平台约定处理（Linux 写 XDG autostart desktop
/// 文件，macOS 写 LaunchAgent）。exe 路径取自 `current_exe`，应用名固定为
/// `Eyes`。
pub fn set_autostart(enabled: bool) -> AutostartResult {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return AutostartResult::Failed(format!("无法定位 exe: {e}")),
    };
    let exe_str = match exe.to_str() {
        Some(s) => s.to_string(),
        None => return AutostartResult::Failed("exe 路径含非 UTF-8 字符".into()),
    };

    let settings = match auto_launch::AutoLaunchBuilder::new()
        .set_app_name("Eyes")
        .set_app_path(&exe_str)
        .build()
    {
        Ok(s) => s,
        Err(e) => return AutostartResult::Failed(format!("构建 AutoLaunch 失败: {e}")),
    };

    let res = if enabled {
        settings.enable()
    } else {
        settings.disable()
    };

    match res {
        Ok(_) => AutostartResult::Ok,
        Err(e) => AutostartResult::Failed(format!("{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_dir_points_to_exe_parent() {
        let dir = install_dir().expect("应能拿到 exe 目录");
        assert!(dir.exists(), "exe 父目录应存在: {}", dir.display());
    }

    #[test]
    fn add_resource_dll_dir_is_safe_to_call() {
        // 不 panic 即可；返回值无副作用可测。
        add_resource_dll_dir();
    }

    #[test]
    fn set_autostart_disable_is_idempotent() {
        // 关闭一个本就没开的项应成功或幂等失败——只要不 panic。
        let _ = set_autostart(false);
    }
}
