pub mod calibration_view;
pub mod contract;
pub mod main_view;
pub mod notification;
pub mod platform;
pub mod settings_view;
pub mod tray;

use bevy::prelude::*;

/// 控制主窗口渲染哪一组 UI 节点：主视图、设置面板或校准视图。
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    #[default]
    Main,
    Settings,
    /// VS3 校准流程：倒计时采集 + 实时姿态显示。
    Calibration,
}
