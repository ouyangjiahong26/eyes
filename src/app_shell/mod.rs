pub mod contract;
pub mod main_view;
pub mod settings_view;
pub mod tray;

use bevy::prelude::*;

/// 控制主窗口渲染哪一组 UI 节点：主视图或设置面板。
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    #[default]
    Main,
    Settings,
}
