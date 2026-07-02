// KiBoard host — Fase 5 (perfiles programables).
// WS en LAN + emparejamiento por token/QR + comandos (atajos arbitrarios) + auto-switching
// con perfiles editables por el usuario desde la UI. Protocolo: ver /protocol/README.md

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
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
    /// Si está en la selección recomendada por defecto del perfil (el resto son extras disponibles).
    #[serde(default = "yes")]
    recommended: bool,
}

fn yes() -> bool { true }

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
    Button { label: label.into(), icon: icon.into(), action: action.into(), danger: false, recommended: true }
}
fn bd(label: &str, icon: &str, action: &str) -> Button {
    Button { label: label.into(), icon: icon.into(), action: action.into(), danger: true, recommended: true }
}
/// Botón "extra": disponible en el perfil pero fuera de la selección recomendada por defecto.
fn bx(label: &str, icon: &str, action: &str) -> Button {
    Button { label: label.into(), icon: icon.into(), action: action.into(), danger: false, recommended: false }
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
            b("Negrita", "bold", "ctrl+b"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"), b("Rehacer", "redo", "ctrl+y"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Cursiva", "italic", "ctrl+i"), bx("Subrayado", "underline", "ctrl+u"),
            bx("Cortar", "cut", "ctrl+x"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("gdocs", &["google docs", "documentos de google"], vec![
            b("Negrita", "bold", "ctrl+b"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Cursiva", "italic", "ctrl+i"), bx("Subrayado", "underline", "ctrl+u"),
            bx("Enlace", "link", "ctrl+k"), bx("Rehacer", "redo", "ctrl+y"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("gdrive", &["google drive", "mi unidad"], vec![
            b("Buscar", "find", "ctrl+f"), b("Nueva pestaña", "new", "ctrl+t"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Nueva carpeta", "newfolder", "shift+f"), bx("Renombrar", "rename", "n"),
        ]),
        profile("gmail", &["gmail"], vec![
            b("Redactar", "new", "c"), b("Buscar", "find", "/"),
            b("Responder", "reply", "r"), b("Archivar", "archive", "e"),
            bx("Resp. todos", "replyall", "a"), bx("Reenviar", "forward", "f"),
            bx("Eliminar", "delete", "#"), bx("Destacar", "star", "s"), bx("Enviar", "send", "ctrl+enter"),
        ]),
        profile("youtube", &["youtube"], vec![
            b("Play/Pausa", "play", "k"), b("Silenciar", "mute", "m"),
            b("Pantalla completa", "video", "f"), b("Captura", "screenshot", "screenshot"),
            bx("Adelante", "redo", "l"), bx("Atrás", "undo", "j"),
            bx("Siguiente", "redo", "shift+n"), bx("Subtítulos", "text", "c"), bx("Teatro", "fullscreen", "t"),
        ]),
        // --- Office ---
        profile("word", &["word"], vec![
            b("Guardar", "save", "ctrl+s"), b("Negrita", "bold", "ctrl+b"),
            b("Buscar", "find", "ctrl+f"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Cursiva", "italic", "ctrl+i"), bx("Subrayado", "underline", "ctrl+u"),
            bx("Enlace", "link", "ctrl+k"), bx("Rehacer", "redo", "ctrl+y"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("excel", &["excel"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"), b("Rehacer", "redo", "ctrl+y"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Negrita", "bold", "ctrl+b"), bx("Cursiva", "italic", "ctrl+i"),
            bx("Autosuma", "sum", "alt+="), bx("Filtro", "filter", "ctrl+shift+l"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("powerpoint", &["powerpoint"], vec![
            // Pack presentador: el uso remoto #1 — pasar diapositivas desde el atril.
            b("Siguiente", "next", "right"), b("Anterior", "prev", "left"),
            b("Presentar", "play", "f5"), b("Negro", "dark", "b"),
            b("Guardar", "save", "ctrl+s"), b("Deshacer", "undo", "ctrl+z"),
            bx("Blanco", "light", "w"), bx("Desde actual", "play", "shift+f5"),
            bx("Nueva diap.", "new", "ctrl+m"), bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
            bx("Duplicar", "duplicate", "ctrl+d"), bx("Negrita", "bold", "ctrl+b"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("outlook", &["outlook"], vec![
            b("Nuevo correo", "new", "ctrl+n"), b("Responder", "reply", "ctrl+r"),
            b("Enviar", "send", "ctrl+enter"), b("Buscar", "find", "ctrl+e"),
            bx("Resp. todos", "replyall", "ctrl+shift+r"), bx("Reenviar", "forward", "ctrl+f"),
            bx("Eliminar", "delete", "ctrl+d"), bx("Calendario", "calendar", "ctrl+2"),
        ]),
        profile("acrobat", &["acrobat"], vec![
            b("Buscar", "find", "ctrl+f"), b("Imprimir", "print", "ctrl+p"),
            b("Guardar", "save", "ctrl+s"), b("Copiar", "copy", "ctrl+c"),
            bx("Zoom +", "zoomin", "ctrl+="), bx("Zoom -", "zoomout", "ctrl+-"),
            bx("Pant. completa", "fullscreen", "ctrl+l"),
        ]),
        // --- Creativas ---
        profile("photoshop", &["photoshop"], vec![
            b("Deshacer", "undo", "ctrl+z"), b("Pincel", "brush", "b"), b("Mover", "move", "v"),
            b("Guardar", "save", "ctrl+s"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Texto", "text", "t"), bx("Borrador", "eraser", "e"), bx("Recortar", "crop", "c"),
            bx("Zoom", "zoomin", "z"), bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        profile("illustrator", &["illustrator"], vec![
            b("Deshacer", "undo", "ctrl+z"), b("Selección", "cursor", "v"),
            b("Guardar", "save", "ctrl+s"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Pluma", "pencil", "p"), bx("Texto", "text", "t"), bx("Rectángulo", "rect", "m"),
            bx("Zoom", "zoomin", "z"), bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        profile("premiere", &["premiere"], vec![
            b("Play/Pausa", "play", "k"), b("Cortar", "cut", "c"), b("Selección", "cursor", "v"),
            b("Guardar", "save", "ctrl+s"), b("Deshacer", "undo", "ctrl+z"),
            bx("Entrada", "login", "i"), bx("Salida", "logout", "o"), bx("Marcador", "star", "m"),
            bx("Exportar", "upload", "ctrl+m"), bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        profile("figma", &["figma"], vec![
            b("Mover", "move", "v"), b("Marco", "frame", "f"), b("Comentar", "comment", "c"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"), b("Deshacer", "undo", "ctrl+z"),
            bx("Texto", "text", "t"), bx("Rectángulo", "rect", "r"), bx("Lápiz", "pencil", "p"),
            bx("Duplicar", "duplicate", "ctrl+d"), bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        // --- Comunicación / reuniones ---
        profile("slack", &["slack"], vec![
            b("Saltar a", "find", "ctrl+k"), b("Negrita", "bold", "ctrl+b"),
            b("Hilos", "comment", "ctrl+shift+t"), b("Copiar", "copy", "ctrl+c"),
            bx("Cursiva", "italic", "ctrl+i"), bx("Buscar", "find", "ctrl+f"),
            bx("Subir archivo", "upload", "ctrl+u"), bx("Editar último", "redo", "ctrl+up"),
        ]),
        profile("discord", &["discord"], vec![
            b("Silenciar", "mute", "ctrl+shift+m"), b("Audio", "video", "ctrl+shift+d"),
            b("Buscar", "find", "ctrl+f"),
            bx("Sgte canal", "next", "alt+down"), bx("Canal ant.", "prev", "alt+up"),
            bx("Marcar leído", "close", "escape"),
        ]),
        profile("teams", &["teams"], vec![
            b("Silenciar", "mic", "ctrl+shift+m"), b("Cámara", "video", "ctrl+shift+o"),
            b("Compartir", "share", "ctrl+shift+e"), b("Colgar", "close", "ctrl+shift+h"),
            bx("Levantar mano", "hand", "ctrl+shift+k"), bx("Chat", "comment", "ctrl+2"),
            bx("Aceptar", "play", "ctrl+shift+s"), bx("Rechazar", "close", "ctrl+shift+d"),
        ]),
        profile("zoom", &["zoom"], vec![
            b("Silenciar", "mic", "alt+a"), b("Vídeo", "video", "alt+v"),
            b("Compartir", "share", "alt+s"), b("Salir", "close", "alt+q"),
            bx("Levantar mano", "hand", "alt+y"), bx("Chat", "comment", "alt+h"),
            bx("Grabar", "record", "alt+r"), bx("Pant. completa", "fullscreen", "alt+f"),
            bx("Participantes", "people", "alt+u"),
        ]),
        profile("notion", &["notion"], vec![
            b("Buscar", "find", "ctrl+p"), b("Negrita", "bold", "ctrl+b"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Cursiva", "italic", "ctrl+i"), bx("Nueva página", "new", "ctrl+n"),
            bx("Enlace", "link", "ctrl+k"), bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        // --- Multimedia ---
        profile("spotify", &["spotify"], vec![
            b("Play/Pausa", "play", "space"), b("Siguiente", "next", "ctrl+right"),
            b("Anterior", "prev", "ctrl+left"),
            bx("+ Volumen", "vol", "ctrl+up"), bx("- Volumen", "vol", "ctrl+down"),
            bx("Aleatorio", "shuffle", "ctrl+s"), bx("Repetir", "repeat", "ctrl+r"), bx("Buscar", "find", "ctrl+l"),
        ]),
        profile("vlc", &["vlc"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "fullscreen", "f"),
            b("Silenciar", "mute", "m"), b("+ Volumen", "vol", "ctrl+up"), b("- Volumen", "vol", "ctrl+down"),
            bx("Siguiente", "next", "n"), bx("Anterior", "prev", "p"),
            bx("Subtítulos", "subtitles", "v"), bx("Captura", "screenshot", "shift+s"),
        ]),
        // --- Desarrollo / sistema ---
        profile("editor", &["code", "devenv", "visual studio", "cursor"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Paleta", "new", "ctrl+shift+p"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Reemplazar", "replace", "ctrl+h"), bx("Comentar", "comment", "ctrl+/"),
            bx("Terminal", "terminal", "ctrl+`"), bx("Ir a línea", "find", "ctrl+g"),
            bx("Formato", "format", "shift+alt+f"),
        ]),
        profile("terminal", &["powershell", "windows terminal", "símbolo del sistema", "command prompt", "warp"], vec![
            // OJO: ctrl+c en consola es SIGINT (mata el proceso). Windows Terminal/PowerShell
            // aceptan ctrl+shift+c/v para el portapapeles.
            b("Copiar", "copy", "ctrl+shift+c"), b("Pegar", "paste", "ctrl+shift+v"),
            b("Nueva pestaña", "new", "ctrl+shift+t"), b("Buscar", "find", "ctrl+f"),
            bx("Cerrar pestaña", "close", "ctrl+shift+w"), bx("Dividir", "new", "alt+shift+d"),
            bx("Nueva ventana", "new", "ctrl+shift+n"), bx("Panel sgte", "redo", "ctrl+tab"),
        ]),
        profile("notepadpp", &["notepad++"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Reemplazar", "replace", "ctrl+h"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Ir a línea", "find", "ctrl+g"), bx("Duplicar línea", "new", "ctrl+d"),
            bx("Comentar", "comment", "ctrl+q"), bx("Imprimir", "print", "ctrl+p"), bx("Guardar todo", "save", "ctrl+shift+s"),
        ]),
        profile("explorer", &["explorador", "file explorer", "explorer"], vec![
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            b("Nueva carpeta", "newfolder", "ctrl+shift+n"), b("Renombrar", "rename", "f2"),
            bx("Cortar", "cut", "ctrl+x"), bx("Atrás", "undo", "alt+left"),
            bx("Subir nivel", "redo", "alt+up"), bx("Propiedades", "settings", "alt+enter"), bx("Buscar", "find", "ctrl+f"),
        ]),
        // --- Lote 1: apps/web añadidas hacia ~100 (van antes de "browser" para ganar por título) ---
        profile("chatgpt", &["chatgpt"], vec![
            b("Nuevo chat", "new", "ctrl+shift+o"), b("Barra lateral", "tab", "ctrl+shift+s"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Borrar chat", "delete", "ctrl+shift+backspace"), bx("Enfocar entrada", "text", "shift+escape"),
        ]),
        profile("gcalendar", &["google calendar"], vec![
            b("Crear", "new", "c"), b("Hoy", "home", "t"), b("Buscar", "find", "/"), b("Semana", "tab", "w"),
            bx("Día", "tab", "d"), bx("Mes", "tab", "m"), bx("Agenda", "apps", "a"), bx("Año", "tab", "y"),
        ]),
        profile("onenote", &["onenote"], vec![
            b("Nueva página", "new", "ctrl+n"), b("Negrita", "bold", "ctrl+b"),
            b("Buscar", "find", "ctrl+e"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Cursiva", "italic", "ctrl+i"), bx("Subrayado", "underline", "ctrl+u"),
            bx("Resaltar", "highlight", "ctrl+shift+h"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("thunderbird", &["thunderbird"], vec![
            b("Nuevo", "new", "ctrl+n"), b("Responder", "reply", "ctrl+r"),
            b("Reenviar", "forward", "ctrl+l"), b("Buscar", "find", "ctrl+shift+k"),
            bx("Resp. todos", "replyall", "ctrl+shift+r"), bx("Enviar", "send", "ctrl+enter"),
            bx("Eliminar", "delete", "delete"), bx("Archivar", "archive", "a"),
        ]),
        profile("sublime", &["sublime"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Paleta", "new", "ctrl+shift+p"), b("Ir a", "find", "ctrl+p"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Reemplazar", "replace", "ctrl+h"), bx("Comentar", "comment", "ctrl+/"),
            bx("Duplicar línea", "duplicate", "ctrl+shift+d"), bx("Ir a línea", "find", "ctrl+g"),
        ]),
        profile("obsidian", &["obsidian"], vec![
            b("Nueva nota", "new", "ctrl+n"), b("Cambiador", "find", "ctrl+o"),
            b("Editar/Ver", "redo", "ctrl+e"), b("Paleta", "new", "ctrl+p"),
            bx("Buscar", "find", "ctrl+shift+f"), bx("Negrita", "bold", "ctrl+b"),
            bx("Cursiva", "italic", "ctrl+i"), bx("Enlace", "link", "ctrl+k"),
        ]),
        profile("gimp", &["gimp"], vec![
            b("Deshacer", "undo", "ctrl+z"), b("Pincel", "brush", "p"), b("Borrador", "eraser", "shift+e"),
            b("Guardar", "save", "ctrl+s"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Lápiz", "pencil", "n"), bx("Texto", "text", "t"), bx("Rellenar", "fill", "shift+b"),
            bx("Exportar", "upload", "ctrl+e"), bx("Rehacer", "redo", "ctrl+y"),
        ]),
        profile("canva", &["canva"], vec![
            b("Texto", "text", "t"), b("Deshacer", "undo", "ctrl+z"), b("Duplicar", "duplicate", "ctrl+d"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Rehacer", "redo", "ctrl+y"), bx("Rectángulo", "rect", "r"),
            bx("Círculo", "ellipse", "c"), bx("Línea", "line", "l"), bx("Agrupar", "group", "ctrl+g"),
        ]),
        // Paint: la barra de herramientas no tiene atajos → se pulsan por UI Automation ("uia:<nombre>").
        // Nombres reales de la barra (Paint Win11 ES) verificados con UIAutomation.
        profile("paint", &["paint"], vec![
            b("Lápiz", "pencil", "uia:Lápiz"), b("Relleno", "fill", "uia:Relleno"),
            b("Texto", "text", "uia:Texto"), b("Borrador", "eraser", "uia:Borrador"),
            b("Selector color", "colorpick", "uia:Selector de colores"),
            b("Rectángulo", "rect", "uia:Rectángulo"), b("Elipse", "ellipse", "uia:Elipse"),
            b("Línea", "line", "uia:Línea"),
            bx("Lupa", "zoomin", "uia:Lupa"),
            bx("Recortar", "crop", "uia:Recortar"),
            bx("Girar dcha.", "rotate", "uia:Girar>>Girar 90º a la derecha"),
            bx("Girar izq.", "rotate", "uia:Girar>>Girar 90º a la izquierda"),
            bx("Deshacer", "undo", "ctrl+z"), bx("Rehacer", "redo", "ctrl+y"),
            bx("Guardar", "save", "ctrl+s"), bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
            // Selector de color: un solo botón abre una paleta local en el móvil ("colorpicker:...");
            // cada swatch envía "uia:<Nombre>". La paleta son ListItems UIA con SelectionItemPattern
            // (ya soportado). Nombres EXACTOS de Paint Win11 ES (verificados en vivo); el match exacto
            // evita chocar con "Color 1: Negro"/"Color 2: Blanco". El hex es solo para pintar el swatch.
            b("Colores", "palette", "colorpicker:Negro=000000;Blanco=FFFFFF;Gris=7F7F7F;Rojo=ED1C24;\
              Naranja=FF7F27;Amarillo=FFF200;Verde=22B14C;Turquesa=00A2E8;Añil=3F48CC;\
              Púrpura=A349A4;Marrón=B97A57;Rosa=FFAEC9"),
        ]),
        profile("davinci", &["davinci", "resolve"], vec![
            b("Play/Pausa", "play", "space"), b("Entrada", "login", "i"), b("Salida", "logout", "o"),
            b("Cortar", "cut", "ctrl+b"), b("Deshacer", "undo", "ctrl+z"),
            bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"), bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        profile("audacity", &["audacity"], vec![
            b("Reproducir", "play", "space"), b("Grabar", "record", "r"),
            b("Deshacer", "undo", "ctrl+z"), b("Cortar", "cut", "ctrl+x"), b("Copiar", "copy", "ctrl+c"),
            bx("Pegar", "paste", "ctrl+v"), bx("Silencio", "mute", "ctrl+l"), bx("Rehacer", "redo", "ctrl+y"),
        ]),
        profile("jetbrains", &["intellij", "pycharm", "webstorm", "phpstorm", "rider", "goland", "clion", "datagrip", "android studio"], vec![
            b("Buscar", "find", "ctrl+f"), b("Comentar", "comment", "ctrl+/"),
            b("Reformatear", "format", "ctrl+alt+l"), b("Ejecutar", "play", "shift+f10"),
            b("Deshacer", "undo", "ctrl+z"),
            bx("Reemplazar", "replace", "ctrl+r"), bx("Ir a línea", "find", "ctrl+g"),
            bx("Renombrar", "rename", "shift+f6"), bx("Buscar acción", "find", "ctrl+shift+a"),
        ]),
        profile("lightroom", &["lightroom"], vec![
            b("Cuadrícula", "apps", "g"), b("Lupa", "zoomin", "e"), b("Revelar", "brush", "d"),
            b("Deshacer", "undo", "ctrl+z"),
            bx("Importar", "download", "ctrl+shift+i"), bx("Exportar", "upload", "ctrl+shift+e"),
            bx("Marcar", "star", "p"), bx("Rechazar", "delete", "x"),
        ]),
        profile("telegram", &["telegram"], vec![
            b("Buscar", "find", "ctrl+f"), b("Sgte chat", "redo", "alt+down"),
            b("Chat ant.", "undo", "alt+up"), b("Guardados", "star", "ctrl+0"),
        ]),
        // --- Lote 2: más apps/web hacia ~100 (también antes de "browser") ---
        profile("gmeet", &["google meet", "meet -"], vec![
            b("Silenciar", "mic", "ctrl+d"), b("Cámara", "video", "ctrl+e"),
            b("Levantar mano", "hand", "ctrl+alt+h"),
            bx("Pant. completa", "fullscreen", "f"), bx("Captura", "screenshot", "screenshot"),
        ]),
        profile("trello", &["trello"], vec![
            b("Buscar", "find", "/"), b("Filtrar", "filter", "f"),
            b("Tableros", "apps", "b"), b("Mis tarjetas", "star", "q"),
            bx("Archivar", "archive", "c"), bx("Etiquetas", "link", "l"),
            bx("Vencimiento", "calendar", "d"), bx("Miembros", "people", "m"),
        ]),
        profile("todoist", &["todoist"], vec![
            b("Añadir rápido", "new", "q"), b("Añadir tarea", "new", "a"),
            bx("Deshacer", "undo", "ctrl+z"), bx("Sincronizar", "refresh", "ctrl+r"),
        ]),
        profile("linear", &["linear"], vec![
            b("Crear", "new", "c"), b("Buscar", "find", "/"),
            b("Asignar", "assign", "a"), b("Estado", "redo", "s"),
            bx("Prioridad", "star", "p"), bx("Etiqueta", "link", "l"), bx("Vencimiento", "calendar", "d"),
        ]),
        profile("jira", &["jira"], vec![
            b("Crear", "new", "c"), b("Buscar", "find", "/"),
            b("Asignar", "assign", "a"), b("Editar", "redo", "e"),
            bx("Comentar", "comment", "m"), bx("Asignarme", "star", "i"),
        ]),
        profile("miro", &["miro"], vec![
            b("Texto", "text", "t"), b("Nota", "note", "s"),
            b("Lápiz", "pencil", "p"), b("Deshacer", "undo", "ctrl+z"),
            bx("Rectángulo", "rect", "r"), bx("Marco", "frame", "f"),
            bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
        ]),
        profile("postman", &["postman"], vec![
            b("Enviar", "send", "ctrl+enter"), b("Guardar", "save", "ctrl+s"),
            b("Nuevo", "new", "ctrl+n"), b("Buscar", "find", "ctrl+f"),
            bx("Nueva pestaña", "new", "ctrl+t"), bx("Cerrar pestaña", "close", "ctrl+w"),
        ]),
        profile("github", &["github"], vec![
            b("Buscar", "find", "/"), b("Archivos", "find", "t"),
            b("Ir a línea", "find", "l"), b("Cambiar rama", "tab", "w"),
            bx("Editor web", "text", "."), bx("Blame", "comment", "b"),
        ]),
        profile("overleaf", &["overleaf"], vec![
            b("Guardar/Compilar", "play", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Rehacer", "redo", "ctrl+y"), bx("Comentar", "comment", "ctrl+/"),
        ]),
        profile("gkeep", &["google keep"], vec![
            b("Nueva nota", "new", "c"), b("Buscar", "find", "/"),
            b("Lista nueva", "new", "l"), b("Archivar", "archive", "e"),
            bx("Fijar", "pin", "f"),
        ]),
        profile("aftereffects", &["after effects"], vec![
            b("Vista previa", "play", "space"), b("Selección", "cursor", "v"),
            b("Deshacer", "undo", "ctrl+z"), b("Guardar", "save", "ctrl+s"),
            bx("Mano", "hand", "h"), bx("Zoom", "zoomin", "z"),
            bx("Rehacer", "redo", "ctrl+shift+z"), bx("Copiar", "copy", "ctrl+c"),
        ]),
        profile("krita", &["krita"], vec![
            b("Pincel", "brush", "b"), b("Borrador", "eraser", "e"),
            b("Deshacer", "undo", "ctrl+z"), b("Guardar", "save", "ctrl+s"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Zoom +", "zoomin", "ctrl+="), bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        profile("inkscape", &["inkscape"], vec![
            b("Seleccionar", "cursor", "s"), b("Lápiz", "pencil", "p"),
            b("Texto", "text", "t"), b("Deshacer", "undo", "ctrl+z"),
            bx("Rectángulo", "rect", "r"), bx("Elipse", "ellipse", "e"),
            bx("Guardar", "save", "ctrl+s"), bx("Rehacer", "redo", "ctrl+y"),
        ]),
        // --- Lote 3: más apps/web hacia ~100 (también antes de "browser") ---
        profile("whatsapp", &["whatsapp"], vec![
            b("Nuevo chat", "new", "ctrl+n"), b("Buscar", "find", "ctrl+f"),
            b("Sgte chat", "next", "ctrl+shift+]"), b("Chat ant.", "prev", "ctrl+shift+["),
            bx("Archivar", "archive", "ctrl+e"), bx("Silenciar", "mute", "ctrl+shift+m"),
        ]),
        profile("twitch", &["twitch"], vec![
            b("Play/Pausa", "play", "space"), b("Silenciar", "mute", "m"),
            b("Pant. completa", "fullscreen", "f"), b("Teatro", "video", "alt+t"),
        ]),
        profile("netflix", &["netflix"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "fullscreen", "f"),
            b("Silenciar", "mute", "m"),
        ]),
        profile("xtwitter", &["twitter", "/ x"], vec![
            b("Nuevo post", "new", "n"), b("Buscar", "find", "/"),
            b("Me gusta", "star", "l"), b("Responder", "reply", "r"),
            bx("Repost", "redo", "t"),
        ]),
        profile("eclipse", &["eclipse"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Comentar", "comment", "ctrl+/"), b("Ejecutar", "play", "ctrl+f11"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Formato", "format", "ctrl+shift+f"), bx("Organizar imports", "redo", "ctrl+shift+o"),
        ]),
        profile("dbeaver", &["dbeaver"], vec![
            b("Ejecutar", "play", "ctrl+enter"), b("Guardar", "save", "ctrl+s"),
            b("Buscar", "find", "ctrl+f"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Comentar", "comment", "ctrl+/"), bx("Formato", "format", "ctrl+shift+f"),
        ]),
        profile("unity", &["unity"], vec![
            b("Reproducir", "play", "ctrl+p"), b("Mano", "hand", "q"),
            b("Mover", "move", "w"), b("Rotar", "rotate", "e"),
            b("Guardar", "save", "ctrl+s"), b("Deshacer", "undo", "ctrl+z"),
            bx("Duplicar", "duplicate", "ctrl+d"), bx("Buscar", "find", "ctrl+f"),
        ]),
        profile("godot", &["godot"], vec![
            b("Ejecutar", "play", "f5"), b("Escena actual", "play", "f6"),
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"),
            bx("Comentar", "comment", "ctrl+k"), bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
        ]),
        profile("capcut", &["capcut"], vec![
            b("Play/Pausa", "play", "space"), b("Dividir", "cut", "ctrl+b"),
            b("Deshacer", "undo", "ctrl+z"), b("Guardar", "save", "ctrl+s"),
            bx("Eliminar", "delete", "delete"), bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
        ]),
        profile("affinity", &["affinity"], vec![
            b("Mover", "move", "v"), b("Pincel", "brush", "b"), b("Borrador", "eraser", "e"),
            b("Texto", "text", "t"), b("Deshacer", "undo", "ctrl+z"), b("Guardar", "save", "ctrl+s"),
            bx("Recortar", "crop", "c"), bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
        ]),
        profile("ableton", &["ableton"], vec![
            b("Play/Pausa", "play", "space"), b("Grabar", "record", "f9"),
            b("Deshacer", "undo", "ctrl+z"), b("Guardar", "save", "ctrl+s"),
            bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"), bx("Bucle", "repeat", "ctrl+l"),
        ]),
        profile("reaper", &["reaper"], vec![
            b("Play/Pausa", "play", "space"), b("Grabar", "record", "ctrl+r"),
            b("Guardar", "save", "ctrl+s"), b("Deshacer", "undo", "ctrl+z"),
            bx("Marcador", "star", "m"), bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
        ]),
        // --- Lote 4: más apps/web hacia ~100 (también antes de "browser") ---
        profile("lowriter", &["libreoffice writer"], vec![
            b("Guardar", "save", "ctrl+s"), b("Negrita", "bold", "ctrl+b"),
            b("Buscar", "find", "ctrl+f"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Cursiva", "italic", "ctrl+i"), bx("Subrayado", "underline", "ctrl+u"),
            bx("Rehacer", "redo", "ctrl+y"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("localc", &["libreoffice calc"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"), b("Rehacer", "redo", "ctrl+y"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Negrita", "bold", "ctrl+b"), bx("Cursiva", "italic", "ctrl+i"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("loimpress", &["libreoffice impress"], vec![
            // Pack presentador (en modo presentación: flechas pasan diapositiva, b = negro, w = blanco).
            b("Siguiente", "next", "right"), b("Anterior", "prev", "left"),
            b("Presentar", "play", "f5"), b("Negro", "dark", "b"),
            b("Guardar", "save", "ctrl+s"), b("Deshacer", "undo", "ctrl+z"),
            bx("Blanco", "light", "w"), bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
            bx("Negrita", "bold", "ctrl+b"), bx("Rehacer", "redo", "ctrl+y"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("notepad", &["notepad"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Reemplazar", "replace", "ctrl+h"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Cortar", "cut", "ctrl+x"), bx("Ir a línea", "find", "ctrl+g"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("sumatra", &["sumatra"], vec![
            b("Buscar", "find", "ctrl+f"), b("Ir a página", "find", "ctrl+g"),
            b("Zoom +", "zoomin", "ctrl+="), b("Zoom -", "zoomout", "ctrl+-"),
            bx("Pant. completa", "fullscreen", "f11"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("foxit", &["foxit"], vec![
            b("Buscar", "find", "ctrl+f"), b("Guardar", "save", "ctrl+s"),
            b("Imprimir", "print", "ctrl+p"), b("Copiar", "copy", "ctrl+c"),
            bx("Zoom +", "zoomin", "ctrl+="), bx("Zoom -", "zoomout", "ctrl+-"),
        ]),
        profile("shotcut", &["shotcut"], vec![
            b("Play/Pausa", "play", "space"), b("Dividir", "cut", "s"),
            b("Deshacer", "undo", "ctrl+z"), b("Guardar", "save", "ctrl+s"),
            bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"), bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        profile("musescore", &["musescore"], vec![
            b("Reproducir", "play", "space"), b("Guardar", "save", "ctrl+s"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Rehacer", "redo", "ctrl+shift+z"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("soundcloud", &["soundcloud"], vec![
            b("Play/Pausa", "play", "space"), b("Siguiente", "next", "shift+right"),
            b("Anterior", "prev", "shift+left"), b("Me gusta", "star", "l"),
        ]),
        profile("potplayer", &["potplayer"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "fullscreen", "enter"),
            b("Silenciar", "mute", "m"), b("+ Volumen", "vol", "up"), b("- Volumen", "vol", "down"),
        ]),
        profile("primevideo", &["prime video"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "fullscreen", "f"),
            b("Silenciar", "mute", "m"),
        ]),
        profile("disney", &["disney+"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "fullscreen", "f"),
            b("Silenciar", "mute", "m"),
        ]),
        // --- Lote 5: más apps/web hacia ~100 (también antes de "browser") ---
        profile("gslides", &["google slides", "presentaciones de google"], vec![
            // Pack presentador (en modo presentación las flechas pasan diapositiva; b = negro, w = blanco).
            b("Siguiente", "next", "right"), b("Anterior", "prev", "left"),
            b("Presentar", "play", "ctrl+f5"), b("Negro", "dark", "b"),
            b("Deshacer", "undo", "ctrl+z"),
            bx("Blanco", "light", "w"), bx("Nueva diap.", "new", "ctrl+m"),
            bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
            bx("Rehacer", "redo", "ctrl+y"), bx("Duplicar", "duplicate", "ctrl+d"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("evernote", &["evernote"], vec![
            b("Nueva nota", "new", "ctrl+n"), b("Buscar", "find", "ctrl+shift+f"),
            b("Negrita", "bold", "ctrl+b"), b("Cursiva", "italic", "ctrl+i"),
            bx("Subrayado", "underline", "ctrl+u"), bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
        ]),
        profile("joplin", &["joplin"], vec![
            b("Nueva nota", "new", "ctrl+n"), b("Buscar", "find", "ctrl+f"),
            b("Negrita", "bold", "ctrl+b"), b("Cursiva", "italic", "ctrl+i"),
            bx("Enlace", "link", "ctrl+k"), bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
        ]),
        profile("rstudio", &["rstudio"], vec![
            b("Ejecutar", "play", "ctrl+enter"), b("Guardar", "save", "ctrl+s"),
            b("Buscar", "find", "ctrl+f"), b("Deshacer", "undo", "ctrl+z"),
            bx("Comentar", "comment", "ctrl+shift+c"), bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
        ]),
        profile("visio", &["visio"], vec![
            b("Guardar", "save", "ctrl+s"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"), b("Buscar", "find", "ctrl+f"),
            bx("Rehacer", "redo", "ctrl+y"), bx("Duplicar", "duplicate", "ctrl+d"),
        ]),
        profile("coreldraw", &["coreldraw"], vec![
            b("Deshacer", "undo", "ctrl+z"), b("Guardar", "save", "ctrl+s"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Rehacer", "redo", "ctrl+shift+z"), bx("Duplicar", "duplicate", "ctrl+d"),
        ]),
        profile("kdenlive", &["kdenlive"], vec![
            b("Play/Pausa", "play", "space"), b("Guardar", "save", "ctrl+s"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        profile("scribus", &["scribus"], vec![
            b("Guardar", "save", "ctrl+s"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Rehacer", "redo", "ctrl+shift+z"), bx("Buscar", "find", "ctrl+f"),
        ]),
        profile("mpchc", &["mpc-hc", "mpc-be", "media player classic"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "fullscreen", "alt+enter"),
            b("+ Volumen", "vol", "up"), b("- Volumen", "vol", "down"),
        ]),
        profile("vimeo", &["vimeo"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "fullscreen", "f"),
            b("Silenciar", "mute", "m"),
        ]),
        profile("crunchyroll", &["crunchyroll"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "fullscreen", "f"),
            b("Silenciar", "mute", "m"),
        ]),
        profile("hbomax", &["hbo max", "hbomax", "max -"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "fullscreen", "f"),
            b("Silenciar", "mute", "m"),
        ]),
        // --- Lote 6: cierre hacia ~100 (también antes de "browser") ---
        profile("insomnia", &["insomnia"], vec![
            b("Enviar", "send", "ctrl+enter"), b("Nuevo", "new", "ctrl+n"),
            b("Buscar", "find", "ctrl+f"),
            bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
        ]),
        profile("hulu", &["hulu"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "fullscreen", "f"),
            b("Silenciar", "mute", "m"),
        ]),
        profile("plex", &["plex"], vec![
            b("Play/Pausa", "play", "space"), b("Pant. completa", "fullscreen", "f"),
            b("Silenciar", "mute", "m"),
        ]),
        profile("cubase", &["cubase"], vec![
            b("Play/Pausa", "play", "space"), b("Guardar", "save", "ctrl+s"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        profile("camtasia", &["camtasia"], vec![
            b("Play/Pausa", "play", "space"), b("Guardar", "save", "ctrl+s"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Rehacer", "redo", "ctrl+y"),
        ]),
        profile("filmora", &["filmora"], vec![
            b("Play/Pausa", "play", "space"), b("Guardar", "save", "ctrl+s"),
            b("Deshacer", "undo", "ctrl+z"), b("Eliminar", "delete", "delete"),
            bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"), bx("Rehacer", "redo", "ctrl+y"),
        ]),
        profile("vegas", &["vegas pro"], vec![
            b("Play/Pausa", "play", "space"), b("Dividir", "cut", "s"),
            b("Guardar", "save", "ctrl+s"), b("Deshacer", "undo", "ctrl+z"),
            bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"), bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        profile("geany", &["geany"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Reemplazar", "replace", "ctrl+h"), b("Ejecutar", "play", "f5"),
            b("Deshacer", "undo", "ctrl+z"),
            bx("Comentar", "comment", "ctrl+e"), bx("Ir a línea", "find", "ctrl+l"),
        ]),
        profile("spyder", &["spyder"], vec![
            b("Ejecutar", "play", "f5"), b("Guardar", "save", "ctrl+s"),
            b("Buscar", "find", "ctrl+f"), b("Deshacer", "undo", "ctrl+z"),
            bx("Comentar", "comment", "ctrl+1"), bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
        ]),
        profile("autocad", &["autocad"], vec![
            b("Guardar", "save", "ctrl+s"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Rehacer", "redo", "ctrl+y"), bx("Buscar", "find", "ctrl+f"),
        ]),
        // --- Gaming / streaming ---
        // OBS por websocket (obs:): funciona aunque el juego esté en primer plano, con estado en
        // vivo en el botón. Mientras OBS graba o transmite, este perfil se pinea (ver layout_for)
        // y se le añaden las escenas del usuario como botones.
        profile("obs", &["obs studio", "obs64"], vec![
            b("Grabar", "record", "obs:record"), bd("Directo", "stream", "obs:stream"),
            b("Mic", "mic", "obs:mic"), b("Clip", "clip", "obs:replay"),
            bx("Buffer clips", "clip", "obs:replaybuffer"),
        ]),
        profile("steam", &["steam"], vec![
            b("Overlay", "apps", "shift+tab"), b("Captura", "screenshot", "f12"),
        ]),
        // --- Navegador genérico (cualquier pestaña no específica) ---
        profile("browser", &["chrome", "edge", "firefox", "brave", "opera", "vivaldi"], vec![
            b("Nueva pestaña", "new", "ctrl+t"), b("Cerrar pestaña", "close", "ctrl+w"),
            b("Recargar", "refresh", "f5"), b("Buscar", "find", "ctrl+f"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Reabrir pestaña", "redo", "ctrl+shift+t"), bx("Favorito", "star", "ctrl+d"),
            bx("Historial", "history", "ctrl+h"), bx("Descargas", "download", "ctrl+j"),
            bx("Incógnito", "new", "ctrl+shift+n"), bx("Zoom +", "zoomin", "ctrl+="), bx("Zoom -", "zoomout", "ctrl+-"),
        ]),
        // --- Fallback: control remoto de sofá (teclas multimedia del sistema, valen en cualquier app) ---
        profile("generic", &[], vec![
            b("Play/Pausa", "play", "playpause"), b("Vol +", "vol", "volup"),
            b("Vol -", "voldown", "voldown"), b("Silencio", "mute", "volmute"),
            b("Captura", "screenshot", "screenshot"),
            bx("Siguiente", "next", "nexttrack"), bx("Anterior", "prev", "prevtrack"),
            // Game Bar de Windows: atajos FIJOS del sistema, valen dentro de cualquier juego.
            bx("Clip 30s", "clip", "win+alt+g"), bx("Grabar juego", "record", "win+alt+r"),
            bx("Game Bar", "apps", "win+g"),
            bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"), bx("Cortar", "cut", "ctrl+x"),
            bx("Deshacer", "undo", "ctrl+z"), bx("Rehacer", "redo", "ctrl+y"),
            bx("Guardar", "save", "ctrl+s"), bx("Buscar", "find", "ctrl+f"), bx("Imprimir", "print", "ctrl+p"),
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
const PROFILES_VERSION: u32 = 20;

#[derive(Serialize, Deserialize, Default)]
struct Config {
    token: String,
    #[serde(default)]
    paired: Vec<String>,
    #[serde(default)]
    profiles: Vec<Profile>,
    #[serde(default)]
    profiles_version: u32,
    /// Contraseña del servidor WebSocket de OBS (vacía si el server no tiene auth).
    #[serde(default)]
    obs_password: String,
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
    // Elegir perfil y COPIAR sus botones (id, label, action, icon, danger, recommended); soltar el
    // lock antes de consultar UIA (lento) para no bloquear al resto.
    // Estado de OBS: si está grabando o en directo, el perfil "obs" se PINEA aunque la ventana al
    // frente sea el juego (los atajos por título no ven a OBS detrás del juego en pantalla completa).
    let obs = obs_state().lock().unwrap().clone();
    let obs_live = obs.connected && (obs.recording || obs.streaming);
    let (profile_id, raw): (String, Vec<(usize, String, String, String, bool, bool)>) = {
        let cfg = config().lock().unwrap();
        let hay = format!("{app} {title}").to_lowercase();
        let profile = if obs_live { cfg.profiles.iter().find(|p| p.id == "obs") } else { None }
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
    // Ocultar los botones uia cuyo control esté deshabilitado ahora (estado dinámico de la app:
    // p. ej. "Recortar" solo se habilita con una selección hecha).
    let disabled = uia_disabled_actions(&raw);
    let mut buttons: Vec<_> = raw
        .iter()
        .filter(|(_, _, action, _, _, _)| !disabled.contains(action))
        // Sin micrófono en OBS (ni global ni en escenas): el botón Mic no haría nada → ocultarlo.
        .filter(|(_, _, action, _, _, _)| {
            !(action == "obs:mic" && obs.connected && obs.mic_name.is_empty())
        })
        .map(|(i, label, action, icon, danger, rec)| {
            // Estado en vivo para los botones OBS (REC encendido, mic muteado…).
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
            // Etiqueta dinámica: el toggle dice lo que va a HACER, no lo que es.
            let label = match (action.as_str(), on) {
                ("obs:record", Some(true)) => "Detener grab.",
                ("obs:stream", Some(true)) => "Cortar directo",
                _ => label.as_str(),
            };
            let mut j = json!({ "id": i, "label": label, "action": action, "icon": icon, "danger": danger, "recommended": rec });
            if let Some(on) = on {
                j["on"] = json!(on);
            }
            j
        })
        .collect();
    // Botones de escena autogenerados desde OBS: el usuario ve SUS escenas sin configurar nada.
    if profile_id == "obs" && obs.connected {
        for (i, s) in obs.scenes.iter().enumerate() {
            buttons.push(json!({
                "id": 1000 + i, "label": s, "action": format!("obs:scene:{s}"), "icon": "scene",
                "danger": false, "recommended": true, "on": *s == obs.current_scene
            }));
        }
    }
    // Volumen del sistema para el dock del móvil (slider). null si no hay dispositivo de audio.
    let sys = system_volume()
        .map(|(vol, muted)| json!({ "vol": vol, "muted": muted }))
        .unwrap_or(serde_json::Value::Null);
    json!({
        "v": 1, "type": "layout", "profileId": profile_id,
        "appName": app, "appIcon": icon_b64, "buttons": buttons, "sys": sys
    })
    .to_string()
}

/// Devuelve las acciones uia: cuyo control está deshabilitado en la ventana al frente (para ocultarlas).
/// Vacío si el perfil no tiene botones uia → coste cero para apps normales.
fn uia_disabled_actions(
    buttons: &[(usize, String, String, String, bool, bool)],
) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    use uiautomation::types::Handle;
    use uiautomation::UIAutomation;
    let mut out = HashSet::new();
    // (acción completa, nombre del primer paso) de cada botón uia.
    let targets: Vec<(&str, &str)> = buttons
        .iter()
        .filter_map(|(_, _, action, _, _, _)| {
            action
                .strip_prefix("uia:")
                .map(|c| (action.as_str(), c.split(">>").next().unwrap_or(c).trim()))
        })
        .collect();
    if targets.is_empty() {
        return out;
    }
    let automation = match UIAutomation::new() {
        Ok(a) => a,
        Err(_) => return out,
    };
    let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    let root = match automation.element_from_handle(Handle::from(hwnd.0 as isize)) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for (action, name) in targets {
        if let Ok(el) = automation
            .create_matcher()
            .from_ref(&root)
            .name(name)
            .depth(20)
            .timeout(200)
            .find_first()
        {
            if !el.is_enabled().unwrap_or(true) {
                out.insert(action.to_string());
            }
        }
    }
    out
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
        let layout = {
            let (a, t, ic) = (app.clone(), title.clone(), icon.clone());
            tokio::task::spawn_blocking(move || layout_for(&a, &t, &ic))
                .await
                .unwrap_or_default()
        };
        if !layout.is_empty() && layout != last_layout {
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
        Some("list_windows") => {
            if !*authed {
                return json!({"v":1,"type":"windows","items":[]}).to_string();
            }
            list_windows_json()
        }
        // Perfil escaneado de un QR "kbprofile:" en otro KiBoard: se añade al catálogo local.
        Some("import_profile") => {
            if !*authed {
                return json!({"v":1,"type":"command_result","ok":false,"error":"not_paired"}).to_string();
            }
            let Ok(p) = serde_json::from_value::<Profile>(val["profile"].clone()) else {
                return json!({"v":1,"type":"command_result","ok":false,"error":"bad_profile"}).to_string();
            };
            let mut cfg = config().lock().unwrap();
            cfg.profiles.retain(|q| q.id != p.id); // re-importar el mismo id lo reemplaza
            cfg.profiles.insert(0, p); // al frente: gana el matching sobre los genéricos
            cfg.save();
            json!({"v":1,"type":"command_result","ok":true,"imported":true}).to_string()
        }
        Some("focus_window") => {
            if !*authed {
                return json!({"v":1,"type":"command_result","ok":false,"error":"not_paired"}).to_string();
            }
            let id = val["id"].as_i64().unwrap_or(0) as isize;
            focus_window(id);
            json!({"v":1,"type":"command_result","ok":true}).to_string()
        }
        _ => json!({"v":1,"type":"command_result","ok":false,"error":"unknown_type"}).to_string(),
    }
}

// ---------------------------------------------------------------------------
// Volumen maestro del sistema (Core Audio) — para el slider del dock del móvil
// ---------------------------------------------------------------------------

/// Ejecuta `f` con el IAudioEndpointVolume del dispositivo de salida por defecto.
fn with_endpoint_volume<T>(
    f: impl FnOnce(&windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume) -> windows::core::Result<T>,
) -> Option<T> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    unsafe {
        // ponytail: COM init/uninit por llamada; a 2Hz el coste es irrelevante.
        let init = CoInitializeEx(None, COINIT_MULTITHREADED);
        let out = (|| -> windows::core::Result<T> {
            let enu: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let dev = enu.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None)?;
            f(&vol)
        })();
        if init.is_ok() {
            CoUninitialize();
        }
        out.ok()
    }
}

/// (volumen 0-100, muteado) del dispositivo de salida por defecto.
fn system_volume() -> Option<(u8, bool)> {
    with_endpoint_volume(|v| unsafe {
        let level = v.GetMasterVolumeLevelScalar()?;
        let muted = v.GetMute()?.as_bool();
        Ok(((level * 100.0).round() as u8, muted))
    })
}

/// Fija el volumen maestro (0.0–1.0) y des-mutea (mover el slider = quiero oír).
fn set_system_volume(level: f32) -> Result<(), &'static str> {
    with_endpoint_volume(|v| unsafe {
        v.SetMasterVolumeLevelScalar(level, std::ptr::null())?;
        v.SetMute(false, std::ptr::null())
    })
    .ok_or("sin_audio")
}

// ---------------------------------------------------------------------------
// OBS (obs-websocket v5, integrado en OBS ≥ 28, puerto 4455)
// ---------------------------------------------------------------------------
// Cliente permanente con reconexión. El estado (grabando/directo/mic/escenas) vive en OBS_STATE;
// layout_for lo lee en cada sondeo (500ms) → los botones del móvil muestran estado en vivo sin
// añadir mensajes nuevos al protocolo host↔móvil.
// ponytail: puerto fijo 4455 (default de OBS); hacerlo configurable si algún usuario lo pide.

#[derive(Default, Clone)]
struct ObsState {
    connected: bool,
    recording: bool,
    streaming: bool,
    replay_active: bool, // buffer de repetición activo
    mic_muted: bool,
    mic_name: String, // primer mic de GetSpecialInputs
    scenes: Vec<String>,
    current_scene: String,
}

static OBS_STATE: OnceLock<Mutex<ObsState>> = OnceLock::new();
fn obs_state() -> &'static Mutex<ObsState> {
    OBS_STATE.get_or_init(|| Mutex::new(ObsState::default()))
}
static OBS_TX: OnceLock<tokio::sync::mpsc::UnboundedSender<String>> = OnceLock::new();
static OBS_REQ_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Mapea una acción "obs:<cmd>" al request obs-websocket. Pura para poder testearla.
fn obs_request_for(cmd: &str, mic_name: &str) -> Result<(&'static str, serde_json::Value), &'static str> {
    match cmd {
        "record" => Ok(("ToggleRecord", json!({}))),
        "stream" => Ok(("ToggleStream", json!({}))),
        "replay" => Ok(("SaveReplayBuffer", json!({}))),
        "replaybuffer" => Ok(("ToggleReplayBuffer", json!({}))),
        "mic" => {
            if mic_name.is_empty() {
                return Err("obs_sin_mic");
            }
            Ok(("ToggleInputMute", json!({ "inputName": mic_name })))
        }
        s => match s.strip_prefix("scene:") {
            Some(name) if !name.is_empty() => {
                Ok(("SetCurrentProgramScene", json!({ "sceneName": name })))
            }
            _ => Err("obs_accion_desconocida"),
        },
    }
}

/// Encola un request hacia OBS (fuego y olvido: el resultado vuelve como evento de estado).
fn obs_send(request_type: &str, data: serde_json::Value) -> Result<(), &'static str> {
    if !obs_state().lock().unwrap().connected {
        return Err("obs_desconectado");
    }
    let id = OBS_REQ_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let msg = json!({
        "op": 6,
        "d": { "requestType": request_type, "requestId": id.to_string(), "requestData": data }
    })
    .to_string();
    OBS_TX.get().ok_or("obs_desconectado")?.send(msg).map_err(|_| "obs_desconectado")
}

fn obs_action(cmd: &str) -> Result<(), &'static str> {
    let mic = obs_state().lock().unwrap().mic_name.clone();
    let (rtype, data) = obs_request_for(cmd, &mic)?;
    obs_send(rtype, data)
}

/// Auth v5: b64(sha256(b64(sha256(password+salt)) + challenge)).
fn obs_auth(password: &str, salt: &str, challenge: &str) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let b64 = base64::engine::general_purpose::STANDARD;
    let secret = b64.encode(Sha256::digest(format!("{password}{salt}")));
    b64.encode(Sha256::digest(format!("{secret}{challenge}")))
}

/// Bucle permanente: conecta, sirve, y al caerse limpia el estado y reintenta a los 5s.
async fn obs_client_loop(mut rx: tokio::sync::mpsc::UnboundedReceiver<String>) {
    loop {
        // Descartar comandos encolados mientras estuvimos caídos (evita toggles fantasma al volver).
        while rx.try_recv().is_ok() {}
        let _ = obs_serve(&mut rx).await;
        *obs_state().lock().unwrap() = ObsState::default();
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn obs_serve(rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>) -> Result<(), ()> {
    use futures_util::{SinkExt, StreamExt};
    let (ws, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:4455")
        .await
        .map_err(|_| ())?;
    let (mut write, mut read) = ws.split();

    // Hello (op 0) → Identify (op 1) con auth si el server la exige.
    let hello = obs_next_json(&mut read).await.ok_or(())?;
    let mut identify = json!({
        "rpcVersion": 1,
        // Scenes(4) | Inputs(8) | Outputs(64): escenas, mute de mic, estado de grabación/directo.
        "eventSubscriptions": 76
    });
    if let Some(auth) = hello["d"].get("authentication") {
        let password = config().lock().unwrap().obs_password.clone();
        let salt = auth["salt"].as_str().unwrap_or("");
        let challenge = auth["challenge"].as_str().unwrap_or("");
        identify["authentication"] = json!(obs_auth(&password, salt, challenge));
    }
    write
        .send(Message::text(json!({"op": 1, "d": identify}).to_string()))
        .await
        .map_err(|_| ())?;
    // Identified (op 2); con contraseña mala OBS cierra el socket y caemos al retry.
    let identified = obs_next_json(&mut read).await.ok_or(())?;
    if identified["op"].as_i64() != Some(2) {
        return Err(());
    }
    obs_state().lock().unwrap().connected = true;

    // Estado inicial (las respuestas op 7 lo van rellenando).
    for req in ["GetSceneList", "GetSpecialInputs", "GetRecordStatus", "GetStreamStatus", "GetReplayBufferStatus"] {
        let _ = obs_send(req, json!({}));
    }

    loop {
        tokio::select! {
            cmd = rx.recv() => {
                let Some(cmd) = cmd else { return Err(()) };
                write.send(Message::text(cmd)).await.map_err(|_| ())?;
            }
            msg = read.next() => {
                let Some(Ok(msg)) = msg else { return Err(()) };
                if msg.is_close() { return Err(()); }
                if !msg.is_text() { continue; }
                let txt = msg.into_text().map(|t| t.to_string()).unwrap_or_default();
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                    obs_apply(&v);
                }
            }
        }
    }
}

async fn obs_next_json<S>(read: &mut S) -> Option<serde_json::Value>
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    use futures_util::StreamExt;
    loop {
        let msg = read.next().await?.ok()?;
        if msg.is_close() {
            return None;
        }
        if !msg.is_text() {
            continue;
        }
        let txt = msg.into_text().ok()?.to_string();
        if let Ok(v) = serde_json::from_str(&txt) {
            return Some(v);
        }
    }
}

/// Aplica un evento (op 5) o respuesta (op 7) de OBS al estado compartido.
fn obs_apply(v: &serde_json::Value) {
    // Request de seguimiento a enviar DESPUÉS de soltar el lock (obs_send también lo toma).
    let mut followup: Option<(&'static str, serde_json::Value)> = None;
    {
        let mut st = obs_state().lock().unwrap();
        match v["op"].as_i64() {
            Some(5) => {
                let d = &v["d"]["eventData"];
                match v["d"]["eventType"].as_str().unwrap_or("") {
                    "RecordStateChanged" => st.recording = d["outputActive"].as_bool().unwrap_or(st.recording),
                    "StreamStateChanged" => st.streaming = d["outputActive"].as_bool().unwrap_or(st.streaming),
                    "ReplayBufferStateChanged" => st.replay_active = d["outputActive"].as_bool().unwrap_or(st.replay_active),
                    "InputMuteStateChanged" => {
                        if d["inputName"].as_str() == Some(st.mic_name.as_str()) {
                            st.mic_muted = d["inputMuted"].as_bool().unwrap_or(st.mic_muted);
                        }
                    }
                    "CurrentProgramSceneChanged" => {
                        st.current_scene = d["sceneName"].as_str().unwrap_or("").to_string();
                    }
                    "SceneListChanged" => st.scenes = obs_scene_names(&d["scenes"]),
                    _ => {}
                }
            }
            Some(7) => {
                let d = &v["d"]["responseData"];
                match v["d"]["requestType"].as_str().unwrap_or("") {
                    "GetSceneList" => {
                        st.scenes = obs_scene_names(&d["scenes"]);
                        st.current_scene = d["currentProgramSceneName"].as_str().unwrap_or("").to_string();
                    }
                    "GetSpecialInputs" => {
                        st.mic_name = ["mic1", "mic2", "mic3", "mic4"]
                            .iter()
                            .find_map(|k| d[k].as_str().filter(|s| !s.is_empty()))
                            .unwrap_or("")
                            .to_string();
                        if st.mic_name.is_empty() {
                            // Sin mic global: buscar un input de micrófono en las escenas.
                            followup = Some(("GetInputList", json!({})));
                        } else {
                            followup = Some(("GetInputMute", json!({ "inputName": st.mic_name })));
                        }
                    }
                    "GetInputList" => {
                        // Fallback: primer input de captura de micrófono (wasapi_input_capture).
                        st.mic_name = d["inputs"]
                            .as_array()
                            .and_then(|a| {
                                a.iter().find(|i| {
                                    i["inputKind"].as_str().unwrap_or("").contains("input_capture")
                                })
                            })
                            .and_then(|i| i["inputName"].as_str())
                            .unwrap_or("")
                            .to_string();
                        if !st.mic_name.is_empty() {
                            followup = Some(("GetInputMute", json!({ "inputName": st.mic_name })));
                        }
                    }
                    "GetRecordStatus" => st.recording = d["outputActive"].as_bool().unwrap_or(false),
                    "GetStreamStatus" => st.streaming = d["outputActive"].as_bool().unwrap_or(false),
                    "GetReplayBufferStatus" => st.replay_active = d["outputActive"].as_bool().unwrap_or(false),
                    "GetInputMute" => st.mic_muted = d["inputMuted"].as_bool().unwrap_or(false),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if let Some((rtype, data)) = followup {
        let _ = obs_send(rtype, data);
    }
}

/// GetSceneList devuelve las escenas de abajo hacia arriba → invertir para el orden de la UI de OBS.
fn obs_scene_names(scenes: &serde_json::Value) -> Vec<String> {
    scenes
        .as_array()
        .map(|a| {
            a.iter()
                .rev()
                .filter_map(|s| s["sceneName"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Ejecución de acciones (atajos arbitrarios) sobre la ventana en primer plano
// ---------------------------------------------------------------------------

fn run_action(action: &str) -> Result<(), &'static str> {
    // "uia:<nombre>" → pulsar un botón de la app en primer plano por su nombre de accesibilidad
    // (para barras de herramientas sin atajo de teclado, p. ej. Paint).
    // Si la acción EMPIEZA por "uia:", toda la cadena ">>" son pasos UIA (menú/flyout → opción):
    // "uia:Girar>>Girar 90° a la derecha" → abre "Girar", espera, clica la opción.
    if let Some(chain) = action.strip_prefix("uia:") {
        for (i, step) in chain.split(">>").enumerate() {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(250)); // que el flyout se renderice
            }
            invoke_uia(step.trim())?;
        }
        return Ok(());
    }
    // Macro general: pasos separados por ">>", cada uno un atajo, "type:", "uia:" o "screenshot".
    // Ej.: "ctrl+c>>alt+tab>>ctrl+v". ponytail: un snippet type: no puede contener ">>".
    for (i, step) in action.split(">>").enumerate() {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(250)); // que la app procese el paso previo
        }
        run_step(step.trim())?;
    }
    Ok(())
}

/// Un paso atómico: captura, botón UIA, texto literal o atajo de teclado.
fn run_step(step: &str) -> Result<(), &'static str> {
    if step == "screenshot" {
        return take_screenshot();
    }
    if let Some(name) = step.strip_prefix("uia:") {
        return invoke_uia(name.trim());
    }
    // "obs:<cmd>" → request al obs-websocket (funciona con el juego en primer plano).
    if let Some(cmd) = step.strip_prefix("obs:") {
        return obs_action(cmd);
    }
    // "vol:<0-100>" → volumen maestro absoluto (el slider del dock).
    if let Some(v) = step.strip_prefix("vol:") {
        let pct: f32 = v.trim().parse().map_err(|_| "bad_vol")?;
        return set_system_volume((pct / 100.0).clamp(0.0, 1.0));
    }
    // "type:<texto>" → escribe el texto literal (snippets: respuestas enlatadas, emails, fórmulas).
    if let Some(text) = step.strip_prefix("type:") {
        use enigo::{Enigo, Keyboard, Settings};
        let mut e = Enigo::new(&Settings::default()).map_err(|_| "internal")?;
        return e.text(text).map_err(|_| "internal");
    }
    run_hotkey(step)
}

/// Localiza por nombre (coincidencia parcial) un elemento de la ventana en primer plano y lo clica.
/// ponytail: busca desde el root y toma el primer match; basta porque la app objetivo está al frente.
fn invoke_uia(name: &str) -> Result<(), &'static str> {
    use uiautomation::types::Handle;
    use uiautomation::UIAutomation;
    let automation = UIAutomation::new().map_err(|_| "uia_init")?;
    let desktop = automation.get_root_element().map_err(|_| "uia_root")?;
    // Acotar a la ventana en primer plano (árbol pequeño → rápido). Si ahí no aparece (un menú/
    // flyout emergente puede vivir en otra ventana), reintentar desde el escritorio.
    let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    let fg = automation.element_from_handle(Handle::from(hwnd.0 as isize)).ok();
    // Candidatos por raíz: nombre exacto primero ("Rectángulo" no choca con "Rectángulo
    // redondeado"); si no hay, coincidencia parcial.
    let gather = |root: &uiautomation::UIElement| -> Vec<uiautomation::UIElement> {
        let exact = automation
            .create_matcher()
            .from_ref(root)
            .name(name)
            .depth(20)
            .timeout(600)
            .find_all()
            .unwrap_or_default();
        if !exact.is_empty() {
            return exact;
        }
        automation
            .create_matcher()
            .from_ref(root)
            .contains_name(name)
            .depth(20)
            .timeout(600)
            .find_all()
            .unwrap_or_default()
    };
    let mut candidates = fg.as_ref().map(&gather).unwrap_or_default();
    if candidates.is_empty() {
        candidates = gather(&desktop);
    }
    if candidates.is_empty() {
        return Err("uia_not_found");
    }
    // Activar por patrón UIA, NO con un clic de ratón (el.click() movería el cursor).
    // Varias apps duplican el nombre: un contenedor (GridViewItem, solo Legacy) y el botón
    // real (Toggle/Invoke). Preferimos el candidato que exponga un patrón accionable.
    use uiautomation::patterns::{
        UIInvokePattern, UILegacyIAccessiblePattern, UISelectionItemPattern, UITogglePattern,
    };
    // Preferir candidatos HABILITADOS (Paint deshabilita "Tamaño"/"Recortar" según el contexto:
    // un control deshabilitado "se invoca" pero no hace nada → parecería roto).
    let mut saw_disabled = false;
    for el in &candidates {
        if !el.is_enabled().unwrap_or(true) {
            saw_disabled = true;
            continue;
        }
        if let Ok(p) = el.get_pattern::<UIInvokePattern>() {
            return p.invoke().map_err(|_| "uia_invoke");
        }
        if let Ok(p) = el.get_pattern::<UITogglePattern>() {
            return p.toggle().map_err(|_| "uia_toggle");
        }
        if let Ok(p) = el.get_pattern::<UISelectionItemPattern>() {
            return p.select().map_err(|_| "uia_select");
        }
    }
    // Ningún candidato accionable habilitado: "default action" de MSAA (sin mover el ratón).
    for el in &candidates {
        if el.is_enabled().unwrap_or(true) {
            if let Ok(p) = el.get_pattern::<UILegacyIAccessiblePattern>() {
                return p.do_default_action().map_err(|_| "uia_legacy");
            }
        }
    }
    // Solo había coincidencias deshabilitadas: avisar al teléfono en vez de no hacer nada.
    if saw_disabled {
        return Err("uia_disabled");
    }
    candidates[0].click().map_err(|_| "uia_click") // ponytail: último recurso; este sí mueve el ratón
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
        // Teclas multimedia del SISTEMA: funcionan en cualquier app (control remoto de sofá).
        "volup" => Ok(Key::VolumeUp),
        "voldown" => Ok(Key::VolumeDown),
        "volmute" => Ok(Key::VolumeMute),
        "playpause" => Ok(Key::MediaPlayPause),
        "nexttrack" => Ok(Key::MediaNextTrack),
        "prevtrack" => Ok(Key::MediaPrevTrack),
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
// Cambiador de apps: enumerar ventanas visibles y enfocarlas (Win32)
// ---------------------------------------------------------------------------

unsafe extern "system" fn enum_windows_cb(
    hwnd: windows::Win32::Foundation::HWND,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::core::BOOL {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible, GWL_EXSTYLE,
        WS_EX_TOOLWINDOW,
    };
    let list = &mut *(lparam.0 as *mut Vec<(isize, String)>);
    if !IsWindowVisible(hwnd).as_bool() {
        return windows::core::BOOL(1);
    }
    // Ventanas "cloaked" (UWP suspendidas: Configuración, Tienda…): Windows las lista como
    // visibles pero NO están en pantalla — son los fantasmas duplicados del cambiador.
    let mut cloaked: u32 = 0;
    let _ = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut _ as *mut core::ffi::c_void,
        std::mem::size_of::<u32>() as u32,
    );
    if cloaked != 0 {
        return windows::core::BOOL(1);
    }
    // Tool windows (paletas flotantes, overlays) no aparecen en el alt-tab de Windows: fuera.
    if (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32) & WS_EX_TOOLWINDOW.0 != 0 {
        return windows::core::BOOL(1);
    }
    let len = GetWindowTextLengthW(hwnd);
    if len > 0 {
        let mut buf = vec![0u16; len as usize + 1];
        let read = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..read as usize]);
        if !title.is_empty() && title != "Program Manager" {
            list.push((hwnd.0 as isize, title));
        }
    }
    windows::core::BOOL(1)
}

/// Ruta del ejecutable dueño de una ventana (para sacarle el ícono).
fn window_process_path(hwnd: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return String::new();
        }
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = windows::Win32::Foundation::CloseHandle(h);
        match ok {
            Ok(()) => String::from_utf16_lossy(&buf[..len as usize]),
            Err(_) => String::new(),
        }
    }
}

/// Ícono por ruta de exe con caché (extraerlo es caro y los íconos no cambian).
fn icon_cached(path: &str) -> String {
    use std::collections::HashMap;
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    if path.is_empty() {
        return String::new();
    }
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(b) = cache.lock().unwrap().get(path) {
        return b.clone();
    }
    let b = extract_icon_b64(path);
    cache.lock().unwrap().insert(path.to_string(), b.clone());
    b
}

/// Lista las ventanas de apps abiertas (JSON `windows`), con el ícono real de cada exe.
fn list_windows_json() -> String {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
    let mut list: Vec<(isize, String)> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_windows_cb), LPARAM(&mut list as *mut _ as isize));
    }
    let items: Vec<_> = list
        .into_iter()
        .map(|(id, title)| {
            let path = window_process_path(HWND(id as *mut core::ffi::c_void));
            json!({ "id": id, "title": title, "icon": icon_cached(&path) })
        })
        .collect();
    json!({ "v": 1, "type": "windows", "items": items }).to_string()
}

/// Trae una ventana al primer plano (la restaura si está minimizada).
/// Windows bloquea el robo de foco desde un proceso en segundo plano; lo sorteamos con un
/// toque de Alt + AttachThreadInput (técnica estándar) para forzar el primer plano de verdad.
fn focus_window(id: isize) {
    use enigo::{
        Direction::{Press, Release},
        Enigo, Key, Keyboard, Settings,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };
    // Mantener Alt presionado durante el cambio sortea el bloqueo de primer plano de Windows.
    let mut enigo = Enigo::new(&Settings::default()).ok();
    if let Some(e) = enigo.as_mut() {
        let _ = e.key(Key::Alt, Press);
    }
    unsafe {
        let h = HWND(id as *mut core::ffi::c_void);
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let _ = ShowWindow(h, SW_SHOW);
        let _ = BringWindowToTop(h);
        let _ = SetForegroundWindow(h);
    }
    if let Some(e) = enigo.as_mut() {
        let _ = e.key(Key::Alt, Release);
    }
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

/// Estado de la integración OBS para la UI del host.
#[tauri::command]
fn obs_info() -> serde_json::Value {
    let (connected, scenes) = {
        let st = obs_state().lock().unwrap();
        (st.connected, st.scenes.len())
    };
    let password_set = !config().lock().unwrap().obs_password.is_empty();
    json!({ "connected": connected, "scenes": scenes, "passwordSet": password_set })
}

/// Guarda la contraseña del WebSocket de OBS; el cliente la usa en el próximo (re)intento (≤5s).
#[tauri::command]
fn set_obs_password(password: String) -> serde_json::Value {
    let mut cfg = config().lock().unwrap();
    cfg.obs_password = password.trim().to_string();
    cfg.save();
    json!({ "ok": true })
}

/// QR de un perfil para compartirlo: otro KiBoard lo escanea desde el móvil y lo importa.
#[tauri::command]
fn profile_qr(profile: Profile) -> String {
    let payload = format!("kbprofile:{}", serde_json::to_string(&profile).unwrap_or_default());
    qr_svg(&payload)
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
    use super::{obs_request_for, parse_key};

    #[test]
    fn parsea_teclas() {
        assert!(parse_key("c").is_ok());
        assert!(parse_key("F4").is_ok());
        assert!(parse_key("enter").is_ok());
        assert!(parse_key("left").is_ok());
        assert!(parse_key("nope").is_err());
        // Teclas multimedia del sistema
        for t in ["volup", "voldown", "volmute", "playpause", "nexttrack", "prevtrack"] {
            assert!(parse_key(t).is_ok(), "token multimedia {t}");
        }
    }

    #[test]
    fn mapea_acciones_obs() {
        assert_eq!(obs_request_for("record", "").unwrap().0, "ToggleRecord");
        assert_eq!(obs_request_for("stream", "").unwrap().0, "ToggleStream");
        assert_eq!(obs_request_for("replay", "").unwrap().0, "SaveReplayBuffer");
        let (t, d) = obs_request_for("scene:Gameplay", "").unwrap();
        assert_eq!(t, "SetCurrentProgramScene");
        assert_eq!(d["sceneName"], "Gameplay");
        let (t, d) = obs_request_for("mic", "Mic/Aux").unwrap();
        assert_eq!(t, "ToggleInputMute");
        assert_eq!(d["inputName"], "Mic/Aux");
        assert!(obs_request_for("mic", "").is_err()); // sin mic detectado
        assert!(obs_request_for("scene:", "").is_err());
        assert!(obs_request_for("nope", "").is_err());
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
            save_profiles,
            profile_qr,
            obs_info,
            set_obs_password
        ])
        // La X oculta la ventana (la app vive en el tray); sin esto la destruye y sale la app.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            tauri::async_runtime::spawn(run_ws_server());
            tauri::async_runtime::spawn(watch_active_app());
            let (obs_tx, obs_rx) = tokio::sync::mpsc::unbounded_channel();
            let _ = OBS_TX.set(obs_tx);
            tauri::async_runtime::spawn(obs_client_loop(obs_rx));

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
                // Click primario → abre la ventana (QR) directo; el menú queda solo en el secundario.
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
