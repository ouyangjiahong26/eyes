use std::path::PathBuf;

/// 将平台用户配置目录解析为应用配置目录 `eyes/`。
///
/// 生产环境中 `base` 通常来自 `dirs::config_dir()`。
/// 若 `base` 缺失，返回空路径（避免 panic，后续 `create_dir_all` 会按平台规则失败）。
pub fn app_config_dir(base: Option<PathBuf>) -> PathBuf {
    base.map(|d| d.join("eyes")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn app_config_dir_uses_eyes_subdirectory() {
        let resolved = app_config_dir(Some(PathBuf::from("/home/user/.config")));
        assert!(
            resolved.ends_with("eyes"),
            "resolved path should end with 'eyes': {resolved:?}"
        );
    }

    #[test]
    fn app_config_dir_defaults_to_empty_when_base_missing() {
        let resolved = app_config_dir(None);
        assert!(
            resolved.as_os_str().is_empty(),
            "missing base should resolve to an empty path"
        );
    }
}
