//! Empty implementation of the platform surface so the crate compiles on non-Windows targets.
//! No macOS/Linux backend exists yet (docs/implementation-plan.md §7) — this is scaffolding
//! only, not a real port.

pub fn extract_icon_b64(_path: &str) -> String {
    String::new()
}

pub fn detect_shell_kind(_root_pid: u32, _title: &str) -> Option<&'static str> {
    None
}

pub fn uia_disabled_actions(
    _buttons: &[(usize, String, String, String, bool, bool)],
) -> std::collections::HashSet<String> {
    std::collections::HashSet::new()
}

pub fn invoke_uia(_name: &str) -> Result<(), &'static str> {
    Err("unsupported_platform")
}

pub fn system_volume() -> Option<(u8, bool)> {
    None
}

pub fn set_system_volume(_level: f32) -> Result<(), &'static str> {
    Err("unsupported_platform")
}

pub fn icon_cached(_path: &str) -> String {
    String::new()
}

pub fn list_windows_json() -> String {
    serde_json::json!({ "v": 1, "type": "windows", "items": [] }).to_string()
}

pub fn focus_window(_id: isize) {}

pub fn process_running(_names: &[&str]) -> bool {
    false
}

pub fn take_screenshot() -> Result<(), &'static str> {
    Err("unsupported_platform")
}
