// 全局默认值常量。
//
// 所有模块共享同一份默认值，消除跨文件重复定义。
// 修改默认值只需改此处。

// ── 分类器阈值 ───────────────────────────────────────────────────

/// yaw 轴偏离阈值（度）。
pub const YAW_DEG: f64 = 1.0;

/// yaw 轴滞后（度）。
pub const YAW_HYSTERESIS_DEG: f64 = 0.5;

/// pitch 轴偏离阈值（度）。
pub const PITCH_DEG: f64 = 5.0;

/// pitch 轴滞后（度）。
pub const PITCH_HYSTERESIS_DEG: f64 = 2.5;

// ── 姿态引擎阈值 ─────────────────────────────────────────────────

/// 偏离连续时长阈值（秒）。
pub const OFF_AXIS_STREAK_THRESHOLD: f64 = 0.3;

/// 偏离重复提醒间隔（秒）。
pub const OFF_AXIS_REPEAT_INTERVAL: f64 = 10.0;

/// 正面朝向累积时长阈值（秒）。
pub const FACING_THRESHOLD: f64 = 300.0;

/// 用眼休息累积时长阈值（秒）。
pub const EYEREST_THRESHOLD: f64 = 900.0;
