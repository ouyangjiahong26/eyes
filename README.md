<div align="center">

# 👁️ Eyes 护眼助手

**桌面坐姿监测与护眼提醒工具**

[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=white)](https://tauri.app/)
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
- **静默模式** — 暂停提醒 30 分钟、1 小时或手动恢复

## 安装

### Windows 安装包

从 [Releases](https://github.com/cislunarspace/eyes/releases) 下载 `.msi` 安装包，双击安装。

> 运行时组件已内置（OpenCV、ONNX Runtime、检测模型），无需额外安装。

### Linux 安装

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

**安装 .deb / .rpm**：从 [Releases](https://github.com/cislunarspace/eyes/releases) 下载对应包：

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
git clone https://github.com/cislunarspace/eyes.git
cd eyes
cargo build --release                 # 需先装好 OpenCV 开发包
cargo install cargo-bundle            # 打包工具（首次）
cargo bundle --format deb             # 产出 .deb
cargo bundle --format rpm             # 产出 .rpm
```

打包前需将 ONNX Runtime 的 `.so` 暂存到 `lib/` 目录（见 `Cargo.toml` 的 `[package.metadata.bundle]`）。

### 从源码构建

```bash
git clone https://github.com/cislunarspace/eyes.git
cd eyes
npm install
npm run tauri dev           # 开发模式
scripts\build-windows.cmd   # 构建 MSI
```

构建需要：

| 依赖 | 说明 |
|------|------|
| Rust | 1.80+ |
| Node.js | 18+ |
| OpenCV | 4.x，设置 `OPENCV_LINK_PATHS` |
| ONNX Runtime | 1.x，设置 `ORT_LIB_LOCATION` 和 `ORT_STRATEGY=system` |

---

## 使用说明

应用启动后在系统托盘运行，通过摄像头检测头部姿态。摄像头不可用时自动重试。

### 系统托盘菜单

- **静默 30 分钟 / 1 小时 / 无限静默** — 暂停提醒
- **恢复** — 提前结束静默
- **设置** — 打开设置页面
- **退出** — 完全退出应用

### 中性校准

面对屏幕保持放松姿势 5 秒，应用自动记录你的个人基准。也可在设置中手动触发。

---

## 设置

通过托盘菜单打开设置。

| 设置项 | 说明 |
|--------|------|
| 偏航阈值 | 转头容差（度）。超出则判定偏离。 |
| 俯仰阈值 | 抬头/低头容差（度）。超出则判定偏离。 |
| 摄像头 | 选择摄像头设备。 |
| 语言 | 中文 / English |
| 开机自启 | 随系统启动（Windows 用户级）。 |

---

## 配置与日志

配置保存在用户数据目录：

| 平台 | 路径 |
|------|------|
| Windows | `%APPDATA%\eyes\config.yaml` |
| macOS | `~/Library/Application Support/eyes/config.yaml` |
| Linux | `~/.config/eyes/config.yaml` |

事件日志（JSONL 格式）也写入同一目录。

---

## 开发

```bash
npm install                                    # 安装依赖
npm run tauri dev                              # 开发模式
cd src-tauri && cargo test --no-default-features   # Rust 测试（不需要 OpenCV/ONNX）
cd src-tauri && cargo clippy --no-default-features # 代码检查
```

### 架构

```text
src-tauri/                   Rust 后端 (Tauri 2)
├── src/
│   ├── lib.rs               应用入口、Tauri Builder 配置
│   ├── app_state.rs         共享状态容器
│   ├── commands.rs          Tauri commands + 后台 worker
│   ├── app_shell/           托盘、窗口、事件
│   ├── domain/              纯领域逻辑（分类、计时、校准、配置）
│   └── monitoring/          摄像头、检测器、worker
└── tests/                   行为测试

src/                         React + TypeScript 前端
models/                      ONNX 模型文件
docs/                        ADR、PRD、迁移计划
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
    → 计时器推进 → 生成 SenseEvent 列表
  → 前端事件 (pose-updated / correction / good-posture / eye-rest)
```

循环频率约 10 Hz（每 tick 100ms）。

---

## 常见问题

**关闭窗口后应用还在运行？** 关闭窗口时应用最小化到系统托盘，不会退出。要完全退出，点托盘菜单的**退出**。

**摄像头被其他应用占用？** 每 5 秒自动重试。关闭占用摄像头的应用（Zoom、Teams 等）后自动恢复。

**阈值太严 / 太松？** 从托盘菜单打开**设置**，调整阈值。

**卸载后配置还在吗？** 卸载只删除应用文件，用户配置保留在 `%APPDATA%\eyes\`。如需清除，手动删除该目录。

## 许可证

MIT License，详见 [LICENSE](LICENSE)。
