// KiBoard host — Fase 5 (perfiles programables).
// WS en LAN + emparejamiento por token/QR + comandos (atajos arbitrarios) + auto-switching
// con perfiles editables por el usuario desde la UI. Protocolo: ver /protocol/README.md

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use tauri_plugin_updater::UpdaterExt;
use tauri_plugin_autostart::ManagerExt;

const WS_PORT: u16 = 8770;
const HOST_NAME: &str = "KiBoard Host";

// ---------------------------------------------------------------------------
// Modelo de datos
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
struct Button {
    label: String,
    icon: String,
    /// Atajo ("ctrl+shift+p", "alt+F4", "ctrl+c") o la palabra clave "screenshot".
    action: String,
    /// Acción peligrosa: el móvil la pinta en rojo y pide confirmación.
    #[serde(default)]
    danger: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct Profile {
    id: String,
    /// Subcadenas que deben aparecer en el nombre de la app activa O en el título de la ventana
    /// (esto permite sub-perfiles por pestaña: "Google Sheets", "Google Drive"…). Vacío = fallback.
    #[serde(default)]
    matches: Vec<String>,
    buttons: Vec<Button>,
}

fn b(label: &str, icon: &str, action: &str) -> Button {
    Button { label: label.into(), icon: icon.into(), action: action.into(), danger: false }
}
fn bd(label: &str, icon: &str, action: &str) -> Button {
    Button { label: label.into(), icon: icon.into(), action: action.into(), danger: true }
}
fn profile(id: &str, matches: &[&str], buttons: Vec<Button>) -> Profile {
    Profile { id: id.into(), matches: matches.iter().map(|s| s.to_string()).collect(), buttons }
}

/// Catálogo de perfiles por defecto. El orden importa: los más específicos (pestañas de Chrome)
/// van primero, para que ganen sobre el perfil genérico de "browser". A todos se les añade al final
/// el botón "Cerrar app" (rojo + confirmación). Todos son editables desde la UI del host.
fn default_profiles() -> Vec<Profile> {
    let mut list = vec![
        // --- Pestañas de Chrome/navegador (coinciden por TÍTULO de la ventana) ---
        profile("gsheets", &["google sheets", "hojas de cálculo"], vec![
            b("Negrita", "undo", "ctrl+b"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"), b("Rehacer", "redo", "ctrl+y"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        profile("gdocs", &["google docs", "documentos de google"], vec![
            b("Negrita", "undo", "ctrl+b"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        profile("gdrive", &["google drive", "mi unidad"], vec![
            b("Buscar", "find", "ctrl+f"), b("Nueva pestaña", "new", "ctrl+t"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        profile("gmail", &["gmail"], vec![
            b("Redactar", "new", "c"), b("Buscar", "find", "/"),
            b("Responder", "redo", "r"), b("Archivar", "close", "e"),
        ]),
        profile("youtube", &["youtube"], vec![
            b("Play/Pausa", "play", "k"), b("Silenciar", "mute", "m"),
            b("Pantalla completa", "video", "f"), b("Captura", "screenshot", "screenshot"),
        ]),
        // --- Office ---
        profile("word", &["word"], vec![
            b("Guardar", "save", "ctrl+s"), b("Negrita", "undo", "ctrl+b"),
            b("Buscar", "find", "ctrl+f"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        profile("excel", &["excel"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"), b("Rehacer", "redo", "ctrl+y"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        profile("powerpoint", &["powerpoint"], vec![
            b("Presentar", "play", "f5"), b("Nueva diap.", "new", "ctrl+m"),
            b("Guardar", "save", "ctrl+s"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        profile("outlook", &["outlook"], vec![
            b("Nuevo correo", "new", "ctrl+n"), b("Responder", "redo", "ctrl+r"),
            b("Enviar", "play", "ctrl+enter"), b("Buscar", "find", "ctrl+e"),
        ]),
        profile("acrobat", &["acrobat"], vec![
            b("Buscar", "find", "ctrl+f"), b("Imprimir", "save", "ctrl+p"),
            b("Guardar", "save", "ctrl+s"), b("Copiar", "copy", "ctrl+c"),
        ]),
        // --- Creativas ---
        profile("photoshop", &["photoshop"], vec![
            b("Deshacer", "undo", "ctrl+z"), b("Pincel", "new", "b"), b("Mover", "tab", "v"),
            b("Guardar", "save", "ctrl+s"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        profile("illustrator", &["illustrator"], vec![
            b("Deshacer", "undo", "ctrl+z"), b("Selección", "tab", "v"),
            b("Guardar", "save", "ctrl+s"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        profile("premiere", &["premiere"], vec![
            b("Play/Pausa", "play", "k"), b("Cortar", "new", "c"), b("Selección", "tab", "v"),
            b("Guardar", "save", "ctrl+s"), b("Deshacer", "undo", "ctrl+z"),
        ]),
        profile("figma", &["figma"], vec![
            b("Mover", "tab", "v"), b("Marco", "new", "f"), b("Comentar", "redo", "c"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"), b("Deshacer", "undo", "ctrl+z"),
        ]),
        // --- Comunicación / reuniones ---
        profile("slack", &["slack"], vec![
            b("Saltar a", "find", "ctrl+k"), b("Negrita", "undo", "ctrl+b"),
            b("Hilos", "redo", "ctrl+shift+t"), b("Copiar", "copy", "ctrl+c"),
        ]),
        profile("discord", &["discord"], vec![
            b("Silenciar", "mute", "ctrl+shift+m"), b("Audio", "video", "ctrl+shift+d"),
            b("Buscar", "find", "ctrl+f"),
        ]),
        profile("teams", &["teams"], vec![
            b("Silenciar", "mic", "ctrl+shift+m"), b("Cámara", "video", "ctrl+shift+o"),
            b("Compartir", "new", "ctrl+shift+e"), b("Colgar", "close", "ctrl+shift+h"),
        ]),
        profile("zoom", &["zoom"], vec![
            b("Silenciar", "mic", "alt+a"), b("Vídeo", "video", "alt+v"),
            b("Compartir", "new", "alt+s"), b("Salir", "close", "alt+q"),
        ]),
        profile("notion", &["notion"], vec![
            b("Buscar", "find", "ctrl+p"), b("Negrita", "undo", "ctrl+b"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        // --- Multimedia ---
        profile("spotify", &["spotify"], vec![
            b("Play/Pausa", "play", "space"), b("Siguiente", "redo", "ctrl+right"),
            b("Anterior", "undo", "ctrl+left"),
        ]),
        profile("vlc", &["vlc"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "video", "f"),
            b("Silenciar", "mute", "m"), b("+ Volumen", "vol", "ctrl+up"), b("- Volumen", "vol", "ctrl+down"),
        ]),
        // --- Desarrollo / sistema ---
        profile("editor", &["code", "devenv", "visual studio"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Paleta", "new", "ctrl+shift+p"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        profile("terminal", &["powershell", "windows terminal", "símbolo del sistema", "command prompt"], vec![
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            b("Nueva pestaña", "new", "ctrl+shift+t"), b("Buscar", "find", "ctrl+f"),
        ]),
        profile("notepadpp", &["notepad++"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Reemplazar", "redo", "ctrl+h"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        profile("explorer", &["explorador", "file explorer", "explorer"], vec![
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            b("Nueva carpeta", "new", "ctrl+shift+n"), b("Renombrar", "redo", "f2"),
        ]),
        // --- Navegador genérico (cualquier pestaña no específica) ---
        profile("browser", &["chrome", "edge", "firefox", "brave"], vec![
            b("Nueva pestaña", "new", "ctrl+t"), b("Cerrar pestaña", "close", "ctrl+w"),
            b("Recargar", "refresh", "f5"), b("Buscar", "find", "ctrl+f"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
        ]),
        // --- Fallback ---
        profile("generic", &[], vec![
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            b("Captura", "screenshot", "screenshot"),
        ]),
    ];
    // "Cerrar app" (rojo + confirmación) en TODOS los perfiles.
    for p in &mut list {
        p.buttons.push(bd("Cerrar app", "close", "alt+F4"));
    }
    list
}

// ---------------------------------------------------------------------------
// Configuración persistente
// ---------------------------------------------------------------------------

/// Versión de los perfiles integrados. Subir cuando se cambian los `default_profiles`
/// para que se refresquen en hosts ya instalados (conservando token y emparejamiento).
const PROFILES_VERSION: u32 = 3;

#[derive(Serialize, Deserialize, Default)]
struct Config {
    token: String,
    #[serde(default)]
    paired: Vec<String>,
    #[serde(default)]
    profiles: Vec<Profile>,
    #[serde(default)]
    profiles_version: u32,
}

impl Config {
    fn path() -> std::path::PathBuf {
        let dir = dirs::config_dir().unwrap_or(std::env::temp_dir()).join("KiBoard");
        let _ = std::fs::create_dir_all(&dir);
        dir.join("config.json")
    }
    fn load() -> Config {
        let mut c: Config = std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if c.token.is_empty() {
            c.token = new_token();
        }
        // Refresca los perfiles integrados en instalaciones nuevas o al subir la versión.
        if c.profiles.is_empty() || c.profiles_version < PROFILES_VERSION {
            c.profiles = default_profiles();
            c.profiles_version = PROFILES_VERSION;
        }
        c.save();
        c
    }
    fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), s);
        }
    }
}

static CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();
fn config() -> &'static Mutex<Config> {
    CONFIG.get_or_init(|| Mutex::new(Config::load()))
}

static TX: OnceLock<broadcast::Sender<String>> = OnceLock::new();
fn tx() -> &'static broadcast::Sender<String> {
    TX.get_or_init(|| broadcast::channel(16).0)
}
static CURRENT_LAYOUT: OnceLock<Mutex<String>> = OnceLock::new();
fn current_layout() -> &'static Mutex<String> {
    CURRENT_LAYOUT.get_or_init(|| Mutex::new(String::new()))
}

fn new_token() -> String {
    use rand::Rng;
    let bytes: [u8; 16] = rand::thread_rng().gen();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Auto-switching
// ---------------------------------------------------------------------------

/// Construye el JSON de layout para la app dada, según los perfiles. Matchea contra el nombre de la
/// app Y el título de la ventana (esto habilita sub-perfiles por pestaña, p. ej. Google Sheets).
fn layout_for(app: &str, title: &str, icon_b64: &str) -> String {
    let cfg = config().lock().unwrap();
    let hay = format!("{app} {title}").to_lowercase();
    let profile = cfg
        .profiles
        .iter()
        .find(|p| p.matches.iter().any(|m| hay.contains(&m.to_lowercase())))
        .or_else(|| cfg.profiles.iter().find(|p| p.matches.is_empty()))
        .or_else(|| cfg.profiles.last());
    let (profile_id, buttons): (String, Vec<_>) = match profile {
        Some(p) => (
            p.id.clone(),
            p.buttons
                .iter()
                .enumerate()
                .map(|(i, btn)| json!({ "id": i, "label": btn.label, "action": btn.action, "icon": btn.icon, "danger": btn.danger }))
                .collect(),
        ),
        None => ("empty".into(), vec![]),
    };
    json!({
        "v": 1, "type": "layout", "profileId": profile_id,
        "appName": app, "appIcon": icon_b64, "buttons": buttons
    })
    .to_string()
}

/// Extrae el ícono del ejecutable como PNG en base64. Cadena vacía si no se puede.
/// ponytail: usa el ícono real del .exe en vez de empaquetar logos; cubre cualquier app.
fn extract_icon_b64(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let p = path.to_string();
    let b64 = std::panic::catch_unwind(move || windows_icons::get_icon_base64_by_path(&p))
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    // por si viniera como data URI ("data:image/png;base64,...")
    b64.rsplit(',').next().unwrap_or("").to_string()
}

/// Sondea la app en primer plano y publica el layout cuando cambia (de app o por edición de perfiles).
async fn watch_active_app() {
    let mut last_app = String::new();
    let mut icon = String::new();
    let mut last_layout = String::new();
    loop {
        let (app, title, path) = match active_win_pos_rs::get_active_window() {
            Ok(w) => (w.app_name, w.title, w.process_path.to_string_lossy().to_string()),
            Err(_) => (String::new(), String::new(), String::new()),
        };
        if app != last_app {
            last_app = app.clone();
            icon = extract_icon_b64(&path); // solo al cambiar de app
        }
        let layout = layout_for(&app, &title, &icon);
        if layout != last_layout {
            last_layout = layout.clone();
            *current_layout().lock().unwrap() = layout.clone();
            let _ = tx().send(layout);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

// ---------------------------------------------------------------------------
// Servidor WebSocket
// ---------------------------------------------------------------------------

async fn run_ws_server() {
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
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(w) => w,
        Err(_) => return,
    };
    let (mut write, mut read) = ws.split();
    let mut rx = tx().subscribe();
    let mut authed = false;
    let mut keepalive = tokio::time::interval(Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = keepalive.tick() => {
                // Ping de texto (el móvil ignora type=ping); mantiene viva la conexión.
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
                if write.send(Message::text(reply)).await.is_err() { break; }
                if !was_authed && authed {
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
            let mut cfg = config().lock().unwrap();
            if !token.is_empty() && token == cfg.token {
                *authed = true;
                if !cfg.paired.contains(&device) {
                    cfg.paired.push(device);
                    cfg.save();
                }
                json!({"v":1,"type":"hello_ack","ok":true,"name":HOST_NAME}).to_string()
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
        _ => json!({"v":1,"type":"command_result","ok":false,"error":"unknown_type"}).to_string(),
    }
}

// ---------------------------------------------------------------------------
// Ejecución de acciones (atajos arbitrarios) sobre la ventana en primer plano
// ---------------------------------------------------------------------------

fn run_action(action: &str) -> Result<(), &'static str> {
    if action == "screenshot" {
        return take_screenshot();
    }
    run_hotkey(action)
}

/// Ejecuta un atajo como "ctrl+shift+p", "alt+F4", "ctrl+c".
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
    let res = e.key(key, Click);
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
    // Teclas de función f1..f12
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
        s if s.chars().count() == 1 => Ok(Key::Unicode(s.chars().next().unwrap())),
        _ => Err("bad_key"),
    }
}

fn take_screenshot() -> Result<(), &'static str> {
    use xcap::Monitor;
    let monitor = Monitor::all().map_err(|_| "internal")?.into_iter().next().ok_or("no_monitor")?;
    let img = monitor.capture_image().map_err(|_| "internal")?;
    let dir = dirs::picture_dir().unwrap_or(std::env::temp_dir()).join("KiBoard");
    std::fs::create_dir_all(&dir).map_err(|_| "internal")?;
    let path = dir.join(format!("screenshot-{}.png", now_ts()));
    img.save(&path).map_err(|_| "internal")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Comandos de la UI (emparejamiento + editor de perfiles)
// ---------------------------------------------------------------------------

/// Elige la mejor IP de LAN para que el móvil alcance el PC.
/// Evita loopback/APIPA y prioriza redes domésticas (192.168 > 10 > 172) sobre adaptadores virtuales.
fn best_lan_ip() -> String {
    use std::net::IpAddr;
    let mut v4s: Vec<std::net::Ipv4Addr> = local_ip_address::list_afinet_netifas()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, ip)| match ip {
            IpAddr::V4(v4) => Some(v4),
            _ => None,
        })
        .filter(|v4| !v4.is_loopback() && !v4.is_link_local())
        .collect();
    v4s.sort_by_key(|v4| {
        let o = v4.octets();
        if o[0] == 192 && o[1] == 168 {
            0
        } else if o[0] == 10 {
            1
        } else if o[0] == 172 {
            2
        } else {
            3
        }
    });
    v4s.first().map(|v| v.to_string()).unwrap_or_else(|| "127.0.0.1".into())
}

#[tauri::command]
fn pairing_info() -> serde_json::Value {
    let token = config().lock().unwrap().token.clone();
    let ip = best_lan_ip();
    let payload = json!({ "ip": ip, "port": WS_PORT, "token": token, "name": HOST_NAME });
    let svg = qr_svg(&payload.to_string());
    json!({ "ip": ip, "port": WS_PORT, "token": token, "svg": svg })
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

/// Guarda los perfiles editados. El watcher detecta el cambio y reenvía el layout al instante.
#[tauri::command]
fn save_profiles(profiles: Vec<Profile>) -> serde_json::Value {
    let mut cfg = config().lock().unwrap();
    cfg.profiles = profiles;
    cfg.save();
    json!({ "ok": true })
}

fn qr_svg(data: &str) -> String {
    use qrcode::{render::svg, QrCode};
    match QrCode::new(data.as_bytes()) {
        Ok(code) => code
            .render::<svg::Color>()
            .min_dimensions(220, 220)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build(),
        Err(_) => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::parse_key;

    #[test]
    fn parsea_teclas() {
        assert!(parse_key("c").is_ok());
        assert!(parse_key("F4").is_ok());
        assert!(parse_key("enter").is_ok());
        assert!(parse_key("left").is_ok());
        assert!(parse_key("nope").is_err());
    }
}

// ---------------------------------------------------------------------------
// Bootstrap Tauri
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    config();

    tauri::Builder::default()
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
            save_profiles
        ])
        .setup(|app| {
            tauri::async_runtime::spawn(run_ws_server());
            tauri::async_runtime::spawn(watch_active_app());

            // Arranca con Windows (idempotente).
            let _ = app.autolaunch().enable();

            // Busca actualizaciones en GitHub al arrancar (silencioso si no hay/conexión falla).
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

            let pair = MenuItem::with_id(app, "pair", "Abrir KiBoard…", true, None::<&str>)?;
            let unpair = MenuItem::with_id(app, "unpair", "Desvincular todo", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Salir de KiBoard", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&pair, &unpair, &quit])?;
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip(HOST_NAME)
                .menu(&menu)
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
