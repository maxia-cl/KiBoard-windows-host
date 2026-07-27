//! WebSocket server: sessions, framing hardening, message routing. Protocol v2: see
//! KiBoard-protocol/protocol/README.md. Discovery and pairing (§1-2) are real as of F1; as of F2
//! manual mode speaks the positional Deck/Page/Key model (§3, §4.2). Auto mode's `layout` still
//! carries v1's Profile/Button shape — F3 is what moves it over.
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

use crate::config::{config, KeyKind, Profile};
use crate::engine::actions::run_action;
use crate::engine::deck::{self, Grid};
use crate::platform;

pub const WS_PORT: u16 = 8770;

static TX: OnceLock<broadcast::Sender<String>> = OnceLock::new();
fn tx() -> &'static broadcast::Sender<String> {
    TX.get_or_init(|| broadcast::channel(16).0)
}
static CURRENT_LAYOUT: OnceLock<Mutex<String>> = OnceLock::new();
fn current_layout() -> &'static Mutex<String> {
    CURRENT_LAYOUT.get_or_init(|| Mutex::new(String::new()))
}

/// Authenticated mobile clients right now (for the host UI's badge).
pub static CLIENTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Pushes a freshly resolved layout to every connected (authenticated) client, and remembers it
/// so a client that connects afterwards gets it immediately (see `handle_conn`).
pub fn publish_layout(layout: String) {
    *current_layout().lock().unwrap() = layout.clone();
    let _ = tx().send(layout);
}

pub async fn run_ws_server() {
    let listener = match TcpListener::bind(("0.0.0.0", WS_PORT)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("KiBoard: no se pudo abrir el puerto {WS_PORT}: {e}");
            return;
        }
    };
    eprintln!("KiBoard: escuchando en ws://0.0.0.0:{WS_PORT}");
    while let Ok((stream, _addr)) = listener.accept().await {
        tokio::spawn(handle_conn(stream));
    }
}

async fn handle_conn(stream: TcpStream) {
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
    let mut session = Session::default();
    let mut failed_auth = 0u32; // token/code brute-force brake
    let mut keepalive = tokio::time::interval(Duration::from_secs(15));

    loop {
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
                if write.send(Message::text(reply)).await.is_err() { break; }
                if !was_authed && session.authed {
                    CLIENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let cur = current_layout().lock().unwrap().clone();
                    if !cur.is_empty() && write.send(Message::text(cur)).await.is_err() { break; }
                }
            }
            pushed = rx.recv() => {
                if let Ok(layout) = pushed {
                    // A client in manual mode is looking at its own deck; auto mode's broadcast
                    // would yank the layout out from under it on the next foreground-app change.
                    if session.authed && !session.manual
                        && write.send(Message::text(layout)).await.is_err() { break; }
                }
            }
        }
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
    pub(crate) manual: bool,
    pub(crate) deck_id: String,
    pub(crate) page_id: String,
    pub(crate) page: usize,
}

/// Builds the `layout` for whatever deck/page this session is on, snapping the session onto the
/// first deck/page when it is pointing at something that no longer exists (deck deleted mid-use).
fn manual_layout(s: &mut Session) -> Option<String> {
    let cfg = config().lock().unwrap();
    let deck = cfg.decks.iter().find(|d| d.id == s.deck_id).or_else(|| cfg.decks.first())?;
    let page = deck.page(&s.page_id).or_else(|| deck.pages.first())?;
    s.deck_id = deck.id.clone();
    s.page_id = page.id.clone();
    Some(deck::layout_json(deck, page, s.grid, s.page))
}

/// What a press at (page, pos) means. Resolved against the host's OWN config — §4.2: the phone
/// sends a position, never an action, so an authenticated client cannot ask for something that was
/// not on the layout it was given.
enum Press {
    /// Run this action string.
    Run(String),
    /// Navigate to this page of the current deck.
    Go(String),
}

fn resolve_press(s: &Session, page: usize, pos: usize, kind: &str) -> Result<Press, &'static str> {
    let cfg = config().lock().unwrap();
    let deck = cfg
        .decks
        .iter()
        .find(|d| d.id == s.deck_id)
        .or_else(|| cfg.decks.first())
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
        KeyKind::Action => key.action_for(kind).map(|a| Press::Run(a.to_string())).ok_or("no_such_key"),
        KeyKind::Empty => Err("no_such_key"),
    }
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
        Some("click") => match val["button"].as_str() {
            Some("right") => Ok("click:right".into()),
            _ => Ok("click:left".into()),
        },
        Some("vol") => Ok(format!("vol:{}", num("level").clamp(0, 100))),
        // Dictation. The text is typed verbatim; `type:` never interprets it as a hotkey.
        Some("text") => match val["text"].as_str() {
            Some(t) if !t.is_empty() => Ok(format!("type:{t}")),
            _ => Err("unknown_action"),
        },
        _ => Err("unknown_action"),
    }
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
                    // Client's language: catalogue labels are served translated. The 500ms poll
                    // re-broadcasts the layout on its own (the JSON changes when the locale changes).
                    crate::i18n::set_locale(val["locale"].as_str().unwrap_or("es"));
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
                Ok(Press::Run(action)) => match run_action(&action) {
                    Ok(()) => json!({"v":2,"type":"key_result","id":id,"ok":true}).to_string(),
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
                let cur = current_layout().lock().unwrap().clone();
                if cur.is_empty() { json!({"v":2,"type":"command_result","ok":true}).to_string() } else { cur }
            }
        }
        Some("set_page") => {
            if !s.authed {
                return json!({"v":2,"type":"command_result","ok":false,"error":"not_paired"}).to_string();
            }
            s.page = val["page"].as_u64().unwrap_or(0) as usize;
            manual_layout(s)
                .unwrap_or_else(|| json!({"v":2,"type":"command_result","ok":false,"error":"no_such_key"}).to_string())
        }
        Some("list_windows") => {
            if !s.authed {
                return json!({"v":2,"type":"windows","items":[]}).to_string();
            }
            platform::list_windows_json()
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
