// KiBoard host — Phase 5 (programmable profiles), ported from v1 and split into modules for F0.
// LAN WebSocket + token/QR pairing + commands (arbitrary shortcuts) + auto-switching with
// user-editable profiles from the UI. Protocol: see KiBoard-protocol/protocol/README.md (v1
// today; F2 freezes v2).
use serde_json::json;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_updater::UpdaterExt;

mod config;
mod engine;
mod i18n;
mod integrations;
mod net;
mod platform;

use config::{config, new_token, Device, Profile};

pub(crate) const HOST_NAME: &str = "KiBoard Host";

pub(crate) fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Analytics
// ---------------------------------------------------------------------------

/// Anonymous analytics (Aptabase, direct HTTP API): feature events only, no PII.
/// ponytail: our own client; the official 1.0 plugin panics on Tauri 2 ("no reactor running").
/// No offline queue: no network = lost event, good enough for statistics.
fn track_event(name: &'static str) {
    if !config().lock().unwrap().analytics {
        return;
    }
    // Aptabase sessionId format: epoch_seconds * 1e8 + an 8-digit random number (the server
    // decodes the session start from the id; any other format = "Session is too old").
    let session_id = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let r: u64 = rand::random::<u64>() % 100_000_000;
        (secs * 100_000_000 + r).to_string()
    };
    tauri::async_runtime::spawn(async move {
        let body = json!({
            "timestamp": chrono_like_now(),
            "sessionId": session_id,
            "eventName": name,
            "systemProps": {
                "isDebug": cfg!(debug_assertions),
                "locale": "es",
                "osName": "windows",
                "osVersion": "",
                "appVersion": env!("CARGO_PKG_VERSION"),
                "sdkVersion": "kiboard-min@1",
            },
            "props": {},
        });
        let _ = reqwest::Client::new()
            .post("https://us.aptabase.com/api/v0/event")
            .header("App-Key", "A-US-9332956172")
            .json(&body)
            .send()
            .await;
    });
}

/// ISO-8601 UTC with no chrono dependency (Aptabase only needs seconds).
fn chrono_like_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    // Civil conversion (Howard Hinnant's algorithm, days -> y/m/d).
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

// ---------------------------------------------------------------------------
// UI commands (pairing + profile editor)
// ---------------------------------------------------------------------------

/// Tries out a profile-editor action. Waits 2s so the user can focus the target app (the action
/// runs on the foreground window, same as from the phone).
#[tauri::command]
async fn test_action(action: String) -> Result<(), String> {
    // enigo/UIA are blocking -> a separate thread so the host UI doesn't freeze.
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(2000));
        engine::actions::run_action(&action).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "internal".to_string())?
}

/// State for the UI header: connected devices, version, and the analytics toggle.
#[tauri::command]
fn host_status() -> serde_json::Value {
    let cfg = config().lock().unwrap();
    json!({
        "clients": net::ws::CLIENTS.load(std::sync::atomic::Ordering::Relaxed),
        "version": env!("CARGO_PKG_VERSION"),
        "analytics": cfg.analytics,
        "manualEnabled": cfg.manual_enabled,
    })
}

/// The language the editor window should speak: this PC's, never the phone's.
///
/// Same source as the tray menu ([`i18n::host_lang`]), so the two halves of the host cannot end up
/// in different languages. The strings themselves live in `src/lib/i18n.js` — they are Svelte's,
/// and a table crossing this bridge would buy nothing.
#[tauri::command]
fn host_lang() -> &'static str {
    match i18n::host_lang() {
        i18n::Lang::En => "en",
        i18n::Lang::Zh => "zh",
        i18n::Lang::Es => "es",
    }
}

#[tauri::command]
fn set_analytics(on: bool) {
    let mut cfg = config().lock().unwrap();
    cfg.analytics = on;
    cfg.save();
}

/// Opens the donations page in the browser (the link lives here, in one place).
#[tauri::command]
fn open_donate(app: tauri::AppHandle) {
    use tauri_plugin_opener::OpenerExt;
    let _ = app
        .opener()
        // Ko-fi while GitHub Sponsors is still in review (trade-controls flag, 2026-07).
        .open_url("https://ko-fi.com/kiboard", None::<&str>);
}

#[tauri::command]
fn pairing_info() -> serde_json::Value {
    let token = config().lock().unwrap().token.clone();
    let ip = net::pairing::best_lan_ip();
    let payload = json!({ "ip": ip, "port": net::ws::WS_PORT, "token": token, "name": HOST_NAME });
    let svg = net::pairing::qr_svg(&payload.to_string());
    json!({ "ip": ip, "port": net::ws::WS_PORT, "token": token, "svg": svg })
}

#[tauri::command]
fn unpair_all() -> serde_json::Value {
    let mut cfg = config().lock().unwrap();
    cfg.token = new_token();
    cfg.paired.clear();
    cfg.save();
    json!({ "ok": true })
}

#[tauri::command]
fn get_profiles() -> Vec<Profile> {
    config().lock().unwrap().profiles.clone()
}

/// Saves the edited profiles. The watcher detects the change and re-pushes the layout instantly.
#[tauri::command]
fn save_profiles(profiles: Vec<Profile>) -> serde_json::Value {
    let mut cfg = config().lock().unwrap();
    cfg.profiles = profiles;
    cfg.save();
    json!({ "ok": true })
}

// ---------------------------------------------------------------------------
// Editor (F5): the decks of manual mode, on real data
// ---------------------------------------------------------------------------

/// The decks as stored. Same struct that travels on the wire (§3), so the editor cannot drift
/// from the protocol by editing a shape only it understands.
#[tauri::command]
fn get_decks() -> Vec<config::Deck> {
    config().lock().unwrap().decks.clone()
}

/// Saves the edited decks. Every connected phone in manual mode is re-sent its layout, so the
/// editor's device and the real one never disagree about what a key does.
#[tauri::command]
fn save_decks(decks: Vec<config::Deck>) -> serde_json::Value {
    if let Err(e) = engine::deck::validate(&decks) {
        // Refusing beats saving a deck whose keys resolve to nothing on the phone.
        return json!({ "ok": false, "error": e });
    }
    {
        let mut cfg = config().lock().unwrap();
        cfg.decks = decks;
        cfg.save();
        // A key that lost its second state, or a page that was deleted, leaves its remembered face
        // behind — and a toggle added at that address later would arrive already switched on.
        engine::deck::forget_orphans(&cfg.decks);
    }
    // Disk is the truth again, so the preview has nothing left to stand in for. Clearing it also
    // re-pushes, which is what puts the saved deck on every phone.
    net::ws::clear_preview();
    net::ws::push_manual_layouts();
    json!({ "ok": true })
}

/// Live preview (§5, F5): shows UNSAVED decks on every phone in manual mode, so a key can be
/// dragged and felt on the real device before anything is written.
///
/// Validated exactly like a save. A preview that the phone cannot resolve is not a preview, it is
/// a broken deck in someone's hand — and unlike a save, nobody would be looking at the error.
#[tauri::command]
fn preview_decks(decks: Vec<config::Deck>) -> serde_json::Value {
    if let Err(e) = engine::deck::validate(&decks) {
        return json!({ "ok": false, "error": e });
    }
    net::ws::set_preview(decks);
    json!({ "ok": true })
}

/// Drops the preview and puts the saved decks back. The editor calls this when it closes, when it
/// leaves manual mode, and when changes are discarded.
#[tauri::command]
fn clear_preview() {
    net::ws::clear_preview();
}

/// The machine's app catalogue (F4) with its real icons, for the editor's Apps group.
#[tauri::command]
fn app_catalogue() -> Vec<serde_json::Value> {
    platform::apps::catalogue()
        .iter()
        .map(|a| {
            let icon = platform::apps::icon(&a.id);
            json!({
                "id": a.id,
                "name": a.name,
                "image": (!icon.is_empty()).then(|| format!("data:image/png;base64,{icon}")),
            })
        })
        .collect()
}

/// The user's own OBS scenes, by name. `obs_info` only reports how MANY there are, which is all a
/// status badge needs; the editor's catalogue needs to offer them one by one.
#[tauri::command]
fn obs_scenes() -> Vec<String> {
    engine::state::obs_state().lock().unwrap().scenes.clone()
}

/// OBS integration status for the host UI.
#[tauri::command]
fn obs_info() -> serde_json::Value {
    let (connected, scenes) = {
        let st = engine::state::obs_state().lock().unwrap();
        (st.connected, st.scenes.len())
    };
    let password_set = !config().lock().unwrap().obs_password.is_empty();
    // running without connected = OBS open with its WebSocket server off -> the case to guide.
    let running = platform::process_running(&["obs64.exe", "obs32.exe"]);
    json!({ "connected": connected, "scenes": scenes, "passwordSet": password_set, "running": running })
}

/// Saves the OBS WebSocket password; the client uses it on the next (re)connect attempt (<=5s).
#[tauri::command]
fn set_obs_password(password: String) -> serde_json::Value {
    let mut cfg = config().lock().unwrap();
    cfg.obs_password = password.trim().to_string();
    cfg.save();
    json!({ "ok": true })
}

/// Writes one deck to `<config>/decks/<id>.kbdeck.json` and shows it in Explorer.
///
/// Sharing a deck is a file operation, not a protocol one: the file holds exactly the `Deck` of
/// §3, so importing it is the editor reading JSON back. ponytail: no file-dialog plugin — the
/// export lands in one predictable place and Explorer opens on it, which is enough to attach it
/// to an e-mail. A "save as" dialog is a dependency away if anyone asks.
#[tauri::command]
fn export_deck(app: tauri::AppHandle, deck_id: String) -> serde_json::Value {
    let deck = config()
        .lock()
        .unwrap()
        .decks
        .iter()
        .find(|d| d.id == deck_id)
        .cloned();
    let Some(deck) = deck else {
        return json!({ "ok": false, "error": "no_such_deck" });
    };
    let dir = config::config_dir().join("decks");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return json!({ "ok": false, "error": e.to_string() });
    }
    let path = dir.join(format!("{}.kbdeck.json", safe_file_name(&deck.id)));
    let body = serde_json::to_string_pretty(&deck).unwrap_or_default();
    if let Err(e) = std::fs::write(&path, body) {
        return json!({ "ok": false, "error": e.to_string() });
    }
    use tauri_plugin_opener::OpenerExt;
    let _ = app.opener().reveal_item_in_dir(&path);
    json!({ "ok": true, "path": path.to_string_lossy() })
}

/// A deck id comes from `config.json` or from a file somebody else wrote, and here it becomes a
/// path. Everything that is not a plain name is flattened, so an id of `../../evil` cannot write
/// outside the decks folder.
fn safe_file_name(id: &str) -> String {
    let name: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if name.trim_matches('-').is_empty() {
        "deck".into()
    } else {
        name
    }
}

/// QR of a profile to share it: another KiBoard scans it from the phone and imports it.
#[tauri::command]
fn profile_qr(profile: Profile) -> String {
    let payload = format!(
        "kbprofile:{}",
        serde_json::to_string(&profile).unwrap_or_default()
    );
    net::pairing::qr_svg(&payload)
}

// ---------------------------------------------------------------------------
// v2 pairing + device management (F1, protocol/README.md §2)
// ---------------------------------------------------------------------------

/// The host's id/name and whether it's currently accepting new pair_request attempts, plus the
/// pending code if a phone is mid-pairing right now — for the host UI to show
/// "«device» wants to connect: 418203".
#[tauri::command]
fn pairing_status() -> serde_json::Value {
    // Do not hold the config lock while asking the pairing and network subsystems for their
    // state. `pairing::confirm` takes those locks in the opposite order, and keeping the guard
    // here would make the status poll capable of deadlocking a phone that confirms at the same
    // time.
    let (host_id, pairing_open, manual_enabled) = {
        let cfg = config().lock().unwrap();
        (cfg.host_id.clone(), cfg.pairing_open, cfg.manual_enabled)
    };
    let pending = net::pairing::pending_status();
    json!({
        "hostId": host_id,
        "pairingOpen": pairing_open,
        "manualEnabled": manual_enabled,
        // R1: the phone can always be pointed at an address by hand, for the networks that never
        // pass mDNS on. That only helps if the PC says what to type, so it is shown next to the
        // code rather than left for the user to go and find in Windows' settings.
        "ip": net::pairing::best_lan_ip(),
        "port": net::ws::WS_PORT,
        // §2.2: what the phone pins. Shown so the two can be compared by eye when a device starts
        // refusing to connect — that is either a new certificate or someone in the middle, and the
        // phone cannot tell the user which.
        "fingerprint": net::tls::fingerprint(),
        "pending": pending.map(|(device, code, expires_in)| json!({
            "device": device, "code": code, "expiresIn": expires_in
        })),
    })
}

#[tauri::command]
fn list_devices() -> Vec<Device> {
    config().lock().unwrap().devices.clone()
}

/// Revokes one device's token. Everyone else stays connected — F1's whole point over v1's single
/// shared token.
#[tauri::command]
fn revoke_device(device_id: String) -> serde_json::Value {
    let mut cfg = config().lock().unwrap();
    cfg.devices.retain(|d| d.device_id != device_id);
    cfg.save();
    json!({ "ok": true })
}

/// Opens or closes the host to new pair_request attempts (mDNS TXT `pair`). Already-paired
/// devices keep working either way.
#[tauri::command]
fn set_pairing_open(open: bool) {
    // `advertise` reads the config to rebuild the mDNS TXT record. Release this guard first:
    // `std::sync::Mutex` is not re-entrant, so calling it while the guard is alive blocks this
    // Tauri command forever and leaves the checkbox awaiting a response.
    {
        let mut cfg = config().lock().unwrap();
        cfg.pairing_open = open;
        cfg.save();
    }
    net::discovery::advertise("auto");
}

#[tauri::command]
fn set_manual_enabled(enabled: bool) -> serde_json::Value {
    let show_intro = net::ws::set_manual_enabled(enabled);
    json!({ "ok": true, "manualEnabled": enabled, "showIntro": show_intro })
}

// ---------------------------------------------------------------------------
// Tauri bootstrap
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    config();

    tauri::Builder::default()
        // Single instance: if one is already running, the second launch closes and instead
        // shows/focuses the existing window. Must be registered FIRST (Tauri's recommendation).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            pairing_info,
            unpair_all,
            get_profiles,
            save_profiles,
            get_decks,
            save_decks,
            app_catalogue,
            preview_decks,
            clear_preview,
            export_deck,
            obs_scenes,
            profile_qr,
            obs_info,
            set_obs_password,
            host_status,
            host_lang,
            set_analytics,
            open_donate,
            test_action,
            pairing_status,
            list_devices,
            revoke_device,
            set_pairing_open,
            set_manual_enabled
        ])
        // The X hides the window (the app lives in the tray); without this it gets destroyed and
        // the app quits.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            // One event per launch; client connections are already reported by the phone (paired_ok).
            track_event("app_started");
            tauri::async_runtime::spawn(net::ws::run_ws_server());
            tauri::async_runtime::spawn(engine::layout::watch_active_app());
            // Permanent mDNS advertisement (protocol/README.md §1). "auto" until F4 adds manual
            // mode switching on the host side.
            net::discovery::advertise("auto");
            let (obs_tx, obs_rx) = tokio::sync::mpsc::unbounded_channel();
            integrations::obs::set_sender(obs_tx);
            tauri::async_runtime::spawn(integrations::obs::obs_client_loop(obs_rx));

            // Launch with Windows. Avoid rewriting the Run key on every launch: besides being
            // unnecessary, repeated unsigned registry writes look exactly like a configuration
            // modifier to endpoint protection. Still repair a missing entry, and surface failures
            // instead of silently pretending startup was configured.
            //
            // Debug builds must NOT register themselves: the exe under target/debug loads
            // `devUrl`, so at boot it opens a "connection refused" window because Vite is absent.
            if !cfg!(debug_assertions) {
                match app.autolaunch().is_enabled() {
                    Ok(true) => {}
                    Ok(false) => {
                        if let Err(error) = app.autolaunch().enable() {
                            eprintln!("KiBoard: failed to enable Windows autostart: {error}");
                        }
                    }
                    Err(error) => {
                        eprintln!("KiBoard: failed to inspect Windows autostart: {error}");
                    }
                }
            }

            // Checks for updates on GitHub at launch (silent if there's none/the connection fails).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(updater) = handle.updater() {
                    if let Ok(Some(update)) = updater.check().await {
                        if update.download_and_install(|_, _| {}, || {}).await.is_ok() {
                            handle.restart();
                        }
                    }
                }
            });

            let pair = MenuItem::with_id(
                app,
                "pair",
                crate::i18n::ui("tray.open"),
                true,
                None::<&str>,
            )?;
            let unpair = MenuItem::with_id(
                app,
                "unpair",
                crate::i18n::ui("tray.unpair"),
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(
                app,
                "quit",
                crate::i18n::ui("tray.quit"),
                true,
                None::<&str>,
            )?;
            let menu = Menu::with_items(app, &[&pair, &unpair, &quit])?;
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip(HOST_NAME)
                .menu(&menu)
                // Primary click -> opens the window (QR) directly; the menu is secondary-click only.
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(w) = tray.app_handle().get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "pair" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "unpair" => {
                        let _ = unpair_all();
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            if let Some(w) = app.get_webview_window("main") {
                let _ = w.hide();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::safe_file_name;

    /// `export_deck` turns a deck id into a path, and a deck id can arrive from a file somebody
    /// else wrote. Everything below would otherwise escape the decks folder or name a device.
    #[test]
    fn a_deck_id_cannot_escape_the_decks_folder() {
        assert_eq!(safe_file_name("launcher"), "launcher");
        assert_eq!(safe_file_name("my-deck_2"), "my-deck_2");
        assert!(!safe_file_name("../../evil").contains('.'));
        assert!(!safe_file_name("..\\..\\evil").contains('\\'));
        assert!(!safe_file_name("C:/Windows/System32/x").contains(':'));
        assert!(!safe_file_name("con.txt").contains('.'));
        // An id made entirely of punctuation would otherwise produce an empty file name.
        assert_eq!(safe_file_name("///"), "deck");
        assert_eq!(safe_file_name(""), "deck");
    }
}
