//! 构建脚本。
//!
//! 仅在 Linux 且启用 `opencv-camera` feature 时生效：用 `pkg-config` 验证
//! 系统 OpenCV（`opencv4`）可被发现。失败时给出明确的安装指引，而不是让
//! `opencv` crate 的构建脚本抛出晦涩错误。
//!
//! 其他平台或未启用摄像头 feature 时为空操作，不影响 Windows 构建。

fn main() {
    // `cfg!(feature = ...)` 在 build.rs 中可用：build script 与 crate 共享 feature 集。
    #[cfg(all(target_os = "linux", feature = "opencv-camera"))]
    check_opencv_pkgconfig();
}

#[cfg(all(target_os = "linux", feature = "opencv-camera"))]
fn check_opencv_pkgconfig() {
    use std::process::Command;

    // 先确认 pkg-config 本体存在，避免把“没装 pkg-config”误报成“没装 OpenCV”。
    if Command::new("pkg-config").arg("--version").status().is_err() {
        panic!(
            "\n未找到 pkg-config。\n  \
             Debian/Ubuntu: sudo apt install pkg-config\n  \
             Fedora/RHEL:   sudo dnf install pkg-config\n"
        );
    }

    let found = Command::new("pkg-config")
        .args(["--exists", "opencv4"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !found {
        panic!(
            "\n未检测到系统 OpenCV（pkg-config opencv4 失败）。\n\
             Eyes 在 Linux 上依赖系统 OpenCV 采集摄像头，请先安装开发包：\n  \
             Debian/Ubuntu: sudo apt install libopencv-dev\n  \
             Fedora/RHEL:   sudo dnf install opencv-devel\n  \
             Arch:          sudo pacman -S opencv\n\
             安装后重新运行 cargo build。\n\
             （OpenCV .so 不打包进发行包，由系统提供。）\n"
        );
    }

    // 顺带打印检测到的版本，便于诊断环境问题。
    if let Ok(output) = Command::new("pkg-config")
        .args(["--modversion", "opencv4"])
        .output()
    {
        if output.status.success() {
            let ver = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            println!("cargo:warning=检测到系统 OpenCV {ver}（pkg-config opencv4）");
        }
    }
}
