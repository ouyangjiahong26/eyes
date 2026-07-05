# 0009 — 合并 PostureTickEngine 双计数器为单一连续时长

issue #84 重新评估 `PostureTickEngine` 的双 off-axis 计数器是否合并。原 issue #32（已关闭）已明确要求合并为单一 `continuous_seconds` 驱动 Correction 与 WarningLevel 两类事件，但 Rust 侧实现遗留了两个独立字段，且重置语义不对称。

## 考虑过的选项

- **保留 `streak` + `continuous_seconds` 双计数器，但显式化** —— 否决：与 #32 方向不一致；当前两事件已经几乎同步发生（10.0s 升 Severe、10.3s 第二次 Correction），合并后产品行为不变。
- **合并为单一 `continuous_seconds`，但 `continuous_seconds` 在升 Warning 时重置** —— 否决：实质抵消合并意义，又回到双时间轴。
- **合并为单一 `continuous_seconds`，起点 = 进入 off-axis** —— 采纳。
- **Normal → Warning 立即触发** —— 否决：与合并后的"同一时间轴"哲学错位，且与首次 Correction 不同步。
- **Normal/Corrected → Warning 等 `continuous_seconds >= streak_threshold`** —— 采纳：所有事件从同一时间轴派生。
- **Severe 阈值复用 `repeat_interval`** —— 否决：两件事是独立产品维度（"多久升级状态"与"多久重复提醒"），未来产品可能单独调整。
- **引入独立 `severe_threshold_seconds` 参数** —— 采纳（默认值 10.0s 与当前行为一致）。
- **Corrected 缓冲 2s 参数化** —— 否决：固定产品体验细节，不需要用户调节；`update_timing` 已经有 4 个参数，再加会膨胀配置 UI。
- **Corrected → Warning 立即触发** —— 否决：与 Normal → Warning 不对称，导致 UI 颜色变化与提醒弹窗时机错开。
- **Corrected → Warning 等 `streak_threshold`** —— 采纳：状态机对称优先。

## 决策

1. 删除 `OffAxisState.streak` 字段；只保留 `continuous_seconds`。
2. `continuous_seconds` 起点语义：进入本轴 off-axis（`is_off_for_self(state)` 为真）那一刻从 0 开始；切回 `FacingScreen` 或 `NoFace` 时重置为 0。
3. **Normal/Corrected → Warning 触发条件**：`is_off_for_self(state) && continuous_seconds >= streak_threshold`。与首次 Correction 同步发出。
4. **Warning → Severe 触发条件**：`continuous_seconds >= severe_threshold_seconds`（新增独立参数，默认 10.0s）。
5. **Correction 触发时机**：首次在 `continuous_seconds >= streak_threshold` 时发出，与 Warning 升级同帧（同一 `if` 块，`posture_tick_engine.rs:149-158`）；发出后置 `next_correction_threshold = continuous_seconds + repeat_interval`（`:156-157`），此后每越过该阈值再发一次并递增（`:170-175`）。Severe 升级是另一个独立判断 `continuous_seconds >= severe_threshold`（`:162`），与 Correction 阈值互不依赖。默认 `repeat_interval == severe_threshold_seconds == 10s` 时第二次 Correction 恰好与 Severe 升级同帧；但二者是独立可调的产品参数——`repeat_interval` 管“重复提醒间隔”、`severe_threshold_seconds` 管“状态升级时机”——独立调整后不再严格同步。这是设计意图，而非耦合（见选项“Severe 阈值复用 `repeat_interval`——否决”）。
6. **Corrected → Normal** 缓冲保持硬编码 2.0s，不参数化。
7. **NoFace 状态**：`reset_warning()` → WarningLevel = Normal 且 continuous_seconds = 0。直接清零，**不**走 Corrected 流程。
8. **FacingScreen 状态**：Warning/Severe → Corrected 缓冲 2s → Normal；`continuous_seconds` 在进入 Corrected 时清零。
9. **代码注释必须明确**：NoFace 与 FacingScreen 的重置语义不对称是**有意的产品设计**——人离开屏幕前警告无对象；主动纠正给正向反馈缓冲。
10. 新增常量 `defaults::OFF_AXIS_SEVERE_THRESHOLD: f64 = 10.0`。
11. `Config` 结构体新增字段 `off_axis_severe_threshold_seconds: f64`，默认等于常量。
12. `PostureTickEngine::new` 与 `update_timing` 各加一个 `severe_threshold_seconds` 参数。
13. `OffAxisState::new_yaw` / `new_pitch` 不再需要 streak_threshold/repeat_interval 之外的额外参数。
14. **双轴（yaw / pitch）仍各自独立**：每轴一个 `OffAxisState`、一个 `continuous_seconds`。这是产品上有意为之——yaw 偏离与 pitch 偏离是两件独立的事。

## 后果

- `OffAxisState` 字段数从 7 个减到 5 个（删 streak、删 last_emit_at、删 repeat_due_at；保留 warning_level、continuous_seconds、corrected_remaining_seconds）。新增字段起点语义由方法自身保证。
- 状态机更紧凑：所有 off-axis 派生事件从同一 `continuous_seconds` 字段判定。
- `update_timing` 参数从 4 个增加到 5 个：`off_axis_streak_threshold`、`off_axis_repeat_interval`、`off_axis_severe_threshold`、`facing_threshold`、`eyerest_threshold`。
- 配置文件 schema 新增 `off_axis_severe_threshold_seconds` 字段（迁移指南需更新）。
- 测试需要重写：当前 `tests/posture_tick_engine_behavior.rs` 内的 `update_streak_preserves_accumulated_state` 等测试需要适配新状态机；新增"WarningLevel 状态变更时机与 Correction 时机同步"的回归测试。
- #84 关闭后可继续 #85（yaw/pitch 阈值实测）——实测结果可能调整 `streak_threshold`、`repeat_interval`、`severe_threshold` 的默认值。
- `CONTEXT.md` 中"偏离连续时长（Off-Axis Streak）"条目描述的是 Python 时代的旧实现，需要在本 ADR 实施时同步更新术语：改为描述"`PostureTickEngine` 的 `continuous_seconds`"以及 WarningLevel FSM。
