//! 通过 DirectShow 枚举系统中的摄像头设备。
//!
//! COM vtable 定义分离在 `win32::directshow_ffi` 中，
//! 本文件只包含纯枚举逻辑。

use serde::Serialize;

/// 摄像头设备信息。
#[derive(Debug, Clone, Serialize)]
pub struct CameraDevice {
    pub index: u32,
    pub name: String,
}

/// 枚举系统中的摄像头设备。
///
/// 返回 `(index, name)` 列表，index 与 OpenCV `VideoCapture::new(index, ...)` 一致。
#[cfg(target_os = "windows")]
pub fn list_cameras() -> Result<Vec<CameraDevice>, String> {
    use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize};
    use windows_sys::core::HRESULT;

    let hr: HRESULT =
        unsafe { CoInitializeEx(core::ptr::null_mut(), 0 /* COINIT_MULTITHREADED */) };
    if hr != 0 && hr != 1 {
        return Err(format!("CoInitializeEx 失败: 0x{:08X}", hr));
    }

    let result = unsafe { enumerate_video_devices() };

    unsafe {
        CoUninitialize();
    }

    result
}

/// 内部枚举逻辑。`CoInitializeEx` 已在调用方完成。
#[cfg(target_os = "windows")]
unsafe fn enumerate_video_devices() -> Result<Vec<CameraDevice>, String> {
    use windows_sys::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

    use super::win32::directshow_ffi::*;

    let mut dev_enum_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
    let hr = CoCreateInstance(
        &CLSID_SYSTEM_DEVICE_ENUM,
        core::ptr::null_mut(),
        CLSCTX_INPROC_SERVER,
        &IID_ICREATE_DEV_ENUM,
        &mut dev_enum_ptr,
    );
    if hr != 0 || dev_enum_ptr.is_null() {
        return Err(format!(
            "CoCreateInstance(ICreateDevEnum) 失败: 0x{:08X}",
            hr
        ));
    }
    let dev_enum = &mut *(dev_enum_ptr as *mut ICreateDevEnum);

    let mut enum_ptr: *mut IEnumMoniker = core::ptr::null_mut();
    let hr = ((*dev_enum.vtable).create_class_enumerator)(
        dev_enum,
        &CLSID_VIDEO_INPUT_DEVICE_CATEGORY,
        &mut enum_ptr,
        0,
    );
    ((*dev_enum.vtable).release)(dev_enum);

    if hr != 0 || enum_ptr.is_null() {
        return Err(format!(
            "CreateClassEnumerator 失败: 0x{:08X}",
            hr
        ));
    }

    let mut cameras = Vec::new();
    let mut index: u32 = 0;

    loop {
        let mut moniker: *mut IMoniker = core::ptr::null_mut();
        let mut fetched: u32 = 0;
        let hr = ((*(*enum_ptr).vtable).next)(enum_ptr, 1, &mut moniker, &mut fetched);
        if hr != 0 || moniker.is_null() {
            break;
        }

        let name = super::win32::directshow_ffi::get_device_friendly_name(moniker)
            .unwrap_or_else(|| format!("摄像头 {}", index));

        cameras.push(CameraDevice { index, name });

        ((*(*moniker).vtable).release)(moniker);
        index += 1;
    }

    ((*(*enum_ptr).vtable).release)(enum_ptr);
    Ok(cameras)
}

/// 非 Windows 平台回退：返回空列表。
#[cfg(not(target_os = "windows"))]
pub fn list_cameras() -> Result<Vec<CameraDevice>, String> {
    Ok(vec![])
}
