//! 声音提醒：订阅 `MonitoringEvent::SoundAlert`，通过 `bevy_kira_audio` 播放 wav。
//!
//! 与 VS4 的系统通知是平行通道——一条走系统 toast，一条走扬声器，互不依赖。
//!
//! 音频资产由 `build.rs` 在构建期生成到 `OUT_DIR`，运行时用 `include_bytes!`
//! 嵌入二进制，无需随包分发 wav 文件。`alert_type` 映射：
//! - `"posture"`（偏头纠正）→ `off_axis.wav`：短促上升音
//! - `"eyerest"`（护眼提醒）→ `eyest.wav`：下降音
//!
//! `AppConfig.sound_enabled=false` 时静默（不播放）。

use std::io::Cursor;

use bevy::prelude::*;
use bevy_kira_audio::{Audio, AudioControl, AudioSource};
use kira::sound::static_sound::StaticSoundData;

use crate::monitoring::events::MonitoringEvent;

// build.rs 写入 OUT_DIR 的 wav 字节，编译期嵌入。
const OFF_AXIS_WAV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/off_axis.wav"));
const EYEST_WAV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/eyest.wav"));

/// 两段提示音的资产句柄，setup 阶段加载一次，后续重复播放。
#[derive(Resource)]
pub struct SoundAssets {
    posture: Handle<AudioSource>,
    eyerest: Handle<AudioSource>,
}

/// Startup：把两段嵌入的 wav 解码为 `AudioSource` 并注册到 `Assets`。
pub fn setup_audio(mut commands: Commands, mut assets: ResMut<Assets<AudioSource>>) {
    commands.insert_resource(SoundAssets {
        posture: assets.add(decode_source(OFF_AXIS_WAV)),
        eyerest: assets.add(decode_source(EYEST_WAV)),
    });
}

/// 把嵌入的 wav 字节解码为 kira 可播放的 `AudioSource`。
///
/// `expect` 合理：字节由 `build.rs` 生成，格式固定，失败即构建产物损坏。
fn decode_source(bytes: &'static [u8]) -> AudioSource {
    AudioSource {
        sound: StaticSoundData::from_cursor(Cursor::new(bytes.to_vec()))
            .expect("build.rs 生成的 wav 应可被 kira 解码"),
    }
}

/// 订阅 `SoundAlert`：`sound_enabled=false` 时跳过；否则按 `alert_type` 播放对应 wav。
pub fn play_sound_alert_system(
    mut reader: EventReader<MonitoringEvent>,
    config: Res<crate::AppResources>,
    sounds: Res<SoundAssets>,
    audio: Res<Audio>,
) {
    if !config.config_state.get().sound_enabled {
        // 关闭声音时消费掉事件，避免积压。
        reader.clear();
        return;
    }

    for event in reader.read() {
        if let MonitoringEvent::SoundAlert { alert_type } = event {
            match alert_type.as_str() {
                // 偏头纠正 → 短促上升音。
                "posture" => {
                    audio.play(sounds.posture.clone());
                }
                // 护眼提醒 → 下降音。
                "eyerest" => {
                    audio.play(sounds.eyerest.clone());
                }
                _ => {}
            }
        }
    }
}
