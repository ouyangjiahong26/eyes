pub const MAIN_WINDOW_LABEL: &str = "main";
pub const TRAY_ID: &str = "eyes-tray";
pub const MENU_SHOW_ID: &str = "show";
pub const MENU_SETTINGS_ID: &str = "settings";
pub const MENU_QUIT_ID: &str = "quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDecision {
    HideToTray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondInstanceDecision {
    FocusMainWindow,
}

pub fn close_requested_decision() -> CloseDecision {
    CloseDecision::HideToTray
}

pub fn second_instance_decision() -> SecondInstanceDecision {
    SecondInstanceDecision::FocusMainWindow
}

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

    #[test]
    fn close_request_hides_to_tray_instead_of_exiting() {
        assert_eq!(close_requested_decision(), CloseDecision::HideToTray);
    }

    #[test]
    fn second_instance_focuses_the_existing_main_window() {
        assert_eq!(
            second_instance_decision(),
            SecondInstanceDecision::FocusMainWindow
        );
    }
}
