//! WebSocket server: sessions, framing hardening, message routing. Protocol: see
//! KiBoard-protocol/protocol/README.md (v1 today — F2 freezes v2).
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

use crate::config::{config, Profile};
use crate::engine::actions::run_action;
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
    let mut cfg = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
    cfg.max_message_size = Some(64 * 1024);
    cfg.max_frame_size = Some(64 * 1024);
    let ws = match tokio_tungstenite::accept_async_with_config(stream, Some(cfg)).await {
        Ok(w) => w,
        Err(_) => return,
    };
    let (mut write, mut read) = ws.split();
    let mut rx = tx().subscribe();
    let mut authed = false;
    let mut failed_auth = 0u32; // token brute-force brake
    let mut keepalive = tokio::time::interval(Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = keepalive.tick() => {
                // Text ping (the phone ignores type=ping); keeps the connection alive.
                if authed && write.send(Message::text("{\"v\":1,\"type\":\"ping\"}")).await.is_err() {
                    break;
                }
            }
            incoming = read.next() => {
                let Some(Ok(msg)) = incoming else { break };
                if msg.is_close() { break; }
                if !msg.is_text() { continue; }
                let txt = msg.into_text().map(|t| t.to_string()).unwrap_or_default();
                let was_authed = authed;
                let reply = handle_message(&txt, &mut authed);
                // Token brake: every failed "hello" attempt costs 500ms and cuts off at 5,
                // bounding the brute-force rate without affecting a legitimate client.
                if !authed && txt.contains("\"hello\"") {
                    failed_auth += 1;
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if failed_auth >= 5 {
                        let _ = write.send(Message::text(reply)).await;
                        break;
                    }
                }
                if write.send(Message::text(reply)).await.is_err() { break; }
                if !was_authed && authed {
                    CLIENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let cur = current_layout().lock().unwrap().clone();
                    if !cur.is_empty() && write.send(Message::text(cur)).await.is_err() { break; }
                }
            }
            pushed = rx.recv() => {
                if let Ok(layout) = pushed {
                    if authed && write.send(Message::text(layout)).await.is_err() { break; }
                }
            }
        }
    }
    if authed {
        CLIENTS.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn handle_message(txt: &str, authed: &mut bool) -> String {
    let val: serde_json::Value = match serde_json::from_str(txt) {
        Ok(v) => v,
        Err(_) => return json!({"v":1,"type":"command_result","ok":false,"error":"bad_json"}).to_string(),
    };
    match val["type"].as_str() {
        Some("hello") => {
            let token = val["token"].as_str().unwrap_or("");
            let device = val["device"].as_str().unwrap_or("desconocido").to_string();
            if crate::net::pairing::authenticate(token, &device) {
                *authed = true;
                // Client's language: catalogue labels are served translated. The 500ms poll
                // re-broadcasts the layout on its own (the JSON changes when the locale changes).
                crate::i18n::set_locale(val["locale"].as_str().unwrap_or("es"));
                json!({"v":1,"type":"hello_ack","ok":true,"name":crate::HOST_NAME}).to_string()
            } else {
                json!({"v":1,"type":"hello_ack","ok":false,"error":"invalid_token"}).to_string()
            }
        }
        Some("command") => {
            let id = val["id"].as_str().unwrap_or("").to_string();
            if !*authed {
                return json!({"v":1,"type":"command_result","id":id,"ok":false,"error":"not_paired"}).to_string();
            }
            let action = val["action"].as_str().unwrap_or("");
            match run_action(action) {
                Ok(()) => json!({"v":1,"type":"command_result","id":id,"ok":true}).to_string(),
                Err(e) => json!({"v":1,"type":"command_result","id":id,"ok":false,"error":e}).to_string(),
            }
        }
        Some("list_windows") => {
            if !*authed {
                return json!({"v":1,"type":"windows","items":[]}).to_string();
            }
            platform::list_windows_json()
        }
        // Profile scanned from a "kbprofile:" QR on another KiBoard: added to the local catalogue.
        Some("import_profile") => {
            if !*authed {
                return json!({"v":1,"type":"command_result","ok":false,"error":"not_paired"}).to_string();
            }
            let Ok(p) = serde_json::from_value::<Profile>(val["profile"].clone()) else {
                return json!({"v":1,"type":"command_result","ok":false,"error":"bad_profile"}).to_string();
            };
            let mut cfg = config().lock().unwrap();
            cfg.profiles.retain(|q| q.id != p.id); // re-importing the same id replaces it
            cfg.profiles.insert(0, p); // at the front: wins matching over the generic ones
            cfg.save();
            json!({"v":1,"type":"command_result","ok":true,"imported":true}).to_string()
        }
        Some("focus_window") => {
            if !*authed {
                return json!({"v":1,"type":"command_result","ok":false,"error":"not_paired"}).to_string();
            }
            let id = val["id"].as_i64().unwrap_or(0) as isize;
            platform::focus_window(id);
            json!({"v":1,"type":"command_result","ok":true}).to_string()
        }
        _ => json!({"v":1,"type":"command_result","ok":false,"error":"unknown_type"}).to_string(),
    }
}
