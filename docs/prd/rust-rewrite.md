# PRD — Eyes 全量 Rust/Tauri 重写

## 问题陈述

我是 Eyes 的维护者，也是唯一的用户。目前这个应用是 Python 3.12 + PySide6 + MediaPipe 的桌面程序，通过摄像头观察我的头部姿态，提示我坐直或休息眼睛。它能正常工作，但我想把整个技术栈迁移到 Rust——这既是 vibe coding 的学习练习，也是为了在保持相同产品行为的前提下，用 Rust + Tauri 应用替换 Python/PySide/MediaPipe，以便后续持续迭代。我不打算用 PyO3 绑定做渐进式核心抽取——迁移结束时，Python 实现要彻底消失。

当前实现还把我绑在 MediaPipe FaceLandmarker、一个重量级的 Python 安装包、以及一个在 Windows 上显得笨拙的 PyInstaller 打包方案上。这次重写正好让我切换到 `ort`（ONNX Runtime）、Rust 版 OpenCV 和 Tauri 安装程序。

## 方案

将 Eyes 完全用 Rust 重写，以 Tauri 2 桌面应用的形式发布，前端使用 React + TypeScript + Vite 做薄薄的一层。Rust 后端负责摄像头、检测器、姿态状态机、配置和系统集成（托盘、开机启动、单实例）。Web 前端负责主窗口、置顶提醒窗口和设置页面，通过 Tauri 命令和事件与后端通信。

功能对等是刚性约束：

- 相同的头部姿态纠正、"坐姿良好"表扬和眼睛休息提醒，在相同的配置间隔下触发。
- 相同的警告级别状态机（NORMAL → WARNING → SEVERE → CORRECTED → NORMAL）驱动窗口内横幅。
- 相同的校准流程（5 秒中位数姿态）和相同的贪睡持久化语义（`null` / `"indefinite"` / 未来 ISO 时间，含格式错误/过期处理），跨重启生效。
- 现有 YAML 配置文件可兼容读写，确保我当前的设置在切换后依然有效。

ONNX 检测器替代 MediaPipe。默认头部姿态路径是 2D 关键点加 OpenCV `solvePnP`。检测对等是**行为层面**的：校准后的姿态分类和提醒要与 Python 版本一致，但原始 yaw/roll 数值不需要与 MediaPipe 匹配。

迁移分八个可运行的里程碑（M1 Tauri 骨架 → M8 遗留代码清理）执行。Python 源码、测试和 CI 在 M7 验收前保留在仓库中不做改动，M8 时移除。

第一阶段的必选平台是 Windows。macOS 和 Linux 在 Windows 完全验证后再考虑。

## 用户故事（User Story）

1. 作为维护者，我希望有一个带 React + TypeScript + Vite 前端的 Tauri 2 应用骨架，以便我在熟悉的壳子上 vibe coding 完成后续重写工作。
2. 作为用户，我希望启动应用时主窗口自动打开，以便我能看到 Eyes 正在运行。
3. 作为用户，我希望应用运行时系统托盘图标始终可见，以便我随时了解它的状态。
4. 作为用户，我希望关闭主窗口时只是隐藏而非退出，以便监控能在后台默默继续。
5. 作为用户，我希望托盘菜单提供"显示""设置"和"退出"选项，以便我能回到应用或主动结束会话。
6. 作为用户，我希望只有"退出"才会真正结束进程，以免我误杀后台监控。
7. 作为用户，我希望同时最多只有一个 Eyes 实例运行，以免开机启动和手动启动同时抢占摄像头。
8. 作为用户，我希望第二次启动时能把已有窗口带到前台，以便"双击图标"的体验符合操作系统预期。
9. 作为维护者，我希望 Rust 后端在现有的平台路径上读写现有 YAML 配置，以便我已配置的阈值、校准、贪睡状态、语言和开机启动偏好在迁移后依然有效。
10. 作为维护者，我希望配置写入是原子操作（临时文件 + 重命名），以免写入中途崩溃导致配置损坏。
11. 作为维护者，我希望缺失的配置字段自动填充默认值，以便较旧或不完整的配置文件仍能加载。
12. 作为用户，我希望保留相同的头部姿态分类器语义（FACING_SCREEN、OFF_AXIS_LEFT、OFF_AXIS_RIGHT、OFF_AXIS_OTHER、NO_FACE），以便重写后产品感觉不变。
13. 作为用户，我希望保留相同的姿态 tick 引擎语义——偏头连击与重复间隔、正视时间表扬、在场时间眼睛休息提醒、以及警告级别状态机——以便提醒时机不变。
14. 作为用户，我希望保留相同的校准流程（5 秒，yaw 和 roll 的中位数），以便重新校准产生我习惯的基线。
15. 作为用户，我希望校准运行期间提醒静音，以免校准提示被其他提示淹没。
16. 作为用户，我希望全程无人脸的校准失败时不影响已保存的中性姿态，以免一次糟糕的校准毁掉我的基线。
17. 作为用户，我希望能在校准进行中取消它，以便被打断时可以中止。
18. 作为用户，我希望保留和现在一样的贪睡选项——30 分钟、1 小时、无限期、以及恢复——通过托盘菜单操作，以便暂停提醒的体验与原来一致。
19. 作为用户，我希望贪睡状态以 Python 相同的方式跨重启持久化（`null`、`indefinite`、未来 ISO，含格式错误/过期处理），以免退出再打开时丢失或误恢复我的暂停状态。
20. 作为用户，我希望贪睡期间检测继续运行但提醒保持静音，以便恢复时能立即接上。
21. 作为维护者，我希望有一个 Rust 监控 Worker 拥有摄像头、检测器和姿态引擎，以便 Tauri 命令处理层保持精简、Worker 可以独立测试。
22. 作为维护者，我希望 Worker 由外部 `Tick` 命令以 10 Hz 驱动，以便测试可以确定性地控制时间而不依赖真实时钟。
23. 作为用户，我希望主窗口显示实时的 yaw/roll 读数和当前姿态状态徽标，以便我能看到检测器报告的内容。
24. 作为用户，我希望主窗口显示低帧率的摄像头预览，以便我确认摄像头工作正常、构图正确。
25. 作为用户，我希望摄像头不可用时在主窗口显示明确的横幅，以便我注意到监控实际没有在运行。
26. 作为用户，我希望摄像头不可用时每 5 秒自动重试，以便我重新插上摄像头后能自动恢复。
27. 作为用户，我希望启动时摄像头缺失或被占用不会导致应用崩溃，以便我仍能打开设置调整摄像头编号。
28. 作为用户，我希望保留和现在一样的窗口内警告横幅行为（NORMAL → WARNING → SEVERE → CORRECTED → NORMAL），以便视觉升级的体验熟悉。
29. 作为用户，我希望纠正提示（左/右）、坐姿良好表扬、眼睛休息提醒和"已纠正"确认出现在一个置顶的自动消失提醒窗口中，以便我获得与现在一致的覆盖体验（与 ADR 0004 一致）。
30. 作为用户，我希望即使主窗口隐藏到托盘也能触发提醒，以便产品在后台仍能正常工作。
31. 作为维护者，我希望头部姿态检测器使用通过 `ort` 加载的 ONNX 人脸检测 + 关键点/头部姿态模型，以便不再绑定 MediaPipe 的 Python 包。
32. 作为维护者，我希望有一个 `Detector` trait，使 ONNX 实现可以被替换或伪造，以便集成测试不必加载真实模型。
33. 作为维护者，我希望默认头部姿态路径是 2D 关键点 + OpenCV `solvePnP`，以便不依赖模型提供 3D 关键点深度。
34. 作为维护者，我希望在 ADR 中记录所选 ONNX 模型的许可证、来源、文件大小和 CPU 推理延迟，以便后续维护有据可查。
35. 作为维护者，我希望模型选型探查的时间上限是两个工作日，并有备选方案（OpenCV YuNet + 更简单的几何方法），以免检测器选择悄悄吞噬迁移进度。
36. 作为用户，我希望 yaw 正负号约定不变（正值 = 头转向自己的右侧），以便校准基线仍然对应"目视前方"。
37. 作为维护者，我希望 yaw/roll 的对等标准是行为而非数值，以便只要分类和提示结果一致，任何合理的 ONNX 模型都可以使用。
38. 作为用户，我希望设置页面暴露 yaw 阈值、roll 阈值、摄像头编号、语言、声音开关和开机启动开关，以便我在 UI 中就能配置 Eyes 而不必编辑 YAML。
39. 作为用户，我希望高级时间字段（偏头连击阈值、偏头重复间隔、正视时间阈值、眼睛休息阈值）即使不在 UI 中展示也能在 YAML 中完整往返，以便高级用户仍能调优时间参数。
40. 作为用户，我希望保存设置后立即生效于运行中的 Worker，以便我能反复调整阈值而不必重启。
41. 作为用户，我希望在设置中更改摄像头编号后通过重试路径重新打开摄像头，以便我切换摄像头时不会丢失不可用横幅行为。
42. 作为用户，我希望切换语言后主窗口、提醒窗口和托盘菜单文字全部刷新，以便应用的本地化一致。
43. 作为用户，我希望 UI 至少支持简体中文和美式英语，以便我使用熟悉的语言。
44. 作为用户，我希望设置页面上有"校准"按钮，能触发 Worker 侧的 5 秒校准会话，以便我不需要单独的流程就能重新校准。
45. 作为用户，我希望校准 UI 显示剩余秒数和采样数量，以便我了解进度并确认样本正在采集。
46. 作为用户，我希望校准完成后将 `neutral_yaw` 和 `neutral_roll` 写入 YAML 并立即生效于运行中的分类器，以便我不需要重启。
47. 作为用户，我希望 Windows 开机启动开关实际在用户级别注册/注销启动项，以便 Eyes 确实随我的会话启动。
48. 作为维护者，我希望开机启动通过 `tauri-plugin-autostart` 实现，以免手动维护注册表/快捷方式代码。
49. 作为用户，我希望 `sound_enabled` 作为设置持久化并在 UI 中展示，以便即使将来再加音频播放，开关也能正常往返。
50. 作为维护者，我希望所有后端事件通过一套小而稳定的 schema 流转（status-updated、preview-frame、prompt-fired、camera-state-changed、calibration-updated、config-updated、snooze-updated），以便前端可以基于固定契约编码。
51. 作为维护者，我希望预览帧走独立的事件通道，以免高频 JPEG 载荷膨胀状态更新。
52. 作为维护者，我希望在平台数据目录下写入 JSONL 事件日志，覆盖 STATE_CHANGE、PROMPT_FIRED、CAMERA_UNAVAILABLE、CAMERA_RESUMED、SNOOZE_START、SNOOZE_END 和 WARNING_LEVEL_CHANGED，以便事后审查行为。
53. 作为维护者，我希望 JSONL 日志与 Python 事件日志信息等价而非字节等价，以便采用结构化字段（kind、timestamp、payload）而不必拖着旧格式走。
54. 作为维护者，我希望后端诊断日志（tracing）和 JSONL 业务日志分开，以免一方淹没另一方。
55. 作为维护者，我希望现有 Python 测试充当迁移基准，以便姿态、贪睡、校准、分类器、显示方案或配置的任何偏差都被视为 Rust bug，而非"Rust 选了不同的做法"。
56. 作为维护者，我希望每个移植的领域模块都有 Rust 单元测试覆盖至少与 Python 对等的场景，以便行为对等可以在 CI 中验证。
57. 作为维护者，我希望摄像头和检测器层通过 fake（录制的帧 + 脚本化的头部姿态序列）做集成测试，以便领域 CI 不需要 OpenCV 或 ONNX Runtime。
58. 作为维护者，我希望 Tauri Windows 安装程序捆绑 ONNX Runtime、OpenCV 运行时和所选 ONNX 模型文件，以便全新机器首次运行时不需要下载任何东西。
59. 作为用户，我希望安装后的应用将配置和日志写到 `%APPDATA%\eyes\` 下，以免卸载时带走我的设置、安装目录保持只读。
60. 作为用户，我希望卸载只移除应用文件而不触碰我的用户配置（在 README 中记录），以便重装时基线保留。
61. 作为维护者，我希望在 README 中记录安装程序大小作为基线，以便后续包体积优化有参考。
62. 作为维护者，我希望遗留 Python 实现、测试、打包和 CI 在 M7 之前原样保留在仓库中，以便迁移基准可用、回滚简单。
63. 作为维护者，我希望 M8 移除所有 Python 源码、测试、打包和 Python CI 工作流，以便仓库最终成为一个干净的 Rust/Tauri 项目。
64. 作为维护者，我希望新的 `cargo test` CI 任务拆分为 Linux 领域专用运行和 Windows 完整运行，以便领域回归快速被捕获，只有重量级任务才需要 OpenCV/ONNX。
65. 作为维护者，我希望有一个单独的 Tauri 构建 CI 任务按需产出 MSI 产物，以便冒烟安装方便，而不必让每个 PR 都跑完整打包流水线。
66. 作为维护者，我希望头部姿态检测器和 Worker 的其余部分通过 trait 接线，以便 Worker 的行为测试可以在没有 ONNX Runtime 的情况下运行。
67. 作为用户，我希望重写后以单个 MSI 安装程序发布（NSIS 作为备选），以便安装只需一个熟悉的 Windows 步骤。
68. 作为维护者，我希望代码签名延期并在 future TODO 中记录，以免 M7 被证书申请阻塞。
69. 作为维护者，我希望这个 PRD 作为 M1 到 M8 的 issue 级拆分的父级，以便每个里程碑都有自己的 ready-for-agent 工单。

## 实现决策

### Shell、前端和进程布局

- Tauri 2 桌面应用；React + TypeScript + Vite 前端。前端保持轻薄，只负责渲染状态更新、设置页面和提醒窗口。所有业务逻辑在 Rust 中。
- 单个 Tauri Rust crate，内部分 `domain/`、`monitoring/`、`platform/` 模块。暂不用 Cargo workspace；如果将来库边界有必要，再提取 `eyes-core`。
- `tauri-plugin-single-instance` 是强制要求——开机启动加手动启动不能产生两个摄像头 Worker。第二次启动时聚焦已有窗口。
- `tauri-plugin-autostart` 处理 Windows 用户级开机启动，免去手动维护注册表/快捷方式代码。
- 后端诊断日志使用 `tracing`（或 `tauri-plugin-log`），与 JSONL 业务日志分开。

### 监控 Worker 契约

监控 Worker 拥有 `CameraSource`、`Detector` trait、`PostureTickEngine` 和 `CalibrationSession`。它不依赖任何 Tauri 类型，因此可以用 fake 做单元测试。Tauri 命令处理层负责 Tauri 事件/命令与 Worker 枚举之间的适配。

设计讨论中确定的 Worker 契约，用 Rust 枚举表示：

```rust
enum WorkerCommand {
    Tick,
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

`Tick` 由外部以 10 Hz 注入，测试可以确定性地控制时间，与 `PostureTickEngine` 当前的做法相同。

### Tauri 命令和事件 schema

前端 → 后端命令（封闭列表，幂等，返回 `Result<T, AppError>`，其中 `AppError` 是封闭的 Rust 枚举）：

- `get_status() -> Status`
- `get_config() -> AppConfig`
- `update_config(patch: PartialConfig) -> AppConfig`
- `set_camera_index(index: u32)`
- `start_calibration()`
- `cancel_calibration()`
- `pause_snooze(duration_seconds?: u64)`  // 省略 = 无限期
- `resume_snooze()`
- `set_language(lang: string)`
- `set_autostart(enabled: bool)`
- `quit_app()`

后端 → 前端事件：

- `status-updated` — `Status` 快照（姿态状态、最近 yaw/roll、警告级别、贪睡状态、摄像头状态、校准状态、语言）。
- `preview-frame` — `{ image_data_url, width, height, captured_at_ms }`，独立通道，不膨胀状态更新。
- `prompt-fired` — `{ kind: "correction" | "good_posture" | "eye_rest" | "corrected", direction?: "left" | "right", message_key, auto_dismiss_ms }`。
- `camera-state-changed` — `{ state: "starting" | "available" | "unavailable", message_key? }`。
- `calibration-updated` — `idle | running | completed | failed` 判别联合体。
- `config-updated` — 完整 `AppConfig`（UI 不必自己合并补丁）。
- `snooze-updated` — `{ state: "inactive" | "active" | "indefinite", until_iso? }`。

### 领域模块（深层，行为驱动测试）

- **Classifier** — 纯函数 `classify(pose, neutral, thresholds) -> PoseState`。
- **PostureTickEngine** — 拥有偏头连击、偏头重复间隔、正视时间累加器、在场时间累加器和警告级别状态机；每 tick 产出 `SenseEvent`。
- **SnoozeEvaluation** — 纯函数 `evaluate_snooze(iso, now) -> SnoozeState`，覆盖 `Inactive | Indefinite | Active | Expired | Malformed`。
- **CalibrationSession** — 5 秒采样生命周期（start、feed、tick、result），含中位数姿态计算。
- **ConfigStore** — 原子 YAML 读写，serde 默认值；保留现有 schema（yaw 阈值、roll 阈值、neutral yaw、neutral roll、摄像头编号、贪睡 ISO、声音开关、开机启动开关、语言、偏头连击阈值、偏头重复间隔、正视阈值、眼睛休息阈值）。
- **DisplayPlan** — 纯归约器，将 `PoseState`/`WarningLevelEvent` 历史转为 UI 方案（徽标文本/颜色、横幅可见性、自动消失时机）。
- **EventLog (JSONL)** — 追加写入的结构化日志，记录 `AppEventKind` 值（STATE_CHANGE、PROMPT_FIRED、CAMERA_UNAVAILABLE、CAMERA_RESUMED、SNOOZE_START、SNOOZE_END、WARNING_LEVEL_CHANGED）到平台数据目录。

### 检测和摄像头

- 摄像头采集使用 OpenCV Rust crate，在后端运行；前端不接触 `getUserMedia`。
- `Detector` trait 是检测与 Worker 其余部分之间的接缝。第一个实现 `OnnxDetector` 使用 `ort` 和一个由 2 天探查选出的人脸检测 + 关键点/头部姿态模型。选型结果记录在后续 ADR 中。
- 默认头部姿态路径是 2D 关键点送入 OpenCV `solvePnP` 推导 yaw/roll。如果在探查中找到一步到位的头部姿态 ONNX 模型，可以替换关键点 + `solvePnP` 路径；无论哪条路径，行为对等都是刚性约束。
- 检测对等是行为层面的。移植后的 Python 测试是提示时机、姿态分类和警告级别转换的基准；原始 yaw/roll 数值不需要与 MediaPipe 匹配。
- yaw 的正负号约定保持"正值 = 头转向用户自己的右侧"，校准语义不变。

### 提醒和主窗口体验

- 提醒在专用的 Tauri 置顶窗口中渲染，由 `prompt-fired` 驱动。这延续了 ADR 0004 的思路（提示面由我们掌控，操作系统通知器不是产品）。
- 主窗口显示低帧率编码预览（JPEG 通过 Tauri 事件传输）、实时 yaw/roll 读数、姿态状态徽标和警告横幅。推理使用原始帧；预览使用缩小的副本。
- 关闭主窗口只是隐藏。只有托盘的"退出"才结束进程。贪睡期间检测继续运行，提醒静音。

### 校准

- 5 秒 Worker 自管会话，语义与 Python 实现一致。
- 会话激活期间提醒静音。
- 结果由采集到的 yaw/roll 样本的中位数计算；缺失样本 = `CalibrationFailed(NoFace)`，已保存的中性姿态不受影响。
- 取消运行中的会话干净中止，不写入配置。

### 贪睡

- 持久化语义与 Python `SnoozeManager` 一致：`null` → 未激活，`"indefinite"` → 无限期，否则为 ISO 时间戳；naive 时间戳按 UTC 解释；`now == until` 视为已过期。
- 启动时，过期的贪睡自动清除并发出 `SNOOZE_END`；无限期或未到期的 ISO 贪睡成为当前状态；格式错误的值自动清除并记录日志。

### 配置兼容性

- Rust `AppConfig` 保留现有字段集，我当前的 YAML 无需手动迁移即可加载。
- 原子写入（临时文件 + 重命名）与 Python 实现一致。
- 未知的 YAML 字段被忽略；缺失字段回退到 serde 默认值，不完整的文件仍能加载。

### 国际化

- 轻量级前端字典，至少覆盖 `zh-CN` 和 `en-US`。
- 后端事件携带稳定的 `message_key`；前端将其映射为本地化字符串。
- 切换语言后主窗口、提醒窗口和托盘菜单文字全部刷新（必要时重建）。
- 第一个 Rust 版本不做系统语言自动检测。

### 日志

- JSONL 事件日志写到平台数据目录（Windows 上是 `%APPDATA%\eyes\`）。每行一个 JSON 对象：`{ ts, kind, payload }`。
- 与 Python 事件日志信息等价而非字节等价。
- 写日志失败是非致命的，只通过 tracing 表面。

### 打包、开机启动和平台

- Windows 是唯一的首阶段平台。macOS 和 Linux 延后。
- `cargo tauri build` 产出 MSI（NSIS 作为备选），捆绑 ONNX Runtime、OpenCV 运行时和所选 ONNX 模型文件。首次运行不下载任何东西。
- ONNX 模型放在捆绑资源目录下，运行时通过 Tauri app handle 的路径 API 解析。
- 配置和日志在 `%APPDATA%\eyes\` 下；安装目录保持只读。
- 代码签名延期并在 future TODO 中记录。
- `models/MANIFEST.toml`（或等效文件）记录模型文件名、sha256、来源 URL 和许可证；启动时记录当前生效的模型版本。

### CI

- 现有 `linux-build.yml`（Python）在 M8 之前不做改动。
- 新的 `cargo test` 任务在 `ubuntu-latest` 上运行（领域专用，无 OpenCV/ONNX）和 `windows-latest` 上运行（完整 crate）。`cargo fmt --check` 和 `cargo clippy` 包含在此任务中。
- 新的 Tauri 构建任务在 `windows-latest` 上为打标签的构建或手动触发运行，产出 MSI 产物。
- 领域测试不得依赖 OpenCV 或 ONNX Runtime。

### 迁移里程碑

执行分为八个里程碑。每个里程碑在 `docs/legacy/migration-plan.md` 中有自己的验收清单：

- M1 — Tauri 骨架
- M2 — Rust 领域核心 + 移植的测试
- M3 — 摄像头预览
- M4 — ONNX 检测器探查（有时间上限；备选方案已记录）
- M5 — 监控闭环（分类 → 提示 → 提醒窗口 → 日志 → 贪睡/恢复/警告级别）
- M6 — 设置 + 校准 UI + Windows 开机启动
- M7 — Windows 打包
- M8 — 遗留代码清理（移除 Python 源码、测试、打包和 CI；重写 README）

## 测试决策

- **怎样算好的重写测试：** 断言外部可观测行为——发出的事件、配置字节的往返、写入的 JSONL 行、给定 tick 流产生的提示序列——永远不测内部字段名或调用顺序。测试必须通过 `Tick` 和 `dt` 确定性地驱动时间，不使用真实时钟。测试不得要求 OpenCV 或 ONNX Runtime。
- **必须有 Rust 单元测试的模块**（镜像 Python 测试套件——它们就是迁移基准）：
  - Classifier — 必须覆盖与 Python `test_classifier` 相同的场景。
  - PostureTickEngine — 必须覆盖偏头连击/重复、正视时间表扬、在场时间眼睛休息、以及 `test_posture_tick_engine` 中当前断言的每一个警告级别转换。
  - SnoozeEvaluation — 必须覆盖 `Inactive`、`Indefinite`、`Active`、`Expired` 和 `Malformed` 情况，包括 naive 时间戳的 UTC 解释和 `now == until` 边界。
  - CalibrationSession — 必须覆盖 start/feed/tick/result 生命周期和中位数计算，包括奇数/偶数样本数量。
  - ConfigStore — 必须覆盖原子写入语义、缺失字段默认值、未知字段容忍度、以及与现有 YAML schema 的往返。
  - DisplayPlan — 必须覆盖 `test_display_plan` 中当前的归约器转换（徽标、横幅、自动消失）。
  - EventLog (JSONL) — 必须覆盖每个 `AppEventKind` 的序列化和追加写入契约。
- **通过 fake 做集成测试覆盖的模块**（无 OpenCV/ONNX 依赖）：
  - MonitoringWorker — 由注入的 `Tick`、返回录制 BGR 帧的 fake 摄像头和返回脚本化 `HeadPose` 序列的 fake 检测器驱动。测试验证发出的 `WorkerEvent` 流在贪睡、校准、警告级别和提示场景中的正确性。
  - Tauri 命令/事件适配器 — 验证 `WorkerEvent` 值转换为文档中记录的 Tauri 事件名和载荷形状。
- **在 Windows 上手动冒烟测试（不在 CI 中）：**
  - OnnxDetector 使用通过 `ort` 加载的真实模型——记录在 M4 探查 ADR 中；本 PRD 不要求自动化 Rust 测试。
  - 摄像头在物理断开/重连时的重试。
  - MSI 安装/卸载、开机启动开关、单实例行为、代码签名提示（或缺失）。
- **应镜像的先例：**
  - `tests/` 下的 Python 测试（尤其是 `test_classifier`、`test_posture_tick_engine`、`test_snooze_evaluation`、`test_calibration`、`test_config_store`、`test_display_plan`、`test_event_log`）定义了 Rust 移植版必须匹配的行为契约。
  - 现有的 `tests/test_controller.py` 和 `test_sense_loop.py` 展示了集成风格的断言，Rust 监控 Worker 测试在 fake 就位后应遵循同样的模式。

## 范围之外

- Rust ONNX 检测器与 Python MediaPipe 检测器之间 yaw/roll 的数值对等。
- 第一个 Rust 版本的 macOS 和 Linux 交付。Linux/macOS 的开机启动、打包和托盘细节在 Windows 完全验证后再考虑。
- `sound_enabled` 的音频播放。该标志在配置和 UI 中保留，但第一个 Rust 版本不需要真正播放声音。
- Windows 安装程序的代码签名。记录为 future TODO。
- WebView 侧的摄像头采集（`getUserMedia`）。
- Cargo workspace 拆分（如提取 `eyes-core` 为独立库）。保持在单个 Tauri crate 中。
- 用 TOML/JSON/SQLite 替换 YAML 配置格式。
- 为国际化实现系统语言自动检测。
- 在 Tauri Windows 现有能力之外，为置顶提醒窗口写新的透明度/多显示器/Wayland 方案。
- 用 JSONL 以外的文件格式替换现有事件日志 schema。
- 通过 PyO3 渐进迁移 Python 实现。

## 补充说明

- 这个 PRD 是里程碑级 issue 的父级。每个里程碑（M1–M8）在本 PRD 被接受后，应作为独立的 ready-for-agent issue 根据 `docs/legacy/migration-plan.md` 提交。
- 仓库中已有的 ADR 约束本次重写的行为：
  - ADR 0001（仅 yaw/roll——保持相同的轴）。
  - ADR 0002（累计时间计时器——在 PostureTickEngine 中保留）。
  - ADR 0003（MediaPipe 选型——Rust 版本由本次重写明确取代）。
  - ADR 0004（自定义置顶提醒窗口——通过 Tauri 提醒窗口延续）。
  - ADR 0005（本次重写方向）。
- M4 的模型选型探查必须产出 ADR 0006，记录所选 ONNX 模型的许可证、文件大小和 CPU 延迟。如果探查超出时间预算，回退到 OpenCV YuNet 加更简单的几何估算，以免阻塞 M5。
- 贪睡持久化语义、校准语义、警告级别状态机和 JSONL 事件类型是 vibe coding 过程中最容易悄悄退化的四个领域。正因如此，它们被移植的测试显式覆盖。
- 单实例行为不可妥协。没有它，开机启动加手动启动可能产生两个摄像头 Worker 争夺设备。
