<div align="center">

# 👁️ Eyes 护眼助手

**桌面坐姿监测与护眼提醒工具**

[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Bevy](https://img.shields.io/badge/Bevy-0.15-1F2F35?logo=bevy&logoColor=white)](https://bevyengine.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

*通过摄像头监测头部姿态，提醒你保持正确坐姿、适时休息。*

</div>

---

## 功能

### 姿态检测

- **头部姿态检测** — 通过摄像头检测人脸关键点，计算偏航角和俯仰角
- **状态分类** — 正对屏幕、左偏、右偏、抬头、低头、无人脸
- **中性校准** — 保持放松姿势 5 秒，设定个人基准
- **阈值调整** — 在设置中调整偏航和俯仰容差

### 提醒策略

- **坐姿纠正** — 偏离一段时间后提醒，之后定时重复
- **坐姿表扬** — 正对屏幕累计 5 分钟后鼓励
- **护眼提醒** — 检测到人脸累计 15 分钟后提醒远眺
- **声音提醒** — 可选提示音（与系统通知独立开关）
- **静默模式** — 暂停提醒 30 分钟、1 小时或手动恢复

## 安装

### Windows

从 [Releases](https://github.com/ouyangjiahong26/eyes/releases) 下载 `.msi` 安装包，双击安装。安装包含可执行文件、检测模型与运行时库（OpenCV、ONNX Runtime），无需额外装任何运行时依赖。

安装后：

- 开始菜单出现 **Eyes** 快捷方式，双击启动。
- 启动后系统托盘出现图标。
- DLL 搜索路径在启动时由 `SetDllDirectoryW` 自动设置，`onnxruntime.dll` / `opencv_world4100.dll` 能被正确加载。
- 在设置里打开「开机自启」会写注册表项 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Eyes`。
- 从「设置 → 应用 → 已安装的应用」卸载会干净移除可执行文件、模型与 DLL。用户配置（`%APPDATA%\eyes\`）保留，如需清除请手动删除该目录。

### Linux

**前置依赖**：Eyes 通过系统 OpenCV 采集摄像头，需先安装 OpenCV 开发包与 `pkg-config`（构建期由 `build.rs` 校验）：

| 发行版 | 命令 |
|--------|------|
| Debian / Ubuntu | `sudo apt install libopencv-dev pkg-config` |
| Fedora / RHEL | `sudo dnf install opencv-devel pkg-config` |
| Arch | `sudo pacman -S opencv pkgconf` |

> OpenCV 作为系统库提供，**不**打包进发行包。包内仅内置 ONNX Runtime（`.so`）与检测模型。

**桌面环境**：

- KDE / XFCE：托盘与通知开箱即用。
- GNOME：需安装 AppIndicator 扩展（托盘走 AppIndicator / StatusNotifier 协议）：
  ```bash
  sudo apt install gnome-shell-extension-appindicator   # Debian/Ubuntu
  ```
  安装后注销重新登录生效。

**安装 .deb / .rpm**：从 [Releases](https://github.com/ouyangjiahong26/eyes/releases) 下载对应包：

```bash
# Debian / Ubuntu
sudo dpkg -i eyes_0.4.0_amd64.deb
sudo apt install -f          # 自动补齐系统依赖

# Fedora / RHEL
sudo rpm -i eyes-0.4.0.x86_64.rpm
```

**开机自启**：在设置中开启后写入 `~/.config/autostart/eyes.desktop`（符合 XDG Autostart 规范）；关闭后该文件自动删除。

**卸载**：

```bash
sudo dpkg -r eyes    # Debian/Ubuntu
sudo rpm -e eyes     # Fedora/RHEL
```

卸载只删除应用文件；用户配置保留在 `~/.config/eyes/`，需手动删除。

**从源码构建与打包**：

```bash
git clone https://github.com/ouyangjiahong26/eyes.git
cd eyes
cargo build --release                 # 需先装好 OpenCV 开发包
cargo install cargo-bundle            # 打包工具（首次）
cargo bundle --format deb             # 产出 .deb
cargo bundle --format rpm             # 产出 .rpm
```

打包前需将 ONNX Runtime 的 `.so` 暂存到 `lib/` 目录（见 `Cargo.toml` 的 `[package.metadata.bundle]`）。

### 从源码启动

```bash
git clone https://github.com/ouyangjiahong26/eyes.git
cd eyes
```

**最小启动**（只看 UI 和托盘，不接摄像头）——只需 Rust：

```bash
cargo run
```

**完整功能启动**（摄像头 + 姿态检测 + 提醒）——需要 OpenCV、ONNX Runtime 和模型文件：

```bash
cargo run --features opencv-camera,onnx-detector
```

完整功能的前置依赖：

| 依赖 | Windows | Linux |
|------|---------|-------|
| Rust 1.80+（stable） | 必装 | 必装 |
| OpenCV 4.x | `scoop install opencv@4.10.0`，再按下面设置环境变量 | `sudo apt install libopencv-dev`（Debian/Ubuntu）或等价包 |
| ONNX Runtime 1.x | 开发时 `ort` 自动下载 DirectML 版，无需手动准备 | ONNX Runtime `.so` 放入 `lib/`（见 `Cargo.toml` 的 `[package.metadata.bundle]`） |
| 检测模型 | `models/face_detection_yunet_2023mar.onnx`（从 [OpenCV Zoo](https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx) 获取） | 同左 |

#### Windows 完整功能编译步骤

1. **安装 OpenCV 开发库**（推荐用 scoop）：
   ```powershell
   scoop install opencv@4.10.0
   ```
   这会同时安装 `.lib`、头文件和运行时 DLL。

2. **设置构建环境变量**。每次启动新终端时执行：
   ```cmd
   set OPENCV_LINK_LIBS=opencv_world4100
   set OPENCV_LINK_PATHS=%USERPROFILE%\scoop\apps\opencv\current\x64\vc16\lib
   set OPENCV_INCLUDE_PATHS=%USERPROFILE%\scoop\apps\opencv\current\include
   set OPENCV_DISABLE_PROBES=pkg_config,cmake,vcpkg_cmake,vcpkg
   ```
   或者在仓库创建 `.cargo/config.toml`：
   ```toml
   [env]
   OPENCV_LINK_LIBS = "opencv_world4100"
   OPENCV_LINK_PATHS = "C:\\Users\\<你的用户名>\\scoop\\apps\\opencv\\current\\x64\\vc16\\lib"
   OPENCV_INCLUDE_PATHS = "C:\\Users\\<你的用户名>\\scoop\\apps\\opencv\\current\\include"
   OPENCV_DISABLE_PROBES = "pkg_config,cmake,vcpkg_cmake,vcpkg"
   ```

3. **准备检测模型**：
   ```powershell
   curl -L -o models/face_detection_yunet_2023mar.onnx `
     https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx
   ```

4. **把 OpenCV 运行时 DLL 放到 exe 目录**，让 `SetDllDirectoryW` 能找到：
   ```powershell
   Copy-Item "$env:USERPROFILE\scoop\apps\opencv\current\x64\vc16\bin\opencv_world4100.dll" `
             target\debug\opencv_world4100.dll
   ```

5. **编译并运行**：
   ```cmd
   cargo run --features opencv-camera,onnx-detector
   ```

> **关键区别**：`opencv_world4100.dll` 只是运行时库，编译 `opencv-camera` feature 还需要 OpenCV 的**开发文件**——即 `.lib` 导入库和头文件。只把 `.dll` 放在仓库根目录无法通过 `opencv` crate 的构建探测。

### 打包

**Windows MSI**：

```bash
scripts\build-windows.cmd    # 拷贝模型与 DLL，再调 cargo wix 产出 MSI
```

| 依赖 | 说明 |
|------|------|
| cargo-wix | `cargo install cargo-wix` |
| WiX Toolset v3 | `candle.exe` / `light.exe` 加入 PATH |

**Linux .deb / .rpm**：

```bash
cargo install cargo-bundle    # 首次
cargo bundle --format deb     # 产出 .deb
cargo bundle --format rpm     # 产出 .rpm
```

打包前需将 ONNX Runtime 的 `.so` 暂存到 `lib/` 目录（见 `Cargo.toml` 的 `[package.metadata.bundle]`）。

---

## 使用说明

应用启动后在系统托盘运行，通过摄像头检测头部姿态。摄像头不可用时每 5 秒自动重试。

### 系统托盘菜单

- **打开** — 把主窗口拉回前台
- **静默 30 分钟 / 1 小时 / 无限静默** — 暂停提醒
- **恢复** — 提前结束静默
- **退出** — 完全退出应用（其他操作都不会终止进程）

### 主窗口

- 姿态徽标（正对屏幕 / 头偏左 / 头偏右 / 仰头 / 低头 / 检测到人脸）
- 实时偏航 / 俯仰读数
- 摄像头预览（水平镜像，selfie view）
- 摄像头不可用时的状态指示
- ⚙ 设置入口

### 中性校准

在设置中点「开始校准」，面对屏幕保持放松姿势 5 秒，应用自动记录你的个人基准。校准期间有倒计时与实时采样数显示；可取消。

---

## 设置

在主窗口点 ⚙ 进入设置面板。

| 设置项 | 说明 |
|--------|------|
| 偏航阈值 | 转头容差（度，1–30）。超出则判定偏离。 |
| 俯仰阈值 | 抬头/低头容差（度，1–30）。超出则判定偏离。 |
| 摄像头 | 选择摄像头设备。 |
| 声音提醒 | 开 / 关提示音。 |
| 开机自启 | 随系统启动（Windows 写注册表，Linux 写 `.desktop`）。 |
| 语言 | 中文 / English（切换后立即刷新） |
| 高级 | 偏离首次提醒秒数、重复间隔秒数 |

**保存** 写入配置并即时下发到 worker；**取消** 丢弃修改。

---

## 配置与日志

配置保存在用户数据目录：

| 平台 | 路径 |
|------|------|
| Windows | `%APPDATA%\eyes\config.yaml` |
| Linux | `~/.config/eyes/config.yaml` |

事件日志（JSONL 格式）写入同一目录。

> **0.4.0 破坏性变更**：配置目录从 0.3.0 的 `com.cislunarspace.eyes`（Tauri identifier 残留）改为 `eyes`。旧用户需重新校准中性姿态。详见 [CHANGELOG](CHANGELOG.md)。

---

## 开发

```bash
cargo run                                          # 开发模式（默认 feature，不含摄像头/检测器）
cargo test --no-default-features                   # 领域测试（不需要 OpenCV/ONNX）
cargo clippy --no-default-features -- -D warnings  # 代码检查
```

### 架构

纯 Rust 应用：Bevy 0.15 负责窗口、UI、事件总线与音频；后台 worker 线程跑监控管道。

```text
src/
├── main.rs                   二进制入口
├── lib.rs                    Bevy App 构造、系统调度
├── app_shell/                应用壳层
│   ├── mod.rs                模块聚合、AppView 状态
│   ├── contract.rs           托盘菜单项 ID 常量
│   ├── platform.rs           Windows DLL 搜索路径、开机自启
│   ├── system/               系统级集成
│   │   ├── mod.rs
│   │   ├── notification.rs   系统通知（notify-rust）
│   │   └── tray.rs           系统托盘
│   └── ui/                   UI 视图与国际化
│       ├── mod.rs
│       ├── main_view.rs      主视图（姿态徽标、预览、Settings）
│       ├── settings_view.rs  设置面板
│       ├── calibration_view.rs 校准视图
│       └── i18n/             国际化（zh.toml / en.toml）
│           ├── mod.rs
│           ├── zh.toml
│           └── en.toml
├── audio.rs                  声音提醒（bevy_kira_audio）
├── domain/                   纯领域逻辑
│   ├── mod.rs
│   ├── classifier.rs         头部姿态分类
│   ├── config.rs             AppConfig 配置结构
│   ├── defaults.rs           默认阈值
│   ├── calibration.rs        中性姿态校准
│   ├── posture_tick_engine.rs 姿态状态机与计时
│   ├── thresholds.rs         阈值结构
│   ├── snooze.rs             暂停提醒
│   ├── event_log.rs          事件日志
│   └── paths.rs              配置/日志目录
├── monitoring/               摄像头、检测器、worker、事件桥接
│   ├── mod.rs
│   ├── preview.rs            摄像头预览纹理
│   ├── camera/               摄像头采集
│   │   ├── mod.rs
│   │   ├── opencv_camera.rs  OpenCV 摄像头读取
│   │   ├── camera_enumerator.rs 摄像头枚举
│   │   └── win32.rs          Windows DirectShow 枚举
│   ├── vision/               视觉检测
│   │   ├── mod.rs
│   │   ├── onnx_detector.rs  ONNX YuNet 人脸检测
│   │   ├── linalg3.rs        3D 线性代数工具
│   │   └── solve_pnp.rs      solvePnP 头部姿态估计
│   ├── pipeline/             监测管道
│   │   ├── mod.rs
│   │   ├── orchestrator.rs   WorkerOrchestrator
│   │   ├── worker.rs         MonitoringWorker
│   │   ├── channel.rs        worker 命令通道
│   │   └── detector.rs       Detector trait
│   └── events/               事件类型与桥接
│       ├── mod.rs
│       ├── types.rs          MonitoringEvent
│       └── event_mapping.rs  事件到 UI 文本映射
└── worker_setup.rs           后台 worker 启动
tests/                        行为测试
models/                       ONNX 模型文件
docs/                         ADR、PRD、迁移计划
```

### 数据流

```text
摄像头帧 (RGB)
  → OpenCvCamera.read_frame()
  → YuNetDetector.detect(rgb, w, h)
    → 人脸关键点 → solvePnP → HeadPose(yaw, pitch)
  → classifier::classify(pose, config)
    → PoseState (FacingScreen / OffAxisLeft / ...)
  → PostureTickEngine.tick(yaw_state, pitch_state, dt)
    → 计时器推进 → MonitoringEvent 列表
  → BevyEventSink (mpsc channel)
  → Bevy Events<MonitoringEvent>
  → UI 节点 / 系统通知 / 声音提醒
```

循环频率约 10 Hz（每 tick 100ms）。

---

## 常见问题

**关闭窗口后应用还在运行？** 关闭窗口时应用最小化到系统托盘，不会退出。要完全退出，点托盘菜单的**退出**。

**摄像头被其他应用占用？** 每 5 秒自动重试。关闭占用摄像头的应用（Zoom、Teams 等）后自动恢复。

**阈值太严 / 太松？** 主窗口点 ⚙ 进入**设置**，调整阈值。

**卸载后配置还在吗？** 卸载只删除应用文件，用户配置保留在 `%APPDATA%\eyes\`（Windows）或 `~/.config/eyes/`（Linux）。如需清除，手动删除该目录。

## 许可证

MIT License，详见 [LICENSE](LICENSE)。
