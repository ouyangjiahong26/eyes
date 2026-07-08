pub mod contract;
pub mod platform;
pub mod system;
pub mod ui;

// 兼容旧路径：外部代码通过 `app_shell::main_view` 等访问时仍可解析。
pub use system::notification;
pub use system::tray;
pub use ui::calibration_view;
pub use ui::main_view;
pub use ui::settings_draft;
pub use ui::settings_view;

use bevy::prelude::*;

/// 控制主窗口渲染哪一组 UI 节点：主视图、设置面板或校准视图。
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    #[default]
    Main,
    Settings,
    /// 校准流程：倒计时采集 + 实时姿态显示。
    Calibration,
}
