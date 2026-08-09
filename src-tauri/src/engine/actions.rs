//! Macro DSL: chained steps (">>") executed on the foreground window. Kept intact from v1 — the
//! most valuable asset (docs/implementation-plan.md §2).
use std::time::Duration;

use crate::platform;

pub fn run_action(action: &str) -> Result<(), &'static str> {
    // "uia:<name>" -> press an app control by its accessibility name (for toolbars with no
    // keyboard shortcut, e.g. Paint). If the action STARTS with "uia:", the whole ">>"-chained
    // string is UIA steps (menu/flyout -> option): "uia:Rotate>>Rotate 90 right" opens "Rotate",
    // waits, clicks the option.
    if let Some(chain) = action.strip_prefix("uia:") {
        for (i, step) in chain.split(">>").enumerate() {
            if i > 0 {
                std::thread::sleep(Duration::from_millis(250)); // let the flyout render
            }
            platform::invoke_uia(step.trim())?;
        }
        return Ok(());
    }
    // General macro: steps separated by ">>", each one a hotkey, "type:", "uia:" or "screenshot".
    // E.g.: "ctrl+c>>alt+tab>>ctrl+v". ponytail: a type: snippet can't contain ">>".
    for (i, step) in action.split(">>").enumerate() {
        if i > 0 {
            std::thread::sleep(Duration::from_millis(250)); // let the app process the previous step
        }
        run_step(step.trim())?;
    }
    Ok(())
}

/// The concrete action behind a key that asks the phone something first (protocol §4.2).
///
/// `picker:` and `colorpicker:` are a list of named branches; `prompt:` is a template with a hole.
/// The phone draws the list — the whole string is in the layout it was given — and sends back an
/// INDEX or the text typed, never an action. That is §4.2, and it is why the branch is chosen HERE,
/// against the host's own key, rather than trusting anything that came off the wire.
///
/// Anything else is returned unchanged, so every ordinary key goes through this untouched.
///
/// `None` means the choice names nothing that exists — including a client that has never heard of
/// options and pressed one of these blind, which is what these keys did for everybody until now.
pub fn choose(action: &str, option: Option<usize>, text: Option<&str>) -> Option<String> {
    if let Some(list) = action.strip_prefix("picker:") {
        return branch(list, option?).map(|(_, act)| act.to_string());
    }
    if let Some(list) = action.strip_prefix("colorpicker:") {
        // The hex is what the PHONE paints the swatch with. What the PC runs is the palette entry
        // by name — Paint's swatches are UIA ListItems, and the catalogue stores their exact live
        // names for that reason.
        return branch(list, option?).map(|(name, _)| format!("uia:{name}"));
    }
    if let Some(rest) = action.strip_prefix("prompt:") {
        let (_, template) = rest.split_once('=')?;
        let text = text?.trim();
        // `>>` would close the host's step and open one the user was never shown.
        if text.is_empty() || text.contains(">>") {
            return None;
        }
        return Some(template.replace("{}", text));
    }
    Some(action.to_string())
}

/// The nth `Name=value` of a `;`-separated list. Split on the FIRST `=` only: a value is an action
/// chain and carries plenty more of them.
fn branch(list: &str, at: usize) -> Option<(&str, &str)> {
    list.split(';').nth(at)?.split_once('=').map(|(name, value)| (name.trim(), value))
}

/// A single atomic step: screenshot, UIA button, literal text, or a keyboard shortcut.
pub fn run_step(step: &str) -> Result<(), &'static str> {
    if step == "screenshot" {
        return platform::take_screenshot();
    }
    // "wait:<ms>" -> pause between macro steps (e.g. letting a CLI's autocomplete process text
    // before enter). Capped at 2s so a malicious profile can't hang.
    if let Some(ms) = step.strip_prefix("wait:") {
        let ms: u64 = ms.trim().parse().map_err(|_| "bad_wait")?;
        std::thread::sleep(Duration::from_millis(ms.min(2000)));
        return Ok(());
    }
    if let Some(name) = step.strip_prefix("uia:") {
        return platform::invoke_uia(name.trim());
    }
    // "obs:<cmd>" -> request to obs-websocket (works with the game in the foreground).
    if let Some(cmd) = step.strip_prefix("obs:") {
        return crate::integrations::obs::obs_action(cmd);
    }
    // "vol:<0-100>" -> absolute master volume (the dock's slider).
    if let Some(v) = step.strip_prefix("vol:") {
        let pct: f32 = v.trim().parse().map_err(|_| "bad_vol")?;
        return platform::set_system_volume((pct / 100.0).clamp(0.0, 1.0));
    }
    // "scroll:<n>" -> mouse wheel (n>0 down, n<0 up). Holding down = continuous scroll.
    if let Some(n) = step.strip_prefix("scroll:") {
        use enigo::{Axis, Enigo, Mouse, Settings};
        let lines: i32 = n.trim().parse().map_err(|_| "bad_scroll")?;
        let mut e = Enigo::new(&Settings::default()).map_err(|_| "internal")?;
        return e.scroll(lines, Axis::Vertical).map_err(|_| "internal");
    }
    // "mouse:<dx>,<dy>" -> moves the cursor relatively (the phone draws a trackpad; see _mousePad).
    if let Some(rest) = step.strip_prefix("mouse:") {
        use enigo::{Coordinate, Enigo, Mouse, Settings};
        let (dx, dy) = rest.split_once(',').ok_or("bad_mouse")?;
        let dx: i32 = dx.trim().parse().map_err(|_| "bad_mouse")?;
        let dy: i32 = dy.trim().parse().map_err(|_| "bad_mouse")?;
        let mut e = Enigo::new(&Settings::default()).map_err(|_| "internal")?;
        return e.move_mouse(dx, dy, Coordinate::Rel).map_err(|_| "internal");
    }
    // "hold:left|right" / "release:left|right" -> press-and-hold/release the button, for DRAGGING
    // (moving windows, selecting text, dragging files). The phone sends these from the trackpad's
    // "Drag" button. NOTE: if the button stays pressed the desktop becomes unusable, so the phone
    // always releases when leaving the trackpad.
    if let Some(which) = step.strip_prefix("hold:").or_else(|| step.strip_prefix("release:")) {
        use enigo::{Button, Direction, Enigo, Mouse, Settings};
        let btn = if which.trim() == "right" { Button::Right } else { Button::Left };
        let dir = if step.starts_with("hold:") { Direction::Press } else { Direction::Release };
        let mut e = Enigo::new(&Settings::default()).map_err(|_| "internal")?;
        return e.button(btn, dir).map_err(|_| "internal");
    }
    // "click:left|right" -> mouse click (a tap on the trackpad).
    if let Some(which) = step.strip_prefix("click:") {
        use enigo::{Button, Direction, Enigo, Mouse, Settings};
        let btn = match which.trim() {
            "right" => Button::Right,
            _ => Button::Left,
        };
        let mut e = Enigo::new(&Settings::default()).map_err(|_| "internal")?;
        return e.button(btn, Direction::Click).map_err(|_| "internal");
    }
    // "hscroll:<n>" -> horizontal wheel (trackpad: two fingers sideways).
    if let Some(n) = step.strip_prefix("hscroll:") {
        use enigo::{Axis, Enigo, Mouse, Settings};
        let lines: i32 = n.trim().parse().map_err(|_| "bad_scroll")?;
        let mut e = Enigo::new(&Settings::default()).map_err(|_| "internal")?;
        return e.scroll(lines, Axis::Horizontal).map_err(|_| "internal");
    }
    // "zoom:<n>" -> ctrl+wheel (universal zoom). n>0 zoom in, n<0 zoom out.
    if let Some(n) = step.strip_prefix("zoom:") {
        use enigo::{Axis, Direction, Enigo, Key, Keyboard, Mouse, Settings};
        let steps: i32 = n.trim().parse().map_err(|_| "bad_zoom")?;
        let mut e = Enigo::new(&Settings::default()).map_err(|_| "internal")?;
        e.key(Key::Control, Direction::Press).map_err(|_| "internal")?;
        let r = e.scroll(-steps, Axis::Vertical); // wheel up (negative n) = zoom in
        let _ = e.key(Key::Control, Direction::Release);
        return r.map_err(|_| "internal");
    }
    // "type:<text>" -> types the literal text (snippets: canned replies, emails, formulas).
    if let Some(text) = step.strip_prefix("type:") {
        use enigo::{Enigo, Keyboard, Settings};
        let mut e = Enigo::new(&Settings::default()).map_err(|_| "internal")?;
        return e.text(text).map_err(|_| "internal");
    }
    // "open:<url|path>" -> hands it to the default handler (browser, explorer). Only http(s) and
    // absolute paths: without this an "open:" key could invoke a `file://`-adjacent scheme handler
    // (ms-settings:, shell:...) and reach far past "open a link".
    if let Some(target) = step.strip_prefix("open:") {
        let target = target.trim();
        let allowed = target.starts_with("http://")
            || target.starts_with("https://")
            || std::path::Path::new(target).is_absolute();
        if !allowed {
            return Err("blocked_action");
        }
        return platform::open_target(target);
    }
    // App-catalogue actions (protocol §3). The target is an AUMID, a known-folder AppID or an
    // absolute path — `platform::apps` is what knows how to tell them apart.
    if let Some(id) = step.strip_prefix("launch:") {
        return platform::apps::launch(id.trim());
    }
    if let Some(id) = step.strip_prefix("focus:") {
        return platform::apps::focus(id.trim());
    }
    if let Some(id) = step.strip_prefix("kill:") {
        return platform::apps::close(id.trim());
    }
    // "run:<cmd>" is opt-in from Settings and that switch does not exist yet. Until it does, an
    // arbitrary shell command from the network is exactly what must NOT be reachable.
    if step.starts_with("run:") {
        return Err("blocked_action");
    }
    run_hotkey(step)
}

/// Runs a shortcut like "ctrl+shift+p", "alt+F4", "ctrl+c".
fn run_hotkey(combo: &str) -> Result<(), &'static str> {
    use enigo::{Direction::*, Enigo, Keyboard, Settings};
    let parts: Vec<&str> = combo.split('+').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return Err("bad_keys");
    }
    let (key_tok, mod_toks) = parts.split_last().unwrap();
    let mods: Vec<enigo::Key> = mod_toks.iter().map(|m| parse_modifier(m)).collect::<Result<_, _>>()?;
    let key = parse_key(key_tok)?;
    let mut e = Enigo::new(&Settings::default()).map_err(|_| "internal")?;
    for m in &mods {
        e.key(*m, Press).map_err(|_| "internal")?;
    }
    // Gap between the modifier and the key: Electron apps (Claude Code, VS Code, Discord...)
    // drop the modifier if the event arrives in the same tick and process the "bare" key
    // (shift+tab arrived as Tab and moved focus instead of switching mode).
    if !mods.is_empty() {
        std::thread::sleep(Duration::from_millis(40));
    }
    let res = e.key(key, Click);
    if !mods.is_empty() {
        std::thread::sleep(Duration::from_millis(20));
    }
    for m in mods.iter().rev() {
        let _ = e.key(*m, Release);
    }
    res.map_err(|_| "internal")
}

fn parse_modifier(tok: &str) -> Result<enigo::Key, &'static str> {
    use enigo::Key;
    match tok.to_lowercase().as_str() {
        "ctrl" | "control" => Ok(Key::Control),
        "shift" => Ok(Key::Shift),
        "alt" => Ok(Key::Alt),
        "win" | "meta" | "super" | "cmd" => Ok(Key::Meta),
        _ => Err("bad_modifier"),
    }
}

fn parse_key(tok: &str) -> Result<enigo::Key, &'static str> {
    use enigo::Key;
    let t = tok.to_lowercase();
    // Function keys f1..f12
    if let Some(n) = t.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return match n {
            1 => Ok(Key::F1), 2 => Ok(Key::F2), 3 => Ok(Key::F3), 4 => Ok(Key::F4),
            5 => Ok(Key::F5), 6 => Ok(Key::F6), 7 => Ok(Key::F7), 8 => Ok(Key::F8),
            9 => Ok(Key::F9), 10 => Ok(Key::F10), 11 => Ok(Key::F11), 12 => Ok(Key::F12),
            _ => Err("bad_key"),
        };
    }
    match t.as_str() {
        "enter" | "return" => Ok(Key::Return),
        "tab" => Ok(Key::Tab),
        "esc" | "escape" => Ok(Key::Escape),
        "space" => Ok(Key::Space),
        "backspace" => Ok(Key::Backspace),
        "delete" | "del" => Ok(Key::Delete),
        "home" => Ok(Key::Home),
        "end" => Ok(Key::End),
        "up" => Ok(Key::UpArrow),
        "down" => Ok(Key::DownArrow),
        "left" => Ok(Key::LeftArrow),
        "right" => Ok(Key::RightArrow),
        // SYSTEM media keys: work in any app (couch remote control).
        "volup" => Ok(Key::VolumeUp),
        "voldown" => Ok(Key::VolumeDown),
        "volmute" => Ok(Key::VolumeMute),
        "playpause" => Ok(Key::MediaPlayPause),
        "nexttrack" => Ok(Key::MediaNextTrack),
        "prevtrack" => Ok(Key::MediaPrevTrack),
        // Numpad +/- by virtual key (VK_ADD/VK_SUBTRACT): independent of keyboard layout.
        // "ctrl+=" failed on ES/Latin keyboards (= is a Shift key -> enigo maps it wrong);
        // browsers and viewers accept ctrl + numpad plus/minus for zoom.
        "add" => Ok(Key::Other(0x6B)),
        "subtract" => Ok(Key::Other(0x6D)),
        s if s.chars().count() == 1 => Ok(Key::Unicode(s.chars().next().unwrap())),
        _ => Err("bad_key"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_pauses_and_validates() {
        assert!(run_step("wait:50").is_ok());
        assert!(run_step("wait:abc").is_err());
    }

    /// The two keys that started this: `picker:` was in the catalogue from the first day and no
    /// step ever understood it, so "Modelo" and "Esfuerzo" answered `bad_key` and typed nothing.
    #[test]
    fn a_picker_runs_the_branch_that_was_chosen() {
        let a = "picker:Fable=type:/model>>wait:400>>type: claude-fable-5;Opus=type:/model>>wait:400>>type: claude-opus-5";
        assert_eq!(
            choose(a, Some(1), None).as_deref(),
            Some("type:/model>>wait:400>>type: claude-opus-5"),
            "the value is a whole chain, `=` splits once"
        );
        assert!(choose(a, Some(0), None).unwrap().starts_with("type:/model"));
        // No choice, or one that names no branch: nothing to run.
        assert_eq!(choose(a, None, None), None);
        assert_eq!(choose(a, Some(9), None), None);
    }

    /// The swatch's hex paints the phone; the PC clicks the palette entry BY NAME.
    #[test]
    fn a_colour_swatch_runs_the_palette_entry() {
        let a = "colorpicker:Negro=000000;Rojo=ED1C24";
        assert_eq!(choose(a, Some(1), None).as_deref(), Some("uia:Rojo"));
        assert_eq!(choose(a, None, None), None);
    }

    #[test]
    fn a_prompt_fills_the_hole_and_refuses_to_open_a_step() {
        let a = "prompt:Nombre de la carpeta=type:mkdir {}>>enter";
        assert_eq!(choose(a, None, Some("informes")).as_deref(), Some("type:mkdir informes>>enter"));
        assert_eq!(choose(a, None, Some("  ")), None, "empty is not a folder name");
        assert_eq!(choose(a, None, None), None);
        assert_eq!(
            choose(a, None, Some("x>>ctrl+s")),
            None,
            "`>>` would run a step the user was never shown"
        );
    }

    /// Every ordinary key goes through this untouched, options or not.
    #[test]
    fn anything_else_is_returned_as_it_came() {
        assert_eq!(choose("ctrl+c", None, None).as_deref(), Some("ctrl+c"));
        assert_eq!(choose("ctrl+c", Some(3), Some("x")).as_deref(), Some("ctrl+c"));
    }

    #[test]
    fn parses_keys() {
        assert!(parse_key("c").is_ok());
        assert!(parse_key("F4").is_ok());
        assert!(parse_key("enter").is_ok());
        assert!(parse_key("left").is_ok());
        assert!(parse_key("nope").is_err());
        // System media tokens
        for t in ["volup", "voldown", "volmute", "playpause", "nexttrack", "prevtrack"] {
            assert!(parse_key(t).is_ok(), "media token {t}");
        }
    }
}
