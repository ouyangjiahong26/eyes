//! 采集与平台层：OpenCV 摄像头、设备枚举、Win32 DirectShow FFI。

pub mod camera_enumerator;
#[cfg(feature = "opencv-camera")]
pub mod opencv_camera;
#[cfg(target_os = "windows")]
pub mod win32;
