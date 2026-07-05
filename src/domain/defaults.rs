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

/// 偏离严重升级时长阈值（秒）。
pub const OFF_AXIS_SEVERE_THRESHOLD: f64 = 10.0;

/// 正面朝向累积时长阈值（秒）。
pub const FACING_THRESHOLD: f64 = 300.0;

/// 用眼休息累积时长阈值（秒）。
pub const EYEREST_THRESHOLD: f64 = 900.0;

// ── yaw 估计补偿 ───────────────────────────────────────────────

/// yaw 估计补偿系数：yaw_drift ≈ k * pitch²。
///
/// 5 关键点 + 估算下巴的 6 点 DLT 在 pitch ≠ 0 时，估算下巴算法对
/// roll 旋转的传递不完全独立，导致 SVD 正交化引入 yaw 残差。漂移
/// 大致与 pitch 平方成正比（diagnostic 测试在 0°/10°/20°/30°/40°/50°/60°
/// 扫描下观测到）。补偿单位为「度 / 度²」：yaw_correction = k × pitch_deg²。
///
/// k=0.002 是经验值（基于 5 关键点 + 估算下巴的 6 点 DLT 默认布局）。
/// 改此值需重新校准：让用户左右偏头 10° 同时上下俯仰 30°，看 UI
/// 显示的 yaw 是否接近 10°。
pub const YAW_PITCH_COUPLING_K: f64 = 0.002;
