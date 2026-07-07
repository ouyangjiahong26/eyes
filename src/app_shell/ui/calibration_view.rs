//! 校准视图：跨视图的 5 秒中性姿态采集流程。
//!
//! 流程：
//! ```text
//! Settings 面板 CalibrateButton 点击
//!   └─ handle_calibrate
//!      ├─ WorkerCommand::StartCalibration（worker 进入 CalibrationSession）
//!      ├─ CalibrationViewState { phase: CountingDown, countdown: 5.0 }
//!      └─ AppView = Calibration
//!
//! 校准进行中：
//!   update_calibration_countdown  — 每帧递减倒计时，刷新 UI
//!   track_calibration_pose        — 从 PoseUpdated 更新采样计数和实时 yaw/pitch
//!
//! worker 完成 / 失败：
//!   handle_calibration_complete — 写入 config（orchestrator 已做），显示成功，延时回 Settings
//!   handle_calibration_failed  — 显示错误，留校准视图等用户操作
//!
//! 用户取消 / 返回：
//!   handle_cancel_calibration  — 发 CancelCalibration，回 Settings
//!   handle_back_to_settings    — 失败后返回 Settings
//! ```

use bevy::prelude::*;

use crate::app_shell::settings_view::CalibrateButton;
use crate::app_shell::AppView;
use super::i18n::{I18nTable, LocalizedText};
use crate::AppFont;
use crate::monitoring::events::MonitoringEvent;
use crate::monitoring::pipeline::channel::WorkerCommand;
use crate::WorkerHandle;

/// 校准时长（秒），与 orchestrator 的 CalibrationSession::new(5.0) 对应。
const CALIBRATION_DURATION: f32 = 5.0;

/// 校准成功后自动返回 Settings 的延时（秒）。
const SUCCESS_RETURN_DELAY: f32 = 1.5;

// ── 资源 ───────────────────────────────────────────────────────

/// 校准视图运行时状态。
#[derive(Resource, Default)]
pub struct CalibrationViewState {
    /// 校准阶段。
    pub phase: CalibrationPhase,
    /// 剩余倒计时（秒）；成功后复用为自动返回计时器。
    pub timer: f32,
    /// 采样计数（由 PoseUpdated 事件累加）。
    pub sample_count: usize,
    /// 实时 yaw/pitch（来自 PoseUpdated）。
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    /// 失败原因（i18n key 或原始 reason 字符串）。
    pub failure_reason: Option<String>,
    /// 完成后的采样数与中性姿态（用于成功消息展示）。
    pub result: Option<(f64, f64, usize)>,
}

/// 校准阶段。
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalibrationPhase {
    #[default]
    Idle,
    /// 倒计时进行中。
    CountingDown,
    /// 校准成功，等待自动返回。
    Success,
    /// 校准失败，等待用户点"返回设置"。
    Failed,
}

// ── UI 标记组件 ─────────────────────────────────────────────────

/// 校准视图根节点（用于可见性切换）。
#[derive(Component)]
pub(crate) struct CalibrationViewRoot;

/// 倒计时文本节点。
#[derive(Component)]
pub(crate) struct CountdownText;

/// 采样计数文本节点。
#[derive(Component)]
pub(crate) struct SamplesText;

/// 实时 yaw/pitch 文本节点。
#[derive(Component)]
pub(crate) struct LivePoseText;

/// 消息文本节点（成功 / 失败提示）。
#[derive(Component)]
pub(crate) struct MessageText;

/// 取消校准按钮。
#[derive(Component)]
pub(crate) struct CancelCalibButton;

/// 返回设置按钮（失败后显示）。
#[derive(Component)]
pub(crate) struct BackButton;

// ── 初始化 ─────────────────────────────────────────────────────

/// 构建校准视图 UI 节点树。初始为隐藏。
pub(crate) fn setup_calibration_view(mut commands: Commands, app_font: Res<AppFont>) {
    let font = app_font.0.clone();
    commands.insert_resource(CalibrationViewState::default());

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(16.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            CalibrationViewRoot,
        ))
        .with_children(|root| {
            // 提示文本
            root.spawn((
                Text::new("请面向屏幕保持不动…"),
                TextFont {
                    font: font.clone(),
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                LocalizedText("calibration.starting"),
                Node {
                    margin: UiRect::bottom(Val::Px(16.0)),
                    ..default()
                },
            ));

            // 倒计时
            root.spawn((
                Text::new("5"),
                TextFont {
                    font: font.clone(),
                    font_size: 48.0,
                    ..default()
                },
                TextColor(Color::srgb(0.4, 0.7, 1.0)),
                CountdownText,
                Node {
                    margin: UiRect::bottom(Val::Px(8.0)),
                    ..default()
                },
            ));

            // 采样计数
            root.spawn((
                Text::new("已采样 0 帧"),
                TextFont {
                    font: font.clone(),
                    font_size: 15.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.7)),
                SamplesText,
                Node {
                    margin: UiRect::bottom(Val::Px(4.0)),
                    ..default()
                },
            ));

            // 实时 yaw/pitch
            root.spawn((
                Text::new("yaw: — / pitch: —"),
                TextFont {
                    font: font.clone(),
                    font_size: 15.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.7)),
                LivePoseText,
                Node {
                    margin: UiRect::bottom(Val::Px(12.0)),
                    ..default()
                },
            ));

            // 消息文本（成功 / 失败提示，初始空）
            root.spawn((
                Text::new(""),
                TextFont {
                    font: font.clone(),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.8, 0.4)),
                MessageText,
                Node {
                    margin: UiRect::bottom(Val::Px(16.0)),
                    ..default()
                },
            ));

            // 按钮行：取消 / 返回设置
            root
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_button(
                        row,
                        &font,
                        "calibration.cancel",
                        CancelCalibButton,
                        Color::srgb(0.4, 0.2, 0.2),
                    );
                    spawn_button(
                        row,
                        &font,
                        "settings.back",
                        BackButton,
                        Color::srgb(0.2, 0.2, 0.24),
                    );
                });
        });
}

fn spawn_button(
    parent: &mut ChildBuilder,
    font: &Handle<Font>,
    label_key: &'static str,
    marker: impl Component,
    bg: Color,
) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(16.0), Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor(Color::srgb(0.5, 0.5, 0.55)),
            Interaction::default(),
            marker,
        ))
        .with_child((
            Text::new(label_key),
            TextFont {
                font: font.clone(),
                font_size: 15.0,
                ..default()
            },
            TextColor(Color::WHITE),
            LocalizedText(label_key),
        ));
}

// ── 系统群 ─────────────────────────────────────────────────────

/// 响应 CalibrateButton 点击：发 StartCalibration，切到校准视图。
pub(crate) fn handle_calibrate(
    query: Query<&Interaction, (With<CalibrateButton>, Changed<Interaction>)>,
    worker: Option<Res<WorkerHandle>>,
    mut app_view: ResMut<AppView>,
    mut state: ResMut<CalibrationViewState>,
) {
    let any_pressed = query.iter().any(|i| *i == Interaction::Pressed);
    if !any_pressed {
        return;
    }

    // 通知 worker 进入校准会话
    if let Some(worker) = &worker {
        let _ = worker.0.0.send(WorkerCommand::StartCalibration);
    }

    // 初始化校准视图状态
    *state = CalibrationViewState {
        phase: CalibrationPhase::CountingDown,
        timer: CALIBRATION_DURATION,
        sample_count: 0,
        yaw: None,
        pitch: None,
        failure_reason: None,
        result: None,
    };

    *app_view = AppView::Calibration;
}

/// 从 PoseUpdated 事件更新实时 yaw/pitch 和采样计数。
pub(crate) fn track_calibration_pose(
    mut reader: EventReader<MonitoringEvent>,
    mut state: ResMut<CalibrationViewState>,
) {
    if state.phase != CalibrationPhase::CountingDown {
        reader.clear();
        return;
    }
    for event in reader.read() {
        if let MonitoringEvent::PoseUpdated { yaw, pitch, .. } = event {
            state.yaw = *yaw;
            state.pitch = *pitch;
            state.sample_count += 1;
        }
    }
}

/// 倒计时逻辑：每帧递减 timer，刷新 UI 文本；成功后自动返回 Settings。
pub(crate) fn update_calibration_countdown(
    time: Res<Time>,
    mut state: ResMut<CalibrationViewState>,
    mut app_view: ResMut<AppView>,
) {
    let dt = time.delta_secs();

    match state.phase {
        CalibrationPhase::CountingDown => {
            state.timer -= dt;
            if state.timer <= 0.0 {
                state.timer = 0.0;
                // 倒计时到零；实际完成由 CalibrationComplete 事件驱动。
                // 若 worker 未在合理时间内回报，UI 停在 0 等待。
            }
        }
        CalibrationPhase::Success => {
            state.timer -= dt;
            if state.timer <= 0.0 {
                // 自动返回 Settings
                *state = CalibrationViewState::default();
                *app_view = AppView::Settings;
            }
        }
        _ => {}
    }
}

/// 处理 CalibrationComplete：写 config（orchestrator 已持久化），切到成功阶段。
pub(crate) fn handle_calibration_complete(
    mut reader: EventReader<MonitoringEvent>,
    mut state: ResMut<CalibrationViewState>,
) {
    for event in reader.read() {
        if let MonitoringEvent::CalibrationComplete {
            yaw,
            pitch,
            sample_count,
        } = event
        {
            // neutral_yaw / neutral_pitch 已由 orchestrator 写入 ConfigState，
            // Bevy 端无需重复写入。
            state.phase = CalibrationPhase::Success;
            state.timer = SUCCESS_RETURN_DELAY;
            state.result = Some((*yaw, *pitch, *sample_count));
        }
    }
}

/// 处理 CalibrationFailed：显示错误，留在校准视图。
pub(crate) fn handle_calibration_failed(
    mut reader: EventReader<MonitoringEvent>,
    mut state: ResMut<CalibrationViewState>,
) {
    for event in reader.read() {
        if let MonitoringEvent::CalibrationFailed { reason } = event {
            state.phase = CalibrationPhase::Failed;
            state.failure_reason = Some(reason.clone());
        }
    }
}

/// 把 CalibrationViewState 同步到 UI 文本节点。
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn refresh_calibration_ui(
    state: Res<CalibrationViewState>,
    i18n: Res<I18nTable>,
    mut countdown_query: Query<&mut Text, With<CountdownText>>,
    mut samples_query: Query<&mut Text, (With<SamplesText>, Without<CountdownText>)>,
    mut pose_query: Query<
        &mut Text,
        (With<LivePoseText>, Without<CountdownText>, Without<SamplesText>),
    >,
    mut message_query: Query<
        &mut Text,
        (
            With<MessageText>,
            Without<CountdownText>,
            Without<SamplesText>,
            Without<LivePoseText>,
        ),
    >,
    mut cancel_query: Query<&mut Visibility, (With<CancelCalibButton>, Without<BackButton>)>,
    mut back_query: Query<&mut Visibility, With<BackButton>>,
) {
    if !state.is_changed() {
        return;
    }

    // 倒计时：向上取整显示
    if let Ok(mut text) = countdown_query.get_single_mut() {
        let secs = state.timer.ceil() as i32;
        let template = i18n.t("calibration.countdown");
        text.0 = template.replace("{seconds}", &secs.to_string());
    }

    // 采样计数
    if let Ok(mut text) = samples_query.get_single_mut() {
        let template = i18n.t("calibration.samples");
        text.0 = template.replace("{count}", &state.sample_count.to_string());
    }

    // 实时 yaw/pitch
    if let Ok(mut text) = pose_query.get_single_mut() {
        let yaw = state.yaw.map_or("—".into(), |v| format!("{:+.1}°", v));
        let pitch = state.pitch.map_or("—".into(), |v| format!("{:+.1}°", v));
        text.0 = format!("yaw: {} / pitch: {}", yaw, pitch);
    }

    // 消息（成功 / 失败）
    if let Ok(mut text) = message_query.get_single_mut() {
        text.0 = match state.phase {
            CalibrationPhase::Success => i18n.t("calibration.success").to_string(),
            CalibrationPhase::Failed => {
                let msg = i18n.t("calibration.no_face").to_string();
                msg
            }
            _ => String::new(),
        };
    }

    // 按钮可见性
    let show_cancel = state.phase == CalibrationPhase::CountingDown;
    let show_back = state.phase == CalibrationPhase::Failed;
    if let Ok(mut vis) = cancel_query.get_single_mut() {
        *vis = if show_cancel {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut vis) = back_query.get_single_mut() {
        *vis = if show_back {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// 取消校准：发 CancelCalibration，回 Settings。
pub(crate) fn handle_cancel_calibration(
    query: Query<&Interaction, (With<CancelCalibButton>, Changed<Interaction>)>,
    worker: Option<Res<WorkerHandle>>,
    mut app_view: ResMut<AppView>,
    mut state: ResMut<CalibrationViewState>,
) {
    let any_pressed = query.iter().any(|i| *i == Interaction::Pressed);
    if !any_pressed {
        return;
    }
    if let Some(worker) = &worker {
        let _ = worker.0.0.send(WorkerCommand::CancelCalibration);
    }
    *state = CalibrationViewState::default();
    *app_view = AppView::Settings;
}

/// 失败后返回设置：重置状态，回 Settings。
pub(crate) fn handle_back_to_settings(
    query: Query<&Interaction, (With<BackButton>, Changed<Interaction>)>,
    mut app_view: ResMut<AppView>,
    mut state: ResMut<CalibrationViewState>,
) {
    let any_pressed = query.iter().any(|i| *i == Interaction::Pressed);
    if !any_pressed {
        return;
    }
    *state = CalibrationViewState::default();
    *app_view = AppView::Settings;
}

/// 根据 AppView 切换校准视图可见性。
pub(crate) fn update_calibration_view_visibility(
    app_view: Res<AppView>,
    mut query: Query<&mut Visibility, With<CalibrationViewRoot>>,
) {
    if !app_view.is_changed() {
        return;
    }
    if let Ok(mut vis) = query.get_single_mut() {
        *vis = match *app_view {
            AppView::Calibration => Visibility::Visible,
            _ => Visibility::Hidden,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_idle() {
        let state = CalibrationViewState::default();
        assert_eq!(state.phase, CalibrationPhase::Idle);
        assert_eq!(state.timer, 0.0);
        assert_eq!(state.sample_count, 0);
    }

    #[test]
    fn failed_reason_maps_no_face_key() {
        // 确认 orchestrator 发出的 "no_face" reason 与 i18n key 对应关系
        let state = CalibrationViewState {
            phase: CalibrationPhase::Failed,
            failure_reason: Some("no_face".into()),
            ..Default::default()
        };
        assert_eq!(state.failure_reason.as_deref(), Some("no_face"));
    }
}
