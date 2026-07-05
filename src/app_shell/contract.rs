pub const MAIN_WINDOW_LABEL: &str = "main";
pub const TRAY_ID: &str = "eyes-tray";
pub const MENU_SHOW_ID: &str = "show";
pub const MENU_SETTINGS_ID: &str = "settings";
pub const MENU_PAUSE_30_ID: &str = "pause_30";
pub const MENU_PAUSE_60_ID: &str = "pause_60";
pub const MENU_PAUSE_INDEFINITE_ID: &str = "pause_indefinite";
pub const MENU_RESUME_ID: &str = "resume";
pub const MENU_QUIT_ID: &str = "quit";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_menu_exposes_show_settings_and_quit_actions() {
        assert_eq!(MAIN_WINDOW_LABEL, "main");
        assert_eq!(TRAY_ID, "eyes-tray");
        assert_eq!(MENU_SHOW_ID, "show");
        assert_eq!(MENU_SETTINGS_ID, "settings");
        assert_eq!(MENU_QUIT_ID, "quit");
    }
}
