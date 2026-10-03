#![cfg(target_os = "windows")]

//! DirectShow COM vtable 定义。
//!
//! 为了避免引入 `windows` crate（编译时间长、体积大），
//! 手动定义设备枚举所需的 COM 接口布局。
//! 只声明实际调用到的方法，未使用的方法用 `usize` 占位。

use windows_sys::core::GUID;

// ── DirectShow 常量 ─────────────────────────────────────────

// CLSID_SystemDeviceEnum
pub const CLSID_SYSTEM_DEVICE_ENUM: GUID = GUID {
    data1: 0x62BE5D10,
    data2: 0x60EB,
    data3: 0x11D0,
    data4: [0xBD, 0x3B, 0x00, 0xA0, 0xC9, 0x11, 0xCE, 0x86],
};
// CLSID_VideoInputDeviceCategory
pub const CLSID_VIDEO_INPUT_DEVICE_CATEGORY: GUID = GUID {
    data1: 0x860BB310,
    data2: 0x5D01,
    data3: 0x11D0,
    data4: [0xBD, 0x3B, 0x00, 0xA0, 0xC9, 0x11, 0xCE, 0x86],
};
// IID_ICreateDevEnum
pub const IID_ICREATE_DEV_ENUM: GUID = GUID {
    data1: 0x29840822,
    data2: 0x5B84,
    data3: 0x11D0,
    data4: [0xBD, 0x3B, 0x00, 0xA0, 0xC9, 0x11, 0xCE, 0x86],
};
// IID_IPropertyStore {886d8eeb-8cf2-4446-8d02-cdba1dbdcf99}
pub const IID_IPROPERTY_STORE: GUID = GUID {
    data1: 0x886D8EEB,
    data2: 0x8CF2,
    data3: 0x4446,
    data4: [0x8D, 0x02, 0xCD, 0xBA, 0x1D, 0xBD, 0xCF, 0x99],
};

// PKEY_Device_FriendlyName — {a45c254e-df1c-4efd-8020-67d146a850e0}, 14
#[repr(C, packed(1))]
pub struct PropertyKey {
    pub fmtid: GUID,
    pub pid: u32,
}
pub const PKEY_DEVICE_FRIENDLY_NAME: PropertyKey = PropertyKey {
    fmtid: GUID {
        data1: 0xA45C254E,
        data2: 0xDF1C,
        data3: 0x4EFD,
        data4: [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0],
    },
    pid: 14,
};

// VT_LPWSTR = 31
pub const VT_LPWSTR: u16 = 31;

// ── 原始 PROPVARIANT 布局 ──────────────────────────────────
//
// 只关心读取 VT_LPWSTR 值。偏移 0 为 vt (u16)，偏移 8 为 *mut u16。
#[repr(C)]
pub struct RawPropVariant {
    pub vt: u16,
    _r1: u16,
    _r2: u16,
    _r3: u16,
    pub data: [u8; 8],
}

impl RawPropVariant {
    pub fn zeroed() -> Self {
        Self {
            vt: 0,
            _r1: 0,
            _r2: 0,
            _r3: 0,
            data: [0u8; 8],
        }
    }

    /// 读取 VT_LPWSTR 的宽字符串指针。
    pub fn as_lpwsz(&self) -> *const u16 {
        unsafe { core::ptr::read(self.data.as_ptr() as *const *const u16) }
    }
}

// ── ICreateDevEnum ──────────────────────────────────────────

#[repr(C)]
pub struct ICreateDevEnum {
    pub vtable: *const ICreateDevEnumVtbl,
}

#[repr(C)]
pub struct ICreateDevEnumVtbl {
    pub query_interface: unsafe extern "system" fn(
        *mut ICreateDevEnum,
        *const GUID,
        *mut *mut core::ffi::c_void,
    ) -> i32,
    pub add_ref: unsafe extern "system" fn(*mut ICreateDevEnum) -> u32,
    pub release: unsafe extern "system" fn(*mut ICreateDevEnum) -> u32,
    pub create_class_enumerator: unsafe extern "system" fn(
        *mut ICreateDevEnum,
        *const GUID,
        *mut *mut IEnumMoniker,
        u32,
    ) -> i32,
}

// ── IEnumMoniker ────────────────────────────────────────────

#[repr(C)]
pub struct IEnumMoniker {
    pub vtable: *const IEnumMonikerVtbl,
}

#[repr(C)]
pub struct IEnumMonikerVtbl {
    pub query_interface: unsafe extern "system" fn(
        *mut IEnumMoniker,
        *const GUID,
        *mut *mut core::ffi::c_void,
    ) -> i32,
    pub add_ref: unsafe extern "system" fn(*mut IEnumMoniker) -> u32,
    pub release: unsafe extern "system" fn(*mut IEnumMoniker) -> u32,
    pub next:
        unsafe extern "system" fn(*mut IEnumMoniker, u32, *mut *mut IMoniker, *mut u32) -> i32,
    pub skip: unsafe extern "system" fn(*mut IEnumMoniker, u32) -> i32,
    pub reset: unsafe extern "system" fn(*mut IEnumMoniker) -> i32,
    pub clone: unsafe extern "system" fn(*mut IEnumMoniker, *mut *mut IEnumMoniker) -> i32,
}

// ── IMoniker（只声明用到的方法） ────────────────────────────

#[repr(C)]
pub struct IMoniker {
    pub vtable: *const IMonikerVtbl,
}

#[repr(C)]
pub struct IMonikerVtbl {
    pub query_interface:
        unsafe extern "system" fn(*mut IMoniker, *const GUID, *mut *mut core::ffi::c_void) -> i32,
    pub add_ref: unsafe extern "system" fn(*mut IMoniker) -> u32,
    pub release: unsafe extern "system" fn(*mut IMoniker) -> u32,
    // IPersistStream → IPersist::GetClassID
    pub _get_class_id: usize,
    // IPersistStream
    pub _is_dirty: usize,
    pub _load: usize,
    pub _save: usize,
    pub _get_size_max: usize,
    // IMoniker
    pub _bind_to_storage: usize,
    pub _bind_to_object: usize,
    pub _reduce: usize,
    pub _compose_with: usize,
    pub _enum: usize,
    pub _is_equal: usize,
    pub _hash: usize,
    pub _is_running: usize,
    pub _get_time_of_last_change: usize,
    pub _inverse: usize,
    pub _common_prefix_with: usize,
    pub _relative_path_to: usize,
    pub get_display_name: unsafe extern "system" fn(
        *mut IMoniker,
        *mut core::ffi::c_void, /* pbc */
        *mut core::ffi::c_void, /* pmkToLeft */
        *mut *mut u16,
    ) -> i32,
    pub _parse_display_name: usize,
    pub _is_system_moniker: usize,
}

// ── IPropertyStore（手动定义，windows-sys 未暴露） ─────────

#[repr(C)]
pub struct IPropertyStore {
    pub vtable: *const IPropertyStoreVtbl,
}

#[repr(C)]
pub struct IPropertyStoreVtbl {
    pub query_interface: unsafe extern "system" fn(
        *mut IPropertyStore,
        *const GUID,
        *mut *mut core::ffi::c_void,
    ) -> i32,
    pub add_ref: unsafe extern "system" fn(*mut IPropertyStore) -> u32,
    pub release: unsafe extern "system" fn(*mut IPropertyStore) -> u32,
    pub get_count: unsafe extern "system" fn(*mut IPropertyStore, *mut u32) -> i32,
    pub get_at: unsafe extern "system" fn(*mut IPropertyStore, u32, *mut PropertyKey) -> i32,
    pub get_value: unsafe extern "system" fn(
        *mut IPropertyStore,
        *const PropertyKey,
        *mut RawPropVariant,
    ) -> i32,
    pub set_value: usize,
    pub commit: usize,
}

// ── 辅助函数 ────────────────────────────────────────────────

/// 从 moniker 读取设备友好名称。
///
/// 优先通过 `IPropertyStore` → `PKEY_Device_FriendlyName` 获取；
/// 回退到 `IMoniker::GetDisplayName`。
///
/// # Safety
///
/// `moniker` 必须指向一个有效的、由 COM 创建的 `IMoniker` 实例。
/// 调用方负责确保 moniker 的生命周期覆盖此函数调用。
pub unsafe fn get_device_friendly_name(moniker: *mut IMoniker) -> Option<String> {
    let vtable = &*(*moniker).vtable;

    let mut ps_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
    let hr = (vtable.query_interface)(moniker, &IID_IPROPERTY_STORE, &mut ps_ptr);

    if hr == 0 && !ps_ptr.is_null() {
        let ps = ps_ptr as *mut IPropertyStore;
        let mut prop_value = RawPropVariant::zeroed();

        if ((*(*ps).vtable).get_value)(ps, &PKEY_DEVICE_FRIENDLY_NAME, &mut prop_value) == 0
            && prop_value.vt == VT_LPWSTR
        {
            let pwstr = prop_value.as_lpwsz();
            if !pwstr.is_null() {
                let name = pwstr_to_string(pwstr);
                ((*(*ps).vtable).release)(ps);
                if name.is_some() {
                    return name;
                }
            }
        }
        ((*(*ps).vtable).release)(ps);
    }

    let mut display_ptr: *mut u16 = core::ptr::null_mut();
    let hr = (vtable.get_display_name)(
        moniker,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        &mut display_ptr,
    );
    if hr == 0 && !display_ptr.is_null() {
        let name = pwstr_to_string(display_ptr);
        windows_sys::Win32::System::Com::CoTaskMemFree(display_ptr as *const core::ffi::c_void);
        return name;
    }

    None
}

/// 将以 null 结尾的 UTF-16 指针转为 `String`。
unsafe fn pwstr_to_string(ptr: *const u16) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let mut len = 0;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = core::slice::from_raw_parts(ptr, len);
    String::from_utf16(slice).ok()
}
