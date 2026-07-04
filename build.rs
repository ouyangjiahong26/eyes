//! 构建脚本。
//!
//! 两件事：
//! 1. 生成声音提醒的两段 wav 到 `OUT_DIR`（VS5），运行时用 `include_bytes!`
//!    嵌入二进制。参考 tanchishe 的 wav 生成套路：短促上升/下降音 + 线性包络。
//! 2. 仅在 Linux 且启用 `opencv-camera` feature 时：用 `pkg-config` 验证
//!    系统 OpenCV（`opencv4`）可被发现。失败时给出明确的安装指引，而不是让
//!    `opencv` crate 的构建脚本抛出晦涩错误。

fn main() {
    generate_sound_assets();

    // `cfg!(feature = ...)` 在 build.rs 中可用：build script 与 crate 共享 feature 集。
    #[cfg(all(target_os = "linux", feature = "opencv-camera"))]
    check_opencv_pkgconfig();
}

// ── 声音资产生成 ──────────────────────────────────────────────────

/// 采样率（Hz）。
const SAMPLE_RATE: u32 = 22050;

/// 把 PCM 样本写成单声道 16-bit WAV（标准 RIFF/WAVE 头）。
fn write_wav(path: &std::path::Path, samples: &[i16]) {
    use std::fs;
    use std::io::Write;

    let data_len = samples.len() * 2;
    let file_len = 36 + data_len;

    let mut bytes: Vec<u8> = Vec::with_capacity(44 + data_len);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(file_len as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // 子块大小
    bytes.extend_from_slice(&1u16.to_le_bytes()); // 音频格式：PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // 声道数：单声道
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // 字节率
    bytes.extend_from_slice(&2u16.to_le_bytes()); // 块对齐
    bytes.extend_from_slice(&16u16.to_le_bytes()); // 采样位数
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut file = fs::File::create(path).unwrap();
    file.write_all(&bytes).unwrap();
}

/// 从起始频率扫到结束频率的正弦波，带线性衰减包络。
fn sweep(start_freq: f32, end_freq: f32, duration_sec: f32) -> Vec<i16> {
    let num_samples = (SAMPLE_RATE as f32 * duration_sec) as usize;
    let mut samples = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        let t = i as f32 / SAMPLE_RATE as f32;
        let phase = i as f32 / num_samples as f32;
        let freq = start_freq + (end_freq - start_freq) * phase;
        let sample = (t * freq * 2.0 * std::f32::consts::PI).sin();
        // 线性衰减包络，避免首尾爆音。
        let env = 1.0 - phase;
        samples.push((sample * env * 3000.0) as i16);
    }
    samples
}

/// 生成 off_axis.wav（偏头纠正：短促上升音）和 eyest.wav（护眼提醒：下降音）到 OUT_DIR。
fn generate_sound_assets() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR 未设置");
    let out_dir = std::path::Path::new(&out_dir);

    write_wav(&out_dir.join("off_axis.wav"), &sweep(800.0, 1200.0, 0.15));
    write_wav(&out_dir.join("eyest.wav"), &sweep(900.0, 500.0, 0.3));

    println!("cargo:rerun-if-changed=build.rs");
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
