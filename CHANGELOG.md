# Changelog

## [0.4.3] — 2026-07-05

### 修复

- **`.deb` / `.rpm` 安装后检测不到人脸**：`src/monitoring/vision/onnx_detector.rs::resolve_model_path` 的候选路径只覆盖 `<exe_dir>/models/`（MSI 布局）和 `<exe_dir>/../../models/`（cargo run 开发态），不覆盖 Linux 包实际布局 `/usr/lib/<pkg_name>/models/`。补该候选路径，路径用 `env!("CARGO_PKG_NAME")` 编译期拼装，避免硬编码。验证 feedback loop 复现并转绿。

## [0.4.2] — 2026-07-05

### 修复

- **Linux 中文显示为方块（tofu）**：`src/lib.rs::load_system_cjk_font` 的 `CANDIDATES` 数组在 `target_os != "windows"` 时为空，Linux 用户启动后所有 CJK 字符走 Bevy 默认字体（不含 CJK 字形）→ 显示为方块。补 Linux 候选路径（`NotoSansCJK-{Regular,Bold}.ttc`、`NotoSerifCJK-Regular.ttc`、`wqy-zenhei.ttc`、`arphic/uming.ttc`）；并在 `.deb` `Depends` 加 `fonts-noto-cjk` 让 apt 自动安装。

## [0.4.1] — 2026-07-05

### 修复

- **`.deb` 依赖解析失败**：v0.4.0 .deb 的 `Depends` 声明 `libopencv-core4.6 (>= 4.6.0)`，但 Debian 12+/Ubuntu 24.04 上 OpenCV 4.6 实际包名是 `libopencv-core406t64`（406 = 4.6 主版本号 dot-to-0 转换，t64 = time_t 64-bit 过渡后缀）。修正为按 ldd 实际链接的 4 个子包：`libopencv-core406t64`、`libopencv-imgproc406t64`、`libopencv-imgcodecs406t64`、`libopencv-videoio406t64`。

## [0.4.0] — 2026-07-03

### 破坏性变更

- **配置目录变更**：从 `com.cislunarspace.eyes`（Tauri identifier 残留）改为 `eyes`（Windows `%APPDATA%\eyes\`，Linux `~/.config/eyes/`）。**旧用户需重新校准中性姿态**，旧配置不会自动迁移。
- **移除 Tauri / React / WebView / TypeScript / Vite 依赖**：UI 层整体替换为纯 Rust Bevy 应用。
- **不再需要 Node.js / npm**：构建链路从 Rust + npm + Vite + Tauri 缩减为单一 `cargo build`。

### UI 层重写（Bevy 替换 Tauri）

- 纯 Rust Bevy 0.15 应用：winit 原生窗口、ECS 架构、`DefaultPlugins`
- `BevyEventSink`：worker 事件经 mpsc channel → Bevy `Events<MonitoringEvent>` 总线
- 主视图：姿态徽标、实时 yaw/pitch 读数、摄像头预览（水平镜像）、摄像头状态指示
- 设置面板：阈值滑块、摄像头选择、声音/自启开关、语言切换、高级设置、Save/Cancel
- 校准视图：5 秒倒计时、采样计数、实时姿态、取消按钮
- i18n 运行时刷新（中文 / English），TOML 字典

### 新增功能

- **系统通知**（`notify-rust`）：偏头提醒、护眼提醒走系统 toast（Windows WinRT / Linux D-Bus）
- **声音提醒**（`bevy_kira_audio`）：`build.rs` 生成提示音 wav，`SoundAlert` 事件驱动播放，可独立开关
- **托盘 Pause/Resume 菜单**：30 分钟 / 1 小时 / 无限静默 / 恢复

### 平台与打包

- **Windows MSI**（`cargo wix`）：可执行文件 + ONNX 模型 + `onnxruntime.dll` + `opencv_world4100.dll`，开始菜单快捷方式，注册卸载项，`SetDllDirectoryW` 管理 DLL 搜索路径
- **Linux deb / rpm**（`cargo bundle`）：依赖系统 OpenCV，ONNX Runtime 静态链接到二进制，models/ 随包分发
- **Linux autostart**：写 `~/.config/autostart/eyes.desktop`（XDG 规范）
- `build.rs`：Linux 构建期 `pkg-config opencv4` 校验，失败打印明确指引

### 项目结构

- 扁平 cargo 项目：根 `Cargo.toml` + `src/`
- 删除 `src-tauri/`、React `src/`、`index.html`、`vite.config.ts`、`tsconfig.json`、`package.json`、`package-lock.json`
- 领域代码（`domain/*`、`monitoring/orchestrator.rs`、`worker.rs`）原样搬迁，零行为改动
- 测试 138+ 通过，clippy 零警告

### 相关 ADR

- [ADR-0009 — 用 Bevy 替换 Tauri UI 层](docs/adr/0009-bevy-ui-replacement.md)

## [0.3.0] — 2026-06-28

### M7 — Windows 打包

- Tauri 构建流水线产出 MSI 安装包
- ONNX Runtime 和 OpenCV DLL 随安装包分发
- ONNX 模型文件通过 bundle.resources 打包
- `build-windows.cmd` 构建辅助脚本
- `models/MANIFEST.toml` 模型溯源清单

### M8 — 旧代码清理

- 删除全部 Python 源码、测试、PyInstaller 规格、构建脚本
- 删除 Python CI 流水线（`.github/workflows/linux-build.yml`）
- 删除 `pyproject.toml`、`uv.lock`、`.python-version`
- 旧 README 存档至 `docs/legacy/`
- 新 README 描述 Rust/Tauri 应用、Windows 安装流程、配置路径、卸载策略

## [0.2.0] — 2026-06-27

### Rust/Tauri 重写

Python 版本的完整 Rust 移植，使用 Tauri 2 构建桌面壳。

**新增：**

- Tauri 2 桌面壳：系统托盘、关闭时最小化、单实例锁
- Domain 层纯逻辑：校准、姿态分类、时间累积、显示计划、事件日志、静默
- 摄像头预览：OpenCV 捕获 → RGB → PNG data URL → 前端
- 后台 worker 线程，~10 Hz tick，5 秒摄像头重试
- 共享状态容器（AppState），Tauri commands 暴露给前端
- Detector trait（M4 ONNX 实现待接入）
- 42 个 Rust 行为测试，cargo clippy 零警告

## [0.1.0] — 2025-xx-xx

初始 Python 版本：摄像头监测、姿态分类、坐姿提示、休息提醒。
