//! 前端 → worker 控制通道。

use crate::domain::config::AppConfig;

/// 前端 → worker 控制指令。
#[derive(Debug)]
pub enum WorkerCommand {
    SetCameraIndex(u32),
    SetConfig(Box<AppConfig>),
    Snooze(f64),
    Resume,
    StartCalibration,
    CancelCalibration,
    Stop,
}

/// 线程安全的命令发送端，Tauri command handler 通过它向 worker 发指令。
#[derive(Clone)]
pub struct WorkerSender(std::sync::mpsc::Sender<WorkerCommand>);

impl WorkerSender {
    pub fn send(&self, cmd: WorkerCommand) -> Result<(), String> {
        self.0.send(cmd).map_err(|_| "worker 已停止".to_string())
    }
}

pub type WorkerReceiver = std::sync::mpsc::Receiver<WorkerCommand>;

pub fn channel() -> (WorkerSender, WorkerReceiver) {
    let (tx, rx) = std::sync::mpsc::channel();
    (WorkerSender(tx), rx)
}
