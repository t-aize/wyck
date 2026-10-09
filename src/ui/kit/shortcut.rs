//! Shortcut text for the platform. Key bindings use `secondary`, which is Cmd on macOS and Ctrl
//! elsewhere, so a hint typed as "Ctrl+S" would be wrong on a Mac. Write hints with `Ctrl` (and
//! `Alt`) and pass them through [`text`].

use gpui::SharedString;

/// `text` with the modifier names of this platform.
pub fn text(text: &str) -> SharedString {
    for_platform(text, cfg!(target_os = "macos")).into()
}

fn for_platform(text: &str, mac: bool) -> String {
    if mac {
        text.replace("Ctrl/Cmd", "Cmd")
            .replace("Ctrl", "Cmd")
            .replace("Alt", "Option")
    } else {
        text.replace("Ctrl/Cmd", "Ctrl")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mac_reads_cmd_and_option() {
        assert_eq!(for_platform("Save (Ctrl+S)", true), "Save (Cmd+S)");
        assert_eq!(for_platform("Ctrl/Cmd+C", true), "Cmd+C");
        assert_eq!(for_platform("Ctrl+Alt+R", true), "Cmd+Option+R");
    }

    #[test]
    fn other_platforms_keep_ctrl_and_alt() {
        assert_eq!(for_platform("Ctrl/Cmd+C", false), "Ctrl+C");
        assert_eq!(for_platform("Ctrl+Alt+R", false), "Ctrl+Alt+R");
    }
}
