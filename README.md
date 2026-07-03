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

### Windows

从 [Releases](https://github.com/cislunarspace/eyes/releases) 下载 `.msi` 安装包，双击安装。安装包含可执行文件、检测模型与运行时库（OpenCV、ONNX Runtime），无需额外装任何运行时依赖。

安装后：

- 开始菜单出现 **Eyes** 快捷方式，双击启动。
- 启动后系统托盘出现图标。
- DLL 搜索路径在启动时由 `SetDllDirectoryW` 自动设置，`onnxruntime.dll` / `opencv_world4100.dll` 能被正确加载。
- 在设置里打开「开机自启」会写注册表项 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Eyes`。
- 从「设置 → 应用 → 已安装的应用」卸载会干净移除可执行文件、模型与 DLL。用户配置（`%APPDATA%\eyes\`）保留，如需清除请手动删除该目录。

### 从源码构建

```bash
git clone https://github.com/cislunarspace/eyes.git
cd eyes
cargo run                    # 开发模式
scripts\build-windows.cmd    # 构建 MSI
```

开发构建（默认 feature，不含摄像头/检测器）只需 Rust，无需 OpenCV / ONNX Runtime：

| 依赖 | 说明 |
|------|------|
| Rust | 1.80+（stable）|

完整打包（`scripts\build-windows.cmd`）还需要：

| 依赖 | 说明 |
|------|------|
| cargo-wix | `cargo install cargo-wix` |
| WiX Toolset v3 | `candle.exe` / `light.exe` 加入 PATH |
| 检测模型 | `models/face_detection_yunet_2023mar.onnx` |
| 运行时 DLL | `onnxruntime.dll`、`opencv_world4100.dll`（放仓库根目录） |

启用摄像头/检测器 feature 编译时还需：

| 依赖 | 环境变量 |
|------|---------|
| OpenCV 4.x | `OPENCV_LINK_PATHS` |
| ONNX Runtime 1.x | `ORT_LIB_LOCATION`、`ORT_STRATEGY=system` |

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
cargo run                                       # 开发模式（默认 feature，不含摄像头/检测器）
cargo test --no-default-features                # Rust 测试（不需要 OpenCV/ONNX）
cargo clippy --no-default-features -- -D warnings  # 代码检查
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
