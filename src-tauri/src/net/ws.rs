//! WebSocket server: sessions, framing hardening, message routing. Protocol v2: see
//! KiBoard-protocol/protocol/README.md. Discovery and pairing (§1-2) are real as of F1; as of F2
//! manual mode speaks the positional Deck/Page/Key model (§3, §4.2). Auto mode's `layout` still
//! carries v1's Profile/Button shape — F3 is what moves it over.
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

use crate::config::{config, KeyKind, Profile};
use crate::engine::actions::run_action;
use crate::engine::deck::{self, Grid};
use crate::engine::layout::{auto_layout_json, AutoLayout};
use crate::platform;

pub const WS_PORT: u16 = 8770;

static TX: OnceLock<broadcast::Sender<AutoLayout>> = OnceLock::new();
fn tx() -> &'static broadcast::Sender<AutoLayout> {
    TX.get_or_init(|| broadcast::channel(16).0)
}
static CURRENT_LAYOUT: OnceLock<Mutex<Option<AutoLayout>>> = OnceLock::new();
fn current_layout() -> &'static Mutex<Option<AutoLayout>> {
    CURRENT_LAYOUT.get_or_init(|| Mutex::new(None))
}

/// The auto-mode layout rendered for THIS session's grid and page, or `None` before the poll has
/// resolved one.
fn auto_for(s: &Session) -> Option<String> {
    let cur = current_layout().lock().unwrap();
    cur.as_ref().map(|a| auto_layout_json(a, s.grid, s.page, s.lang))
}

/// What is in the foreground on the PC right now, for a MANUAL session (§4.1).
///
/// The 500 ms watcher keeps `current_layout` fresh whatever mode a phone is in, so this costs
/// nothing to answer — auto mode was simply the only thing that ever asked. A manual deck follows
/// nothing, which is exactly why the phone needs telling what it is pressing keys at.
fn foreground_app() -> Option<(String, String)> {
    let cur = current_layout().lock().unwrap();
    cur.as_ref().map(|a| (a.app_name.clone(), a.app_icon.clone()))
}

/// Authenticated mobile clients right now (for the host UI's badge).
pub static CLIENTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Pushes a freshly resolved layout to every connected (authenticated) client, and remembers it
/// so a client that connects afterwards gets it immediately (see `handle_conn`).
pub fn publish_layout(layout: AutoLayout) {
    *current_layout().lock().unwrap() = Some(layout.clone());
    let _ = tx().send(layout);
}

/// "The decks changed" — a SEPARATE channel from the auto-mode one, because the two audiences are
/// exactly opposite: auto's broadcast is ignored by manual sessions, and this one is ignored by
/// auto sessions. Carries no payload; each session re-renders its OWN deck, page and grid.
static DECKS_TX: OnceLock<broadcast::Sender<()>> = OnceLock::new();
fn decks_tx() -> &'static broadcast::Sender<()> {
    DECKS_TX.get_or_init(|| broadcast::channel(4).0)
}

/// Re-sends its layout to every phone sitting in manual mode. Called after the editor saves, so
/// the deck on the desk and the deck in the hand cannot disagree about what a key does.
pub fn push_manual_layouts() {
    let _ = decks_tx().send(());
}

/// Every app id a manual deck can currently show, for the watcher's running-apps fingerprint.
///
/// Empty when no phone is connected: enumerating windows twice a second to answer a question
/// nobody is asking is the kind of poll that shows up in a battery report. The guard is "any
/// client" rather than "any manual client" because sessions are per-connection state, and a
/// counter for the finer question would have to be right on every disconnect path.
pub fn manual_app_ids() -> Vec<String> {
    if CLIENTS.load(std::sync::atomic::Ordering::Relaxed) == 0 {
        return Vec::new();
    }
    with_decks(|decks| {
        let mut ids: Vec<String> = decks
            .iter()
            .flat_map(|d| d.pages.iter())
            .flat_map(|p| p.keys.iter())
            .filter_map(|k| {
                let a = k.action.as_deref()?;
                ["launch:", "focus:", "kill:"]
                    .iter()
                    .find_map(|v| a.strip_prefix(v))
                    .map(|id| id.trim().to_string())
            })
            .collect();
        ids.sort();
        ids.dedup();
        ids
    })
}

pub async fn run_ws_server() {
    let listener = match TcpListener::bind(("0.0.0.0", WS_PORT)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("KiBoard: could not open port {WS_PORT}: {e}");
            return;
        }
    };
    // Without a certificate there is nothing to serve. Falling back to plaintext would be worse
    // than not listening: every client would keep working, and the token would keep crossing the
    // LAN in the clear with nobody told (§2.2).
    let Some(acceptor) = crate::net::tls::acceptor() else {
        eprintln!("KiBoard: refusing to serve without TLS");
        return;
    };
    eprintln!("KiBoard: listening on wss://0.0.0.0:{WS_PORT}");
    while let Ok((stream, _addr)) = listener.accept().await {
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            // A handshake that fails is a port scan, a stale plaintext client, or a browser — none
            // of them worth a line in the log.
            if let Ok(tls) = acceptor.accept(stream).await {
                handle_conn(tls).await;
            }
        });
    }
}

async fn handle_conn<S>(stream: S)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    // Frame/message size cap: a legitimate client sends a few KB of JSON (the largest profile,
    // when importing one). 64 KB cuts off an attacker trying to exhaust memory with giant frames.
    let cfg = tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
        max_message_size: Some(64 * 1024),
        max_frame_size: Some(64 * 1024),
        ..Default::default()
    };
    let ws = match tokio_tungstenite::accept_async_with_config(stream, Some(cfg)).await {
        Ok(w) => w,
        Err(_) => return,
    };
    let (mut write, mut read) = ws.split();
    let mut rx = tx().subscribe();
    let mut decks_rx = decks_tx().subscribe();
    let mut session = Session::default();
    let mut failed_auth = 0u32; // token/code brute-force brake
    let mut keepalive = tokio::time::interval(Duration::from_secs(15));

    // Labelled because the preload sends below live in `for` loops, and a dead socket there has to
    // end the connection, not just the loop it is in.
    'conn: loop {
        tokio::select! {
            _ = keepalive.tick() => {
                // Text ping (the phone ignores type=ping); keeps the connection alive.
                if session.authed && write.send(Message::text("{\"v\":2,\"type\":\"ping\"}")).await.is_err() {
                    break;
                }
            }
            incoming = read.next() => {
                let Some(Ok(msg)) = incoming else { break };
                if msg.is_close() { break; }
                if !msg.is_text() { continue; }
                let txt = msg.into_text().map(|t| t.to_string()).unwrap_or_default();
                let was_authed = session.authed;
                let reply = handle_message(&txt, &mut session);
                // Brake: every failed hello/pair_confirm attempt costs 500ms and cuts off at 5,
                // bounding the brute-force rate without affecting a legitimate client.
                if !session.authed && (txt.contains("\"hello\"") || txt.contains("\"pair_confirm\"")) {
                    failed_auth += 1;
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if failed_auth >= 5 {
                        let _ = write.send(Message::text(reply)).await;
                        break;
                    }
                }
                // `set_page`, `set_mode` and `set_grid` are all answered with a layout, and each of
                // them lands the session somewhere with different neighbours.
                let answered_with_layout = reply.contains("\"type\":\"layout\"");
                if write.send(Message::text(reply)).await.is_err() { break; }
                if answered_with_layout {
                    for extra in preloads_for(&session) {
                        if write.send(Message::text(extra)).await.is_err() { break 'conn; }
                    }
                }
                if !was_authed && session.authed {
                    CLIENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    // Rendered for the grid this client just declared in `hello`.
                    if let Some(cur) = auto_for(&session) {
                        if write.send(Message::text(cur)).await.is_err() { break; }
                        for extra in preloads_for(&session) {
                            if write.send(Message::text(extra)).await.is_err() { break 'conn; }
                        }
                    }
                }
            }
            pushed = rx.recv() => {
                if let Ok(layout) = pushed {
                    // A client in manual mode is looking at its own deck; auto mode's broadcast
                    // would yank the layout out from under it on the next foreground-app change.
                    if session.authed && !session.manual {
                        let msg = auto_layout_json(&layout, session.grid, session.page, session.lang);
                        if write.send(Message::text(msg)).await.is_err() { break; }
                        for extra in preloads_for(&session) {
                            if write.send(Message::text(extra)).await.is_err() { break 'conn; }
                        }
                    }
                }
            }
            // The editor saved. The mirror image of the arm above: only manual sessions care, and
            // each renders its own deck/page rather than being handed one.
            saved = decks_rx.recv() => {
                if saved.is_ok() && session.authed && session.manual {
                    if let Some(msg) = manual_layout(&mut session) {
                        if write.send(Message::text(msg)).await.is_err() { break; }
                        for extra in preloads_for(&session) {
                            if write.send(Message::text(extra)).await.is_err() { break 'conn; }
                        }
                    }
                }
            }
        }
    }
    // A phone that vanished mid-drag (Wi-Fi drop, app killed, screen off) never sends its release.
    // Without this the left button stays down and the desktop is unusable.
    if session.dragging {
        eprintln!("KiBoard: client left mid-drag — releasing the left button");
        let _ = run_action("release:left");
    }
    if session.authed {
        CLIENTS.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Per-connection state. The grid and the page a client is looking at are its own — two phones
/// with different screens can be attached to the same host at once, so none of this can be global.
///
/// `mode` is per-session too for now. It becomes host-wide in F4, when the mDNS TXT record and the
/// host UI both have to agree on it.
#[derive(Default)]
pub(crate) struct Session {
    pub(crate) authed: bool,
    /// Defaults to the reference 5×3 until `hello` says otherwise.
    pub(crate) grid: Grid,
    /// The language THIS phone asked for in `hello` (§2). Per session on purpose.
    pub(crate) lang: crate::i18n::Lang,
    pub(crate) manual: bool,
    pub(crate) deck_id: String,
    pub(crate) page_id: String,
    pub(crate) page: usize,
    /// The trackpad has the left button latched down (`input` kind `drag`). Tracked so the
    /// connection can release it if this client disappears: a held button outlives the phone that
    /// pressed it, and a desktop stuck mid-drag is unusable until something lets go.
    pub(crate) dragging: bool,
}

/// Decks the editor is showing but has NOT saved (F5 live preview). While this is set it stands in
/// for `config.decks` everywhere a phone is concerned.
static PREVIEW: OnceLock<Mutex<Option<Vec<crate::config::Deck>>>> = OnceLock::new();
fn preview() -> &'static Mutex<Option<Vec<crate::config::Deck>>> {
    PREVIEW.get_or_init(|| Mutex::new(None))
}

/// Shows unsaved decks on every phone in manual mode. The editor calls this as you drag, so the
/// device on the desk and the one in your hand agree before anything is written to disk.
pub fn set_preview(decks: Vec<crate::config::Deck>) {
    *preview().lock().unwrap() = Some(decks);
    push_manual_layouts();
}

/// Back to what is on disk — the editor saved, closed, or left manual mode. Without this a phone
/// keeps showing a deck that was never saved and no longer exists anywhere.
pub fn clear_preview() {
    let had = preview().lock().unwrap().take().is_some();
    if had {
        push_manual_layouts();
    }
}

/// Runs `f` against the decks a phone should currently see: the editor's unsaved preview when
/// there is one, otherwise the saved config.
///
/// EVERY phone-facing read goes through here, rendering and press resolution alike. If the two
/// disagreed, §4.2's guarantee would break: the phone would be shown one layout and its press
/// resolved against another, which is exactly the class of bug positional keys exist to prevent.
fn with_decks<T>(f: impl FnOnce(&[crate::config::Deck]) -> T) -> T {
    let preview = preview().lock().unwrap();
    match preview.as_ref() {
        Some(decks) => f(decks),
        None => f(&config().lock().unwrap().decks),
    }
}

/// §4.1 `page_preload`: the pages either side of the one this session is on, so a swipe on the
/// phone has something to draw instead of an empty gap.
///
/// Rendered for THIS session's grid and page and sent unasked. One frame each — §4 caps a frame at
/// 64 KB and a page may hold 48 KB of images on its own — and nothing at all when there is only one
/// page, which is most profiles.
fn preloads_for(s: &Session) -> Vec<String> {
    let here = s.page as isize;
    if s.manual {
        with_decks(|decks| {
            let Some(deck) = decks.iter().find(|d| d.id == s.deck_id).or_else(|| decks.first())
            else {
                return Vec::new();
            };
            let Some(page) = deck.page(&s.page_id).or_else(|| deck.pages.first()) else {
                return Vec::new();
            };
            let app = foreground_app();
            let app = app.as_ref().map(|(n, i)| (n.as_str(), i.as_str()));
            [here - 1, here + 1]
                .into_iter()
                .filter_map(|at| deck::preload_json(deck, page, s.grid, at, s.lang, app))
                .collect()
        })
    } else {
        let cur = current_layout().lock().unwrap();
        let Some(a) = cur.as_ref() else { return Vec::new() };
        [here - 1, here + 1]
            .into_iter()
            .filter_map(|at| crate::engine::layout::auto_preload_json(a, s.grid, at, s.lang))
            .collect()
    }
}

/// Builds the `layout` for whatever deck/page this session is on, snapping the session onto the
/// first deck/page when it is pointing at something that no longer exists (deck deleted mid-use).
fn manual_layout(s: &mut Session) -> Option<String> {
    let (deck_id, page_id, json) = with_decks(|decks| {
        let deck = decks.iter().find(|d| d.id == s.deck_id).or_else(|| decks.first())?;
        let page = deck.page(&s.page_id).or_else(|| deck.pages.first())?;
        let app = foreground_app();
        let app = app.as_ref().map(|(n, i)| (n.as_str(), i.as_str()));
        Some((deck.id.clone(), page.id.clone(), deck::layout_json(deck, page, s.grid, s.page, s.lang, app)))
    })?;
    s.deck_id = deck_id;
    s.page_id = page_id;
    Some(json)
}

/// What a press at (page, pos) means. Resolved against the host's OWN config — §4.2: the phone
/// sends a position, never an action, so an authenticated client cannot ask for something that was
/// not on the layout it was given.
enum Press {
    /// Run this action string.
    Run(String),
    /// Navigate to this page of the current deck.
    Go(String),
    /// A two-state key: run the action of the face that was showing, then flip the face at this
    /// address (protocol §3). Separate from `Run` because the flip has to happen after the action
    /// succeeded, and every phone on the deck has to be told to repaint.
    Toggle(String, String),
}

fn resolve_press(s: &Session, page: usize, pos: usize, kind: &str) -> Result<Press, &'static str> {
    // Auto mode resolves against the profile the poll last published; manual mode against the
    // deck. Same addressing either way, which is why a press looks identical on the wire.
    if !s.manual {
        let cur = current_layout().lock().unwrap();
        let auto = cur.as_ref().ok_or("no_such_key")?;
        let pg = auto.as_page();
        let key = deck::resolve(&pg, s.grid, page, pos).ok_or("no_such_key")?;
        return key.action_for(kind).map(|a| Press::Run(a.to_string())).ok_or("no_such_key");
    }
    // Resolved against the SAME decks the phone was shown — preview included, see `with_decks`.
    with_decks(|decks| {
    let deck = decks
        .iter()
        .find(|d| d.id == s.deck_id)
        .or_else(|| decks.first())
        .ok_or("no_such_key")?;
    let pg = deck.page(&s.page_id).or_else(|| deck.pages.first()).ok_or("no_such_key")?;
    let key = deck::resolve(pg, s.grid, page, pos).ok_or("no_such_key")?;
    match key.kind {
        KeyKind::Folder | KeyKind::Page => {
            let target = key.target.as_deref().ok_or("no_such_key")?;
            // A key pointing at a page that was deleted must fail, not silently do nothing else.
            deck.page(target).ok_or("no_such_key")?;
            Ok(Press::Go(target.to_string()))
        }
        // An unbound long/double press is not an error the user should see as a red key: it is
        // simply a key that does not take that gesture. `no_such_key` is the closest code.
        // A two-state key flips on the SHORT press only: `hold` and `double` are separate
        // bindings, not the toggle gesture (protocol §3).
        KeyKind::Action if kind == "short" && key.toggle.is_some() => {
            let at = deck::flat(s.grid, page, pos).ok_or("no_such_key")?;
            let at = deck::addr(&deck.id, &pg.id, at);
            // Reality wins over memory where reality is knowable.
            let live = deck::live_face(key);
            let on = live.unwrap_or_else(|| deck::is_on(&at));
            let action = deck::showing_action(key, on).ok_or("no_such_key")?;
            match live {
                // OBS owns this key's state: run the action and let the 500 ms poll repaint when
                // OBS says so. Remembering a flip here would put our memory against the truth —
                // and the truth can change without anyone pressing anything.
                Some(_) => Ok(Press::Run(action.to_string())),
                None => Ok(Press::Toggle(action.to_string(), at)),
            }
        }
        KeyKind::Action => key.action_for(kind).map(|a| Press::Run(a.to_string())).ok_or("no_such_key"),
        KeyKind::Empty => Err("no_such_key"),
    }
    })
}

/// Continuous input (§4.2 exception): trackpad, volume slider, dictation. These are NOT keys and
/// have no position, so they cannot go through `resolve_press`. The vocabulary is CLOSED — each
/// kind maps to one fixed action shape, so an authenticated client still cannot inject a hotkey
/// through this channel.
fn input_action(val: &serde_json::Value) -> Result<String, &'static str> {
    let num = |k: &str| val[k].as_i64().unwrap_or(0).clamp(-4096, 4096);
    match val["kind"].as_str() {
        Some("mouse") => Ok(format!("mouse:{},{}", num("dx"), num("dy"))),
        Some("scroll") => Ok(format!("scroll:{}", num("n"))),
        Some("hscroll") => Ok(format!("hscroll:{}", num("n"))),
        Some("zoom") => Ok(format!("zoom:{}", num("n"))),
        Some("click") => match val["button"].as_str() {
            Some("right") => Ok("click:right".into()),
            _ => Ok("click:left".into()),
        },
        // Latches the left button so a finger can drag. The RELEASE is also tracked on the session
        // (see `dragging`), because a phone that walks out of Wi-Fi range mid-drag would otherwise
        // leave the button held down and the desktop unusable.
        Some("drag") => Ok(if val["down"].as_bool().unwrap_or(false) {
            "hold:left".into()
        } else {
            "release:left".into()
        }),
        Some("vol") => Ok(format!("vol:{}", num("level").clamp(0, 100))),
        // Dictation. The text is typed verbatim; `type:` never interprets it as a hotkey.
        Some("text") => match val["text"].as_str() {
            Some(t) if !t.is_empty() => Ok(format!("type:{t}")),
            _ => Err("unknown_action"),
        },
        _ => Err("unknown_action"),
    }
}

/// Actions that move THIS session rather than driving the desktop (`deck:`, `mode:`, `windows`),
/// plus the ones that are purely a screen on the phone (§4.2.1).
fn is_session_action(action: &str) -> bool {
    action == "windows"
        || is_client_action(action)
        || action.starts_with("deck:")
        || action.starts_with("mode:")
}

/// Screens the PHONE opens (§4.2.1). The host has no work to do — what arrives afterwards is
/// `input` — but it answers ok so a client that does send the press is not shown a false error.
fn is_client_action(action: &str) -> bool {
    action == "trackpad" || action == "dictate"
}

fn run_session_action(action: &str, s: &mut Session, id: &str) -> String {
    let fail = |e: &str| json!({"v":2,"type":"key_result","id":id,"ok":false,"error":e}).to_string();
    if is_client_action(action) {
        return json!({"v":2,"type":"key_result","id":id,"ok":true}).to_string();
    }
    if let Some(deck_id) = action.strip_prefix("deck:") {
        if !config().lock().unwrap().decks.iter().any(|d| d.id == deck_id) {
            return fail("no_such_key");
        }
        s.deck_id = deck_id.to_string();
        s.page_id = String::new(); // entry page
        s.page = 0;
        s.manual = true;
        return manual_layout(s).unwrap_or_else(|| fail("no_such_key"));
    }
    if let Some(mode) = action.strip_prefix("mode:") {
        s.manual = mode == "manual";
        s.page = 0;
        return if s.manual {
            manual_layout(s).unwrap_or_else(|| fail("no_such_key"))
        } else {
            // Auto mode is owned by the 500 ms poll; hand back whatever it resolved last.
            auto_for(s).unwrap_or_else(|| json!({"v":2,"type":"key_result","id":id,"ok":true}).to_string())
        };
    }
    if action == "windows" {
        return crate::engine::windows::windows_json(s.grid, 0);
    }
    fail("unknown_action")
}

fn handle_message(txt: &str, s: &mut Session) -> String {
    let val: serde_json::Value = match serde_json::from_str(txt) {
        Ok(v) => v,
        Err(_) => return json!({"v":2,"type":"command_result","ok":false,"error":"bad_json"}).to_string(),
    };
    match val["type"].as_str() {
        // --- Pairing (protocol/README.md §2) — no auth needed yet, this IS the auth flow. ---
        Some("pair_request") => {
            let device = val["device"].as_str().unwrap_or("unknown");
            let platform = val["platform"].as_str().unwrap_or("");
            match crate::net::pairing::start(device, platform) {
                Ok((_, expires_in)) => {
                    json!({"v":2,"type":"pair_challenge","digits":6,"expiresIn":expires_in}).to_string()
                }
                Err(e) => json!({"v":2,"type":"pair_ack","ok":false,"error":e}).to_string(),
            }
        }
        Some("pair_confirm") => {
            let code = val["code"].as_str().unwrap_or("");
            match crate::net::pairing::confirm(code) {
                Ok(device) => json!({
                    "v":2,"type":"pair_ack","ok":true,
                    "token":device.token,"deviceId":device.device_id,"name":crate::HOST_NAME
                })
                .to_string(),
                Err(e) => json!({"v":2,"type":"pair_ack","ok":false,"error":e}).to_string(),
            }
        }
        Some("hello") => {
            if val["v"].as_i64() != Some(2) {
                return json!({"v":2,"type":"hello_ack","ok":false,"error":"protocol_too_old"}).to_string();
            }
            let device_id = val["deviceId"].as_str().unwrap_or("");
            let token = val["token"].as_str().unwrap_or("");
            match crate::net::pairing::authenticate(device_id, token) {
                Ok(()) => {
                    s.authed = true;
                    // 🆕 the client declares its grid; the host paginates every deck to it.
                    s.grid = Grid::new(
                        val["grid"]["rows"].as_u64().unwrap_or(3) as usize,
                        val["grid"]["cols"].as_u64().unwrap_or(5) as usize,
                    );
                    // The client's language, PER SESSION. It used to be one process-global set by
                    // whoever said `hello` last, so a second phone in another language silently
                    // re-labelled the first one's deck. Translation moved to render time — the
                    // only point that knows which phone is being answered.
                    //
                    // The host's own window does not read this at all: it follows Windows
                    // (`i18n::host_lang`), because that window belongs to the PC.
                    s.lang = crate::i18n::lang_of(val["locale"].as_str().unwrap_or("es"));
                    let decks: Vec<_> = config()
                        .lock()
                        .unwrap()
                        .decks
                        .iter()
                        .map(|d| json!({"id": d.id, "name": d.name, "icon": d.icon}))
                        .collect();
                    json!({"v":2,"type":"hello_ack","ok":true,"name":crate::HOST_NAME,
                           "mode": if s.manual { "manual" } else { "auto" }, "decks": decks})
                    .to_string()
                }
                Err(e) => json!({"v":2,"type":"hello_ack","ok":false,"error":e}).to_string(),
            }
        }
        // §4.2 — the phone sends the KEY IT PRESSED, never the action. `command` (v1, where the
        // phone sent an arbitrary action string) is gone: that was the hole this closes.
        Some("key") => {
            let id = val["id"].as_str().unwrap_or("").to_string();
            if !s.authed {
                return json!({"v":2,"type":"key_result","id":id,"ok":false,"error":"not_paired"}).to_string();
            }
            let page = val["page"].as_u64().unwrap_or(0) as usize;
            let pos = val["pos"].as_u64().unwrap_or(0) as usize;
            let press = val["press"].as_str().unwrap_or("short");
            match resolve_press(s, page, pos, press) {
                // Navigation never touches the desktop: it just moves this session and answers
                // with the layout the phone should now be drawing.
                Ok(Press::Go(target)) => {
                    s.page_id = target;
                    s.page = 0;
                    let layout = manual_layout(s).unwrap_or_default();
                    if layout.is_empty() {
                        json!({"v":2,"type":"key_result","id":id,"ok":false,"error":"no_such_key"}).to_string()
                    } else {
                        layout
                    }
                }
                // Session-scoped actions never touch the desktop: they change what THIS client is
                // looking at, so they cannot live in `run_action`, which has no session.
                Ok(Press::Run(action)) if is_session_action(&action) => run_session_action(&action, s, &id),
                Ok(Press::Run(action)) => match run_action(&action) {
                    Ok(()) => json!({"v":2,"type":"key_result","id":id,"ok":true}).to_string(),
                    Err(e) => json!({"v":2,"type":"key_result","id":id,"ok":false,"error":e}).to_string(),
                },
                // A toggle that navigates is not a toggle: run it and leave the face alone, or the
                // deck it switched away from would be left showing a state nobody flips back.
                Ok(Press::Toggle(action, _)) if is_session_action(&action) => {
                    run_session_action(&action, s, &id)
                }
                Ok(Press::Toggle(action, at)) => match run_action(&action) {
                    Ok(()) => {
                        // Flipped only after the action ran: a key whose action failed must not
                        // start claiming it did the thing.
                        deck::flip(&at);
                        // Every phone on this deck repaints, not only the one that pressed.
                        push_manual_layouts();
                        json!({"v":2,"type":"key_result","id":id,"ok":true}).to_string()
                    }
                    Err(e) => json!({"v":2,"type":"key_result","id":id,"ok":false,"error":e}).to_string(),
                },
                Err(e) => json!({"v":2,"type":"key_result","id":id,"ok":false,"error":e}).to_string(),
            }
        }
        // Continuous input: trackpad, volume, dictation. Closed vocabulary (§4.2 exception).
        Some("input") => {
            if !s.authed {
                return json!({"v":2,"type":"key_result","ok":false,"error":"not_paired"}).to_string();
            }
            if val["kind"].as_str() == Some("drag") {
                s.dragging = val["down"].as_bool().unwrap_or(false);
            }
            match input_action(&val).and_then(|a| run_action(&a)) {
                Ok(()) => json!({"v":2,"type":"key_result","ok":true}).to_string(),
                Err(e) => json!({"v":2,"type":"key_result","ok":false,"error":e}).to_string(),
            }
        }
        Some("set_mode") => {
            if !s.authed {
                return json!({"v":2,"type":"command_result","ok":false,"error":"not_paired"}).to_string();
            }
            s.manual = val["mode"].as_str() == Some("manual");
            s.page = 0;
            if let Some(deck_id) = val["deckId"].as_str() {
                s.deck_id = deck_id.to_string();
                s.page_id = String::new(); // entry page of the new deck
            }
            if s.manual {
                manual_layout(s)
                    .unwrap_or_else(|| json!({"v":2,"type":"command_result","ok":false,"error":"no_such_key"}).to_string())
            } else {
                // Auto mode: the 500 ms poll owns the layout — hand back the current one.
                auto_for(s).unwrap_or_else(|| json!({"v":2,"type":"command_result","ok":true}).to_string())
            }
        }
        // §4.4 — the client's grid changes when the phone is rotated, so it cannot be a one-shot
        // declaration in `hello`.
        Some("set_grid") => {
            if !s.authed {
                return json!({"v":2,"type":"command_result","ok":false,"error":"not_paired"}).to_string();
            }
            s.grid = Grid::new(
                val["grid"]["rows"].as_u64().unwrap_or(3) as usize,
                val["grid"]["cols"].as_u64().unwrap_or(5) as usize,
            );
            // The page index is an index into the OLD pagination; a wider grid means fewer pages,
            // so keeping it could land the client past the end.
            s.page = 0;
            let rendered = if s.manual { manual_layout(s) } else { auto_for(s) };
            rendered
                .unwrap_or_else(|| json!({"v":2,"type":"command_result","ok":true}).to_string())
        }
        Some("set_page") => {
            if !s.authed {
                return json!({"v":2,"type":"command_result","ok":false,"error":"not_paired"}).to_string();
            }
            s.page = val["page"].as_u64().unwrap_or(0) as usize;
            let rendered = if s.manual { manual_layout(s) } else { auto_for(s) };
            rendered
                .unwrap_or_else(|| json!({"v":2,"type":"command_result","ok":false,"error":"no_such_key"}).to_string())
        }
        Some("list_windows") => {
            if !s.authed {
                return json!({"v":2,"type":"command_result","ok":false,"error":"not_paired"}).to_string();
            }
            crate::engine::windows::windows_json(s.grid, val["page"].as_u64().unwrap_or(0) as usize)
        }
        // Profile scanned from a "kbprofile:" QR on another KiBoard: added to the local catalogue.
        Some("import_profile") => {
            if !s.authed {
                return json!({"v":2,"type":"command_result","ok":false,"error":"not_paired"}).to_string();
            }
            let Ok(p) = serde_json::from_value::<Profile>(val["profile"].clone()) else {
                return json!({"v":2,"type":"command_result","ok":false,"error":"bad_profile"}).to_string();
            };
            let mut cfg = config().lock().unwrap();
            cfg.profiles.retain(|q| q.id != p.id); // re-importing the same id replaces it
            cfg.profiles.insert(0, p); // at the front: wins matching over the generic ones
            cfg.save();
            json!({"v":2,"type":"command_result","ok":true,"imported":true}).to_string()
        }
        Some("focus_window") => {
            if !s.authed {
                return json!({"v":2,"type":"command_result","ok":false,"error":"not_paired"}).to_string();
            }
            let id = val["id"].as_i64().unwrap_or(0) as isize;
            platform::focus_window(id);
            json!({"v":2,"type":"command_result","ok":true}).to_string()
        }
        _ => json!({"v":2,"type":"command_result","ok":false,"error":"unknown_type"}).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Deck, Key, KeyKind, Page};

    fn deck_saying(label: &str) -> Vec<Deck> {
        vec![Deck {
            id: "d".into(),
            name: "D".into(),
            pages: vec![Page {
                id: "p0".into(),
                keys: vec![Key {
                    pos: 0,
                    label: label.into(),
                    action: Some(format!("type:{label}")),
                    kind: KeyKind::Action,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }]
    }

    /// One deck, one page, `n` keys — enough to paginate on the default 5×3 grid once n > 15.
    fn deck_of(n: usize) -> Vec<Deck> {
        vec![Deck {
            id: "d".into(),
            name: "D".into(),
            pages: vec![Page {
                id: "p0".into(),
                keys: (0..n)
                    .map(|pos| Key {
                        pos,
                        label: format!("k{pos}"),
                        action: Some("noop".into()),
                        kind: KeyKind::Action,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        }]
    }

    fn session() -> Session {
        Session { authed: true, manual: true, deck_id: "d".into(), page_id: "p0".into(), ..Default::default() }
    }

    /// The preview is process-global, and `cargo test` runs tests in parallel. Everything that
    /// touches it takes this first, or the two tests below overwrite each other's fixture.
    static PREVIEW_LOCK: Mutex<()> = Mutex::new(());

    /// The preview must own BOTH halves. If it only replaced what is drawn, the phone would show
    /// the unsaved deck while a press resolved against the saved one — the phone pressing position
    /// 0 and the desktop running somebody else's action. That is precisely what §4.2's positional
    /// keys exist to make impossible, so it is worth a test rather than a comment.
    #[test]
    fn a_preview_owns_both_the_layout_and_the_press() {
        let _guard = PREVIEW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_preview();
        set_preview(deck_saying("PREVIEW"));

        let mut s = session();
        let layout = manual_layout(&mut s).expect("a layout");
        assert!(layout.contains("PREVIEW"), "the preview is what gets drawn");

        match resolve_press(&s, 0, 0, "short") {
            Ok(Press::Run(action)) => assert_eq!(action, "type:PREVIEW", "and what gets run"),
            other => panic!("expected the preview's action, got {:?}", other.is_ok()),
        }

        // Dropping it falls back to the saved config, whatever that happens to be.
        clear_preview();
        let mut s2 = session();
        let after = manual_layout(&mut s2);
        assert!(
            after.map(|l| !l.contains("PREVIEW")).unwrap_or(true),
            "the preview must not survive being cleared"
        );
    }

    /// §4.1 `page_preload`. What this guards is the SELECTION — one either side, nothing outside
    /// the deck, and nothing at all when there is only one page. Sending a page that does not
    /// exist would have the phone draw a copy of the page it is already on coming in behind the
    /// swipe; sending them for a single-page profile would triple every push for nothing.
    #[test]
    fn preloads_are_the_two_neighbours_and_never_more() {
        let _guard = PREVIEW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_preview();
        set_preview(deck_of(20)); // 2 pages on the default 5x3 grid

        let mut s = session();
        let first = preloads_for(&s);
        assert_eq!(first.len(), 1, "page 0 has nothing before it");
        assert!(first[0].contains("\"type\":\"page_preload\""));
        assert!(first[0].contains("\"page\":1"));

        s.page = 1;
        let last = preloads_for(&s);
        assert_eq!(last.len(), 1, "the last page has nothing after it");
        assert!(last[0].contains("\"page\":0"));

        // A profile that fits on one page — most of them — costs nothing.
        set_preview(deck_saying("ONE"));
        assert!(preloads_for(&session()).is_empty());

        clear_preview();
    }
}
