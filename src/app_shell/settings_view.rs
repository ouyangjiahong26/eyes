//! VS2 设置面板：yaw/pitch 阈值滑块、校准入口、摄像头下拉、声音/自启开关、
//! 语言选择、高级设置、保存/取消。
//!
//! 数据流：
//! ```text
//! ConfigState::get() → SettingsDraft（打开设置面板时初始化）
//!                       ↑ 用户交互修改
//!                  Save → ConfigState::set() + WorkerCommand::SetConfig
//!                  Cancel → 丢弃，切回 Main
//! ```

use bevy::prelude::*;

use crate::domain::config::AppConfig;
use crate::i18n::LocalizedText;
use crate::monitoring::camera_enumerator::CameraDevice;
use crate::monitoring::channel::WorkerCommand;
use crate::AppFont;

// ── 资源 ───────────────────────────────────────────────────────

/// 设置面板正在编辑的草稿值。打开面板时从 `ConfigState` 初始化。
#[derive(Resource, Clone)]
pub struct SettingsDraft {
    pub yaw_threshold: f64,
    pub pitch_threshold: f64,
    pub camera_index: u32,
    pub sound_enabled: bool,
    pub autostart_enabled: bool,
    pub language: String,
    pub off_axis_streak_threshold: f64,
    pub off_axis_repeat_interval: f64,
    pub camera_list: Vec<CameraDevice>,
    pub advanced_visible: bool,
}

impl SettingsDraft {
    pub fn from_config(config: &AppConfig, camera_list: Vec<CameraDevice>) -> Self {
        Self {
            yaw_threshold: config.yaw_threshold,
            pitch_threshold: config.pitch_threshold,
            camera_index: config.camera_index,
            sound_enabled: config.sound_enabled,
            autostart_enabled: config.autostart_enabled,
            language: config.language.clone(),
            off_axis_streak_threshold: config.off_axis_streak_threshold_seconds,
            off_axis_repeat_interval: config.off_axis_repeat_interval_seconds,
            camera_list,
            advanced_visible: false,
        }
    }
}

/// 设置面板状态机。
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPanelState {
    #[default]
    Closed,
    OpenDirty,
    OpenSaving,
}

// ── UI 常量 ─────────────────────────────────────────────────────

const YAW_PITCH_MIN: f64 = 1.0;
const YAW_PITCH_MAX: f64 = 30.0;
const YAW_PITCH_STEP: f64 = 0.5;

const STREAK_MIN: f64 = 0.0;
const STREAK_MAX: f64 = 5.0;
const STREAK_STEP: f64 = 0.1;

const REPEAT_MIN: f64 = 1.0;
const REPEAT_MAX: f64 = 60.0;
const REPEAT_STEP: f64 = 1.0;

const LANGUAGES: &[(&str, &str)] = &[("zh-CN", "中文"), ("en", "English")];

// ── 标记组件 ─────────────────────────────────────────────────────

/// 设置面板根节点。
#[derive(Component)]
pub(crate) struct SettingsPanelRoot;

/// 标记高级设置折叠区域容器。
#[derive(Component)]
pub(crate) struct AdvancedSection;

/// 滑块类型标识，用于点击轨道时定位对应的 draft 字段。
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SliderKind {
    Yaw,
    Pitch,
    Streak,
    Repeat,
}

/// 滑块轨道（可点击区域）。
#[derive(Component)]
pub(crate) struct SliderTrack(pub SliderKind);

/// 滑块填充条，宽度反映当前值比例。
#[derive(Component)]
pub(crate) struct SliderFill(pub SliderKind);

/// 滑块值文本。
#[derive(Component)]
pub(crate) struct SliderValueText(pub SliderKind);

// ── 按钮标记 ─────────────────────────────────────────────────────

#[derive(Component)]
pub(crate) struct CalibrateButton;
#[derive(Component)]
pub(crate) struct SaveButton;
#[derive(Component)]
pub(crate) struct CancelButton;
#[derive(Component)]
pub(crate) struct AdvancedToggleButton;

/// 摄像头切换：上/下一个设备。
#[derive(Component, Clone, Copy)]
pub(crate) enum CameraNav {
    Prev,
    Next,
}

/// 语言切换：上/下一个选项。
#[derive(Component, Clone, Copy)]
pub(crate) enum LangNav {
    Prev,
    Next,
}

/// 声音开关按钮。
#[derive(Component)]
pub(crate) struct SoundToggle;

/// 开机自启开关按钮。
#[derive(Component)]
pub(crate) struct AutostartToggle;

/// 摄像头名称文本。
#[derive(Component)]
pub(crate) struct CameraLabel;

/// 语言名称文本。
#[derive(Component)]
pub(crate) struct LangLabel;

/// 声音开关状态文本。
#[derive(Component)]
pub(crate) struct SoundStateLabel;

/// 自启开关状态文本。
#[derive(Component)]
pub(crate) struct AutostartStateLabel;

// ── 初始化 ─────────────────────────────────────────────────────

/// 构建设置面板的完整 UI 节点树。初始为隐藏（`Visibility::Hidden`）。
pub(crate) fn setup_settings_panel(mut commands: Commands, app_font: Res<AppFont>) {
    let font = app_font.0.clone();

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(16.0)),
                ..default()
            },
            Visibility::Hidden,
            SettingsPanelRoot,
        ))
        .with_children(|panel| {
            // ── 标题 ─────────────────────────────────────────────
            panel.spawn((
                Text::new("Settings"),
                TextFont {
                    font: font.clone(),
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                LocalizedText("settings.title"),
                Node {
                    margin: UiRect::bottom(Val::Px(16.0)),
                    ..default()
                },
            ));

            // ── yaw 阈值 ─────────────────────────────────────────
            spawn_slider_row(panel, &font, SliderKind::Yaw, "settings.yaw_threshold");
            // ── pitch 阈值 ───────────────────────────────────────
            spawn_slider_row(panel, &font, SliderKind::Pitch, "settings.pitch_threshold");

            // ── 校准按钮 ─────────────────────────────────────────
            panel
                .spawn(Node {
                    margin: UiRect::bottom(Val::Px(12.0)),
                    ..default()
                })
                .with_children(|row| {
                    spawn_button(
                        row,
                        &font,
                        "settings.calibrate",
                        CalibrateButton,
                        Color::srgb(0.15, 0.3, 0.5),
                    );
                });

            // ── 摄像头选择 ───────────────────────────────────────
            spawn_nav_row(panel, &font, "settings.camera_index", CameraLabel, CameraNav::Prev, CameraNav::Next);

            // ── 语言选择 ─────────────────────────────────────────
            spawn_nav_row(panel, &font, "settings.language", LangLabel, LangNav::Prev, LangNav::Next);

            // ── 声音开关 ─────────────────────────────────────────
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    margin: UiRect::bottom(Val::Px(8.0)),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("Sound"),
                        TextFont { font: font.clone(), font_size: 16.0, ..default() },
                        TextColor(Color::srgb(0.8, 0.8, 0.8)),
                        LocalizedText("settings.sound_enabled"),
                    ));
                    spawn_toggle_button(row, SoundToggle, SoundStateLabel);
                });

            // ── 开机自启开关（仅 Windows） ─────────────────────────
            #[cfg(target_os = "windows")]
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    margin: UiRect::bottom(Val::Px(8.0)),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("Autostart"),
                        TextFont { font: font.clone(), font_size: 16.0, ..default() },
                        TextColor(Color::srgb(0.8, 0.8, 0.8)),
                        LocalizedText("settings.autostart_enabled"),
                    ));
                    spawn_toggle_button(row, AutostartToggle, AutostartStateLabel);
                });

            // ── 高级设置折叠 ─────────────────────────────────────
            panel
                .spawn(Node {
                    margin: UiRect::bottom(Val::Px(8.0)),
                    ..default()
                })
                .with_child((
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.18, 0.18, 0.22)),
                    BorderColor(Color::srgb(0.35, 0.35, 0.4)),
                    Interaction::default(),
                    AdvancedToggleButton,
                ))
                .with_child((
                    Text::new("Advanced"),
                    TextFont { font: font.clone(), font_size: 15.0, ..default() },
                    TextColor(Color::srgb(0.7, 0.7, 0.7)),
                    LocalizedText("settings.advanced"),
                ));

            // ── 高级设置区域（初始隐藏） ─────────────────────────
            panel
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        margin: UiRect::bottom(Val::Px(12.0)),
                        ..default()
                    },
                    Visibility::Hidden,
                    AdvancedSection,
                ))
                .with_children(|adv| {
                    spawn_slider_row(adv, &font, SliderKind::Streak, "settings.streak_threshold");
                    spawn_slider_row(adv, &font, SliderKind::Repeat, "settings.repeat_interval");
                });

            // ── 底部按钮 ─────────────────────────────────────────
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    margin: UiRect::top(Val::Px(12.0)),
                    ..default()
                })
                .with_children(|row| {
                    spawn_button(row, &font, "settings.save", SaveButton, Color::srgb(0.2, 0.4, 0.2));
                    spawn_button(row, &font, "settings.cancel", CancelButton, Color::srgb(0.4, 0.2, 0.2));
                });
        });
}

/// 生成一行：标签 + 滑块轨道 + 值文本。
fn spawn_slider_row(
    parent: &mut ChildBuilder,
    font: &Handle<Font>,
    kind: SliderKind,
    label_key: &'static str,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            margin: UiRect::bottom(Val::Px(12.0)),
            ..default()
        })
        .with_children(|col| {
            // 标签 + 值
            col.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                margin: UiRect::bottom(Val::Px(4.0)),
                ..default()
            })
            .with_children(|label_row| {
                label_row.spawn((
                    Text::new(label_key),
                    TextFont { font: font.clone(), font_size: 15.0, ..default() },
                    TextColor(Color::srgb(0.8, 0.8, 0.8)),
                    LocalizedText(label_key),
                ));
                label_row.spawn((
                    Text::new("—"),
                    TextFont { font: font.clone(), font_size: 15.0, ..default() },
                    TextColor(Color::srgb(0.9, 0.9, 0.9)),
                    SliderValueText(kind),
                ));
            });

            // 滑块轨道
            col.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(24.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.12, 0.12, 0.15)),
                BorderColor(Color::srgb(0.3, 0.3, 0.35)),
                Interaction::default(),
                bevy::ui::RelativeCursorPosition::default(),
                SliderTrack(kind),
            ))
            .with_child((
                Node {
                    width: Val::Percent(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.3, 0.5, 0.8)),
                SliderFill(kind),
            ));
        });
}

/// 生成一行：标签 + Prev 按钮 + 当前值 + Next 按钮。
fn spawn_nav_row(
    parent: &mut ChildBuilder,
    font: &Handle<Font>,
    label_key: &'static str,
    label_marker: impl Component,
    prev: impl Component + Clone,
    next: impl Component + Clone,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            margin: UiRect::bottom(Val::Px(8.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label_key),
                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
                LocalizedText(label_key),
            ));

            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|nav| {
                spawn_nav_button(nav, "‹", prev);
                nav.spawn((
                    Text::new("—"),
                    TextFont { font: font.clone(), font_size: 15.0, ..default() },
                    TextColor(Color::WHITE),
                    Node {
                        width: Val::Px(180.0),
                        ..default()
                    },
                    label_marker,
                ));
                spawn_nav_button(nav, "›", next);
            });
        });
}

fn spawn_nav_button(parent: &mut ChildBuilder, _label: &str, nav: impl Component + Clone) {
    parent.spawn((
        Node {
            width: Val::Px(32.0),
            height: Val::Px(28.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::srgb(0.2, 0.2, 0.24)),
        BorderColor(Color::srgb(0.4, 0.4, 0.45)),
        Interaction::default(),
        nav,
    ));
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
            TextFont { font: font.clone(), font_size: 15.0, ..default() },
            TextColor(Color::WHITE),
            LocalizedText(label_key),
        ));
}

fn spawn_toggle_button(
    parent: &mut ChildBuilder,
    toggle_marker: impl Component,
    state_marker: impl Component,
) {
    parent
        .spawn((
            Node {
                width: Val::Px(48.0),
                height: Val::Px(24.0),
                align_items: AlignItems::Center,
                padding: UiRect::left(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.15, 0.15, 0.18)),
            BorderColor(Color::srgb(0.35, 0.35, 0.4)),
            Interaction::default(),
            toggle_marker,
        ))
        .with_child((
            Node {
                width: Val::Px(20.0),
                height: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.5, 0.5, 0.55)),
            state_marker,
        ));
}

// ── 系统群 ─────────────────────────────────────────────────────

/// 处理滑块轨道点击：根据光标在轨道内的相对位置更新 draft 值。
pub(crate) fn handle_slider_click(
    draft: Option<ResMut<SettingsDraft>>,
    query: Query<
        (&Interaction, &bevy::ui::RelativeCursorPosition, &SliderTrack),
        Changed<Interaction>,
    >,
) {
    let Some(mut draft) = draft else { return; };
    for (interaction, rel_cursor, track) in &query {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(cursor) = rel_cursor.normalized else {
            continue;
        };
        let frac = cursor.x.clamp(0.0, 1.0) as f64;
        match track.0 {
            SliderKind::Yaw => {
                draft.yaw_threshold = snap(frac, YAW_PITCH_MIN, YAW_PITCH_MAX, YAW_PITCH_STEP);
            }
            SliderKind::Pitch => {
                draft.pitch_threshold = snap(frac, YAW_PITCH_MIN, YAW_PITCH_MAX, YAW_PITCH_STEP);
            }
            SliderKind::Streak => {
                draft.off_axis_streak_threshold =
                    snap(frac, STREAK_MIN, STREAK_MAX, STREAK_STEP);
            }
            SliderKind::Repeat => {
                draft.off_axis_repeat_interval =
                    snap(frac, REPEAT_MIN, REPEAT_MAX, REPEAT_STEP);
            }
        }
    }
}

/// 把 [0,1] 映射到 [min,max] 并按 step 对齐。
fn snap(frac: f64, min: f64, max: f64, step: f64) -> f64 {
    let raw = min + frac * (max - min);
    let snapped = ((raw - min) / step).round() * step + min;
    snapped.clamp(min, max)
}

/// 摄像头 Prev/Next 点击。
pub(crate) fn handle_camera_nav(
    draft: Option<ResMut<SettingsDraft>>,
    query: Query<(&CameraNav, &Interaction), Changed<Interaction>>,
) {
    let Some(mut draft) = draft else { return; };
    let count = draft.camera_list.len().max(1) as u32;
    for (nav, interaction) in &query {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match nav {
            CameraNav::Prev => {
                draft.camera_index = draft.camera_index.checked_sub(1).unwrap_or(count - 1);
            }
            CameraNav::Next => {
                draft.camera_index = (draft.camera_index + 1) % count;
            }
        }
    }
}

/// 语言 Prev/Next 点击。
pub(crate) fn handle_lang_nav(
    draft: Option<ResMut<SettingsDraft>>,
    query: Query<(&LangNav, &Interaction), Changed<Interaction>>,
) {
    let Some(mut draft) = draft else { return; };
    let current = LANGUAGES
        .iter()
        .position(|(code, _)| *code == draft.language)
        .unwrap_or(0);
    for (nav, interaction) in &query {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let len = LANGUAGES.len();
        let new_idx = match nav {
            LangNav::Prev => (current + len - 1) % len,
            LangNav::Next => (current + 1) % len,
        };
        draft.language = LANGUAGES[new_idx].0.to_string();
    }
}

/// 声音开关切换。
pub(crate) fn handle_sound_toggle(
    draft: Option<ResMut<SettingsDraft>>,
    query: Query<&Interaction, (With<SoundToggle>, Changed<Interaction>)>,
) {
    let Some(mut draft) = draft else { return; };
    for interaction in &query {
        if *interaction == Interaction::Pressed {
            draft.sound_enabled = !draft.sound_enabled;
        }
    }
}

/// 开机自启开关切换。
pub(crate) fn handle_autostart_toggle(
    draft: Option<ResMut<SettingsDraft>>,
    query: Query<&Interaction, (With<AutostartToggle>, Changed<Interaction>)>,
) {
    let Some(mut draft) = draft else { return; };
    for interaction in &query {
        if *interaction == Interaction::Pressed {
            draft.autostart_enabled = !draft.autostart_enabled;
        }
    }
}

/// 高级设置折叠 / 展开。
pub(crate) fn handle_advanced_toggle(
    draft: Option<ResMut<SettingsDraft>>,
    query: Query<&Interaction, (With<AdvancedToggleButton>, Changed<Interaction>)>,
) {
    let Some(mut draft) = draft else { return; };
    for interaction in &query {
        if *interaction == Interaction::Pressed {
            draft.advanced_visible = !draft.advanced_visible;
        }
    }
}

/// 把 draft 的当前值同步到 UI：滑块填充宽度、值文本、开关指示器。
///
/// 多个 Query 都请求 `&mut Text`，Bevy 无法自动证明互斥，用 `ParamSet` 避免
/// ECS 冲突（error[B0001]）。
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub(crate) fn refresh_settings_ui(
    draft: Option<Res<SettingsDraft>>,
    mut fill_query: Query<(&SliderFill, &mut Node)>,
    mut text_queries: ParamSet<(
        Query<(&SliderValueText, &mut Text)>,
        Query<&mut Text, With<CameraLabel>>,
        Query<&mut Text, With<LangLabel>>,
    )>,
    mut sound_state: Query<&mut BackgroundColor, (With<SoundToggle>, Without<SoundStateLabel>)>,
    mut autostart_state: Query<
        &mut BackgroundColor,
        (With<AutostartToggle>, Without<SoundToggle>),
    >,
    mut advanced_section: Query<&mut Visibility, With<AdvancedSection>>,
) {
    let Some(draft) = draft else { return; };

    if !draft.is_changed() {
        return;
    }

    // 滑块填充与值文本
    for (fill, mut node) in &mut fill_query {
        let (frac, label) = match fill.0 {
            SliderKind::Yaw => (
                pct(draft.yaw_threshold, YAW_PITCH_MIN, YAW_PITCH_MAX),
                format!("{:.1}°", draft.yaw_threshold),
            ),
            SliderKind::Pitch => (
                pct(draft.pitch_threshold, YAW_PITCH_MIN, YAW_PITCH_MAX),
                format!("{:.1}°", draft.pitch_threshold),
            ),
            SliderKind::Streak => (
                pct(
                    draft.off_axis_streak_threshold,
                    STREAK_MIN,
                    STREAK_MAX,
                ),
                format!("{:.1}s", draft.off_axis_streak_threshold),
            ),
            SliderKind::Repeat => (
                pct(
                    draft.off_axis_repeat_interval,
                    REPEAT_MIN,
                    REPEAT_MAX,
                ),
                format!("{:.0}s", draft.off_axis_repeat_interval),
            ),
        };
        node.width = Val::Percent(frac as f32 * 100.0);

        // 更新对应的值文本
        let mut value_query = text_queries.p0();
        for (value_text, mut text) in &mut value_query {
            if value_text.0 == fill.0 {
                text.0 = label.clone();
            }
        }
    }

    // 摄像头名称
    let mut camera_label = text_queries.p1();
    if let Ok(mut label) = camera_label.get_single_mut() {
        let name = draft
            .camera_list
            .iter()
            .find(|c| c.index == draft.camera_index)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("Camera {}", draft.camera_index));
        label.0 = name;
    }

    // 语言名称
    let mut lang_label = text_queries.p2();
    if let Ok(mut label) = lang_label.get_single_mut() {
        let display = LANGUAGES
            .iter()
            .find(|(code, _)| *code == draft.language)
            .map(|(_, name)| *name)
            .unwrap_or(&draft.language);
        label.0 = display.to_string();
    }

    // 声音开关指示色
    for mut bg in &mut sound_state {
        *bg = if draft.sound_enabled {
            BackgroundColor(Color::srgb(0.2, 0.5, 0.2))
        } else {
            BackgroundColor(Color::srgb(0.15, 0.15, 0.18))
        };
    }

    // 自启开关指示色
    for mut bg in &mut autostart_state {
        *bg = if draft.autostart_enabled {
            BackgroundColor(Color::srgb(0.2, 0.5, 0.2))
        } else {
            BackgroundColor(Color::srgb(0.15, 0.15, 0.18))
        };
    }

    // 高级区域可见性
    if let Ok(mut vis) = advanced_section.get_single_mut() {
        *vis = if draft.advanced_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// 把 draft 转成 AppConfig。
pub(crate) fn draft_to_config(draft: &SettingsDraft, base: &AppConfig) -> AppConfig {
    let mut cfg = base.clone();
    cfg.yaw_threshold = draft.yaw_threshold;
    cfg.pitch_threshold = draft.pitch_threshold;
    cfg.camera_index = draft.camera_index;
    cfg.sound_enabled = draft.sound_enabled;
    cfg.autostart_enabled = draft.autostart_enabled;
    cfg.language = draft.language.clone();
    cfg.off_axis_streak_threshold_seconds = draft.off_axis_streak_threshold;
    cfg.off_axis_repeat_interval_seconds = draft.off_axis_repeat_interval;
    cfg
}

fn pct(value: f64, min: f64, max: f64) -> f64 {
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

// ── Save / Cancel 处理（由 lib.rs 调用） ──────────────────────────

/// Save 按钮的语义化命令，通过事件在系统间传递。
#[derive(Event)]
pub struct SaveSettings;

/// Cancel 按钮的语义化命令。
#[derive(Event)]
pub struct CancelSettings;

/// 处理 Save：写 ConfigState + 发 WorkerCommand + 切回 Main。
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_save(
    mut events: EventReader<SaveSettings>,
    draft: Option<Res<SettingsDraft>>,
    resources: Option<Res<crate::AppResources>>,
    worker: Option<Res<crate::WorkerHandle>>,
    mut app_view: ResMut<crate::app_shell::AppView>,
    mut panel_state: ResMut<SettingsPanelState>,
    mut i18n: ResMut<crate::i18n::I18nTable>,
    mut commands: Commands,
) {
    let mut reader = events.read();
    if reader.next().is_none() {
        return;
    }

    let Some(draft) = draft else { return; };
    let Some(resources) = resources else { return; };

    let base = resources.config_state.get();
    let new_config = draft_to_config(&draft, &base);

    // 持久化到磁盘（原子写：temp + rename）
    if let Err(e) = resources.config_state.set(new_config.clone()) {
        eprintln!("[eyes] 配置写入失败: {e}");
    }

    // 通知 worker 用新阈值 / 新摄像头
    if let Some(worker) = &worker {
        let _ = worker.0.send(WorkerCommand::SetConfig(Box::new(new_config.clone())));
    }

    // 语言切换 → 刷新 i18n 资源
    if i18n.language != new_config.language {
        *i18n = crate::i18n::I18nTable::for_language(&new_config.language);
    }

    // 开机自启：写注册表（仅 Windows）
    #[cfg(target_os = "windows")]
    {
        apply_autostart(new_config.autostart_enabled);
    }

    // 切回主视图
    *app_view = crate::app_shell::AppView::Main;
    *panel_state = SettingsPanelState::Closed;
    commands.remove_resource::<SettingsDraft>();
}

/// 处理 Cancel：丢弃修改，切回 Main。
pub(crate) fn handle_cancel(
    mut events: EventReader<CancelSettings>,
    mut app_view: ResMut<crate::app_shell::AppView>,
    mut panel_state: ResMut<SettingsPanelState>,
    mut commands: Commands,
) {
    let mut reader = events.read();
    if reader.next().is_none() {
        return;
    }
    *app_view = crate::app_shell::AppView::Main;
    *panel_state = SettingsPanelState::Closed;
    commands.remove_resource::<SettingsDraft>();
}

/// 按钮点击 → 发 SaveSettings / CancelSettings 事件。
pub(crate) fn dispatch_button_click(
    save: Query<&Interaction, (With<SaveButton>, Changed<Interaction>)>,
    cancel: Query<&Interaction, (With<CancelButton>, Changed<Interaction>)>,
    mut save_writer: EventWriter<SaveSettings>,
    mut cancel_writer: EventWriter<CancelSettings>,
) {
    for interaction in &save {
        if *interaction == Interaction::Pressed {
            save_writer.send(SaveSettings);
        }
    }
    for interaction in &cancel {
        if *interaction == Interaction::Pressed {
            cancel_writer.send(CancelSettings);
        }
    }
}

/// Windows：通过 `auto-launch` 写 / 清注册表 Run 键。
#[cfg(target_os = "windows")]
fn apply_autostart(enabled: bool) {
    use auto_launch::AutoLaunchBuilder;
    let exe = std::env::current_exe().unwrap_or_default();
    let path = exe.to_string_lossy();
    match AutoLaunchBuilder::new()
        .set_app_name("Eyes")
        .set_app_path(&path)
        .build()
    {
        Ok(auto) => {
            let result = if enabled {
                auto.enable()
            } else {
                auto.disable()
            };
            if let Err(e) = result {
                eprintln!("[eyes] autostart {:?} 失败: {e}", enabled);
            }
        }
        Err(e) => {
            eprintln!("[eyes] AutoLaunch 构造失败: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_aligns_to_step() {
        // frac 0.0 → min
        assert!((snap(0.0, 1.0, 30.0, 0.5) - 1.0).abs() < 1e-9);
        // frac 1.0 → max
        assert!((snap(1.0, 1.0, 30.0, 0.5) - 30.0).abs() < 1e-9);
        // frac 0.5 → 约 15.5
        assert!((snap(0.5, 1.0, 30.0, 0.5) - 15.5).abs() < 1e-9);
    }

    #[test]
    fn snap_clamps() {
        assert!((snap(-1.0, 1.0, 30.0, 0.5) - 1.0).abs() < 1e-9);
        assert!((snap(2.0, 1.0, 30.0, 0.5) - 30.0).abs() < 1e-9);
    }

    #[test]
    fn draft_from_config_copies_fields() {
        let mut config = AppConfig::default();
        config.yaw_threshold = 12.0;
        config.language = "en".to_string();
        let draft = SettingsDraft::from_config(&config, vec![]);
        assert_eq!(draft.yaw_threshold, 12.0);
        assert_eq!(draft.language, "en");
    }

    #[test]
    fn draft_to_config_preserves_unedited_fields() {
        let mut base = AppConfig::default();
        base.neutral_yaw = 3.0;
        base.facing_threshold_seconds = 600.0;
        let draft = SettingsDraft::from_config(&base, vec![]);
        let result = draft_to_config(&draft, &base);
        assert_eq!(result.neutral_yaw, 3.0);
        assert_eq!(result.facing_threshold_seconds, 600.0);
    }
}
