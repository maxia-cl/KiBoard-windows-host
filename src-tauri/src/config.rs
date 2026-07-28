//! Persisted config: the ~100-profile catalogue, per-device pairing list, OBS/analytics settings,
//! and the v2 `Deck`/`Page`/`Key` model for manual mode (protocol/README.md §3).
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Button {
    pub(crate) label: String,
    pub(crate) icon: String,
    /// A shortcut ("ctrl+shift+p", "alt+F4", "ctrl+c") or the keyword "screenshot".
    pub(crate) action: String,
    /// Dangerous action: the phone paints it red and asks for confirmation.
    #[serde(default)]
    pub(crate) danger: bool,
    /// Whether it's in the profile's default recommended selection (the rest are available extras).
    #[serde(default = "yes")]
    pub(crate) recommended: bool,
}

fn yes() -> bool { true }

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Profile {
    pub(crate) id: String,
    /// Substrings that must appear in the active app's name OR the window title (this enables
    /// per-tab sub-profiles: "Google Sheets", "Google Drive"...). Empty = fallback.
    #[serde(default)]
    pub(crate) matches: Vec<String>,
    pub(crate) buttons: Vec<Button>,
}

// ---------------------------------------------------------------------------
// v2 manual-mode model: Deck > Page > Key (protocol/README.md §3)
// ---------------------------------------------------------------------------

/// What a key does when pressed. `folder` and `page` both navigate; the difference is only how the
/// client animates it (a folder dives in and offers Back, a page is a sibling swipe).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum KeyKind {
    Action,
    Folder,
    Page,
    /// A hole in the grid. Carries no label and executes nothing.
    #[default]
    Empty,
}

/// One key on the grid. The stored shape and the wire shape are deliberately the same struct: a
/// `layout` message is this serialized as-is, so the editor cannot drift from the protocol.
///
/// `pos = row * cols + col`, and it is an index into the DECK's own grid, not the client's — the
/// host repaginates for whatever grid the client declared in `hello`.
#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct Key {
    pub(crate) pos: usize,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) icon: String,
    /// Real app icon or a custom image, as a `data:` URI. Set at runtime by F4's icon cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) color: Option<String>,
    /// Short press. `None` for `folder`/`page`/`empty` keys, which navigate via `target`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hold: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) double: Option<String>,
    /// Page this key navigates to, for `kind: folder | page`. Names a `Page::id` in the same deck.
    ///
    /// PROTOCOL AMENDMENT (F2): the draft's fixtures show `folder` keys with no destination
    /// because the FP mock-up hardcoded the jump client-side. Real navigation has to name a page,
    /// so `target` is added rather than overloading `action` with a second grammar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) danger: bool,
    /// Live state (recording, app running, mic muted). Filled in per-send, never persisted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) state: Option<serde_json::Value>,
    pub(crate) kind: KeyKind,
}

impl Key {
    /// The action for a press type, or `None` if that press is unbound. `long`/`double` do NOT
    /// fall back to the short action: a key that only defines `action` must ignore a long press
    /// rather than fire twice on an accidental hold.
    pub(crate) fn action_for(&self, press: &str) -> Option<&str> {
        match press {
            "short" => self.action.as_deref(),
            "long" => self.hold.as_deref(),
            "double" => self.double.as_deref(),
            _ => None,
        }
    }
}

/// A grid's worth of keys. `id` exists so `folder`/`page` keys can name a destination.
#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct Page {
    pub(crate) id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) name: String,
    pub(crate) keys: Vec<Key>,
}

/// A deck the user picks in manual mode. `pages[0]` is the entry page.
#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct Deck {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) icon: String,
    pub(crate) pages: Vec<Page>,
}

impl Deck {
    pub(crate) fn page(&self, id: &str) -> Option<&Page> {
        self.pages.iter().find(|p| p.id == id)
    }
}

fn b(label: &str, icon: &str, action: &str) -> Button {
    Button { label: label.into(), icon: icon.into(), action: action.into(), danger: false, recommended: true }
}
fn bd(label: &str, icon: &str, action: &str) -> Button {
    Button { label: label.into(), icon: icon.into(), action: action.into(), danger: true, recommended: true }
}
/// "Extra" button: available in the profile but outside the default recommended selection.
fn bx(label: &str, icon: &str, action: &str) -> Button {
    Button { label: label.into(), icon: icon.into(), action: action.into(), danger: false, recommended: false }
}
/// "Extra" and dangerous button: outside the default selection + red with confirmation (e.g. Delete).
fn bxd(label: &str, icon: &str, action: &str) -> Button {
    Button { label: label.into(), icon: icon.into(), action: action.into(), danger: true, recommended: false }
}
fn profile(id: &str, matches: &[&str], buttons: Vec<Button>) -> Profile {
    Profile { id: id.into(), matches: matches.iter().map(|s| s.to_string()).collect(), buttons }
}
/// Default profile catalogue. Order matters: the most specific ones (Chrome tabs) go first, so
/// they win over the generic "browser" profile. Every profile gets a "Close app" button appended
/// at the end (red + confirmation). All of them are editable from the host UI.
pub(crate) fn default_profiles() -> Vec<Profile> {
    let mut list = vec![
        // --- Terminal AI agents (Claude Code, Codex, aider, Gemini CLI) ---
        // Matched by TITLE: these CLIs set the terminal title to their own name (verified:
        // Claude Code sets it to "Claude"). Goes FIRST to win over the "terminal" profile
        // (which matches the "windows terminal" app) and the generic one. The flow is universal
        // in a terminal: up/down arrows pick the alternative, Enter accepts, Esc rejects.
        // ponytail: matched by title; if a browser AI chat (claude.ai) matches too, the user can
        // fine-tune the "matches" from the host UI.
        profile("ai", &["claude", "codex", "aider", "gemini"], vec![
            b("Aceptar", "accept", "enter"), b("Rechazar", "close", "esc"),
            b("Subir", "scrollup", "up"), b("Bajar", "scrolldown", "down"),
            // Model/Effort: picker ON THE PHONE (picker:) — the chosen option types the
            // slash-command with the argument directly (/model <id>, /effort <level>).
            // NOTE (real bug, 2026-07): the command and the argument go in SEPARATE type: steps
            // with a wait in between. In one shot ("type:/effort low") the TUI's autocomplete
            // eats the space and you get "/effortlow" -> unknown command.
            b("Modelo", "model",
              "picker:Fable=type:/model>>wait:400>>type: claude-fable-5>>wait:250>>enter;Opus=type:/model>>wait:400>>type: claude-opus-4-8>>wait:250>>enter;Sonnet=type:/model>>wait:400>>type: sonnet>>wait:250>>enter;Haiku=type:/model>>wait:400>>type: haiku>>wait:250>>enter"),
            b("Esfuerzo", "effort",
              "picker:Low=type:/effort>>wait:400>>type: low>>wait:250>>enter;Medium=type:/effort>>wait:400>>type: medium>>wait:250>>enter;High=type:/effort>>wait:400>>type: high>>wait:250>>enter;Max=type:/effort>>wait:400>>type: max>>wait:250>>enter"),
            // Mode: opens the app's mode menu and is picked with Up/Down/Accept from this same
            // button pad. NOTE: ctrl+alt+m is NOT a stock shortcut — it's a user keybinding
            // (~/.claude/keybindings.json -> "ctrl+alt+m": "chat:cycleMode"); in the desktop app
            // that action opens the mode menu. shift+tab does focus navigation there (why it
            // never worked), and there's no slash-command or per-mode action.
            b("Modo", "mode", "ctrl+alt+m"),
            bx("Nueva línea", "text", "shift+enter"),
            bx("Copiar", "copy", "ctrl+shift+c"), bx("Pegar", "paste", "ctrl+shift+v"),
            bx("Dictar", "mic", "dictate"),
        ]),
        // --- Chrome/browser tabs (matched by window TITLE) ---
        profile("gsheets", &["google sheets", "hojas de cálculo"], vec![
            b("Negrita", "bold", "ctrl+b"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"), b("Rehacer", "redo", "ctrl+y"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Cursiva", "italic", "ctrl+i"), bx("Subrayado", "underline", "ctrl+u"),
            bx("Cortar", "cut", "ctrl+x"), bx("Imprimir", "print", "ctrl+p"),
            bx("Dictar", "mic", "dictate"),
        ]),
        profile("gdocs", &["google docs", "documentos de google"], vec![
            b("Negrita", "bold", "ctrl+b"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"), b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Cursiva", "italic", "ctrl+i"), bx("Subrayado", "underline", "ctrl+u"),
            bx("Enlace", "link", "ctrl+k"), bx("Rehacer", "redo", "ctrl+y"), bx("Imprimir", "print", "ctrl+p"),
            bx("Sel. todo", "selectall", "ctrl+a"), bxd("Eliminar", "delete", "delete"),
            bx("Dictar", "mic", "dictate"),
        ]),
        profile("gdrive", &["google drive", "mi unidad"], vec![
            b("Buscar", "find", "ctrl+f"), b("Nueva pestaña", "new", "ctrl+t"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Nueva carpeta", "newfolder", "shift+f"), bx("Renombrar", "rename", "n"),
            bx("Sel. todo", "selectall", "ctrl+a"), bxd("Eliminar", "delete", "delete"),
        ]),
        profile("gmail", &["gmail"], vec![
            b("Redactar", "new", "c"), b("Buscar", "find", "/"),
            b("Responder", "reply", "r"), b("Archivar", "archive", "e"),
            bx("Resp. todos", "replyall", "a"), bx("Reenviar", "forward", "f"),
            bx("Eliminar", "delete", "#"), bx("Destacar", "star", "s"), bx("Enviar", "send", "ctrl+enter"),
            bx("Dictar", "mic", "dictate"),
        ]),
        profile("youtube", &["youtube"], vec![
            b("Play/Pausa", "play", "k"), b("Silenciar", "mute", "m"),
            b("Pantalla completa", "video", "f"),
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
            bx("Sel. todo", "selectall", "ctrl+a"), bxd("Eliminar", "delete", "delete"),
            bx("Dictar", "mic", "dictate"),
        ]),
        profile("excel", &["excel"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Deshacer", "undo", "ctrl+z"), b("Rehacer", "redo", "ctrl+y"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Negrita", "bold", "ctrl+b"), bx("Cursiva", "italic", "ctrl+i"),
            bx("Autosuma", "sum", "alt+="), bx("Filtro", "filter", "ctrl+shift+l"), bx("Imprimir", "print", "ctrl+p"),
            bx("Dictar", "mic", "dictate"),
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
            bx("Dictar", "mic", "dictate"),
        ]),
        profile("acrobat", &["acrobat"], vec![
            b("Buscar", "find", "ctrl+f"), b("Imprimir", "print", "ctrl+p"),
            b("Guardar", "save", "ctrl+s"), b("Copiar", "copy", "ctrl+c"),
            bx("Zoom +", "zoomin", "ctrl+add"), bx("Zoom -", "zoomout", "ctrl+subtract"),
            bx("Pant. completa", "fullscreen", "ctrl+l"),
        ]),
        // --- Creative ---
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
        // --- Communication / meetings ---
        profile("slack", &["slack"], vec![
            b("Saltar a", "find", "ctrl+k"), b("Negrita", "bold", "ctrl+b"),
            b("Hilos", "comment", "ctrl+shift+t"), b("Copiar", "copy", "ctrl+c"),
            bx("Cursiva", "italic", "ctrl+i"), bx("Buscar", "find", "ctrl+f"),
            bx("Subir archivo", "upload", "ctrl+u"), bx("Editar último", "redo", "ctrl+up"),
            bx("Dictar", "mic", "dictate"),
        ]),
        profile("discord", &["discord"], vec![
            b("Silenciar", "mute", "ctrl+shift+m"), b("Audio", "video", "ctrl+shift+d"),
            b("Buscar", "find", "ctrl+f"),
            bx("Sgte canal", "next", "alt+down"), bx("Canal ant.", "prev", "alt+up"),
            bx("Marcar leído", "close", "escape"),
            bx("Dictar", "mic", "dictate"),
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
            bx("Dictar", "mic", "dictate"),
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
        // --- Development / system ---
        profile("editor", &["code", "devenv", "visual studio", "cursor"], vec![
            b("Guardar", "save", "ctrl+s"), b("Buscar", "find", "ctrl+f"),
            b("Paleta", "new", "ctrl+shift+p"), b("Deshacer", "undo", "ctrl+z"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Reemplazar", "replace", "ctrl+h"), bx("Comentar", "comment", "ctrl+/"),
            bx("Terminal", "terminal", "ctrl+`"), bx("Ir a línea", "find", "ctrl+g"),
            bx("Formato", "format", "shift+alt+f"),
            bx("Sel. todo", "selectall", "ctrl+a"), bxd("Eliminar", "delete", "delete"),
            bx("Dictar", "mic", "dictate"),
        ]),
        // --- Shells: the window TITLE identifies the active shell (Windows Terminal changes it
        // per tab: "Windows PowerShell", "C:\\...\\cmd.exe", "user@host: ~"). Go BEFORE the
        // "terminal" profile (emulator) to win by title.
        // Commands are typed with type:...>>enter. RULE: nothing destructive in the catalogue —
        // the button types into whatever is focused (vim, a password prompt...), so the worst
        // that can happen should be one extra "ls".
        // Designed as a "daily console user" set from the PHONE: the most valuable thing isn't
        // typed commands but up-arrow (history) + Run -> re-launch the build/server without
        // touching the keyboard. Cancel (ctrl+c) to kill something hung while away from the PC.
        profile("shell-pwsh", &["powershell", "pwsh"], vec![
            // NOTE: "ls -lh" does NOT exist in PowerShell (ls = alias for Get-ChildItem; fails
            // with "A parameter cannot be found that matches parameter name 'lh'"). The
            // equivalent of "more detail" is -Force, which also lists hidden items.
            b("Listar", "folder", "type:ls -Force>>enter"),
            b("Subir nivel", "back", "type:cd ..>>enter"),
            b("Limpiar", "eraser", "type:cls>>enter"),
            b("Anterior", "scrollup", "up"),      // history: recalls the last command
            b("Ejecutar", "play", "enter"),        // ...and runs it. The key combo from the phone.
            b("Cancelar", "close", "ctrl+c"),
            bx("Siguiente", "scrolldown", "down"),
            bx("Dónde estoy", "pin", "type:pwd>>enter"),
            bx("Autocompletar", "tab", "tab"),     // handy after dictating a half-typed path
            // Git: only make sense inside a repo -> all extras.
            bx("Git estado", "find", "type:git status>>enter"),
            bx("Git pull", "download", "type:git pull>>enter"),
            bx("Git push", "upload", "type:git push>>enter"),
            bx("Git log", "history", "type:git log --oneline -10>>enter"),
            bx("Git diff", "replace", "type:git diff>>enter"),
            bx("Copiar", "copy", "ctrl+shift+c"), bx("Pegar", "paste", "ctrl+shift+v"),
            bx("Nueva carpeta", "newfolder", "prompt:Nombre de la carpeta=type:mkdir {}>>enter"),
            bx("Dictar", "mic", "dictate"),
        ]),
        profile("shell-cmd", &["cmd.exe", "símbolo del sistema", "command prompt"], vec![
            b("Listar", "folder", "type:dir /a>>enter"), // /a incluye ocultos; dir ya da fecha y tamaño
            b("Subir nivel", "back", "type:cd ..>>enter"),
            b("Limpiar", "eraser", "type:cls>>enter"),
            b("Anterior", "scrollup", "up"),
            b("Ejecutar", "play", "enter"),
            b("Cancelar", "close", "ctrl+c"),
            bx("Siguiente", "scrolldown", "down"),
            bx("Dónde estoy", "pin", "type:cd>>enter"), // cmd with no args prints the directory
            bx("Autocompletar", "tab", "tab"),
            bx("Git estado", "find", "type:git status>>enter"),
            bx("Git pull", "download", "type:git pull>>enter"),
            bx("Git push", "upload", "type:git push>>enter"),
            bx("Copiar", "copy", "ctrl+shift+c"), bx("Pegar", "paste", "ctrl+shift+v"),
            bx("Nueva carpeta", "newfolder", "prompt:Nombre de la carpeta=type:md {}>>enter"),
            bx("Dictar", "mic", "dictate"),
        ]),
        profile("shell-bash", &["wsl", "ubuntu", "debian", "bash", "mingw"], vec![
            b("Listar", "folder", "type:ls -lh>>enter"), // -l detail, -h human-readable sizes
            b("Subir nivel", "back", "type:cd ..>>enter"),
            b("Limpiar", "eraser", "type:clear>>enter"),
            b("Anterior", "scrollup", "up"),
            b("Ejecutar", "play", "enter"),
            b("Cancelar", "close", "ctrl+c"),
            bx("Siguiente", "scrolldown", "down"),
            bx("Dónde estoy", "pin", "type:pwd>>enter"),
            bx("Autocompletar", "tab", "tab"),
            bx("Carpeta anterior", "undo", "type:cd ->>enter"), // bash only: toggles two folders
            bx("Git estado", "find", "type:git status>>enter"),
            bx("Git pull", "download", "type:git pull>>enter"),
            bx("Git push", "upload", "type:git push>>enter"),
            bx("Git log", "history", "type:git log --oneline -10>>enter"),
            bx("Git diff", "replace", "type:git diff>>enter"),
            bx("Copiar", "copy", "ctrl+shift+c"), bx("Pegar", "paste", "ctrl+shift+v"),
            bx("Nueva carpeta", "newfolder", "prompt:Nombre de la carpeta=type:mkdir {}>>enter"),
            bx("Dictar", "mic", "dictate"),
        ]),
        // Terminal emulator (tabs/panes/clipboard). Fallback for when the title doesn't give the
        // shell away. NOTE: the process name is "WindowsTerminal" WITHOUT a space — the string
        // "windows terminal" never matched (bug fixed 2026-07-19).
        profile("terminal", &["windowsterminal", "terminal", "warp", "conhost"], vec![
            // NOTE: ctrl+c in a console is SIGINT (kills the process). Windows Terminal/
            // PowerShell accept ctrl+shift+c/v for the clipboard.
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
            bx("Sel. todo", "selectall", "ctrl+a"), bxd("Eliminar", "delete", "delete"),
            bx("Dictar", "mic", "dictate"),
        ]),
        profile("explorer", &["explorador", "file explorer", "explorer"], vec![
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            b("Nueva carpeta", "newfolder", "ctrl+shift+n"), b("Renombrar", "rename", "f2"),
            bx("Cortar", "cut", "ctrl+x"), bx("Atrás", "undo", "alt+left"),
            bx("Subir nivel", "redo", "alt+up"), bx("Propiedades", "settings", "alt+enter"), bx("Buscar", "find", "ctrl+f"),
            bx("Sel. todo", "selectall", "ctrl+a"), bxd("Eliminar", "delete", "delete"),
        ]),
        // --- Batch 1: apps/web added towards ~100 (go before "browser" to win by title) ---
        profile("chatgpt", &["chatgpt"], vec![
            b("Nuevo chat", "new", "ctrl+shift+o"), b("Barra lateral", "tab", "ctrl+shift+s"),
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            bx("Borrar chat", "delete", "ctrl+shift+backspace"), bx("Enfocar entrada", "text", "shift+escape"),
            bx("Dictar", "mic", "dictate"),
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
            bx("Sel. todo", "selectall", "ctrl+a"), bxd("Eliminar", "delete", "delete"),
            bx("Dictar", "mic", "dictate"),
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
        // Paint: the toolbar has no shortcuts -> pressed via UI Automation ("uia:<name>"). Real
        // toolbar names (Paint Win11 ES) verified live with UIAutomation.
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
            // Color picker: a single button opens a local palette on the phone ("colorpicker:...");
            // each swatch sends "uia:<Name>". The palette entries are UIA ListItems with
            // SelectionItemPattern (already supported). EXACT names from Paint Win11 ES (verified
            // live); the exact match avoids colliding with "Color 1: Black"/"Color 2: White". The
            // hex value is only used to paint the swatch.
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
        // --- Batch 2: more apps/web towards ~100 (also before "browser") ---
        profile("gmeet", &["google meet", "meet -"], vec![
            b("Silenciar", "mic", "ctrl+d"), b("Cámara", "video", "ctrl+e"),
            b("Levantar mano", "hand", "ctrl+alt+h"),
            bx("Pant. completa", "fullscreen", "f"),
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
            bx("Zoom +", "zoomin", "ctrl+add"), bx("Rehacer", "redo", "ctrl+shift+z"),
        ]),
        profile("inkscape", &["inkscape"], vec![
            b("Seleccionar", "cursor", "s"), b("Lápiz", "pencil", "p"),
            b("Texto", "text", "t"), b("Deshacer", "undo", "ctrl+z"),
            bx("Rectángulo", "rect", "r"), bx("Elipse", "ellipse", "e"),
            bx("Guardar", "save", "ctrl+s"), bx("Rehacer", "redo", "ctrl+y"),
        ]),
        // --- Batch 3: more apps/web towards ~100 (also before "browser") ---
        profile("whatsapp", &["whatsapp"], vec![
            b("Nuevo chat", "new", "ctrl+n"), b("Buscar", "find", "ctrl+f"),
            b("Sgte chat", "next", "ctrl+shift+]"), b("Chat ant.", "prev", "ctrl+shift+["),
            bx("Archivar", "archive", "ctrl+e"), bx("Silenciar", "mute", "ctrl+shift+m"),
            bx("Dictar", "mic", "dictate"),
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
        // --- Batch 4: more apps/web towards ~100 (also before "browser") ---
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
            // Presenter pack (in presentation mode: arrows advance slides, b = black, w = white).
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
            bx("Sel. todo", "selectall", "ctrl+a"), bxd("Eliminar", "delete", "delete"),
            bx("Dictar", "mic", "dictate"),
        ]),
        profile("sumatra", &["sumatra"], vec![
            b("Buscar", "find", "ctrl+f"), b("Ir a página", "find", "ctrl+g"),
            b("Zoom +", "zoomin", "ctrl+add"), b("Zoom -", "zoomout", "ctrl+subtract"),
            bx("Pant. completa", "fullscreen", "f11"), bx("Imprimir", "print", "ctrl+p"),
        ]),
        profile("foxit", &["foxit"], vec![
            b("Buscar", "find", "ctrl+f"), b("Guardar", "save", "ctrl+s"),
            b("Imprimir", "print", "ctrl+p"), b("Copiar", "copy", "ctrl+c"),
            bx("Zoom +", "zoomin", "ctrl+add"), bx("Zoom -", "zoomout", "ctrl+subtract"),
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
        // --- Batch 5: more apps/web towards ~100 (also before "browser") ---
        profile("gslides", &["google slides", "presentaciones de google"], vec![
            // Presenter pack (in presentation mode arrows advance slides; b = black, w = white).
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
        // --- Batch 6: closing the set towards ~100 (also before "browser") ---
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
        // OBS over websocket (obs:): works even with the game in the foreground, with live state
        // on the button. While OBS is recording or streaming, this profile gets pinned (see
        // layout_for) and the user's scenes are added to it as buttons.
        profile("obs", &["obs studio", "obs64"], vec![
            b("Grabar", "record", "obs:record"), bd("Directo", "stream", "obs:stream"),
            b("Mic", "mic", "obs:mic"), b("Clip", "clip", "obs:replay"),
            bx("Buffer clips", "clip", "obs:replaybuffer"),
        ]),
        profile("steam", &["steam"], vec![
            b("Overlay", "apps", "shift+tab"), b("Captura", "screenshot", "f12"),
        ]),
        // --- Generic browser (any non-specific tab) ---
        // Prioritized for browsing/reading (from the couch): back/forward, reload and scroll
        // up/down (press-and-hold = continuous scroll). Copy/Paste move to extras.
        profile("browser", &["chrome", "edge", "firefox", "brave", "opera", "vivaldi"], vec![
            b("Atrás", "back", "alt+left"), b("Adelante", "fwdnav", "alt+right"),
            b("Recargar", "refresh", "f5"), b("Nueva pestaña", "new", "ctrl+t"),
            // Scroll/mouse live in the phone's global "Mouse" button (top bar, full screen).
            bx("Subir", "scrollup", "scroll:-3"), bx("Bajar", "scrolldown", "scroll:3"),
            bx("Cerrar pestaña", "close", "ctrl+w"), bx("Buscar", "find", "ctrl+f"),
            bx("Copiar", "copy", "ctrl+c"), bx("Pegar", "paste", "ctrl+v"),
            bx("Reabrir pestaña", "redo", "ctrl+shift+t"), bx("Favorito", "star", "ctrl+d"),
            bx("Historial", "history", "ctrl+h"), bx("Descargas", "download", "ctrl+j"),
            bx("Incógnito", "new", "ctrl+shift+n"), bx("Zoom +", "zoomin", "ctrl+add"), bx("Zoom -", "zoomout", "ctrl+subtract"),
            bx("Dictar", "mic", "dictate"),
        ]),
        // --- Fallback (unrecognized app). Volume and screenshot live in the phone's fixed dock;
        // Play/Pause only on profiles for apps that play media (does nothing in Word). ---
        profile("generic", &[], vec![
            b("Copiar", "copy", "ctrl+c"), b("Pegar", "paste", "ctrl+v"),
            b("Deshacer", "undo", "ctrl+z"),
            // Windows Game Bar: FIXED system shortcuts, work inside any game.
            bx("Clip 30s", "clip", "win+alt+g"), bx("Grabar juego", "record", "win+alt+r"),
            bx("Game Bar", "apps", "win+g"),
            bx("Cortar", "cut", "ctrl+x"), bx("Rehacer", "redo", "ctrl+y"),
            bx("Guardar", "save", "ctrl+s"), bx("Buscar", "find", "ctrl+f"), bx("Imprimir", "print", "ctrl+p"),
        ]),
    ];
    // "Close app" (red + confirmation) on EVERY profile.
    for p in &mut list {
        p.buttons.push(bd("Cerrar app", "close", "alt+F4"));
        // The way OUT of auto mode. Without it the switch is one-way: the decks carry an "Auto"
        // key, but nothing in auto mode reaches the decks. An extra rather than a recommended
        // button, so it never displaces an app action on the first page.
        p.buttons.push(bx("Mazos", "deck", "mode:manual"));
    }
    list
}
/// Keys per page in the decks shipped by default. Decks are authored against the reference 5×3
/// device; the host repaginates to whatever grid the client declares in `hello`, so this number is
/// an authoring convention, not a constraint on the client.
const REFERENCE_PAGE: usize = 15;

/// v1 had no manual mode, so there is nothing to carry over — but landing in manual mode with an
/// empty grid looks broken. This seeds one starter deck from the generic profile's recommended
/// buttons, spread over two pages, plus the v2-only keys (window switcher, mode switch) that had
/// no equivalent in v1. F4 replaces it with a deck built from the machine's most-used apps.
fn default_decks() -> Vec<Deck> {
    let generic = default_profiles().into_iter().find(|p| p.id == "generic");
    let mut keys: Vec<Key> = generic
        .map(|p| p.buttons)
        .unwrap_or_default()
        .into_iter()
        .filter(|b| b.recommended)
        .map(|b| Key {
            label: b.label,
            icon: b.icon,
            action: Some(b.action),
            danger: b.danger,
            kind: KeyKind::Action,
            ..Default::default()
        })
        .collect();
    // v2-only keys: they have no v1 button to migrate from.
    keys.push(Key {
        label: "Ventanas".into(),
        icon: "windows".into(),
        action: Some("windows".into()),
        kind: KeyKind::Action,
        ..Default::default()
    });
    // The trackpad and dictation are §4.2.1 client-side screens. They were loose UI in v1; in v2
    // everything is a key, so this is the only way to reach them.
    keys.push(Key {
        label: "Trackpad".into(),
        icon: "mouse".into(),
        action: Some("trackpad".into()),
        kind: KeyKind::Action,
        ..Default::default()
    });
    keys.push(Key {
        label: "Dictar".into(),
        icon: "mic".into(),
        action: Some("dictate".into()),
        kind: KeyKind::Action,
        ..Default::default()
    });
    keys.push(Key {
        label: "Auto".into(),
        icon: "mode".into(),
        action: Some("mode:auto".into()),
        kind: KeyKind::Action,
        ..Default::default()
    });
    // Only offer the jump if the machine actually produced a Launcher — a key that answers
    // no_such_key is worse than no key.
    let launcher = launcher_deck();
    if launcher.is_some() {
        keys.push(Key {
            label: "Launcher".into(),
            icon: "apps".into(),
            action: Some("deck:launcher".into()),
            kind: KeyKind::Action,
            ..Default::default()
        });
    }

    let mut pages: Vec<Page> = keys
        .chunks(REFERENCE_PAGE)
        .enumerate()
        .map(|(i, chunk)| Page {
            id: format!("p{i}"),
            name: String::new(),
            keys: chunk
                .iter()
                .enumerate()
                .map(|(pos, k)| Key { pos, ..k.clone() })
                .collect(),
        })
        .collect();
    if pages.is_empty() {
        pages.push(Page { id: "p0".into(), ..Default::default() });
    }
    let mut decks =
        vec![Deck { id: "starter".into(), name: "KiBoard".into(), icon: "deck".into(), pages }];
    decks.extend(launcher);
    decks
}

/// Cap on the generated Launcher deck. High on purpose: an alphabetical cut at two pages silently
/// hid the browser and the editor, which is exactly what a launcher is for. Paging is cheap, a
/// missing app is not.
const LAUNCHER_APPS: usize = 60;

/// A "Launcher" deck built from the machine's own app catalogue, so manual mode has something real
/// to do on first run. Icons and running state are attached per-send by `engine::deck`, not stored.
///
/// ponytail: alphabetical, minus what lives in the Windows folders — that drops `charmap`,
/// `msconfig` and the rest of the Start menu's system tools without a curated block list. The plan
/// says "most-used", but the only source for that is the ROT13'd UserAssist registry, whose counts
/// are famously unreliable; ordering by real usage belongs with the editor in F5.
fn launcher_deck() -> Option<Deck> {
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()).to_lowercase();
    let apps: Vec<&crate::platform::apps::App> = crate::platform::apps::catalogue()
        .iter()
        .filter(|a| !a.exe.to_lowercase().starts_with(&windir))
        .take(LAUNCHER_APPS)
        .collect();
    if apps.is_empty() {
        return None; // no catalogue (non-Windows, or the enumeration failed): no empty deck
    }
    // The way back, first key: a generated deck has no navigation of its own, and paging through
    // sixty apps to find an exit is not an exit.
    let mut keys = vec![Key {
        pos: 0,
        label: "Auto".into(),
        icon: "mode".into(),
        action: Some("mode:auto".into()),
        kind: KeyKind::Action,
        ..Default::default()
    }];
    keys.extend(apps.into_iter().enumerate().map(|(i, a)| Key {
        pos: i + 1,
        label: a.name.clone(),
        icon: "app".into(),
        action: Some(format!("launch:{}", a.id)),
        // Long press focuses without launching — Elgato's "Key Logic" shape (protocol §3).
        hold: Some(format!("focus:{}", a.id)),
        kind: KeyKind::Action,
        ..Default::default()
    }));
    Some(Deck {
        id: "launcher".into(),
        name: "Launcher".into(),
        icon: "apps".into(),
        pages: keys
            .chunks(REFERENCE_PAGE)
            .enumerate()
            .map(|(i, chunk)| Page {
                id: format!("p{i}"),
                name: String::new(),
                keys: chunk.iter().enumerate().map(|(pos, k)| Key { pos, ..k.clone() }).collect(),
            })
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// Persistent configuration
// ---------------------------------------------------------------------------

/// Version of the built-in profiles. Bump it when `default_profiles` changes so already-installed
/// hosts refresh them (keeping the token and pairing).
const PROFILES_VERSION: u32 = 37;

/// Shape of `config.json`. Bumped when the model changes in a way `#[serde(default)]` cannot
/// absorb; `load` backs the old file up to `config.v1.bak` before rewriting it.
const CONFIG_VERSION: u32 = 2;

/// A phone paired via the v2 six-digit-code flow (protocol/README.md §2). Each device gets its
/// own token, individually revocable — the pre-F1 model had one shared `token` for everyone.
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Device {
    pub(crate) device_id: String,
    pub(crate) name: String,
    pub(crate) platform: String,
    pub(crate) token: String,
    pub(crate) last_seen: u64,
}

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct Config {
    /// Pre-F1 shared token. Dead once every client speaks v2, kept only so F2's config.json
    /// migration (docs/implementation-plan.md, R7) has something to read; never checked anymore.
    pub(crate) token: String,
    #[serde(default)]
    pub(crate) paired: Vec<String>,
    /// Stable per-install id (8 hex chars), advertised in the mDNS TXT record.
    #[serde(default)]
    pub(crate) host_id: String,
    /// v2 paired devices, one token each, individually revocable.
    #[serde(default)]
    pub(crate) devices: Vec<Device>,
    /// Whether the host currently accepts new pair_request attempts (mDNS TXT `pair`).
    #[serde(default = "default_true")]
    pub(crate) pairing_open: bool,
    #[serde(default)]
    pub(crate) profiles: Vec<Profile>,
    #[serde(default)]
    profiles_version: u32,
    /// MANUAL mode: decks the user picks. Absent in a v1 file — seeded on migration.
    #[serde(default)]
    pub(crate) decks: Vec<Deck>,
    /// Shape of this file. 0 = a v1 file (the field did not exist), 2 = the v2 model.
    #[serde(default)]
    config_version: u32,
    /// OBS WebSocket server password (empty if the server has no auth).
    #[serde(default)]
    pub(crate) obs_password: String,
    /// Share anonymous usage stats (Aptabase). Opt-out from the UI.
    #[serde(default = "default_true")]
    pub(crate) analytics: bool,
}

fn default_true() -> bool {
    true
}

impl Config {
    fn path() -> std::path::PathBuf {
        // KIBOARD_CONFIG_DIR points a dev build at a throwaway config. Without it, testing means
        // running against the real one — which holds live pairing tokens and gets rewritten by the
        // v1->v2 migration on first load.
        let dir = match std::env::var_os("KIBOARD_CONFIG_DIR") {
            Some(d) => std::path::PathBuf::from(d),
            None => dirs::config_dir().unwrap_or(std::env::temp_dir()).join("KiBoard"),
        };
        let _ = std::fs::create_dir_all(&dir);
        dir.join("config.json")
    }
    fn load() -> Config {
        let raw = std::fs::read_to_string(Self::path()).ok();
        let mut c: Config = raw
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            // Fresh install (no file): analytics ON by default; derive(Default) would give false.
            .unwrap_or(Config { analytics: true, pairing_open: true, ..Default::default() });
        // Migrating a v1 file (no `config_version`): keep it verbatim before the v2 shape is
        // written over it. Profiles the user edited by hand are otherwise unrecoverable.
        if c.config_version < CONFIG_VERSION {
            if let Some(raw) = raw.as_deref() {
                let _ = std::fs::write(Self::path().with_file_name("config.v1.bak"), raw);
            }
            c.config_version = CONFIG_VERSION;
        }
        if c.decks.is_empty() {
            c.decks = default_decks();
        }
        if c.token.is_empty() {
            c.token = new_token();
        }
        if c.host_id.is_empty() {
            c.host_id = new_host_id();
        }
        // Refreshes the built-in profiles on fresh installs or when the version bumps.
        if c.profiles.is_empty() || c.profiles_version < PROFILES_VERSION {
            c.profiles = default_profiles();
            c.profiles_version = PROFILES_VERSION;
        }
        c.save();
        c
    }
    pub(crate) fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), s);
        }
    }
}
static CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();
pub(crate) fn config() -> &'static Mutex<Config> {
    CONFIG.get_or_init(|| Mutex::new(Config::load()))
}
/// 8 hex chars (4 random bytes) — short, stable per-install id for mDNS/pairing display.
pub(crate) fn new_host_id() -> String {
    use rand::Rng;
    let bytes: [u8; 4] = rand::thread_rng().gen();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) fn new_token() -> String {
    use rand::Rng;
    let bytes: [u8; 16] = rand::thread_rng().gen();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::{default_decks, default_profiles, Key, KeyKind, Profile, REFERENCE_PAGE};

    // Same predicate as layout_for: first profile whose `matches` shows up in "{app} {title}".
    fn profile_for<'a>(profiles: &'a [Profile], app: &str, title: &str) -> &'a str {
        let hay = format!("{app} {title}").to_lowercase();
        profiles
            .iter()
            .find(|p| p.matches.iter().any(|m| hay.contains(&m.to_lowercase())))
            .or_else(|| profiles.iter().find(|p| p.matches.is_empty()))
            .map(|p| p.id.as_str())
            .unwrap_or("empty")
    }

    #[test]
    fn ai_profile_wins_over_terminal_by_title() {
        let profiles = default_profiles();
        // Claude Code sets the terminal title to "Claude" -> "ai" profile.
        assert_eq!(profile_for(&profiles, "WindowsTerminal", "Claude"), "ai");
        assert_eq!(profile_for(&profiles, "WindowsTerminal", "codex — repo"), "ai");
        // A plain terminal (no agent) must NOT fall into "ai".
        assert_eq!(profile_for(&profiles, "WindowsTerminal", "Windows PowerShell"), "shell-pwsh");
    }

    // --- v2 Deck/Page/Key model ---

    #[test]
    fn starter_deck_is_addressable_by_position() {
        let decks = default_decks();
        assert!(!decks.is_empty(), "a starter deck");
        // Every shipped deck, the generated Launcher included, obeys the same addressing rules.
        for deck in &decks {
            assert!(!deck.pages.is_empty());
            for page in &deck.pages {
                assert!(page.keys.len() <= REFERENCE_PAGE);
                // `pos` IS the address the phone sends back in a `key` message: it must be the
                // index, contiguous from 0, or the host resolves a press to the wrong action.
                for (i, k) in page.keys.iter().enumerate() {
                    assert_eq!(k.pos, i, "page {} key {} has pos {}", page.id, i, k.pos);
                }
                // Every navigating key must name a page that exists in this deck.
                for k in &page.keys {
                    if matches!(k.kind, KeyKind::Folder | KeyKind::Page) {
                        let t = k.target.as_deref().expect("a navigating key needs a target");
                        assert!(deck.page(t).is_some(), "key targets missing page {t}");
                    }
                    if matches!(k.kind, KeyKind::Action) {
                        assert!(k.action.is_some(), "an action key needs an action");
                    }
                }
            }
        }
    }

    /// Manual probe against the real machine: `cargo test probe_launcher -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn probe_launcher() {
        let decks = default_decks();
        let l = decks.iter().find(|d| d.id == "launcher").expect("a launcher deck");
        for p in &l.pages {
            for k in &p.keys {
                println!("{:2} {:35} {}", k.pos, k.label, k.action.as_deref().unwrap_or(""));
            }
        }
    }

    /// The same deck as it LEAVES the host: does the wire message carry the real app icon and the
    /// live running flag? `cargo test probe_launcher_layout -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn probe_launcher_layout() {
        use crate::engine::deck::{layout_json, Grid};
        let decks = default_decks();
        let d = decks.iter().find(|d| d.id == "launcher").expect("a launcher deck");
        let msg: serde_json::Value =
            serde_json::from_str(&layout_json(d, &d.pages[0], Grid::new(3, 5), 0)).unwrap();
        for k in msg["keys"].as_array().unwrap() {
            println!(
                "{:35} image {:>6} B  running={}",
                k["label"].as_str().unwrap_or(""),
                k["image"].as_str().map(str::len).unwrap_or(0),
                k["state"]["running"]
            );
        }
    }

    // A hold that is not configured must do NOTHING, not repeat the short press: holding a key
    // bound to "Cerrar app" would otherwise fire it on an accidental long press.
    #[test]
    fn unbound_presses_do_not_fall_back() {
        let k = Key { action: Some("ctrl+c".into()), kind: KeyKind::Action, ..Default::default() };
        assert_eq!(k.action_for("short"), Some("ctrl+c"));
        assert_eq!(k.action_for("long"), None);
        assert_eq!(k.action_for("double"), None);
        assert_eq!(k.action_for("nonsense"), None);
    }

    // An empty key must serialize to just its position and kind: it is sent on every layout, and
    // 15 of them per page adds up against the 64 KB frame cap.
    #[test]
    fn empty_key_serializes_minimally() {
        let json = serde_json::to_string(&Key { pos: 10, ..Default::default() }).unwrap();
        assert_eq!(json, r#"{"pos":10,"kind":"empty"}"#);
    }

    // The TITLE identifies the active shell (Windows Terminal changes it per tab). Real titles
    // verified on Windows 11.
    #[test]
    fn profile_by_shell_from_title() {
        let profiles = default_profiles();
        assert_eq!(profile_for(&profiles, "WindowsTerminal", "Windows PowerShell"), "shell-pwsh");
        assert_eq!(profile_for(&profiles, "cmd", r"C:\WINDOWS\system32\cmd.exe"), "shell-cmd");
        assert_eq!(profile_for(&profiles, "wsl", r"C:\WINDOWS\system32\wsl.exe"), "shell-bash");
        assert_eq!(profile_for(&profiles, "WindowsTerminal", "Ubuntu"), "shell-bash");
        // KNOWN LIMITATION: if bash exports its prompt to the title ("user@host: ~") there's no
        // distinctive word left; adding "@" as a match would hijack half the catalogue (Gmail,
        // paths with @...). Degrades to the emulator profile, not the generic one — and the user
        // can add their own match from the host UI.
        assert_eq!(profile_for(&profiles, "WindowsTerminal", "ricardo@DESKTOP: ~"), "terminal");
        // No shell hint in the title -> emulator profile (used to fall through to generic because
        // "windows terminal" with a space never matched "WindowsTerminal").
        assert_eq!(profile_for(&profiles, "WindowsTerminal", ""), "terminal");
    }
}
