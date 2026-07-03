//! Eyes 应用库入口。
//!
//! VS1：端到端垂直切片——从摄像头采集到主窗口 UI 文本和图像。
//! VS2：设置面板（AppView 切换、阈值滑块、摄像头/语言选择、保存/取消、i18n 运行时刷新）。
//!
//! 监控事件经 `BevyEventSink` → mpsc channel → Bevy `Events<MonitoringEvent>`
//! → ECS 资源 → UI 节点。

pub mod app_shell;
pub mod app_state;
pub mod domain;
pub mod i18n;
pub mod monitoring;
pub mod worker_setup;

use std::sync::{mpsc, Arc, Mutex};

use bevy::prelude::*;
use bevy::window::WindowResolution;

use app_shell::main_view::{
    forward_monitoring_events, handle_settings_button_click, refresh_ui_text,
    setup_main_view, update_pose_state, update_preview_texture, update_view_visibility,
    MonitoringReceiver,
};
use app_shell::settings_view::{
    dispatch_button_click, handle_advanced_toggle, handle_autostart_toggle, handle_calibrate,
    handle_camera_nav, handle_cancel, handle_lang_nav, handle_save, handle_slider_click,
    handle_sound_toggle, refresh_settings_ui, setup_settings_panel, CancelSettings, SaveSettings,
    SettingsPanelState,
};
use app_shell::notification::{
    check_notification_capability, update_system_notification_system, SnoozeResource,
};
use app_shell::tray::{spawn_tray, TrayMenuCommand};
use domain::config::ConfigState;
use i18n::{refresh_localized_text, I18nTable};
use monitoring::channel::{WorkerCommand, WorkerSender};
use monitoring::events::MonitoringEvent;
use worker_setup::spawn_worker;

/// 托盘命令通道，作为 Bevy Resource 注入主线程。
///
/// `mpsc::Receiver` 本身不 `Sync`，用 `Mutex` 包装后可跨 Bevy 系统共享。
#[derive(Resource)]
struct TrayCommands(Mutex<mpsc::Receiver<TrayMenuCommand>>);

/// 后台 worker 的命令发送端。退出时发 `Stop`。
#[derive(Resource)]
pub struct WorkerHandle(WorkerSender);

/// 全局配置资源（Arc<ConfigState> 的 Bevy Resource 包装）。
#[derive(Resource)]
pub struct AppResources {
    pub config_state: Arc<ConfigState>,
}

/// 启动 Eyes 应用。
pub fn run() {
    // VS7：把安装目录加入 DLL 搜索路径，必须在任何 DLL 加载之前完成。
    app_shell::platform::add_resource_dll_dir();

    let config_state = worker_setup::load_config_state();
    let language = config_state.get().language.clone();

    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Eyes".into(),
                    resolution: WindowResolution::new(800.0, 600.0),
                    ..default()
                }),
                ..default()
            }),
        )
        .insert_resource(AppResources { config_state })
        .insert_resource(I18nTable::for_language(&language))
        .insert_resource(SettingsPanelState::default())
        .insert_resource(app_shell::AppView::default())
        .insert_resource(SnoozeResource::default())
        .add_event::<MonitoringEvent>()
        .add_event::<SaveSettings>()
        .add_event::<CancelSettings>()
        .add_systems(Startup, (setup, setup_main_view, setup_settings_panel))
        .add_systems(
            Update,
            (
                handle_tray_commands,
                forward_monitoring_events,
                update_pose_state,
                update_preview_texture,
                update_system_notification_system,
                refresh_ui_text,
                refresh_localized_text,
                handle_settings_button_click,
                update_view_visibility,
                // 设置面板交互
                handle_slider_click,
                handle_camera_nav,
                handle_lang_nav,
                handle_sound_toggle,
                handle_autostart_toggle,
                handle_advanced_toggle,
                handle_calibrate,
                dispatch_button_click,
                refresh_settings_ui,
                handle_save,
                handle_cancel,
            ),
        )
        .run();
}

fn setup(mut commands: Commands, resources: Res<AppResources>) {
    // 托盘：后台线程构建，命令通过 channel 推回主线程。
    let tray_rx = spawn_tray();
    commands.insert_resource(TrayCommands(Mutex::new(tray_rx)));

    // 监控事件通道：后台 worker → Bevy 主线程。
    let (event_tx, event_rx) = mpsc::channel::<MonitoringEvent>();
    commands.insert_resource(MonitoringReceiver(Mutex::new(event_rx)));

    // 后台 worker：用 BevyEventSink 把监控事件推到上面的 channel。
    let worker_tx = spawn_worker(resources.config_state.clone(), event_tx);
    commands.insert_resource(WorkerHandle(worker_tx));

    // 通知能力检测：失败只记日志，不阻塞启动。
    check_notification_capability();
}

/// 轮询托盘命令，转发为 Bevy 行为。
fn handle_tray_commands(
    tray: Res<TrayCommands>,
    worker: Res<WorkerHandle>,
    mut snooze: ResMut<SnoozeResource>,
    mut exit: EventWriter<AppExit>,
) {
    let rx = tray.0.lock().unwrap();
    // try_recv 非阻塞轮询；没事件时立即返回。
    while let Ok(cmd) = rx.try_recv() {
        match cmd {
            TrayMenuCommand::Quit => {
                // 先通知后台 worker 停止，再触发 Bevy 退出。
                let _ = worker.0.send(WorkerCommand::Stop);
                exit.send(AppExit::Success);
            }
            TrayMenuCommand::Open => {
                // 窗口默认可见，Open 暂为 no-op。
                // 后续切片实现窗口隐藏/恢复时在此处理。
            }
            TrayMenuCommand::Pause30Min => {
                let _ = worker.0.send(WorkerCommand::Snooze(30.0 * 60.0));
                snooze.paused = true;
            }
            TrayMenuCommand::Pause1Hour => {
                let _ = worker.0.send(WorkerCommand::Snooze(60.0 * 60.0));
                snooze.paused = true;
            }
            TrayMenuCommand::PauseUntilRestart => {
                let _ = worker.0.send(WorkerCommand::Snooze(f64::INFINITY));
                snooze.paused = true;
            }
            TrayMenuCommand::Resume => {
                let _ = worker.0.send(WorkerCommand::Resume);
                snooze.paused = false;
            }
        }
    }
}
