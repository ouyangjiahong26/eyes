//! 系统托盘：菜单、图标与事件转发。
//!
//! VS0 在独立线程里跑托盘，菜单事件通过 `mpsc::Sender` 推到主线程。
//! Bevy 端把对应 `Receiver` 装到 Resource，由 `Update` 系统轮询并转成
//! `AppExit` 等 Bevy 事件。
//!
//! Linux 平台的 `tray-icon` 依赖 GTK —— 因为 Bevy 主线程跑 winit 事件循环，
//! 需要单独一个线程调 `gtk::init()` 与 `gtk::main()`，由 GTK 的消息泵负责
//! 菜单事件分发。

use std::sync::mpsc::{self, Receiver, Sender};

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIconBuilder};

use crate::app_shell::contract::{
    MENU_PAUSE_30_ID, MENU_PAUSE_60_ID, MENU_PAUSE_INDEFINITE_ID, MENU_QUIT_ID, MENU_RESUME_ID,
    MENU_SHOW_ID,
};

/// 托盘菜单项的语义化命令。
///
/// VS0 只关心 Open / Quit；VS4 扩展了 Pause / Resume 用于 snooze 控制。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayMenuCommand {
    /// 用户点了 "Open"：把主窗口拉回前台。
    Open,
    /// 用户点了 "Quit"：请求干净退出。
    Quit,
    /// 暂停提醒 30 分钟。
    Pause30Min,
    /// 暂停提醒 1 小时。
    Pause1Hour,
    /// 暂停提醒直到重启。
    PauseUntilRestart,
    /// 恢复提醒。
    Resume,
}

/// 启动系统托盘，返回菜单命令的接收端。
///
/// 调用方应把 `Receiver` 装到 Bevy Resource，由主线程的轮询系统消费。
/// 该函数立即返回，托盘构建在后台线程完成。
pub fn spawn_tray() -> Receiver<TrayMenuCommand> {
    let (tx, rx) = mpsc::channel::<TrayMenuCommand>();
    std::thread::Builder::new()
        .name("eyes-tray".into())
        .spawn(move || run_tray_thread(tx))
        .expect("spawn tray thread");
    rx
}

fn run_tray_thread(tx: Sender<TrayMenuCommand>) {
    // Linux：必须在调 tray-icon 之前 init GTK，且当前线程就是 GTK 主线程。
    // 其他平台由 muda 在自己的线程里跑消息泵，不需要这个调用。
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = gtk::init() {
            eprintln!("[eyes] gtk init failed: {e}");
            return;
        }
    }

    // 全局菜单事件回调。muda 在自己的线程触发，回调里只做转发，
    // 避免阻塞 GTK/muda 的事件分发。
    let tx_open = tx.clone();
    let tx_quit = tx.clone();
    let tx_pause30 = tx.clone();
    let tx_pause60 = tx.clone();
    let tx_pause_inf = tx.clone();
    let tx_resume = tx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        match event.id().as_ref() {
            MENU_SHOW_ID => {
                let _ = tx_open.send(TrayMenuCommand::Open);
            }
            MENU_PAUSE_30_ID => {
                let _ = tx_pause30.send(TrayMenuCommand::Pause30Min);
            }
            MENU_PAUSE_60_ID => {
                let _ = tx_pause60.send(TrayMenuCommand::Pause1Hour);
            }
            MENU_PAUSE_INDEFINITE_ID => {
                let _ = tx_pause_inf.send(TrayMenuCommand::PauseUntilRestart);
            }
            MENU_RESUME_ID => {
                let _ = tx_resume.send(TrayMenuCommand::Resume);
            }
            MENU_QUIT_ID => {
                let _ = tx_quit.send(TrayMenuCommand::Quit);
            }
            _ => {}
        }
    }));

    let menu = build_menu();
    let icon = build_icon();

    let _tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Eyes")
        .with_icon(icon)
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[eyes] tray icon build failed: {e}");
            return;
        }
    };

    // Linux：阻塞运行 GTK 主循环，保持托盘存活并处理菜单点击。
    #[cfg(target_os = "linux")]
    gtk::main();

    // 其他平台：muda 在自己的线程里分发菜单事件；
    // 这里保持托盘线程存活，防止 `_tray` 析构后图标消失。
    // 永不关闭的 channel → recv() 永久阻塞。
    #[cfg(not(target_os = "linux"))]
    {
        let (_alive_tx, alive_rx) = mpsc::channel::<()>();
        let _ = alive_rx.recv();
    }
}

fn build_menu() -> Menu {
    let show = MenuItem::with_id(MENU_SHOW_ID, "Open", true, None);
    let pause_30 = MenuItem::with_id(MENU_PAUSE_30_ID, "Pause 30 min", true, None);
    let pause_60 = MenuItem::with_id(MENU_PAUSE_60_ID, "Pause 1 hour", true, None);
    let pause_indef = MenuItem::with_id(
        MENU_PAUSE_INDEFINITE_ID,
        "Pause until restart",
        true,
        None,
    );
    let resume = MenuItem::with_id(MENU_RESUME_ID, "Resume", true, None);
    let quit = MenuItem::with_id(MENU_QUIT_ID, "Quit", true, None);
    let menu = Menu::new();
    let _ = menu.append_items(&[&show]);
    let _ = menu.append_items(&[&pause_30, &pause_60, &pause_indef, &resume]);
    let _ = menu.append_items(&[&quit]);
    menu
}

/// 程序化生成 32×32 RGBA 图标。
///
/// VS0 不引入 asset 资源：用一个深蓝灰实色方块占位。
/// 后续切片在引入 `assets/` 资源时换成真实 PNG。
fn build_icon() -> Icon {
    const WIDTH: u32 = 32;
    const HEIGHT: u32 = 32;
    let mut rgba = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for _ in 0..(WIDTH * HEIGHT) {
        // 深蓝灰
        rgba.extend_from_slice(&[32, 64, 96, 255]);
    }
    Icon::from_rgba(rgba, WIDTH, HEIGHT).expect("build tray icon from raw RGBA")
}
