//! Auto-switching: resolves the foreground app + title into the `layout` JSON the phone renders.
use std::time::Duration;

use serde_json::json;

use crate::config::{config, Key, KeyKind, Page};
use crate::engine::deck::{self, Grid};
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
pub fn layout_for(app: &str, title: &str, icon_b64: &str, shell: Option<&str>) -> AutoLayout {
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
    let mut buttons: Vec<(bool, Key)> = raw
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
            // NOT translated here. This layout is built once by the watcher and broadcast to
            // every phone, so translating at build time meant one language for all of them —
            // whichever said `hello` last. `auto_layout_json` does it per session instead.
            // A "picker:"'s option labels are translated too (each option's action travels
            // intact). Proper nouns (Opus, High...) fall back to themselves.
            let action_out = match action.strip_prefix("picker:") {
                Some(rest) => {
                    let opts: Vec<String> = rest
                        .split(';')
                        .map(|part| match part.split_once('=') {
                            Some((name, act)) => format!("{}={}", name.trim(), act),
                            None => part.to_string(),
                        })
                        .collect();
                    format!("picker:{}", opts.join(";"))
                }
                None => action.clone(),
            };
            (
                *rec,
                Key {
                    pos: *i,
                    label: label.to_string(),
                    icon: icon.clone(),
                    action: Some(action_out),
                    danger: *danger,
                    state: on.map(|on| json!({ "on": on })),
                    kind: KeyKind::Action,
                    ..Default::default()
                },
            )
        })
        .collect();
    // Auto-generated scene buttons from OBS: the user sees THEIR scenes with no setup.
    if profile_id == "obs" && obs.connected {
        for (i, s) in obs.scenes.iter().enumerate() {
            buttons.push((
                true,
                Key {
                    pos: 1000 + i,
                    label: s.clone(),
                    icon: "scene".into(),
                    action: Some(format!("obs:scene:{s}")),
                    state: Some(json!({ "on": *s == obs.current_scene })),
                    kind: KeyKind::Action,
                    ..Default::default()
                },
            ));
        }
    }
    // Recommended keys first, so page 0 is the profile's default selection and the "extras" spill
    // onto later pages instead of pushing useful keys off the first screen. Stable: within each
    // group the catalogue's authored order is kept.
    buttons.sort_by_key(|(rec, _)| !rec);
    pin_close_app(&mut buttons);
    let keys = buttons
        .into_iter()
        .enumerate()
        .map(|(pos, (_, k))| Key { pos, ..k })
        .collect();

    // System volume for the phone's dock (slider). null if there's no audio device.
    let sys = platform::system_volume()
        .map(|(vol, muted)| json!({ "vol": vol, "muted": muted }))
        .unwrap_or(serde_json::Value::Null);
    AutoLayout { profile_id, app_name: app.to_string(), app_icon: icon_b64.to_string(), keys, sys }
}

/// The one key that sits in the same cell on every profile: **third of the first row**, page 0.
///
/// Every one of the ~100 profiles carries "Cerrar app", and the recommended-first sort left it
/// wherever the profile's own list happened to put it — third on two profiles, tenth on another.
/// A key you reach for without looking has to be in the same place every time, and this one is
/// `danger` as well: hunting for it is how you close the wrong thing.
///
/// Pinning the POSITION is enough for both orientations: the grid transposes (5×3 upright, 3×5
/// sideways) but the addressing does not, so cell 2 is the third of the first row either way.
fn pin_close_app(buttons: &mut Vec<(bool, Key)>) {
    /// Identified by its ACTION, not its label: the label is translated at render time, and the
    /// editor lets a user rename the key without it stopping being the close key.
    const CLOSE_APP: &str = "alt+F4";
    const CLOSE_AT: usize = 2;
    let Some(at) =
        buttons.iter().position(|(_, k)| {
            k.action.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(CLOSE_APP))
        })
    else {
        return;
    };
    let key = buttons.remove(at);
    // A profile with fewer keys than that puts it last rather than leaving a hole before it.
    let at = CLOSE_AT.min(buttons.len());
    buttons.insert(at, key);
}

/// The resolved auto-mode layout, before pagination.
///
/// Deliberately grid-independent: it is resolved ONCE by the 500 ms poll and broadcast to every
/// connected phone, and those phones can have different grids. Each connection turns this into its
/// own `layout` message via [`auto_layout_json`].
#[derive(Clone, Default)]
pub struct AutoLayout {
    pub profile_id: String,
    pub app_name: String,
    /// BARE base64 PNG — `extract_icon_b64` strips any prefix. Use [`AutoLayout::app_icon_uri`] to
    /// put it on the wire: §4.1 documents `appIcon` as a data URI, and every other image the
    /// protocol carries is one, so a consumer that decodes key images decodes this too.
    pub app_icon: String,
    pub keys: Vec<Key>,
    pub sys: serde_json::Value,
}

impl AutoLayout {
    /// Same page/key addressing as a deck, so `key` presses resolve identically in both modes.
    pub(crate) fn as_page(&self) -> Page {
        Page { id: self.profile_id.clone(), name: String::new(), keys: self.keys.clone() }
    }
}

impl AutoLayout {
    /// The foreground app's icon as §4.1 promises it: a data URI, or `None` when there is no icon
    /// (an app whose executable has none, or a window the host could not resolve).
    ///
    /// The host was sending the bare base64 the extractor returns, so a client that decoded it the
    /// same way it decodes every other image got nothing — the contract said `data:image/png;…`
    /// and the wire did not.
    pub fn app_icon_uri(&self) -> Option<String> {
        if self.app_icon.is_empty() {
            return None;
        }
        Some(format!("data:image/png;base64,{}", self.app_icon))
    }
}

/// An auto-mode `layout` message for one page of the client's grid (protocol §4.1).
pub fn auto_layout_json(a: &AutoLayout, grid: Grid, page: usize, lang: i18n::Lang) -> String {
    render(a, grid, page, lang, "layout")
}

/// The auto-mode half of [`deck::preload_json`] — one page either side, or `None` if there is no
/// such page. Auto mode paginates too: a profile with more keys than the grid holds has pages, and
/// swiping them should look the same as swiping a deck's.
pub fn auto_preload_json(
    a: &AutoLayout,
    grid: Grid,
    page: isize,
    lang: i18n::Lang,
) -> Option<String> {
    let total = deck::pages(&a.as_page(), grid);
    let page = usize::try_from(page).ok().filter(|&p| p < total)?;
    Some(render(a, grid, page, lang, "page_preload"))
}

fn render(a: &AutoLayout, grid: Grid, page: usize, lang: i18n::Lang, kind: &str) -> String {
    let pg = a.as_page();
    let pages = deck::pages(&pg, grid);
    let page = page.min(pages - 1);
    // Translated HERE, once per session, because this is the only point that knows which phone is
    // being answered. It also catches OBS' dynamic labels, which is why it used to sit at the end
    // of `layout_for`.
    let mut keys = deck::keys_for(&pg, grid, page);
    for k in &mut keys {
        k.label = i18n::tr(lang, &k.label).to_string();
        if let Some(rest) = k.action.as_deref().and_then(|a| a.strip_prefix("picker:")) {
            let opts: Vec<String> = rest
                .split(';')
                .map(|part| match part.split_once('=') {
                    Some((name, act)) => format!("{}={}", i18n::tr(lang, name.trim()), act),
                    None => part.to_string(),
                })
                .collect();
            k.action = Some(format!("picker:{}", opts.join(";")));
        }
    }
    json!({
        "v": 2, "type": kind, "mode": "auto",
        "source": { "kind": "profile", "id": a.profile_id,
                    "appName": a.app_name, "appIcon": a.app_icon_uri() },
        "grid": { "rows": grid.rows, "cols": grid.cols },
        "page": page, "pages": pages,
        "keys": keys,
        "sys": a.sys,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(label: &str, action: &str, rec: bool) -> (bool, Key) {
        (
            rec,
            Key {
                label: label.into(),
                action: Some(action.into()),
                kind: KeyKind::Action,
                ..Default::default()
            },
        )
    }

    fn labels(buttons: &[(bool, Key)]) -> Vec<&str> {
        buttons.iter().map(|(_, k)| k.label.as_str()).collect()
    }

    #[test]
    fn close_app_lands_on_the_third_cell_whatever_the_profile_says() {
        // The real shape: "Cerrar app" authored last but recommended, so the sort puts it fifth.
        let mut b = vec![
            button("Copiar", "ctrl+c", true),
            button("Pegar", "ctrl+v", true),
            button("Nueva carpeta", "ctrl+shift+n", true),
            button("Renombrar", "f2", true),
            button("Cerrar app", "alt+F4", true),
            button("Cortar", "ctrl+x", false),
        ];
        pin_close_app(&mut b);
        assert_eq!(labels(&b), ["Copiar", "Pegar", "Cerrar app", "Nueva carpeta", "Renombrar", "Cortar"]);
        // Idempotent: it is already there, and re-pinning must not rotate the pad every 500 ms.
        pin_close_app(&mut b);
        assert_eq!(labels(&b)[2], "Cerrar app");
    }

    /// The editor writes whatever the user typed, and `alt+f4` is the same key.
    #[test]
    fn the_action_is_matched_however_it_was_typed() {
        let mut b = vec![button("a", "ctrl+c", true), button("Cerrar", "alt+f4", false)];
        pin_close_app(&mut b);
        assert_eq!(labels(&b)[1], "Cerrar", "two keys: last is as close to third as it gets");
    }

    /// A profile with no close key at all (and one whose `uia:` keys were all filtered out) must
    /// come back unchanged rather than shuffled.
    #[test]
    fn a_profile_without_one_is_left_alone() {
        let mut b = vec![button("a", "ctrl+c", true), button("b", "ctrl+v", true)];
        pin_close_app(&mut b);
        assert_eq!(labels(&b), ["a", "b"]);
        let mut empty: Vec<(bool, Key)> = vec![];
        pin_close_app(&mut empty);
        assert!(empty.is_empty());
    }
}

/// Polls the foreground app and publishes the layout when it changes (app change or profile edit).
pub async fn watch_active_app() {
    let mut last_app = String::new();
    let mut icon = String::new();
    // Change detection stays on the JSON: comparing the rendered form is what catches a live-state
    // change (OBS started recording) that leaves the profile identical.
    let mut last_layout = String::new();
    // The auto fingerprint below cannot speak for MANUAL sessions, and since F6 a deck can hold a
    // two-state key whose truth is OBS's rather than ours. It gets its own fingerprint here, on
    // the timer that is already running, rather than a second one or a hook inside obs.rs.
    let mut last_live = String::new();
    // Which apps are open. Nothing watched this: a manual deck was only re-sent when OBS changed,
    // so a `launch:` key's `state.running` never updated and the phone's "opening…" spinner ran to
    // its own timeout with the window already on screen. Same 500 ms poll, and the ids come from
    // the decks a phone is actually showing — empty, and this costs nothing.
    let mut last_apps = String::new();
    loop {
        let ids = crate::net::ws::manual_app_ids();
        if !ids.is_empty() {
            let open = tokio::task::spawn_blocking({
                let ids = ids.clone();
                move || platform::apps::running(&ids)
            })
            .await
            .unwrap_or_default();
            let apps: String =
                open.iter().map(|b| if *b { '1' } else { '0' }).collect();
            if apps != last_apps {
                last_apps = apps;
                crate::net::ws::push_manual_layouts();
            }
        }
        let live = {
            let o = obs_state().lock().unwrap();
            format!(
                "{}|{}|{}|{}|{}|{}",
                o.connected, o.recording, o.streaming, o.replay_active, o.mic_muted, o.current_scene
            )
        };
        if live != last_live {
            last_live = live;
            crate::net::ws::push_manual_layouts();
        }
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
        // Feeds the window switcher's MRU (§4.3) off the poll that is already running: no second
        // timer, and the ordering is right the first time the user opens the switcher.
        crate::engine::windows::touch(platform::foreground_window());
        let layout = {
            let (a, t, ic) = (app.clone(), title.clone(), icon.clone());
            // detect_shell_kind walks the process tree -> onto the blocking thread, like the rest.
            tokio::task::spawn_blocking(move || layout_for(&a, &t, &ic, platform::detect_shell_kind(pid, &t)))
                .await
                .unwrap_or_default()
        };
        // Rendered on the reference grid purely to detect change; every connection re-renders it
        // for its own grid on the way out.
        // The fingerprint renders in the SOURCE language: it exists to notice that the layout
        // changed, and it must not change just because a phone in another language connected.
        let fingerprint = auto_layout_json(&layout, Grid::default(), 0, i18n::Lang::Es);
        if fingerprint != last_layout {
            last_layout = fingerprint;
            crate::net::ws::publish_layout(layout);
            // §4.1: a MANUAL deck carries what the PC has in front too, and the foreground changing
            // is the only thing that can change it. Nothing re-sent it — a phone sitting on a deck
            // kept the app it was opened next to, for hours, which reads as the whole surface
            // having stopped updating.
            crate::net::ws::push_manual_layouts();
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
