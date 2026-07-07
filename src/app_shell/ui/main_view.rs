//! 主视图：姿态徽标 + yaw/pitch 读数 + 预览 + Settings 按钮。
//!
//! 数据流：
//! ```text
//! WorkerOrchestrator
//!   └─ BevyEventSink (mpsc Sender)
//!      └─ mpsc channel
//!         └─ MonitoringReceiver (Bevy Resource)
//!            └─ forward_monitoring_events (Update system)
//!               └─ Events<MonitoringEvent>
//!                  ├─ update_pose_state  → MainViewState (Resource)
//!                  └─ update_preview_texture → Assets<Image>
//!                     └─ refresh_ui_text → Text 组件
//! ```

use std::sync::mpsc::Receiver;

use base64::{engine::general_purpose, Engine};
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::app_shell::settings_view::{SettingsDraft, SettingsPanelState};
use crate::app_shell::AppView;
use super::i18n::{I18nTable, LocalizedText};
use crate::AppFont;
use crate::monitoring::camera::camera_enumerator;
use crate::monitoring::events::MonitoringEvent;

// ── 资源 ───────────────────────────────────────────────────────

/// 包装 mpsc 接收端。`Receiver` 本身不 `Sync`，用 `Mutex` 后可做 Bevy Resource。
#[derive(Resource)]
pub struct MonitoringReceiver(pub std::sync::Mutex<Receiver<MonitoringEvent>>);

/// 主视图的运行时状态，由监控事件驱动更新。
#[derive(Resource, Default)]
pub struct MainViewState {
    /// 姿态 i18n key，如 "pose.facing_screen"
    pub pose_key: String,
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    /// 摄像头状态 i18n key，如 "camera.starting"
    pub camera_key: String,
}

/// 预览 Image 资源的 Handle，供 `update_preview_texture` 写入新帧。
#[derive(Resource)]
pub struct PreviewHandle(pub Handle<Image>);

// ── UI 标记组件 ─────────────────────────────────────────────────

/// 主视图根节点（用于可见性切换）。
#[derive(Component)]
pub(crate) struct MainViewRoot;

/// 标记姿态徽标文本节点。
#[derive(Component)]
pub(crate) struct PoseBadgeText;

/// 标记 yaw/pitch 读数文本节点。
#[derive(Component)]
pub(crate) struct ReadoutText;

/// 标记摄像头状态文本节点。
#[derive(Component)]
pub(crate) struct CameraStatusText;

/// 标记预览图像节点。
#[derive(Component)]
pub(crate) struct PreviewNode;

/// 标记 Settings 按钮。
#[derive(Component)]
pub(crate) struct SettingsButton;

// ── 初始化 ─────────────────────────────────────────────────────

/// 创建 UI 节点树，插入初始 `MainViewState` 和占位预览纹理。
pub(crate) fn setup_main_view(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    app_font: Res<AppFont>,
) {
    // 占位 1×1 黑色纹理
    let placeholder = Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![20, 20, 20, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let handle = images.add(placeholder);
    commands.insert_resource(PreviewHandle(handle.clone()));

    commands.insert_resource(MainViewState {
        pose_key: "pose.no_face".into(),
        yaw: None,
        pitch: None,
        camera_key: "camera.starting".into(),
    });

    // ── UI 节点树 ───────────────────────────────────────────────

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(16.0)),
                ..default()
            },
            MainViewRoot,
        ))
        .with_children(|parent| {
            // ── 顶栏：姿态徽标 + 读数 ───────────────────────────────
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(16.0),
                    margin: UiRect::bottom(Val::Px(12.0)),
                    ..default()
                })
                .with_children(|bar| {
                    // 姿态徽标
                    bar.spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.15, 0.15, 0.18)),
                    ))
                    .with_children(|badge| {
                        badge.spawn((
                            Text::new("—"),
                            TextFont {
                                font: app_font.0.clone(),
                                font_size: 18.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            PoseBadgeText,
                        ));
                    });

                    // yaw / pitch 读数
                    bar.spawn((
                        Text::new("yaw: — / pitch: —"),
                        TextFont {
                            font: app_font.0.clone(),
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.7, 0.7, 0.7)),
                        ReadoutText,
                    ));
                });

            // ── 预览区 ──────────────────────────────────────────────
            parent
                .spawn(Node {
                    flex_grow: 1.0,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                })
                .with_children(|area| {
                    area.spawn((
                        Node {
                            width: Val::Px(320.0),
                            height: Val::Px(240.0),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor(Color::srgb(0.3, 0.3, 0.3)),
                    ))
                    .with_children(|frame| {
                        // 预览 ImageNode
                        // worker 已在上游做了水平镜像（mirror_horizontal），
                        // 这里不再翻转。
                        frame.spawn((
                            ImageNode {
                                image: handle.clone(),
                                ..default()
                            },
                            PreviewNode,
                        ));
                    });
                });

            // ── 摄像头状态 ──────────────────────────────────────────
            parent
                .spawn(Node {
                    margin: UiRect::top(Val::Px(8.0)),
                    ..default()
                })
                .with_child((
                    Text::new("Camera starting..."),
                    TextFont {
                        font: app_font.0.clone(),
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.5, 0.5, 0.5)),
                    LocalizedText("camera.starting"),
                    CameraStatusText,
                ));

            // ── Settings 按钮 ───────────────────────────────────────
            parent
                .spawn(Node {
                    margin: UiRect::top(Val::Px(12.0)),
                    ..default()
                })
                .with_child((
                    Node {
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.2, 0.2, 0.24)),
                    BorderColor(Color::srgb(0.4, 0.4, 0.45)),
                    Interaction::default(),
                    SettingsButton,
                ))
                .with_child((
                    Text::new("⚙ Settings"),
                    TextFont {
                        font: app_font.0.clone(),
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.8, 0.8, 0.8)),
                    LocalizedText("main.settings"),
                ));
        });
}

// ── 系统群 ─────────────────────────────────────────────────────

/// 从 mpsc 接收端批量取事件，写入 Bevy 事件总线。
pub(crate) fn forward_monitoring_events(
    receiver: Res<MonitoringReceiver>,
    mut writer: EventWriter<MonitoringEvent>,
) {
    let rx = receiver.0.lock().unwrap();
    while let Ok(event) = rx.try_recv() {
        writer.send(event);
    }
}

/// 处理 PoseUpdated / CameraStateChanged，更新 `MainViewState`。
pub(crate) fn update_pose_state(
    mut reader: EventReader<MonitoringEvent>,
    mut state: ResMut<MainViewState>,
) {
    for event in reader.read() {
        match event {
            MonitoringEvent::PoseUpdated {
                yaw,
                pitch,
                pose_state,
            } => {
                state.pose_key = pose_state_key(pose_state);
                state.yaw = *yaw;
                state.pitch = *pitch;
                state.camera_key = "camera.available".into();
            }
            MonitoringEvent::CameraStateChanged { state: cam_state }
                if cam_state == "unavailable" =>
            {
                state.camera_key = "camera.unavailable".into();
            }
            _ => {}
        }
    }
}

/// 把 PreviewFrame 的 base64 PNG 解码后上传到 Bevy Image 资源。
pub(crate) fn update_preview_texture(
    mut reader: EventReader<MonitoringEvent>,
    mut images: ResMut<Assets<Image>>,
    preview: Res<PreviewHandle>,
) {
    for event in reader.read() {
        if let MonitoringEvent::PreviewFrame(frame) = event {
            if let Some(image) = decode_preview_frame(frame) {
                if let Some(asset) = images.get_mut(&preview.0) {
                    *asset = image;
                }
            }
        }
    }
}

/// 将 `MainViewState` 的值同步到 UI 文本节点（含 i18n 翻译）。
///
/// 三个 Query 都请求 `&mut Text`，Bevy 无法仅通过 `With<T>` 过滤器自动证明互斥，
/// 因此用 `ParamSet` 避免 ECS 冲突（error[B0001]）。
#[allow(clippy::type_complexity)]
pub(crate) fn refresh_ui_text(
    state: Res<MainViewState>,
    i18n: Res<I18nTable>,
    mut queries: ParamSet<(
        Query<&mut Text, With<PoseBadgeText>>,
        Query<&mut Text, With<ReadoutText>>,
        Query<(&mut Text, &LocalizedText), With<CameraStatusText>>,
    )>,
) {
    if !state.is_changed() {
        return;
    }

    let mut pose_query = queries.p0();
    let mut pose_text = pose_query.single_mut();
    pose_text.0 = i18n.t(&state.pose_key).to_string();

    let mut readout_query = queries.p1();
    let mut readout_text = readout_query.single_mut();
    let yaw = state.yaw.map_or("—".into(), |v| format!("{:+.1}°", v));
    let pitch = state.pitch.map_or("—".into(), |v| format!("{:+.1}°", v));
    readout_text.0 = format!("yaw: {} / pitch: {}", yaw, pitch);

    let mut camera_query = queries.p2();
    let (mut cam_text, _) = camera_query.single_mut();
    cam_text.0 = i18n.t(&state.camera_key).to_string();
}

/// Settings 按钮点击 → 切换到 Settings 视图，初始化 SettingsDraft。
pub(crate) fn handle_settings_button_click(
    query: Query<&Interaction, (With<SettingsButton>, Changed<Interaction>)>,
    resources: Option<Res<crate::AppResources>>,
    mut app_view: ResMut<AppView>,
    mut panel_state: ResMut<SettingsPanelState>,
    mut commands: Commands,
) {
    let any_pressed = query.iter().any(|i| *i == Interaction::Pressed);
    if !any_pressed {
        return;
    }
    let Some(resources) = resources else {
        return;
    };
    let config = resources.config_state.get();
    let camera_list = camera_enumerator::list_cameras().unwrap_or_default();
    let draft = SettingsDraft::from_config(&config, camera_list);
    commands.insert_resource(draft);
    *app_view = AppView::Settings;
    *panel_state = SettingsPanelState::OpenDirty;
}

/// 根据 `AppView` 切换主视图 / 设置面板的可见性。
pub(crate) fn update_view_visibility(
    app_view: Res<AppView>,
    mut main_query: Query<&mut Visibility, With<MainViewRoot>>,
    mut settings_query: Query<
        &mut Visibility,
        (With<crate::app_shell::settings_view::SettingsPanelRoot>, Without<MainViewRoot>),
    >,
) {
    if !app_view.is_changed() {
        return;
    }
    if let Ok(mut vis) = main_query.get_single_mut() {
        *vis = match *app_view {
            AppView::Main => Visibility::Visible,
            AppView::Settings | AppView::Calibration => Visibility::Hidden,
        };
    }
    if let Ok(mut vis) = settings_query.get_single_mut() {
        *vis = match *app_view {
            AppView::Settings => Visibility::Visible,
            AppView::Main | AppView::Calibration => Visibility::Hidden,
        };
    }
}

// ── 辅助 ───────────────────────────────────────────────────────

/// 把 PoseState 的 snake_case 字符串映射到 i18n key。
fn pose_state_key(raw: &str) -> String {
    let key = match raw {
        "facing_screen" => "pose.facing_screen",
        "off_axis_left" => "pose.off_axis_left",
        "off_axis_right" => "pose.off_axis_right",
        "head_up" => "pose.head_up",
        "head_down" => "pose.head_down",
        "no_face" => "pose.no_face",
        _ => "pose.no_face",
    };
    key.to_string()
}

/// 解码 PreviewFrame（base64 PNG data URL）为 Bevy Image。
fn decode_preview_frame(frame: &crate::monitoring::preview::PreviewFrame) -> Option<Image> {
    let b64 = frame
        .image_data_url
        .strip_prefix("data:image/png;base64,")
        .or_else(|| frame.image_data_url.strip_prefix("data:image/jpeg;base64,"))?;

    let png_bytes = general_purpose::STANDARD.decode(b64).ok()?;

    let mime = if frame.image_data_url.starts_with("data:image/jpeg") {
        ImageType::MimeType("image/jpeg")
    } else {
        ImageType::MimeType("image/png")
    };

    Image::from_buffer(
        &png_bytes,
        mime,
        CompressedImageFormats::NONE,
        true,
        ImageSampler::default(),
        RenderAssetUsages::default(),
    )
    .ok()
}
