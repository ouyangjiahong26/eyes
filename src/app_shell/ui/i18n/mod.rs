//! i18n 翻译表：编译时内嵌 TOML，运行时按 `language` 切换。
//!
//! 语言切换 → 写入 config → 触发 `I18nTable` 资源替换 →
//! `refresh_localized_text` 系统刷新所有 `LocalizedText` 节点。

use std::collections::HashMap;

use bevy::prelude::*;

const ZH_TOML: &str = include_str!("zh.toml");
const EN_TOML: &str = include_str!("en.toml");

/// 当前语言的翻译表。
#[derive(Resource)]
pub struct I18nTable {
    pub language: String,
    entries: HashMap<String, String>,
}

impl I18nTable {
    /// 按 config.language 加载翻译表，未知语言回退到中文。
    pub fn for_language(language: &str) -> Self {
        let (lang, raw) = match language {
            "en" => ("en", EN_TOML),
            _ => ("zh-CN", ZH_TOML),
        };
        let entries = toml::from_str(raw).expect("i18n TOML 解析失败");
        Self {
            language: lang.to_string(),
            entries,
        }
    }

    /// 查找 key 对应的翻译；未命中时返回 key 本身。
    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        self.entries.get(key).map(|s| s.as_str()).unwrap_or(key)
    }
}

/// 标记一个 Text 节点需要用指定 i18n key 渲染。
///
/// `refresh_localized_text` 在 `I18nTable` 变化时更新这些节点的文本。
#[derive(Component, Clone, Copy)]
pub struct LocalizedText(pub &'static str);

/// I18nTable 变化时，把所有 `LocalizedText` 节点的 Text 刷新为对应翻译。
pub fn refresh_localized_text(
    table: Res<I18nTable>,
    mut query: Query<(&LocalizedText, &mut Text)>,
) {
    if !table.is_changed() {
        return;
    }
    for (marker, mut text) in &mut query {
        text.0 = table.t(marker.0).to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zh_table_has_all_expected_keys() {
        let table = I18nTable::for_language("zh-CN");
        assert_eq!(table.t("settings.title"), "设置");
        assert_eq!(table.t("settings.save"), "保存");
        assert_eq!(table.t("main.settings"), "设置");
        assert_eq!(table.t("tray.open"), "打开");
    }

    #[test]
    fn en_table_has_all_expected_keys() {
        let table = I18nTable::for_language("en");
        assert_eq!(table.t("settings.title"), "Settings");
        assert_eq!(table.t("settings.save"), "Save");
        assert_eq!(table.t("tray.quit"), "Quit");
    }

    #[test]
    fn unknown_language_falls_back_to_zh() {
        let table = I18nTable::for_language("fr");
        assert_eq!(table.t("settings.title"), "设置");
    }

    #[test]
    fn missing_key_returns_key_itself() {
        let table = I18nTable::for_language("en");
        assert_eq!(table.t("nonexistent.key"), "nonexistent.key");
    }

    #[test]
    fn zh_and_en_have_same_keys() {
        let zh = I18nTable::for_language("zh-CN");
        let en = I18nTable::for_language("en");
        assert_eq!(zh.entries.len(), en.entries.len());
        for key in zh.entries.keys() {
            assert!(en.entries.contains_key(key), "en 缺少 key: {key}");
        }
    }
}
