# Rust 重写迁移计划（历史文档）

> **本文档描述的是已放弃的 Tauri 2 方案，仅作历史记录保留。**
>
> 本计划是 [ADR 0005](../adr/0005-rust-rewrite-direction.md) 的执行层，对应
> 0.2.0–0.3.0 的 Rust/Tauri 重写。该方案在 0.4.0 被整体替换：UI 层从
> Tauri + React + WebView 改为纯 Rust 的 Bevy 应用（详见根目录
> [CHANGELOG.md](../../CHANGELOG.md) 的 0.4.0 条目）。
>
> 因此，本文档中的目录结构（`src-tauri/`、`frontend/`）、Tauri 命令、Tauri 事件
> 适配层等，在当前代码中**均不存在**。它不再代表当前架构。
>
> **当前架构以根目录 [README.md](../../README.md) 的「架构」与「数据流」小节为准。**

---

本计划将完整的 Rust/Tauri 重写拆分为八个可独立运行的里程碑（Milestone），每个里程碑附带明确的验收清单。Python 源码保留至 M8 再删除。

## 架构总览

```text
src-tauri/src/
├── main.rs                  # Tauri builder、插件接线、单实例 + 开机自启
├── app_state.rs             # 共享状态、命令/事件通道、最新快照
├── commands.rs              # Tauri 命令（UI -> 后端）
├── events.rs                # WorkerEvent -> Tauri emit 转译
├── config.rs                # 原子化 YAML 读写、serde 默认值
├── log.rs                   # JSONL 事件日志（与 tracing 分开）
├── i18n_keys.rs             # 稳定的后端消息键
├── monitoring/
│   ├── mod.rs
│   ├── worker.rs            # WorkerCommand/Event 循环，持有 Camera + Detector + Engine
│   ├── camera.rs            # OpenCV 采集、重试、BGR 帧
│   ├── detector.rs          # Detector trait
│   ├── linalg3.rs           # 纯 3×3 线性代数（Jacobi SVD）
│   ├── solve_pnp.rs         # DLT solvePnP + rotation_to_yaw_pitch + MODEL_3D
│   ├── onnx_detector.rs     # YuNet 检测器实现
│   └── preview.rs           # 降采样 + JPEG 编码，用于 UI 预览
├── domain/
│   ├── mod.rs
│   ├── classifier.rs        # NeutralPose、Thresholds、classify()
│   ├── posture_tick_engine.rs
│   ├── snooze.rs            # evaluate_snooze + 状态机
│   ├── calibration.rs       # CalibrationSession、compute_median_pose
│   └── display_plan.rs      # 纯 UI 投影，供渲染器使用
└── platform/
    ├── mod.rs
    ├── autostart_windows.rs # tauri-plugin-autostart 包装
    └── tray.rs              # 托盘菜单、语言刷新、暂停提醒项

frontend/                    # React + TS + Vite
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── pages/
│   │   ├── MainView.tsx
│   │   └── Settings.tsx
│   ├── windows/
│   │   └── Reminder.tsx
│   ├── ipc/                 # invoke 包装 + 事件监听
│   └── i18n/
│       ├── zh-CN.ts
│       └── en-US.ts
└── index.html

models/                      # ONNX 模型，打包进安装程序
docs/                        # ADR + 本计划
src/, tests/, main.py, ...   # 旧版 Python（M8 时删除）
```

## Tauri 命令（UI -> 后端）

```ts
get_status()                            -> Status
get_config()                            -> AppConfig
update_config(patch: PartialConfig)     -> AppConfig
set_camera_index(index: number)         -> void
start_calibration()                     -> void
cancel_calibration()                    -> void
pause_snooze(duration_seconds?: number) -> void   // null/undefined = 无限期
resume_snooze()                         -> void
set_language(lang: "zh-CN" | "en-US")   -> void
set_autostart(enabled: boolean)         -> void
quit_app()                              -> void
```

所有命令都是幂等的。错误以 `Result<T, AppError>` 返回，其中 `AppError` 是一个封闭的 Rust 枚举（Enum），不会向前端泄露内部细节。

## 后端事件（后端 -> UI）

```ts
"status-updated"          Status
"preview-frame"           { image_data_url, width, height, captured_at_ms }
"prompt-fired"            { kind, direction?, message_key, auto_dismiss_ms }
"camera-state-changed"    { state: "starting"|"available"|"unavailable", message_key? }
"calibration-updated"     CalibrationState  // idle | running | completed | failed
"config-updated"          AppConfig
"snooze-updated"          { state: "inactive"|"active"|"indefinite", until_iso? }
```

预览帧数据量大，走独立事件通道，不会拖慢 `status-updated`。

## Worker 协议（Rust 内部）

```rust
enum WorkerCommand {
    Tick,                                          // 由 10 Hz 定时器或测试注入
    UpdateConfig(AppConfig),
    SetCameraIndex(u32),
    StartCalibration { duration_seconds: f64 },
    CancelCalibration,
    PauseSnooze { duration_seconds: Option<u64> },
    ResumeSnooze,
    Shutdown,
}

enum WorkerEvent {
    StatusUpdated(StatusSnapshot),
    PreviewFrame { jpeg: Vec<u8>, width: u32, height: u32 },
    PromptFired(PromptEvent),
    WarningLevelChanged(WarningLevelEvent),
    CameraStateChanged(CameraState),
    CalibrationUpdated(CalibrationState),
    SnoozeUpdated(SnoozeState),
    LogEvent(EventLogEntry),
    Fatal(WorkerError),
}
```

Worker 不直接依赖任何 Tauri 类型。`events.rs` 负责把 `WorkerEvent` 转译为 Tauri 的 `emit_all`/`emit` 调用，这样 Worker 可以用假的摄像头和检测器独立做单元测试。

## 里程碑与验收

### M1 — Tauri 骨架

- [ ] `cargo tauri dev` 能在 Windows 上打开一个空白的 React 主窗口。
- [ ] 托盘图标显示正常。
- [ ] 关闭主窗口后窗口隐藏，进程继续运行。
- [ ] 托盘菜单显示 Show / Settings（占位） / Quit。
- [ ] Quit 能真正退出程序。
- [ ] `tauri-plugin-single-instance` 启用——第二次启动时聚焦已有窗口。

### M2 — Rust 领域核心 + 移植测试

- [ ] `domain::classifier` 的 Rust 单元测试覆盖 `tests/test_classifier.py` 中的所有用例。
- [ ] `domain::posture_tick_engine` 与 `tests/test_posture_tick_engine.py` 对齐。
- [ ] `domain::snooze::evaluate_snooze` 与 `tests/test_snooze_evaluation.py` 对齐，包括畸形/无限期/过期/活跃等场景。
- [ ] `domain::calibration` 与 `tests/test_calibration.py` 对齐（中位数位姿 + 会话生命周期）。
- [ ] `domain::display_plan` 与 `tests/test_display_plan.py` 对齐。
- [ ] `config::ConfigStore` 能读写 `~/.config/eyes/config.yaml`（或平台对应路径），缺失字段使用 serde 默认值，写入采用原子方式（临时文件 + 重命名）。
- [ ] `cargo test` 在 Linux 和 Windows 上全部通过。

### M3 — 摄像头预览

- [ ] Worker 通过 OpenCV Rust crate 打开摄像头 index 0。
- [ ] 主窗口通过 `preview-frame` 事件显示低帧率预览画面。
- [ ] 断开摄像头时发出 `camera-state-changed: unavailable`；UI 显示不可用提示。
- [ ] 5 秒重试后能恢复预览，无需重启应用。
- [ ] 启动时没有摄像头不会导致应用崩溃。
- [ ] 关闭主窗口后 Worker 继续运行；重新打开后能继续接收画面帧。

### M4 — ONNX 检测器验证

- [ ] 定义 `monitoring::detector::Detector` trait；`OnnxDetector` 为首个实现。
- [ ] 选定的人脸检测器 + 关键点头部姿态模型记录在 `docs/adr/0006-onnx-detector-choice.md`，包含许可证、体积和 CPU 延迟。
- [ ] 模型文件通过 `tauri.conf.json` 的 `bundle.resources` 打包；运行时通过 `app_handle.path()` 定位。
- [ ] 检测器返回 `Option<HeadPose { yaw, roll }>`；未检测到人脸时返回 `None`。
- [ ] Yaw 正负号约定与 README 定义一致（正值 = 头转向用户自己的右侧）。
- [ ] Windows 台式机上单帧 CPU 推理耗时 < 60 ms。
- [ ] 检测器 + 关键点资源合计 < 30 MB。
- [ ] 校准后，五种手动姿态（正前方、左转、右转、上看、下看）产生正确的正负方向。
- [ ] 1 分钟连续推理保持稳定（无 panic，无明显泄漏）。
- [ ] 此验证限时 2 个工作日。若无候选方案通过，回退到 OpenCV YuNet 加简单几何估算，确保 M5 不被阻塞。

### M5 — 监测闭环

- [ ] Worker 集成 Detector → Classifier → PostureTickEngine → 事件。
- [ ] 主窗口实时显示 yaw / roll / 姿态状态。
- [ ] 所有 `WarningLevel` 转换（NORMAL → WARNING → SEVERE → CORRECTED → NORMAL）可在本机手动复现。
- [ ] `prompt-fired` 驱动 Tauri 提醒窗口，用于纠正 / 好姿态 / 护眼 / 已纠正。
- [ ] 提醒窗口始终保持置顶，并能自动消失。
- [ ] 暂停 30 分钟、1 小时、无限期均能正确静音提醒；托盘和 UI 状态反映暂停状态。
- [ ] 恢复后重新启用提醒。
- [ ] 应用重启后能正确恢复暂停状态，覆盖活跃/过期/无限期/畸形四种场景（与 `evaluate_snooze` 结果一致）。
- [ ] JSONL 日志捕获 STATE_CHANGE / PROMPT_FIRED / WARNING_LEVEL_CHANGED / CAMERA_UNAVAILABLE / CAMERA_RESUMED / SNOOZE_START / SNOOZE_END。

### M6 — 设置 + 校准 UI + Windows 开机自启

- [ ] 设置页面可编辑 yaw 阈值、roll 阈值、摄像头编号、语言、sound_enabled、autostart_enabled。
- [ ] 高级时间字段（`off_axis_streak_threshold_seconds`、`off_axis_repeat_interval_seconds`、`off_axis_severe_threshold_seconds`、`facing_threshold_seconds`、`eyest_threshold_seconds`）能通过 YAML 正确读写。
- [ ] 保存时发送 `update_config`；Worker 立即应用新阈值和新的中性位姿。
- [ ] 更换摄像头编号后重新打开摄像头，重试路径保持正确。
- [ ] 切换语言后主窗口、提醒窗口、托盘菜单文字同步刷新。
- [ ] 校准按钮启动 5 秒校准会话；UI 显示剩余秒数和采样数。
- [ ] 校准期间提醒静音。校准完成后写入 `neutral_yaw` / `neutral_roll`，引擎立即采用新的中性位姿。
- [ ] 整个校准窗口期间都检测不到人脸时发出 `CalibrationFailed(NoFace)`，配置不变。
- [ ] 取消校准能干净地中止。
- [ ] `tauri-plugin-autostart` 切换 Windows 用户级开机自启；下次登录时自动启动需手动验证。
- [ ] `sound_enabled` 持久化并在 UI 中反映；音频播放功能可以暂不实现。

### M7 — Windows 打包

- [ ] `cargo tauri build` 生成可安装的 MSI（NSIS 作为备选方案也可接受）。
- [ ] 安装后从开始菜单启动正常。
- [ ] OpenCV（`opencv_world*.dll`）和 ONNX Runtime（`onnxruntime.dll`）包含在安装程序中；在干净机器上不会出现"找不到 DLL"错误。
- [ ] ONNX 模型文件包含在安装程序中，通过 `app_handle.path()` 定位。
- [ ] 配置和日志写入 `%APPDATA%\eyes\`，不写入安装目录。
- [ ] 卸载时删除应用文件；用户配置保留（在 README 中说明）。
- [ ] 安装包体积记录在 README 中，作为后续优化的基准。
- [ ] 至少在一台干净的 Windows 机器或虚拟机上验证通过。
- [ ] `models/MANIFEST.toml` 列出模型文件名、sha256、来源 URL 和许可证；启动时记录当前使用的模型版本。

### M8 — 旧代码清理

- [x] Rust 应用通过 M1 到 M7 的全部验收。
- [x] 删除 `src/eyes/`、`tests/`、`main.py`、`eyes.spec`、`eyes-linux.spec`、`pyproject.toml`、`uv.lock`、`.venv/`、`scripts/build*.py`。
- [x] 删除 `.github/workflows/linux-build.yml`（Python 流水线）。
- [x] 为 Rust/Tauri 版本重写 `README.md` 和 `README_zh.md`；旧 README 存档到 `docs/legacy/`。
- [x] 迁移提交使切换点易于定位。
- [x] 仓库根目录只剩 Rust、Tauri、前端、模型、文档。

## 持续集成

| 流水线 | 触发方式 | 运行环境 | 用途 |
| --- | --- | --- | --- |
| `linux-build.yml`（已删除） | — | — | 旧版 Python 构建/测试；M8 已移除 |
| `rust-test.yml`（待建） | push、PR | ubuntu-latest + windows-latest | `cargo fmt --check`、`cargo clippy`、`cargo test`。Linux 任务仅覆盖 `domain/*`，不依赖 OpenCV/ONNX。Windows 任务覆盖整个 crate。 |
| `tauri-build.yml`（待建） | 手动 + tag | windows-latest | `cargo tauri build`，上传 MSI 产物供手动冒烟测试。不涉及打包的 PR 无需此流水线通过。 |

领域测试设计为不依赖 OpenCV 或 ONNX 即可运行，保持快速和可移植。摄像头/检测器代码通过 `Detector` trait 抽象，集成层可在需要时用假检测器测试。

## 插件选型

- `tauri-plugin-autostart` — Windows 用户级开机自启。
- `tauri-plugin-single-instance` — 必须启用；防止自启 + 手动启动时产生两个摄像头 Worker。
- `tauri` 核心 — 托盘、窗口、IPC、打包。
- `tauri-plugin-log` 或 `tracing` + `tracing-subscriber` — 后端诊断日志（与 JSONL 业务日志分开）。

## 迁移基准

`tests/` 下的 Python 测试套件是行为正确性的迁移基准（Migration Oracle）。M2 中移植的每个领域模块，对应的 Rust 测试至少要覆盖相同的场景。行为偏差视为 Rust 端的 bug。

摄像头/检测器层的集成测试使用：

- 假摄像头：返回录制好的 BGR 帧。
- 假检测器：按脚本返回 `HeadPose` 序列。

这样 `cargo test` 在 CI 运行器上无需安装 OpenCV/ONNX 也能通过。
