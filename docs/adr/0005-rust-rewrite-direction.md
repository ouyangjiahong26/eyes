# 0005 — 通过 Tauri 2 全面重写为 Rust

Eyes 正在用 Rust 全面重写为 Tauri 2 桌面应用。Python / PySide6 / MediaPipe 实现被替换而非包装。迁移是用 vibe coding 的直接全面重写，按可运行的里程碑执行，而非一次性大爆炸补丁。功能等价优先于完全匹配当前 PySide UI。

## 考虑过的选项

- **渐进式核心提取（PyO3 绑定到小 Rust 核心，保留 Python 壳）** — 否决：用户明确想迁移整个应用并学习 Rust。混合 FFI 加 PySide 增加摩擦但不降低风险。
- **egui / Slint 原生 Rust UI** — 否决：托盘、置顶提醒窗口、设置对话框、自启动、Windows 打包在 Tauri 的 WebView 壳上明显更容易。不要求 UI 完全一致，薄 web 前端足够。
- **Qt for Rust / C++ Qt 桥接** — 否决：最接近现有 PySide 布局，但是候选方案中最重的构建/分发路径，也是 vibe coding 最痛苦的。
- **MediaPipe C++ 绑定做头部姿态检测** — 否决：官方 Rust 支持不成熟，会主导迁移时间线。通过 `ort` 使用 ONNX 模型集成和打包容易得多。

## 决策

- **壳：** Tauri 2。
- **前端：** React + TypeScript + Vite。薄 UI；业务逻辑留在 Rust。
- **仓库布局：** 单个 Tauri crate（`src-tauri/`），内部 `domain/`、`monitoring/`、`platform/` 模块。暂不分 workspace。
- **进程模型：** 一个监测 worker 拥有 `CameraSource`、`Detector`、`PostureTickEngine`。Worker 由外部 `Tick` 命令以 10 Hz 驱动。Worker 发出 `WorkerEvent`；适配层将其翻译为 Tauri 事件。Worker 与 Tauri 类型解耦，可独立做单元测试。
- **摄像头 + 图像处理：** OpenCV Rust crate。采集和预览编码在 Rust 后端运行，不在 WebView 中。
- **头部姿态检测：** ONNX 人脸检测 + 关键点（或直接头部姿态）模型，通过 `ort` 加载。默认头部姿态路径是 2D 关键点加 OpenCV `solvePnP`。专门用 2 天 spike 选模型；选择记录在 `0006-onnx-detector-choice.md`（待写）中，包含许可证、大小和延迟说明。检测等价是**行为级**的，不是数值级的：姿态分类和提醒匹配 Python 版本；原始偏航/横滚值不要求匹配 MediaPipe。
- **提醒：** 专用 Tauri 置顶提醒窗口，替换 PySide 覆盖层。系统通知是未来增强，不是主要机制（与 [0004](0004-custom-floating-window-over-os-toast.md) 一致）。
- **主窗口预览：** 必须有，但允许是低帧率编码预览（通过 Tauri 事件传输 JPEG）。推理用原始帧；预览用降采样副本。
- **托盘生命周期：** 必须。关闭主窗口只是隐藏；只有"退出"才结束进程。暂停检测继续运行但静音提醒。暂停持久化语义（`null` / `"indefinite"` / 未来 ISO，含格式错误/过期处理）与 Python 版本完全一致。
- **校准：** worker 端 5 秒中位数姿态会话，语义与 Python 实现相同。校准期间提醒暂停。进度和结果作为事件发出。
- **配置：** 保留现有 YAML 格式和磁盘路径。Rust 用 `serde` 对缺失字段取默认值，原子写入（临时文件 + 重命名）。高级时间字段即使不在简化设置 UI 中暴露也保持 YAML 可读。
- **事件日志：** 语义保留，但写为 JSONL（信息等价而非字节兼容）。
- **国际化：** `zh-CN` 和 `en-US`，轻量前端词典加稳定后端消息键。切换设置重建托盘菜单并刷新所有打开的窗口。
- **声音：** `sound_enabled` 作为配置和 UI 保留，但第一个 Rust 闭包不要求播放。
- **自启动：** 第一个 Rust 版本要求真正的 Windows 用户级自启动。实现用 `tauri-plugin-autostart`。macOS/Linux 自启动延后。
- **单实例：** `tauri-plugin-single-instance` 必须，因为自启动加手动启动可能产生两个争抢摄像头的 worker。
- **平台：** Windows 是唯一的第一阶段必须平台。macOS 和 Linux 后续跟进。
- **打包：** 所有资源（ONNX 模型文件、ONNX Runtime DLL、OpenCV DLL）打包进安装程序。运行时不下载任何东西。配置和日志写入 `%APPDATA%\eyes\`。代码签名延后。
- **测试作为基准：** 现有 Python 测试是行为等价的迁移基准。`domain/*` 中的 Rust 单元测试镜像 Python 套件（`test_classifier`、`test_posture_tick_engine`、`test_snooze_evaluation`、`test_calibration`、`test_config_store`、`test_display_plan`）。UI / 摄像头 / ONNX 层用集成冒烟测试，不做逐行等价。
- **共存：** Python 源码、测试、打包和 CI 在仓库中保持不变，直到 Rust 应用达到功能闭合（M7）。M8 清理。
- **CI：** 遗留 Python CI 不动。新的 `cargo test` 任务在 Linux（仅 domain，无 OpenCV/ONNX）和 Windows（完整）上运行。单独的 Tauri 构建任务在 Windows 上产出 MSI 产物。

## 里程碑

1. **M1 — Tauri 骨架。** 窗口、托盘、关闭到托盘、退出。
2. **M2 — Rust 领域核心 + 移植测试。** 分类器、姿态引擎、暂停、校准、配置存储。
3. **M3 — 摄像头预览。** OpenCV 采集、低帧率预览、断连重试。
4. **M4 — ONNX 检测器 spike。** 模型选择、`Detector` trait、2D 关键点 + solvePnP 路径、行为验证。
5. **M5 — 监测闭合。** 分类 → 姿态引擎 → 提醒窗口 → JSONL 日志 → 暂停/恢复/警告级别。
6. **M6 — 设置 + 校准 UI + Windows 自启动。**
7. **M7 — Windows 打包。** MSI/NSIS、打包 DLL 和模型、在干净 Windows 机器上验证。
8. **M8 — 遗留清理。** 移除 Python 源码/测试/打包/CI，重写 README，只留 Rust/Tauri 栈。

## 后果

- 迁移不能缩减为单个 PR；它是按可运行里程碑执行的序列，每个有自己的验收清单（[migration-plan.md](../legacy/migration-plan.md)）。
- OpenCV 和 ONNX Runtime DLL 在 Windows 上的打包是真正的打包工作，必须在 M7 完成前解决。
- 行为级验收意味着 Python 测试套件是姿态、暂停、校准和事件日志语义的事实来源。任何偏离都视为 Rust bug，而非"Rust 版本选择了不同行为"。
- 跳过渲染端摄像头访问（`getUserMedia`）使后台监测可靠，但意味着预览帧必须编码并通过 IPC 通道传输。
- 提醒窗口决策延续了 [0004](0004-custom-floating-window-over-os-toast.md) 的精神：我们拥有提示界面；系统通知不是产品。
- [0003](0003-mediapipe-for-head-pose.md) 中记录的 MediaPipe 理由在 Rust 版本中被取代。新的头部姿态来源是 ONNX + `solvePnP`；行为等价是契约，不是偏航/横滚数值等价。
