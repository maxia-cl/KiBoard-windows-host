//! Auto-switching: resolves the foreground app + title into the `layout` JSON the phone renders.
use std::time::Duration;

use serde_json::json;

use crate::config::config;
use crate::engine::state::obs_state;
use crate::i18n;
use crate::platform;

/// A button copied out of the config lock: (index, label, action, icon, danger, recommended).
/// F2 replaces this with the real `Key` model; until then the tuple keeps the copy cheap.
type RawButton = (usize, String, String, String, bool, bool);

/// Builds the layout JSON for the given app, from the profiles. Matches against both the app
/// name AND the window title (this enables per-tab sub-profiles, e.g. Google Sheets).
/// `shell` = shell detected in the window's process tree (see `platform::detect_shell_kind`);
/// it outranks the title, which lies when you run another shell inside a tab.
pub fn layout_for(app: &str, title: &str, icon_b64: &str, shell: Option<&str>) -> String {
    // Pick a profile and COPY its buttons (id, label, action, icon, danger, recommended); release
    // the lock before querying UIA (slow) so it doesn't block everyone else.
    // OBS state: if it's recording or live, the "obs" profile is PINNED even if the foreground
    // window is the game (title-based shortcuts can't see OBS behind a full-screen game).
    let obs = obs_state().lock().unwrap().clone();
    let obs_live = obs.connected && (obs.recording || obs.streaming);
    let (profile_id, raw): (String, Vec<RawButton>) = {
        let cfg = config().lock().unwrap();
        let hay = format!("{app} {title}").to_lowercase();
        // Priority: OBS live > AI agent by title > REAL detected shell > title > generic.
        // The agent goes before the shell because it runs INSIDE the terminal and its buttons
        // (Accept/Reject/Model) matter more than the shell hosting it.
        let profile = if obs_live { cfg.profiles.iter().find(|p| p.id == "obs") } else { None }
            .or_else(|| {
                cfg.profiles.iter().find(|p| {
                    p.id == "ai" && p.matches.iter().any(|m| hay.contains(&m.to_lowercase()))
                })
            })
            .or_else(|| shell.and_then(|id| cfg.profiles.iter().find(|p| p.id == id)))
            .or_else(|| {
                cfg.profiles
                    .iter()
                    .find(|p| p.matches.iter().any(|m| hay.contains(&m.to_lowercase())))
            })
            .or_else(|| cfg.profiles.iter().find(|p| p.matches.is_empty()))
            .or_else(|| cfg.profiles.last());
        match profile {
            Some(p) => (
                p.id.clone(),
                p.buttons
                    .iter()
                    .enumerate()
                    .map(|(i, b)| {
                        (i, b.label.clone(), b.action.clone(), b.icon.clone(), b.danger, b.recommended)
                    })
                    .collect(),
            ),
            None => ("empty".into(), vec![]),
        }
    };
    // Hide uia buttons whose control is disabled right now (dynamic app state: e.g. "Crop" only
    // enables once a selection is made).
    let disabled = platform::uia_disabled_actions(&raw);
    let mut buttons: Vec<_> = raw
        .iter()
        .filter(|(_, _, action, _, _, _)| !disabled.contains(action))
        // No mic in OBS (neither global nor in the scenes): the Mic button would do nothing -> hide it.
        .filter(|(_, _, action, _, _, _)| {
            !(action == "obs:mic" && obs.connected && obs.mic_name.is_empty())
        })
        .map(|(i, label, action, icon, danger, rec)| {
            // Live state for OBS buttons (REC on, mic muted...).
            let on = if obs.connected {
                match action.as_str() {
                    "obs:record" => Some(obs.recording),
                    "obs:stream" => Some(obs.streaming),
                    "obs:mic" => Some(obs.mic_muted),
                    "obs:replaybuffer" => Some(obs.replay_active),
                    _ => None,
                }
            } else {
                None
            };
            // Dynamic label: the toggle says what it's ABOUT TO DO, not what it is.
            let label = match (action.as_str(), on) {
                ("obs:record", Some(true)) => "Detener grab.",
                ("obs:stream", Some(true)) => "Cortar directo",
                _ => label.as_str(),
            };
            // Translate to the client's language (hello carries the locale). Translated here, at
            // the end, so it also covers OBS' dynamic labels.
            let label = i18n::tr(label);
            // A "picker:"'s option labels are translated too (each option's action travels
            // intact). Proper nouns (Opus, High...) fall back to themselves.
            let action_out = match action.strip_prefix("picker:") {
                Some(rest) => {
                    let opts: Vec<String> = rest
                        .split(';')
                        .map(|part| match part.split_once('=') {
                            Some((name, act)) => format!("{}={}", i18n::tr(name.trim()), act),
                            None => part.to_string(),
                        })
                        .collect();
                    format!("picker:{}", opts.join(";"))
                }
                None => action.clone(),
            };
            let mut j = json!({ "id": i, "label": label, "action": action_out, "icon": icon, "danger": danger, "recommended": rec });
            if let Some(on) = on {
                j["on"] = json!(on);
            }
            j
        })
        .collect();
    // Auto-generated scene buttons from OBS: the user sees THEIR scenes with no setup.
    if profile_id == "obs" && obs.connected {
        for (i, s) in obs.scenes.iter().enumerate() {
            buttons.push(json!({
                "id": 1000 + i, "label": s, "action": format!("obs:scene:{s}"), "icon": "scene",
                "danger": false, "recommended": true, "on": *s == obs.current_scene
            }));
        }
    }
    // System volume for the phone's dock (slider). null if there's no audio device.
    let sys = platform::system_volume()
        .map(|(vol, muted)| json!({ "vol": vol, "muted": muted }))
        .unwrap_or(serde_json::Value::Null);
    json!({
        "v": 2, "type": "layout", "profileId": profile_id,
        "appName": app, "appIcon": icon_b64, "buttons": buttons, "sys": sys
    })
    .to_string()
}

/// Polls the foreground app and publishes the layout when it changes (app change or profile edit).
pub async fn watch_active_app() {
    let mut last_app = String::new();
    let mut icon = String::new();
    let mut last_layout = String::new();
    loop {
        let (app, title, path, pid) = match active_win_pos_rs::get_active_window() {
            Ok(w) => (
                w.app_name,
                w.title,
                w.process_path.to_string_lossy().to_string(),
                w.process_id as u32,
            ),
            Err(_) => (String::new(), String::new(), String::new(), 0),
        };
        if app != last_app {
            last_app = app.clone();
            icon = platform::extract_icon_b64(&path); // only on app change
        }
        let layout = {
            let (a, t, ic) = (app.clone(), title.clone(), icon.clone());
            // detect_shell_kind walks the process tree -> onto the blocking thread, like the rest.
            tokio::task::spawn_blocking(move || layout_for(&a, &t, &ic, platform::detect_shell_kind(pid, &t)))
                .await
                .unwrap_or_default()
        };
        if !layout.is_empty() && layout != last_layout {
            last_layout = layout.clone();
            crate::net::ws::publish_layout(layout);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
