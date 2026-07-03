//! Eyes 应用库入口。
//!
//! VS0 tracer bullet：构造一个最小 Bevy App，验证
//! "Bevy 窗口 + 系统托盘 + 后台 worker 线程"三者能共存并能干净退出。
//!
//! 不包含任何监控事件流、UI 内容。

pub mod app_shell;
pub mod app_state;
pub mod domain;
pub mod monitoring;
pub mod worker_setup;

use std::sync::{mpsc, Arc, Mutex};

use bevy::prelude::*;
use bevy::window::WindowResolution;

use app_shell::tray::{spawn_tray, TrayMenuCommand};
use domain::config::ConfigState;
use monitoring::channel::WorkerSender;
use worker_setup::spawn_worker;

/// 托盘命令通道，作为 Bevy Resource 注入主线程。
///
/// `mpsc::Receiver` 本身不 `Sync`，用 `Mutex` 包装后可跨 Bevy 系统共享。
#[derive(Resource)]
struct TrayCommands(Mutex<mpsc::Receiver<TrayMenuCommand>>);

/// 后台 worker 的命令发送端。退出时发 `Stop`。
#[derive(Resource)]
struct WorkerHandle(WorkerSender);

/// VS0 用到的全局配置资源。
#[derive(Resource)]
struct AppResources {
    config_state: Arc<ConfigState>,
}

/// 启动 Eyes 应用。
pub fn run() {
    let config_state = worker_setup::load_config_state();

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
        .add_systems(Startup, setup)
        .add_systems(Update, handle_tray_commands)
        .run();
}

fn setup(mut commands: Commands, resources: Res<AppResources>) {
    // 托盘：后台线程构建，命令通过 channel 推回主线程。
    let tray_rx = spawn_tray();
    commands.insert_resource(TrayCommands(Mutex::new(tray_rx)));

    // 后台 worker：NullSink 空转，不接事件流。
    let worker_tx = spawn_worker(resources.config_state.clone());
    commands.insert_resource(WorkerHandle(worker_tx));
}

/// 轮询托盘命令，转发为 Bevy 行为。
fn handle_tray_commands(
    tray: Res<TrayCommands>,
    worker: Res<WorkerHandle>,
    mut exit: EventWriter<AppExit>,
) {
    let rx = tray.0.lock().unwrap();
    // try_recv 非阻塞轮询；没事件时立即返回。
    while let Ok(cmd) = rx.try_recv() {
        match cmd {
            TrayMenuCommand::Quit => {
                // 先通知后台 worker 停止，再触发 Bevy 退出。
                use crate::monitoring::channel::WorkerCommand;
                let _ = worker.0.send(WorkerCommand::Stop);
                exit.send(AppExit::Success);
            }
            TrayMenuCommand::Open => {
                // VS0：窗口默认可见，Open 暂为 no-op。
                // 后续切片实现窗口隐藏/恢复时在此处理。
            }
        }
    }
}
