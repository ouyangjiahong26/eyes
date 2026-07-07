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

/// 线程安全的命令发送端，Bevy 系统通过它向 worker 发指令。
#[derive(Clone)]
pub struct WorkerSender(pub std::sync::mpsc::Sender<WorkerCommand>);

pub type WorkerReceiver = std::sync::mpsc::Receiver<WorkerCommand>;

pub fn channel() -> (WorkerSender, WorkerReceiver) {
    let (tx, rx) = std::sync::mpsc::channel();
    (WorkerSender(tx), rx)
}
