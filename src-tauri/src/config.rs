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

fn yes() -> bool {
    true
}

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
    /// Optional glyph-only tint. Unlike `color`, this leaves the key cap itself untouched.
    #[serde(rename = "iconColor", default, skip_serializing_if = "Option::is_none")]
    pub(crate) icon_color: Option<String>,
    /// Short press. `None` for `folder`/`page`/`empty` keys, which navigate via `target`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hold: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) double: Option<String>,
    /// The ON face of a two-state key (protocol §3, F6). Config only: `engine::deck` resolves the
    /// face and STRIPS this before sending, so the phone renders an ordinary key and needs no code
    /// of its own to draw a toggle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) toggle: Option<Face>,
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

/// What a two-state key looks like when it is ON. Every field is optional and overrides only
/// itself: a toggle that changes the label alone keeps the OFF face's icon, colour and action.
#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct Face {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) action: Option<String>,
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
    Button {
        label: label.into(),
        icon: icon.into(),
        action: action.into(),
        danger: false,
        recommended: true,
    }
}
fn bd(label: &str, icon: &str, action: &str) -> Button {
    Button {
        label: label.into(),
        icon: icon.into(),
        action: action.into(),
        danger: true,
        recommended: true,
    }
}
/// "Extra" button: available in the profile but outside the default recommended selection.
fn bx(label: &str, icon: &str, action: &str) -> Button {
    Button {
        label: label.into(),
        icon: icon.into(),
        action: action.into(),
        danger: false,
        recommended: false,
    }
}
/// A deliberate empty position in the recommended grid. Some graphical AI clients do not expose
/// safe equivalents for every context slot; keeping the hole preserves the shared muscle-memory
/// map without inventing a shortcut that may do something else in a future app version.
fn gap() -> Button {
    Button {
        label: String::new(),
        icon: String::new(),
        action: String::new(),
        danger: false,
        recommended: true,
    }
}
/// "Extra" and dangerous button: outside the default selection + red with confirmation (e.g. Delete).
fn bxd(label: &str, icon: &str, action: &str) -> Button {
    Button {
        label: label.into(),
        icon: icon.into(),
        action: action.into(),
        danger: true,
        recommended: false,
    }
}
fn profile(id: &str, matches: &[&str], buttons: Vec<Button>) -> Profile {
    Profile {
        id: id.into(),
        matches: matches.iter().map(|s| s.to_string()).collect(),
        buttons,
    }
}
/// Default profile catalogue. Order matters: the most specific ones (Chrome tabs) go first, so
/// they win over the generic "browser" profile. All of them are editable from the host UI; closing
/// the foreground app lives in the phone's reserved app panel rather than in this catalogue.
pub(crate) fn default_profiles() -> Vec<Profile> {
    let list = vec![
        // --- Terminal AI agents -------------------------------------------------------------
        // These are deliberately separate. Their menus and slash commands look similar, but the
        // arguments do not: the old shared `ai` profile sent Claude model ids and a personal
        // Claude keybinding to Codex, Gemini and Aider. Model catalogues change faster than
        // KiBoard releases, so a default either opens the agent's own selector or asks for a free
        // value on the phone; it never freezes provider model ids here.
        profile(
            "claude-code",
            &["claude"],
            vec![
                b("Modelo", "model", "type:/model>>enter"),
                b("Permisos", "settings", "type:/permissions>>enter"),
                b("Compactar", "archive", "type:/compact>>enter"),
                b("Limpiar", "delete", "type:/clear>>enter"),
                b("Copiar", "copy", "ctrl+shift+c"),
                b("Pegar", "paste", "ctrl+shift+v"),
                b("Aceptar", "accept", "enter"),
                b("Rechazar", "close", "esc"),
                b("Subir", "scrollup", "up"),
                b("Bajar", "scrolldown", "down"),
                b("Izquierda", "prev", "left"),
                b("Derecha", "next", "right"),
                bx("Nueva línea", "text", "shift+enter"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "codex-cli",
            &["codex"],
            vec![
                b("Modelo", "model", "type:/model>>enter"),
                b("Aprobaciones", "settings", "type:/approvals>>enter"),
                b("Compactar", "archive", "type:/compact>>enter"),
                b("Nuevo chat", "new", "type:/new>>enter"),
                b("Copiar", "copy", "ctrl+shift+c"),
                b("Pegar", "paste", "ctrl+shift+v"),
                b("Aceptar", "accept", "enter"),
                b("Rechazar", "close", "esc"),
                b("Subir", "scrollup", "up"),
                b("Bajar", "scrolldown", "down"),
                b("Izquierda", "prev", "left"),
                b("Derecha", "next", "right"),
                bx("Revisar", "find", "type:/review>>enter"),
                bx("Nueva línea", "text", "shift+enter"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "gemini-cli",
            &["gemini"],
            vec![
                b("Modelo", "model", "type:/model>>enter"),
                b("Agentes", "people", "type:/agents>>enter"),
                b("MCP", "link", "type:/mcp reload>>enter"),
                b("Limpiar", "delete", "ctrl+l"),
                b("Copiar", "copy", "ctrl+shift+c"),
                b("Pegar", "paste", "ctrl+shift+v"),
                b("Aceptar", "accept", "enter"),
                b("Rechazar", "close", "esc"),
                b("Subir", "scrollup", "up"),
                b("Bajar", "scrolldown", "down"),
                b("Izquierda", "prev", "left"),
                b("Derecha", "next", "right"),
                bx("Memoria", "history", "type:/memory reload>>enter"),
                bx("Nueva línea", "text", "shift+enter"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "aider",
            &["aider"],
            vec![
                b("Modelo", "model", "prompt:Modelo=type:/model {}>>enter"),
                b(
                    "Esfuerzo",
                    "effort",
                    "prompt:Esfuerzo=type:/reasoning-effort {}>>enter",
                ),
                b("Arquitecto", "model", "type:/architect>>enter"),
                b("Código", "terminal", "type:/code>>enter"),
                b("Deshacer", "undo", "type:/undo>>enter"),
                b("Diferencias", "find", "type:/diff>>enter"),
                b("Aceptar", "accept", "enter"),
                b("Rechazar", "close", "esc"),
                b("Subir", "scrollup", "up"),
                b("Bajar", "scrolldown", "down"),
                b("Izquierda", "prev", "left"),
                b("Derecha", "next", "right"),
                bx("Preguntar", "comment", "type:/ask>>enter"),
                bx("Commit", "save", "type:/commit>>enter"),
                bx("Copiar", "copy", "type:/copy>>enter"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        // Safe fallbacks for graphical AI clients. Product-specific shortcuts can replace these
        // as they are verified; unlike the old terminal profile, none can type a provider command
        // into an unrelated app.
        profile(
            "claude-desktop",
            &["claude"],
            vec![
                gap(),
                b("Dictar", "mic", "dictate"),
                b("Sel. todo", "selectall", "ctrl+a"),
                b("Nueva línea", "text", "shift+enter"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                b("Enviar", "send", "enter"),
                b("Cancelar", "close", "esc"),
                b("Subir", "scrollup", "up"),
                b("Bajar", "scrolldown", "down"),
                b("Izquierda", "prev", "left"),
                b("Derecha", "next", "right"),
            ],
        ),
        profile(
            "codex-desktop",
            &["codex"],
            vec![
                // Agent controls verified against the installed Codex app. Effort and Speed use
                // the user's composer keybindings in ~/.codex/keybindings.json; sending `/fast`
                // as text is not equivalent in Desktop and can leave the word in the composer.
                // Model selection keeps the native picker so ids never freeze.
                b("Modelo", "model", "ctrl+shift+m"),
                b("Esfuerzo -", "zoomout", "ctrl+alt+shift+k"),
                b("Esfuerzo +", "zoomin", "ctrl+alt+shift+i"),
                b("Velocidad", "bolt", "ctrl+alt+shift+f"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Revisar", "find", "ctrl+shift+g"),
                b("Aceptar", "accept", "enter"),
                b("Rechazar", "close", "esc"),
                b("Subir", "scrollup", "up"),
                b("Bajar", "scrolldown", "down"),
                b("Izquierda", "prev", "left"),
                b("Derecha", "next", "right"),
                // A second, contextual page for proposed alternatives. Arrow-key menus already
                // use the main page; button/radio alternatives need focus traversal first.
                bx("Anterior", "prev", "shift+tab"),
                bx("Siguiente", "next", "tab"),
                bx("Seleccionar", "accept", "enter"),
                bx("Cancelar", "close", "esc"),
                bx("Nueva línea", "text", "shift+enter"),
                bx("Dictar", "mic", "ctrl+shift+d"),
            ],
        ),
        profile(
            "lm-studio",
            &["lm studio", "lmstudio"],
            vec![
                gap(),
                gap(),
                b("Dictar", "mic", "dictate"),
                b("Nueva línea", "text", "shift+enter"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                b("Enviar", "send", "enter"),
                b("Detener", "close", "esc"),
                b("Subir", "scrollup", "up"),
                b("Bajar", "scrolldown", "down"),
                b("Izquierda", "prev", "left"),
                b("Derecha", "next", "right"),
            ],
        ),
        // --- Chrome/browser tabs (matched by window TITLE) ---
        profile(
            "gsheets",
            &["google sheets", "hojas de cálculo"],
            vec![
                b("Negrita", "bold", "ctrl+b"),
                b("Buscar", "find", "ctrl+f"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Rehacer", "redo", "ctrl+y"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Cursiva", "italic", "ctrl+i"),
                bx("Subrayado", "underline", "ctrl+u"),
                bx("Cortar", "cut", "ctrl+x"),
                bx("Imprimir", "print", "ctrl+p"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "gdocs",
            &["google docs", "documentos de google"],
            vec![
                b("Negrita", "bold", "ctrl+b"),
                b("Buscar", "find", "ctrl+f"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Cursiva", "italic", "ctrl+i"),
                bx("Subrayado", "underline", "ctrl+u"),
                bx("Enlace", "link", "ctrl+k"),
                bx("Rehacer", "redo", "ctrl+y"),
                bx("Imprimir", "print", "ctrl+p"),
                bx("Sel. todo", "selectall", "ctrl+a"),
                bxd("Eliminar", "delete", "delete"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "gdrive",
            &["google drive", "mi unidad"],
            vec![
                b("Buscar", "find", "ctrl+f"),
                b("Nueva pestaña", "new", "ctrl+t"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Nueva carpeta", "newfolder", "shift+f"),
                bx("Renombrar", "rename", "n"),
                bx("Sel. todo", "selectall", "ctrl+a"),
                bxd("Eliminar", "delete", "delete"),
            ],
        ),
        profile(
            "gmail",
            &["gmail"],
            vec![
                // Gmail ships its single-letter shortcuts disabled. Keep setup inside the deck so
                // the user can enable them without hunting through Settings. `?` opens Gmail's
                // own shortcut dialog even while they are disabled; its first Tab target is the
                // Enable/Disable link. The picker makes the state-changing choice explicit, so an
                // account that already has shortcuts enabled is never toggled accidentally.
                b(
                    "Atajos Gmail",
                    "settings",
                    "picker:Activar atajos=?>>tab>>enter;Abrir configuración=open:https://mail.google.com/mail/u/0/#settings/general;Ver estado=?",
                ),
                b("Redactar", "new", "c"),
                b("Buscar", "find", "/"),
                b("Responder", "reply", "r"),
                b("Archivar", "archive", "e"),
                bx("Resp. todos", "replyall", "a"),
                bx("Reenviar", "forward", "f"),
                bx("Eliminar", "delete", "#"),
                bx("Destacar", "star", "s"),
                bx("Enviar", "send", "ctrl+enter"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "youtube",
            &["youtube"],
            vec![
                b("Play/Pausa", "play", "k"),
                b("Silenciar", "mute", "m"),
                b("Pantalla completa", "video", "f"),
                bx("Adelante", "redo", "l"),
                bx("Atrás", "undo", "j"),
                bx("Siguiente", "redo", "shift+n"),
                bx("Subtítulos", "text", "c"),
                bx("Teatro", "fullscreen", "t"),
            ],
        ),
        // --- Office ---
        profile(
            "word",
            &["word"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Negrita", "bold", "ctrl+b"),
                b("Buscar", "find", "ctrl+f"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Cursiva", "italic", "ctrl+i"),
                bx("Subrayado", "underline", "ctrl+u"),
                bx("Enlace", "link", "ctrl+k"),
                bx("Rehacer", "redo", "ctrl+y"),
                bx("Imprimir", "print", "ctrl+p"),
                bx("Sel. todo", "selectall", "ctrl+a"),
                bxd("Eliminar", "delete", "delete"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "excel",
            &["excel"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Rehacer", "redo", "ctrl+y"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Negrita", "bold", "ctrl+b"),
                bx("Cursiva", "italic", "ctrl+i"),
                bx("Autosuma", "sum", "alt+="),
                bx("Filtro", "filter", "ctrl+shift+l"),
                bx("Imprimir", "print", "ctrl+p"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "powerpoint",
            &["powerpoint"],
            vec![
                // Pack presentador: el uso remoto #1 — pasar diapositivas desde el atril.
                b("Siguiente", "next", "right"),
                b("Anterior", "prev", "left"),
                b("Presentar", "play", "f5"),
                b("Negro", "dark", "b"),
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Blanco", "light", "w"),
                bx("Desde actual", "play", "shift+f5"),
                bx("Nueva diap.", "new", "ctrl+m"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
                bx("Duplicar", "duplicate", "ctrl+d"),
                bx("Negrita", "bold", "ctrl+b"),
                bx("Imprimir", "print", "ctrl+p"),
            ],
        ),
        profile(
            "outlook",
            &["outlook"],
            vec![
                b("Nuevo correo", "new", "ctrl+n"),
                b("Responder", "reply", "ctrl+r"),
                b("Enviar", "send", "ctrl+enter"),
                b("Buscar", "find", "ctrl+e"),
                bx("Resp. todos", "replyall", "ctrl+shift+r"),
                bx("Reenviar", "forward", "ctrl+f"),
                bx("Eliminar", "delete", "ctrl+d"),
                bx("Calendario", "calendar", "ctrl+2"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "acrobat",
            &["acrobat"],
            vec![
                b("Buscar", "find", "ctrl+f"),
                b("Imprimir", "print", "ctrl+p"),
                b("Guardar", "save", "ctrl+s"),
                b("Copiar", "copy", "ctrl+c"),
                bx("Zoom +", "zoomin", "ctrl+add"),
                bx("Zoom -", "zoomout", "ctrl+subtract"),
                bx("Pant. completa", "fullscreen", "ctrl+l"),
            ],
        ),
        // --- Creative ---
        profile(
            "photoshop",
            &["photoshop"],
            vec![
                b("Deshacer", "undo", "ctrl+z"),
                b("Pincel", "brush", "b"),
                b("Mover", "move", "v"),
                b("Guardar", "save", "ctrl+s"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Texto", "text", "t"),
                bx("Borrador", "eraser", "e"),
                bx("Recortar", "crop", "c"),
                bx("Zoom", "zoomin", "z"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
            ],
        ),
        profile(
            "illustrator",
            &["illustrator"],
            vec![
                b("Deshacer", "undo", "ctrl+z"),
                b("Selección", "cursor", "v"),
                b("Guardar", "save", "ctrl+s"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Pluma", "pencil", "p"),
                bx("Texto", "text", "t"),
                bx("Rectángulo", "rect", "m"),
                bx("Zoom", "zoomin", "z"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
            ],
        ),
        profile(
            "premiere",
            &["premiere"],
            vec![
                b("Play/Pausa", "play", "k"),
                b("Cortar", "cut", "c"),
                b("Selección", "cursor", "v"),
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Entrada", "login", "i"),
                bx("Salida", "logout", "o"),
                bx("Marcador", "star", "m"),
                bx("Exportar", "upload", "ctrl+m"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
            ],
        ),
        profile(
            "figma",
            &["figma"],
            vec![
                b("Mover", "move", "v"),
                b("Marco", "frame", "f"),
                b("Comentar", "comment", "c"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Texto", "text", "t"),
                bx("Rectángulo", "rect", "r"),
                bx("Lápiz", "pencil", "p"),
                bx("Duplicar", "duplicate", "ctrl+d"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
            ],
        ),
        // --- Communication / meetings ---
        profile(
            "slack",
            &["slack"],
            vec![
                b("Saltar a", "find", "ctrl+k"),
                b("Negrita", "bold", "ctrl+b"),
                b("Hilos", "comment", "ctrl+shift+t"),
                b("Copiar", "copy", "ctrl+c"),
                bx("Cursiva", "italic", "ctrl+i"),
                bx("Buscar", "find", "ctrl+f"),
                bx("Subir archivo", "upload", "ctrl+u"),
                bx("Editar último", "redo", "ctrl+up"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "discord",
            &["discord"],
            vec![
                b("Silenciar", "mute", "ctrl+shift+m"),
                b("Audio", "video", "ctrl+shift+d"),
                b("Buscar", "find", "ctrl+f"),
                bx("Sgte canal", "next", "alt+down"),
                bx("Canal ant.", "prev", "alt+up"),
                bx("Marcar leído", "close", "escape"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "teams",
            &["teams"],
            vec![
                b("Silenciar", "mic", "ctrl+shift+m"),
                b("Cámara", "video", "ctrl+shift+o"),
                b("Compartir", "share", "ctrl+shift+e"),
                b("Colgar", "close", "ctrl+shift+h"),
                bx("Levantar mano", "hand", "ctrl+shift+k"),
                bx("Chat", "comment", "ctrl+2"),
                bx("Aceptar", "play", "ctrl+shift+s"),
                bx("Rechazar", "close", "ctrl+shift+d"),
            ],
        ),
        profile(
            "zoom",
            &["zoom"],
            vec![
                b("Silenciar", "mic", "alt+a"),
                b("Vídeo", "video", "alt+v"),
                b("Compartir", "share", "alt+s"),
                b("Salir", "close", "alt+q"),
                bx("Levantar mano", "hand", "alt+y"),
                bx("Chat", "comment", "alt+h"),
                bx("Grabar", "record", "alt+r"),
                bx("Pant. completa", "fullscreen", "alt+f"),
                bx("Participantes", "people", "alt+u"),
            ],
        ),
        profile(
            "notion",
            &["notion"],
            vec![
                b("Buscar", "find", "ctrl+p"),
                b("Negrita", "bold", "ctrl+b"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Cursiva", "italic", "ctrl+i"),
                bx("Nueva página", "new", "ctrl+n"),
                bx("Enlace", "link", "ctrl+k"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        // --- Multimedia ---
        profile(
            "spotify",
            &["spotify"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Siguiente", "next", "ctrl+right"),
                b("Anterior", "prev", "ctrl+left"),
                bx("+ Volumen", "vol", "ctrl+up"),
                bx("- Volumen", "vol", "ctrl+down"),
                bx("Aleatorio", "shuffle", "ctrl+s"),
                bx("Repetir", "repeat", "ctrl+r"),
                bx("Buscar", "find", "ctrl+l"),
            ],
        ),
        profile(
            "vlc",
            &["vlc"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Pant. completa", "fullscreen", "f"),
                b("Silenciar", "mute", "m"),
                b("+ Volumen", "vol", "ctrl+up"),
                b("- Volumen", "vol", "ctrl+down"),
                bx("Siguiente", "next", "n"),
                bx("Anterior", "prev", "p"),
                bx("Subtítulos", "subtitles", "v"),
                bx("Captura", "screenshot", "shift+s"),
            ],
        ),
        // --- Development / system ---
        profile(
            "editor",
            &["code", "devenv", "visual studio", "cursor"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Paleta", "new", "ctrl+shift+p"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Reemplazar", "replace", "ctrl+h"),
                bx("Comentar", "comment", "ctrl+/"),
                bx("Terminal", "terminal", "ctrl+`"),
                bx("Ir a línea", "find", "ctrl+g"),
                bx("Formato", "format", "shift+alt+f"),
                bx("Sel. todo", "selectall", "ctrl+a"),
                bxd("Eliminar", "delete", "delete"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        // --- Shells: the window TITLE identifies the active shell (Windows Terminal changes it
        // per tab: "Windows PowerShell", "C:\\...\\cmd.exe", "user@host: ~"). Go BEFORE the
        // "terminal" profile (emulator) to win by title.
        // Commands are typed with type:...>>enter. RULE: nothing destructive in the catalogue —
        // the button types into whatever is focused (vim, a password prompt...), so the worst
        // that can happen should be one extra "ls".
        // Designed as a "daily console user" set from the PHONE: the most valuable thing isn't
        // typed commands but up-arrow (history) + Run -> re-launch the build/server without
        // touching the keyboard. Cancel (ctrl+c) to kill something hung while away from the PC.
        profile(
            "shell-pwsh",
            &["powershell", "pwsh"],
            vec![
                // NOTE: "ls -lh" does NOT exist in PowerShell (ls = alias for Get-ChildItem; fails
                // with "A parameter cannot be found that matches parameter name 'lh'"). The
                // equivalent of "more detail" is -Force, which also lists hidden items.
                b("Listar", "folder", "type:ls -Force>>enter"),
                b("Subir nivel", "back", "type:cd ..>>enter"),
                b("Limpiar", "eraser", "type:cls>>enter"),
                b("Anterior", "scrollup", "up"), // history: recalls the last command
                b("Ejecutar", "play", "enter"),  // ...and runs it. The key combo from the phone.
                b("Cancelar", "close", "ctrl+c"),
                bx("Siguiente", "scrolldown", "down"),
                bx("Dónde estoy", "pin", "type:pwd>>enter"),
                bx("Autocompletar", "tab", "tab"), // handy after dictating a half-typed path
                // Git: only make sense inside a repo -> all extras.
                bx("Git estado", "find", "type:git status>>enter"),
                bx("Git pull", "download", "type:git pull>>enter"),
                bx("Git push", "upload", "type:git push>>enter"),
                bx("Git log", "history", "type:git log --oneline -10>>enter"),
                bx("Git diff", "replace", "type:git diff>>enter"),
                bx("Copiar", "copy", "ctrl+shift+c"),
                bx("Pegar", "paste", "ctrl+shift+v"),
                bx(
                    "Nueva carpeta",
                    "newfolder",
                    "prompt:Nombre de la carpeta=type:mkdir {}>>enter",
                ),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "shell-cmd",
            &["cmd.exe", "símbolo del sistema", "command prompt"],
            vec![
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
                bx("Copiar", "copy", "ctrl+shift+c"),
                bx("Pegar", "paste", "ctrl+shift+v"),
                bx(
                    "Nueva carpeta",
                    "newfolder",
                    "prompt:Nombre de la carpeta=type:md {}>>enter",
                ),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "shell-bash",
            &["wsl", "ubuntu", "debian", "bash", "mingw"],
            vec![
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
                bx("Copiar", "copy", "ctrl+shift+c"),
                bx("Pegar", "paste", "ctrl+shift+v"),
                bx(
                    "Nueva carpeta",
                    "newfolder",
                    "prompt:Nombre de la carpeta=type:mkdir {}>>enter",
                ),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        // Terminal emulator (tabs/panes/clipboard). Fallback for when the title doesn't give the
        // shell away. NOTE: the process name is "WindowsTerminal" WITHOUT a space — the string
        // "windows terminal" never matched (bug fixed 2026-07-19).
        profile(
            "terminal",
            &["windowsterminal", "terminal", "warp", "conhost"],
            vec![
                // NOTE: ctrl+c in a console is SIGINT (kills the process). Windows Terminal/
                // PowerShell accept ctrl+shift+c/v for the clipboard.
                b("Copiar", "copy", "ctrl+shift+c"),
                b("Pegar", "paste", "ctrl+shift+v"),
                b("Nueva pestaña", "new", "ctrl+shift+t"),
                b("Buscar", "find", "ctrl+f"),
                bx("Cerrar pestaña", "close", "ctrl+shift+w"),
                bx("Dividir", "new", "alt+shift+d"),
                bx("Nueva ventana", "new", "ctrl+shift+n"),
                bx("Panel sgte", "redo", "ctrl+tab"),
            ],
        ),
        profile(
            "notepadpp",
            &["notepad++"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Reemplazar", "replace", "ctrl+h"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Ir a línea", "find", "ctrl+g"),
                bx("Duplicar línea", "new", "ctrl+d"),
                bx("Comentar", "comment", "ctrl+q"),
                bx("Imprimir", "print", "ctrl+p"),
                bx("Guardar todo", "save", "ctrl+shift+s"),
                bx("Sel. todo", "selectall", "ctrl+a"),
                bxd("Eliminar", "delete", "delete"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "explorer",
            &["explorador", "file explorer", "explorer"],
            vec![
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                b("Nueva carpeta", "newfolder", "ctrl+shift+n"),
                b("Renombrar", "rename", "f2"),
                bx("Cortar", "cut", "ctrl+x"),
                bx("Atrás", "undo", "alt+left"),
                bx("Subir nivel", "redo", "alt+up"),
                bx("Propiedades", "settings", "alt+enter"),
                bx("Buscar", "find", "ctrl+f"),
                bx("Sel. todo", "selectall", "ctrl+a"),
                bxd("Eliminar", "delete", "delete"),
            ],
        ),
        // --- Batch 1: apps/web added towards ~100 (go before "browser" to win by title) ---
        profile(
            "chatgpt",
            &["chatgpt"],
            vec![
                b("Nuevo chat", "new", "ctrl+shift+o"),
                b("Barra lateral", "tab", "ctrl+shift+s"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Borrar chat", "delete", "ctrl+shift+backspace"),
                bx("Enfocar entrada", "text", "shift+escape"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "gcalendar",
            &["google calendar"],
            vec![
                b("Crear", "new", "c"),
                b("Hoy", "home", "t"),
                b("Buscar", "find", "/"),
                b("Semana", "tab", "w"),
                bx("Día", "tab", "d"),
                bx("Mes", "tab", "m"),
                bx("Agenda", "apps", "a"),
                bx("Año", "tab", "y"),
            ],
        ),
        profile(
            "onenote",
            &["onenote"],
            vec![
                b("Nueva página", "new", "ctrl+n"),
                b("Negrita", "bold", "ctrl+b"),
                b("Buscar", "find", "ctrl+e"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Cursiva", "italic", "ctrl+i"),
                bx("Subrayado", "underline", "ctrl+u"),
                bx("Resaltar", "highlight", "ctrl+shift+h"),
                bx("Imprimir", "print", "ctrl+p"),
            ],
        ),
        profile(
            "thunderbird",
            &["thunderbird"],
            vec![
                b("Nuevo", "new", "ctrl+n"),
                b("Responder", "reply", "ctrl+r"),
                b("Reenviar", "forward", "ctrl+l"),
                b("Buscar", "find", "ctrl+shift+k"),
                bx("Resp. todos", "replyall", "ctrl+shift+r"),
                bx("Enviar", "send", "ctrl+enter"),
                bx("Eliminar", "delete", "delete"),
                bx("Archivar", "archive", "a"),
            ],
        ),
        profile(
            "sublime",
            &["sublime"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Paleta", "new", "ctrl+shift+p"),
                b("Ir a", "find", "ctrl+p"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Reemplazar", "replace", "ctrl+h"),
                bx("Comentar", "comment", "ctrl+/"),
                bx("Duplicar línea", "duplicate", "ctrl+shift+d"),
                bx("Ir a línea", "find", "ctrl+g"),
                bx("Sel. todo", "selectall", "ctrl+a"),
                bxd("Eliminar", "delete", "delete"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "obsidian",
            &["obsidian"],
            vec![
                b("Nueva nota", "new", "ctrl+n"),
                b("Cambiador", "find", "ctrl+o"),
                b("Editar/Ver", "redo", "ctrl+e"),
                b("Paleta", "new", "ctrl+p"),
                bx("Buscar", "find", "ctrl+shift+f"),
                bx("Negrita", "bold", "ctrl+b"),
                bx("Cursiva", "italic", "ctrl+i"),
                bx("Enlace", "link", "ctrl+k"),
            ],
        ),
        profile(
            "gimp",
            &["gimp"],
            vec![
                b("Deshacer", "undo", "ctrl+z"),
                b("Pincel", "brush", "p"),
                b("Borrador", "eraser", "shift+e"),
                b("Guardar", "save", "ctrl+s"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Lápiz", "pencil", "n"),
                bx("Texto", "text", "t"),
                bx("Rellenar", "fill", "shift+b"),
                bx("Exportar", "upload", "ctrl+e"),
                bx("Rehacer", "redo", "ctrl+y"),
            ],
        ),
        profile(
            "canva",
            &["canva"],
            vec![
                b("Texto", "text", "t"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Duplicar", "duplicate", "ctrl+d"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+y"),
                bx("Rectángulo", "rect", "r"),
                bx("Círculo", "ellipse", "c"),
                bx("Línea", "line", "l"),
                bx("Agrupar", "group", "ctrl+g"),
            ],
        ),
        // Paint: the toolbar has no shortcuts -> pressed via UI Automation ("uia:<name>"). Real
        // toolbar names (Paint Win11 ES) verified live with UIAutomation.
        profile(
            "paint",
            &["paint"],
            vec![
                b("Lápiz", "pencil", "uia:Lápiz"),
                b("Relleno", "fill", "uia:Relleno"),
                b("Texto", "text", "uia:Texto"),
                b("Borrador", "eraser", "uia:Borrador"),
                b("Selector color", "colorpick", "uia:Selector de colores"),
                b("Rectángulo", "rect", "uia:Rectángulo"),
                b("Elipse", "ellipse", "uia:Elipse"),
                b("Línea", "line", "uia:Línea"),
                bx("Lupa", "zoomin", "uia:Lupa"),
                bx("Recortar", "crop", "uia:Recortar"),
                bx("Girar dcha.", "rotate", "uia:Girar>>Girar 90º a la derecha"),
                bx(
                    "Girar izq.",
                    "rotate",
                    "uia:Girar>>Girar 90º a la izquierda",
                ),
                bx("Deshacer", "undo", "ctrl+z"),
                bx("Rehacer", "redo", "ctrl+y"),
                bx("Guardar", "save", "ctrl+s"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
                // Color picker: a single button opens a local palette on the phone ("colorpicker:...");
                // each swatch sends "uia:<Name>". The palette entries are UIA ListItems with
                // SelectionItemPattern (already supported). EXACT names from Paint Win11 ES (verified
                // live); the exact match avoids colliding with "Color 1: Black"/"Color 2: White". The
                // hex value is only used to paint the swatch.
                b(
                    "Colores",
                    "palette",
                    "colorpicker:Negro=000000;Blanco=FFFFFF;Gris=7F7F7F;Rojo=ED1C24;\
              Naranja=FF7F27;Amarillo=FFF200;Verde=22B14C;Turquesa=00A2E8;Añil=3F48CC;\
              Púrpura=A349A4;Marrón=B97A57;Rosa=FFAEC9",
                ),
            ],
        ),
        profile(
            "davinci",
            &["davinci", "resolve"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Entrada", "login", "i"),
                b("Salida", "logout", "o"),
                b("Cortar", "cut", "ctrl+b"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
            ],
        ),
        profile(
            "audacity",
            &["audacity"],
            vec![
                b("Reproducir", "play", "space"),
                b("Grabar", "record", "r"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Cortar", "cut", "ctrl+x"),
                b("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
                bx("Silencio", "mute", "ctrl+l"),
                bx("Rehacer", "redo", "ctrl+y"),
            ],
        ),
        profile(
            "jetbrains",
            &[
                "intellij",
                "pycharm",
                "webstorm",
                "phpstorm",
                "rider",
                "goland",
                "clion",
                "datagrip",
                "android studio",
            ],
            vec![
                b("Buscar", "find", "ctrl+f"),
                b("Comentar", "comment", "ctrl+/"),
                b("Reformatear", "format", "ctrl+alt+l"),
                b("Ejecutar", "play", "shift+f10"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Reemplazar", "replace", "ctrl+r"),
                bx("Ir a línea", "find", "ctrl+g"),
                bx("Renombrar", "rename", "shift+f6"),
                bx("Buscar acción", "find", "ctrl+shift+a"),
            ],
        ),
        profile(
            "lightroom",
            &["lightroom"],
            vec![
                b("Cuadrícula", "apps", "g"),
                b("Lupa", "zoomin", "e"),
                b("Revelar", "brush", "d"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Importar", "download", "ctrl+shift+i"),
                bx("Exportar", "upload", "ctrl+shift+e"),
                bx("Marcar", "star", "p"),
                bx("Rechazar", "delete", "x"),
            ],
        ),
        profile(
            "telegram",
            &["telegram"],
            vec![
                b("Buscar", "find", "ctrl+f"),
                b("Sgte chat", "redo", "alt+down"),
                b("Chat ant.", "undo", "alt+up"),
                b("Guardados", "star", "ctrl+0"),
            ],
        ),
        // --- Batch 2: more apps/web towards ~100 (also before "browser") ---
        profile(
            "gmeet",
            &["google meet", "meet -"],
            vec![
                b("Silenciar", "mic", "ctrl+d"),
                b("Cámara", "video", "ctrl+e"),
                b("Levantar mano", "hand", "ctrl+alt+h"),
                bx("Pant. completa", "fullscreen", "f"),
            ],
        ),
        profile(
            "trello",
            &["trello"],
            vec![
                b("Buscar", "find", "/"),
                b("Filtrar", "filter", "f"),
                b("Tableros", "apps", "b"),
                b("Mis tarjetas", "star", "q"),
                bx("Archivar", "archive", "c"),
                bx("Etiquetas", "link", "l"),
                bx("Vencimiento", "calendar", "d"),
                bx("Miembros", "people", "m"),
            ],
        ),
        profile(
            "todoist",
            &["todoist"],
            vec![
                b("Añadir rápido", "new", "q"),
                b("Añadir tarea", "new", "a"),
                bx("Deshacer", "undo", "ctrl+z"),
                bx("Sincronizar", "refresh", "ctrl+r"),
            ],
        ),
        profile(
            "linear",
            &["linear"],
            vec![
                b("Crear", "new", "c"),
                b("Buscar", "find", "/"),
                b("Asignar", "assign", "a"),
                b("Estado", "redo", "s"),
                bx("Prioridad", "star", "p"),
                bx("Etiqueta", "link", "l"),
                bx("Vencimiento", "calendar", "d"),
            ],
        ),
        profile(
            "jira",
            &["jira"],
            vec![
                b("Crear", "new", "c"),
                b("Buscar", "find", "/"),
                b("Asignar", "assign", "a"),
                b("Editar", "redo", "e"),
                bx("Comentar", "comment", "m"),
                bx("Asignarme", "star", "i"),
            ],
        ),
        profile(
            "miro",
            &["miro"],
            vec![
                b("Texto", "text", "t"),
                b("Nota", "note", "s"),
                b("Lápiz", "pencil", "p"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Rectángulo", "rect", "r"),
                bx("Marco", "frame", "f"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
            ],
        ),
        profile(
            "postman",
            &["postman"],
            vec![
                b("Enviar", "send", "ctrl+enter"),
                b("Guardar", "save", "ctrl+s"),
                b("Nuevo", "new", "ctrl+n"),
                b("Buscar", "find", "ctrl+f"),
                bx("Nueva pestaña", "new", "ctrl+t"),
                bx("Cerrar pestaña", "close", "ctrl+w"),
            ],
        ),
        profile(
            "github",
            &["github"],
            vec![
                b("Buscar", "find", "/"),
                b("Archivos", "find", "t"),
                b("Ir a línea", "find", "l"),
                b("Cambiar rama", "tab", "w"),
                bx("Editor web", "text", "."),
                bx("Blame", "comment", "b"),
            ],
        ),
        profile(
            "overleaf",
            &["overleaf"],
            vec![
                b("Guardar/Compilar", "play", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+y"),
                bx("Comentar", "comment", "ctrl+/"),
            ],
        ),
        profile(
            "gkeep",
            &["google keep"],
            vec![
                b("Nueva nota", "new", "c"),
                b("Buscar", "find", "/"),
                b("Lista nueva", "new", "l"),
                b("Archivar", "archive", "e"),
                bx("Fijar", "pin", "f"),
            ],
        ),
        profile(
            "aftereffects",
            &["after effects"],
            vec![
                b("Vista previa", "play", "space"),
                b("Selección", "cursor", "v"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Guardar", "save", "ctrl+s"),
                bx("Mano", "hand", "h"),
                bx("Zoom", "zoomin", "z"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
                bx("Copiar", "copy", "ctrl+c"),
            ],
        ),
        profile(
            "krita",
            &["krita"],
            vec![
                b("Pincel", "brush", "b"),
                b("Borrador", "eraser", "e"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Guardar", "save", "ctrl+s"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Zoom +", "zoomin", "ctrl+add"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
            ],
        ),
        profile(
            "inkscape",
            &["inkscape"],
            vec![
                b("Seleccionar", "cursor", "s"),
                b("Lápiz", "pencil", "p"),
                b("Texto", "text", "t"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Rectángulo", "rect", "r"),
                bx("Elipse", "ellipse", "e"),
                bx("Guardar", "save", "ctrl+s"),
                bx("Rehacer", "redo", "ctrl+y"),
            ],
        ),
        // --- Batch 3: more apps/web towards ~100 (also before "browser") ---
        profile(
            "whatsapp",
            &["whatsapp"],
            vec![
                b("Nuevo chat", "new", "ctrl+n"),
                b("Buscar", "find", "ctrl+f"),
                b("Sgte chat", "next", "ctrl+shift+]"),
                b("Chat ant.", "prev", "ctrl+shift+["),
                bx("Archivar", "archive", "ctrl+e"),
                bx("Silenciar", "mute", "ctrl+shift+m"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "twitch",
            &["twitch"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Silenciar", "mute", "m"),
                b("Pant. completa", "fullscreen", "f"),
                b("Teatro", "video", "alt+t"),
            ],
        ),
        profile(
            "netflix",
            &["netflix"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Pant. completa", "fullscreen", "f"),
                b("Silenciar", "mute", "m"),
            ],
        ),
        profile(
            "xtwitter",
            &["twitter", "/ x"],
            vec![
                b("Nuevo post", "new", "n"),
                b("Buscar", "find", "/"),
                b("Me gusta", "star", "l"),
                b("Responder", "reply", "r"),
                bx("Repost", "redo", "t"),
            ],
        ),
        profile(
            "eclipse",
            &["eclipse"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Comentar", "comment", "ctrl+/"),
                b("Ejecutar", "play", "ctrl+f11"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Formato", "format", "ctrl+shift+f"),
                bx("Organizar imports", "redo", "ctrl+shift+o"),
            ],
        ),
        profile(
            "dbeaver",
            &["dbeaver"],
            vec![
                b("Ejecutar", "play", "ctrl+enter"),
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Comentar", "comment", "ctrl+/"),
                bx("Formato", "format", "ctrl+shift+f"),
            ],
        ),
        profile(
            "unity",
            &["unity"],
            vec![
                b("Reproducir", "play", "ctrl+p"),
                b("Mano", "hand", "q"),
                b("Mover", "move", "w"),
                b("Rotar", "rotate", "e"),
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Duplicar", "duplicate", "ctrl+d"),
                bx("Buscar", "find", "ctrl+f"),
            ],
        ),
        profile(
            "godot",
            &["godot"],
            vec![
                b("Ejecutar", "play", "f5"),
                b("Escena actual", "play", "f6"),
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Comentar", "comment", "ctrl+k"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
            ],
        ),
        profile(
            "capcut",
            &["capcut"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Dividir", "cut", "ctrl+b"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Guardar", "save", "ctrl+s"),
                bx("Eliminar", "delete", "delete"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
            ],
        ),
        profile(
            "affinity",
            &["affinity"],
            vec![
                b("Mover", "move", "v"),
                b("Pincel", "brush", "b"),
                b("Borrador", "eraser", "e"),
                b("Texto", "text", "t"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Guardar", "save", "ctrl+s"),
                bx("Recortar", "crop", "c"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
            ],
        ),
        profile(
            "ableton",
            &["ableton"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Grabar", "record", "f9"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Guardar", "save", "ctrl+s"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
                bx("Bucle", "repeat", "ctrl+l"),
            ],
        ),
        profile(
            "reaper",
            &["reaper"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Grabar", "record", "ctrl+r"),
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Marcador", "star", "m"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
            ],
        ),
        // --- Batch 4: more apps/web towards ~100 (also before "browser") ---
        profile(
            "lowriter",
            &["libreoffice writer"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Negrita", "bold", "ctrl+b"),
                b("Buscar", "find", "ctrl+f"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Cursiva", "italic", "ctrl+i"),
                bx("Subrayado", "underline", "ctrl+u"),
                bx("Rehacer", "redo", "ctrl+y"),
                bx("Imprimir", "print", "ctrl+p"),
            ],
        ),
        profile(
            "localc",
            &["libreoffice calc"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Rehacer", "redo", "ctrl+y"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Negrita", "bold", "ctrl+b"),
                bx("Cursiva", "italic", "ctrl+i"),
                bx("Imprimir", "print", "ctrl+p"),
            ],
        ),
        profile(
            "loimpress",
            &["libreoffice impress"],
            vec![
                // Presenter pack (in presentation mode: arrows advance slides, b = black, w = white).
                b("Siguiente", "next", "right"),
                b("Anterior", "prev", "left"),
                b("Presentar", "play", "f5"),
                b("Negro", "dark", "b"),
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Blanco", "light", "w"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
                bx("Negrita", "bold", "ctrl+b"),
                bx("Rehacer", "redo", "ctrl+y"),
                bx("Imprimir", "print", "ctrl+p"),
            ],
        ),
        profile(
            "notepad",
            &["notepad"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Reemplazar", "replace", "ctrl+h"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Cortar", "cut", "ctrl+x"),
                bx("Ir a línea", "find", "ctrl+g"),
                bx("Imprimir", "print", "ctrl+p"),
                bx("Sel. todo", "selectall", "ctrl+a"),
                bxd("Eliminar", "delete", "delete"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        profile(
            "sumatra",
            &["sumatra"],
            vec![
                b("Buscar", "find", "ctrl+f"),
                b("Ir a página", "find", "ctrl+g"),
                b("Zoom +", "zoomin", "ctrl+add"),
                b("Zoom -", "zoomout", "ctrl+subtract"),
                bx("Pant. completa", "fullscreen", "f11"),
                bx("Imprimir", "print", "ctrl+p"),
            ],
        ),
        profile(
            "foxit",
            &["foxit"],
            vec![
                b("Buscar", "find", "ctrl+f"),
                b("Guardar", "save", "ctrl+s"),
                b("Imprimir", "print", "ctrl+p"),
                b("Copiar", "copy", "ctrl+c"),
                bx("Zoom +", "zoomin", "ctrl+add"),
                bx("Zoom -", "zoomout", "ctrl+subtract"),
            ],
        ),
        profile(
            "shotcut",
            &["shotcut"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Dividir", "cut", "s"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Guardar", "save", "ctrl+s"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
            ],
        ),
        profile(
            "musescore",
            &["musescore"],
            vec![
                b("Reproducir", "play", "space"),
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
                bx("Imprimir", "print", "ctrl+p"),
            ],
        ),
        profile(
            "soundcloud",
            &["soundcloud"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Siguiente", "next", "shift+right"),
                b("Anterior", "prev", "shift+left"),
                b("Me gusta", "star", "l"),
            ],
        ),
        profile(
            "potplayer",
            &["potplayer"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Pant. completa", "fullscreen", "enter"),
                b("Silenciar", "mute", "m"),
                b("+ Volumen", "vol", "up"),
                b("- Volumen", "vol", "down"),
            ],
        ),
        profile(
            "primevideo",
            &["prime video"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Pant. completa", "fullscreen", "f"),
                b("Silenciar", "mute", "m"),
            ],
        ),
        profile(
            "disney",
            &["disney+"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Pant. completa", "fullscreen", "f"),
                b("Silenciar", "mute", "m"),
            ],
        ),
        // --- Batch 5: more apps/web towards ~100 (also before "browser") ---
        profile(
            "gslides",
            &["google slides", "presentaciones de google"],
            vec![
                // Presenter pack (in presentation mode arrows advance slides; b = black, w = white).
                b("Siguiente", "next", "right"),
                b("Anterior", "prev", "left"),
                b("Presentar", "play", "ctrl+f5"),
                b("Negro", "dark", "b"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Blanco", "light", "w"),
                bx("Nueva diap.", "new", "ctrl+m"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+y"),
                bx("Duplicar", "duplicate", "ctrl+d"),
                bx("Imprimir", "print", "ctrl+p"),
            ],
        ),
        profile(
            "evernote",
            &["evernote"],
            vec![
                b("Nueva nota", "new", "ctrl+n"),
                b("Buscar", "find", "ctrl+shift+f"),
                b("Negrita", "bold", "ctrl+b"),
                b("Cursiva", "italic", "ctrl+i"),
                bx("Subrayado", "underline", "ctrl+u"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
            ],
        ),
        profile(
            "joplin",
            &["joplin"],
            vec![
                b("Nueva nota", "new", "ctrl+n"),
                b("Buscar", "find", "ctrl+f"),
                b("Negrita", "bold", "ctrl+b"),
                b("Cursiva", "italic", "ctrl+i"),
                bx("Enlace", "link", "ctrl+k"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
            ],
        ),
        profile(
            "rstudio",
            &["rstudio"],
            vec![
                b("Ejecutar", "play", "ctrl+enter"),
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Comentar", "comment", "ctrl+shift+c"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
            ],
        ),
        profile(
            "visio",
            &["visio"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                b("Buscar", "find", "ctrl+f"),
                bx("Rehacer", "redo", "ctrl+y"),
                bx("Duplicar", "duplicate", "ctrl+d"),
            ],
        ),
        profile(
            "coreldraw",
            &["coreldraw"],
            vec![
                b("Deshacer", "undo", "ctrl+z"),
                b("Guardar", "save", "ctrl+s"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
                bx("Duplicar", "duplicate", "ctrl+d"),
            ],
        ),
        profile(
            "kdenlive",
            &["kdenlive"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
            ],
        ),
        profile(
            "scribus",
            &["scribus"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
                bx("Buscar", "find", "ctrl+f"),
            ],
        ),
        profile(
            "mpchc",
            &["mpc-hc", "mpc-be", "media player classic"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Pant. completa", "fullscreen", "alt+enter"),
                b("+ Volumen", "vol", "up"),
                b("- Volumen", "vol", "down"),
            ],
        ),
        profile(
            "vimeo",
            &["vimeo"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Pant. completa", "fullscreen", "f"),
                b("Silenciar", "mute", "m"),
            ],
        ),
        profile(
            "crunchyroll",
            &["crunchyroll"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Pant. completa", "fullscreen", "f"),
                b("Silenciar", "mute", "m"),
            ],
        ),
        profile(
            "hbomax",
            &["hbo max", "hbomax", "max -"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Pant. completa", "fullscreen", "f"),
                b("Silenciar", "mute", "m"),
            ],
        ),
        // --- Batch 6: closing the set towards ~100 (also before "browser") ---
        profile(
            "insomnia",
            &["insomnia"],
            vec![
                b("Enviar", "send", "ctrl+enter"),
                b("Nuevo", "new", "ctrl+n"),
                b("Buscar", "find", "ctrl+f"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
            ],
        ),
        profile(
            "hulu",
            &["hulu"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Pant. completa", "fullscreen", "f"),
                b("Silenciar", "mute", "m"),
            ],
        ),
        profile(
            "plex",
            &["plex"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Pant. completa", "fullscreen", "f"),
                b("Silenciar", "mute", "m"),
            ],
        ),
        profile(
            "cubase",
            &["cubase"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
            ],
        ),
        profile(
            "camtasia",
            &["camtasia"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+y"),
            ],
        ),
        profile(
            "filmora",
            &["filmora"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Eliminar", "delete", "delete"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+y"),
            ],
        ),
        profile(
            "vegas",
            &["vegas pro"],
            vec![
                b("Play/Pausa", "play", "space"),
                b("Dividir", "cut", "s"),
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+shift+z"),
            ],
        ),
        profile(
            "geany",
            &["geany"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Reemplazar", "replace", "ctrl+h"),
                b("Ejecutar", "play", "f5"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Comentar", "comment", "ctrl+e"),
                bx("Ir a línea", "find", "ctrl+l"),
            ],
        ),
        profile(
            "spyder",
            &["spyder"],
            vec![
                b("Ejecutar", "play", "f5"),
                b("Guardar", "save", "ctrl+s"),
                b("Buscar", "find", "ctrl+f"),
                b("Deshacer", "undo", "ctrl+z"),
                bx("Comentar", "comment", "ctrl+1"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
            ],
        ),
        profile(
            "autocad",
            &["autocad"],
            vec![
                b("Guardar", "save", "ctrl+s"),
                b("Deshacer", "undo", "ctrl+z"),
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                bx("Rehacer", "redo", "ctrl+y"),
                bx("Buscar", "find", "ctrl+f"),
            ],
        ),
        // --- Gaming / streaming ---
        // OBS over websocket (obs:): works even with the game in the foreground, with live state
        // on the button. While OBS is recording or streaming, this profile gets pinned (see
        // layout_for) and the user's scenes are added to it as buttons.
        profile(
            "obs",
            &["obs studio", "obs64"],
            vec![
                b("Grabar", "record", "obs:record"),
                bd("Directo", "stream", "obs:stream"),
                b("Mic", "mic", "obs:mic"),
                b("Clip", "clip", "obs:replay"),
                bx("Buffer clips", "clip", "obs:replaybuffer"),
            ],
        ),
        profile(
            "steam",
            &["steam"],
            vec![
                b("Overlay", "apps", "shift+tab"),
                b("Captura", "screenshot", "f12"),
            ],
        ),
        // --- Generic browser (any non-specific tab) ---
        // Prioritized for browsing/reading (from the couch): back/forward, reload and scroll
        // up/down (press-and-hold = continuous scroll). Copy/Paste move to extras.
        profile(
            "browser",
            &["chrome", "edge", "firefox", "brave", "opera", "vivaldi"],
            vec![
                b("Atrás", "back", "alt+left"),
                b("Adelante", "fwdnav", "alt+right"),
                b("Recargar", "refresh", "f5"),
                b("Nueva pestaña", "new", "ctrl+t"),
                // Scroll/mouse live in the phone's global "Mouse" button (top bar, full screen).
                bx("Subir", "scrollup", "scroll:-3"),
                bx("Bajar", "scrolldown", "scroll:3"),
                bx("Cerrar pestaña", "close", "ctrl+w"),
                bx("Buscar", "find", "ctrl+f"),
                bx("Copiar", "copy", "ctrl+c"),
                bx("Pegar", "paste", "ctrl+v"),
                bx("Reabrir pestaña", "redo", "ctrl+shift+t"),
                bx("Favorito", "star", "ctrl+d"),
                bx("Historial", "history", "ctrl+h"),
                bx("Descargas", "download", "ctrl+j"),
                bx("Incógnito", "new", "ctrl+shift+n"),
                bx("Zoom +", "zoomin", "ctrl+add"),
                bx("Zoom -", "zoomout", "ctrl+subtract"),
                bx("Dictar", "mic", "dictate"),
            ],
        ),
        // --- Fallback (unrecognized app). Volume and screenshot live in the phone's fixed dock;
        // Play/Pause only on profiles for apps that play media (does nothing in Word). ---
        profile(
            "generic",
            &[],
            vec![
                b("Copiar", "copy", "ctrl+c"),
                b("Pegar", "paste", "ctrl+v"),
                b("Deshacer", "undo", "ctrl+z"),
                // Windows Game Bar: FIXED system shortcuts, work inside any game.
                bx("Clip 30s", "clip", "win+alt+g"),
                bx("Grabar juego", "record", "win+alt+r"),
                bx("Game Bar", "apps", "win+g"),
                bx("Cortar", "cut", "ctrl+x"),
                bx("Rehacer", "redo", "ctrl+y"),
                bx("Guardar", "save", "ctrl+s"),
                bx("Buscar", "find", "ctrl+f"),
                bx("Imprimir", "print", "ctrl+p"),
            ],
        ),
    ];
    list
}
/// Keys per page in the decks shipped by default. Decks are authored against the reference 5×3
/// device; the host repaginates to whatever grid the client declares in `hello`, so this number is
/// an authoring convention, not a constraint on the client.
#[cfg(test)]
const REFERENCE_PAGE: usize = 15;

/// A small Manual starter: only actions a non-technical user can recognize immediately.
/// Navigation to Auto, Launcher and the window switcher lives permanently in the phone shell, so
/// repeating it as editable keys teaches the wrong mental model and wastes scarce cells.
pub(crate) fn default_decks() -> Vec<Deck> {
    let keys = [
        ("Copiar", "copy", "ctrl+c"),
        ("Pegar", "paste", "ctrl+v"),
        ("Cortar", "cut", "ctrl+x"),
        ("Deshacer", "undo", "ctrl+z"),
        ("Rehacer", "redo", "ctrl+y"),
        ("Sel. todo", "selectall", "ctrl+a"),
        ("Captura", "screenshot", "screenshot"),
        ("Trackpad", "mouse", "trackpad"),
        ("Dictar", "mic", "dictate"),
    ]
    .into_iter()
    .enumerate()
    .map(|(pos, (label, icon, action))| Key {
        pos,
        label: label.into(),
        icon: icon.into(),
        action: Some(action.into()),
        kind: KeyKind::Action,
        ..Default::default()
    })
    .collect();

    let launcher = launcher_deck();
    let mut decks = vec![Deck {
        id: "starter".into(),
        name: "KiBoard".into(),
        icon: "deck".into(),
        pages: vec![Page {
            id: "p0".into(),
            name: String::new(),
            keys,
        }],
    }];
    decks.extend(launcher);
    decks
}

/// Adds a generated Launcher to decks that already exist, and a key that reaches it.
///
/// Deliberately ADDITIVE: it never edits or reorders what the user has, because these are their
/// decks and this runs behind their back on a launch they did not ask anything of. Kept out of
/// `Config::load` so it can be tested without a real app catalogue behind it.
fn backfill_launcher(decks: &mut Vec<Deck>, launcher: Deck) {
    // Any deck already using the reserved id is normalized by `replace_launcher` later in the
    // migration; this additive step only avoids creating a duplicate.
    if decks.iter().any(|d| d.id == launcher.id) {
        return;
    }
    // The jump key only if nothing already reaches it — the user may have made their own.
    let reachable = decks
        .iter()
        .flat_map(|d| &d.pages)
        .flat_map(|p| &p.keys)
        .any(|k| k.action.as_deref() == Some("deck:launcher"));
    if !reachable {
        if let Some(page) = decks.first_mut().and_then(|d| d.pages.first_mut()) {
            // Appended past the last key rather than into a gap: a hole in the middle is a hole
            // the user left on purpose, and the phone paginates a long page by itself.
            let pos = page
                .keys
                .iter()
                .map(|k| k.pos)
                .max()
                .map_or(0, |last| last + 1);
            page.keys.push(Key {
                pos,
                label: "Launcher".into(),
                icon: "apps".into(),
                action: Some("deck:launcher".into()),
                kind: KeyKind::Action,
                ..Default::default()
            });
        }
    }
    decks.push(launcher);
}

/// Safety cap on the generated Launcher deck. The rolling-month filter normally keeps this much
/// lower; the cap only protects a machine that genuinely used hundreds of GUI apps in one month.
const LAUNCHER_APPS: usize = 60;

/// A "Launcher" deck built from the machine's own app catalogue. It is reached through the Auto
/// controls but uses the deck transport for paging. Icons and running state are attached per-send
/// by `engine::deck`, not stored.
///
/// Only apps used in the rolling 30-day window, newest first. Windows UserAssist bootstraps the
/// first run; KiBoard then persists every foreground app itself, so ordering remains dependable
/// even for launch paths Windows does not record.
fn launcher_deck() -> Option<Deck> {
    let windir = std::env::var("SystemRoot")
        .unwrap_or_else(|_| r"C:\Windows".into())
        .to_lowercase();
    let apps: Vec<&crate::platform::apps::App> = crate::platform::apps::recent_catalogue()
        .into_iter()
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
    // ONE page, not chunks of REFERENCE_PAGE. A `Page` is only reachable through a key that
    // navigates to it, and a generated deck has no such keys — chunked, everything past the first
    // fifteen apps was unreachable (the phone showed two dots for sixty-one keys). A single long
    // page is exactly what repagination is for: the host cuts it to the client's grid and the user
    // swipes.
    Some(Deck {
        id: "launcher".into(),
        name: "Launcher".into(),
        icon: "apps".into(),
        pages: vec![Page {
            id: "p0".into(),
            name: String::new(),
            keys,
        }],
    })
}

/// Replaces KiBoard's automatic Launcher. The id is reserved: older builds allowed editing or
/// deleting this deck, but Launcher is derived from the machine's live/recent applications and is
/// not Manual user content. Returns whether persisted decks changed.
fn replace_launcher(decks: &mut Vec<Deck>, next: Option<Deck>) -> bool {
    let index = decks.iter().position(|deck| deck.id == "launcher");
    match (index, next) {
        (Some(index), Some(next)) => {
            let before = serde_json::to_string(&decks[index]).unwrap_or_default();
            let after = serde_json::to_string(&next).unwrap_or_default();
            if before == after {
                return false;
            }
            decks[index] = next;
        }
        (Some(index), None) => {
            decks.remove(index);
        }
        (None, Some(next)) => decks.push(next),
        (None, None) => return false,
    }
    true
}

/// Reorders the live generated Launcher after the foreground app changes. The editor never receives
/// it; connected phones always get the current persisted automatic deck.
pub(crate) fn refresh_launcher() {
    let next = launcher_deck();
    let changed = {
        let mut cfg = config().lock().unwrap();
        let changed = replace_launcher(&mut cfg.decks, next);
        if changed {
            cfg.save();
        }
        changed
    };
    if changed {
        crate::net::ws::push_manual_layouts();
    }
}

// ---------------------------------------------------------------------------
// Persistent configuration
// ---------------------------------------------------------------------------

/// Version of the built-in profiles. Bump it when `default_profiles` changes so already-installed
/// hosts refresh them (keeping the token and pairing).
const PROFILES_VERSION: u32 = 50;

/// Shape of `config.json`. Bumped when the model changes in a way `#[serde(default)]` cannot
/// absorb; `load` backs the old file up to `config.v1.bak` before rewriting it.
const CONFIG_VERSION: u32 = 2;

/// How far the seeded decks have been brought forward. 1 = Launcher exists, 2 = recent-only.
///
/// Separate from `PROFILES_VERSION` because decks are the USER's, not ours: profiles get replaced
/// wholesale on a bump, decks may only ever be added to. And separate from `CONFIG_VERSION`
/// because nothing about the file's shape changed.
const DECKS_VERSION: u32 = 3;

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
    /// Manual mode is an advanced, opt-in feature. Decks remain stored while it is hidden.
    #[serde(default)]
    pub(crate) manual_enabled: bool,
    /// The opt-in explanation is shown once, by whichever paired surface enables Manual first.
    #[serde(default)]
    pub(crate) manual_intro_seen: bool,
    /// How far `decks` has been brought forward by seeding. 0 = seeded before the Launcher existed.
    #[serde(default)]
    decks_version: u32,
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

/// Where everything KiBoard writes lives.
///
/// KIBOARD_CONFIG_DIR points a dev build at a throwaway config. Without it, testing means running
/// against the real one — which holds live pairing tokens and gets rewritten by the v1->v2
/// migration on first load.
pub(crate) fn config_dir() -> std::path::PathBuf {
    // `cargo test` is not exempt: any test that reaches `config()` runs `Config::load`, which ENDS
    // BY SAVING. An un-isolated test run therefore rewrites the user's live config.json — found
    // the hard way on 2026-07-29, when running the F6 suite moved its timestamp. The environment
    // variable is a thing a human has to remember; this is not.
    #[cfg(test)]
    let dir = std::env::temp_dir().join("KiBoard-test");
    #[cfg(not(test))]
    let dir = match std::env::var_os("KIBOARD_CONFIG_DIR") {
        Some(d) => std::path::PathBuf::from(d),
        None => dirs::config_dir()
            .unwrap_or(std::env::temp_dir())
            .join("KiBoard"),
    };
    let _ = std::fs::create_dir_all(&dir);
    dir
}

impl Config {
    fn path() -> std::path::PathBuf {
        config_dir().join("config.json")
    }
    fn load() -> Config {
        let raw = std::fs::read_to_string(Self::path()).ok();
        let mut c: Config = raw
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            // Fresh install (no file): analytics ON by default; derive(Default) would give false.
            .unwrap_or(Config {
                analytics: true,
                pairing_open: true,
                ..Default::default()
            });
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
        } else {
            // `default_decks` only ever runs on a config with NO decks, so every installation that
            // predates F4 kept its single starter deck and never saw that phase's headline
            // feature: the Launcher built from the machine's own apps. Only someone installing
            // from scratch got it. Backfill it, so a config that has been carried forward ends up
            // matching a fresh one.
            if c.decks_version < 1 {
                if let Some(launcher) = launcher_deck() {
                    backfill_launcher(&mut c.decks, launcher);
                }
            }
            // Launcher is an automatic surface, not a Manual deck. Rebuild it on every host start:
            // its rolling month, ordering, icon policy and visual-app filter can all change while
            // KiBoard is closed. Waiting for the next foreground transition leaves a stale page on
            // the phone immediately after boot.
            replace_launcher(&mut c.decks, launcher_deck());
        }
        // Recorded even when the machine currently has no launchable applications; the live
        // refresher can add Launcher later when an application becomes available.
        c.decks_version = DECKS_VERSION;
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
    use super::{
        backfill_launcher, default_decks, default_profiles, replace_launcher, Button, Config, Deck,
        Key, KeyKind, Page, Profile, REFERENCE_PAGE,
    };

    #[test]
    fn manual_mode_is_opt_in_for_existing_configs() {
        let mut old = serde_json::to_value(Config::default()).expect("config json");
        old.as_object_mut().unwrap().remove("manual_enabled");
        old.as_object_mut().unwrap().remove("manual_intro_seen");
        let config: Config = serde_json::from_value(old).expect("old config");
        assert!(!config.manual_enabled);
        assert!(!config.manual_intro_seen);
    }

    fn launcher() -> Deck {
        Deck {
            id: "launcher".into(),
            name: "Launcher".into(),
            pages: vec![Page {
                id: "p0".into(),
                keys: vec![Key {
                    pos: 0,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn starter(keys: Vec<Key>) -> Vec<Deck> {
        vec![Deck {
            id: "starter".into(),
            name: "KiBoard".into(),
            pages: vec![Page {
                id: "p0".into(),
                keys,
                ..Default::default()
            }],
            ..Default::default()
        }]
    }

    fn generated_launcher(label: &str, id: &str) -> Deck {
        Deck {
            id: "launcher".into(),
            name: "Launcher".into(),
            icon: "apps".into(),
            pages: vec![Page {
                id: "p0".into(),
                keys: vec![
                    Key {
                        pos: 0,
                        label: "Auto".into(),
                        icon: "mode".into(),
                        action: Some("mode:auto".into()),
                        kind: KeyKind::Action,
                        ..Default::default()
                    },
                    Key {
                        pos: 1,
                        label: label.into(),
                        icon: "app".into(),
                        action: Some(format!("launch:{id}")),
                        hold: Some(format!("focus:{id}")),
                        kind: KeyKind::Action,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
        }
    }

    // Title-only matcher used by catalogue tests. Runtime matching adds terminal process context
    // in `engine::layout`; these tests only pin the ordinary profile ordering and fallbacks.
    fn profile_for<'a>(profiles: &'a [Profile], app: &str, title: &str) -> &'a str {
        let hay = format!("{app} {title}").to_lowercase();
        profiles
            .iter()
            .find(|p| p.matches.iter().any(|m| hay.contains(&m.to_lowercase())))
            .or_else(|| profiles.iter().find(|p| p.matches.is_empty()))
            .map(|p| p.id.as_str())
            .unwrap_or("empty")
    }

    fn key_at(pos: usize, action: &str) -> Key {
        Key {
            pos,
            action: Some(action.into()),
            kind: KeyKind::Action,
            ..Default::default()
        }
    }

    /// The migration runs behind the user's back on a launch they asked nothing of, so what it must
    /// NOT do matters more than what it does.
    #[test]
    fn backfilling_the_launcher_only_ever_adds() {
        // The case it exists for: a config seeded before F4 has one deck and no way to any app.
        let mut decks = starter(vec![key_at(0, "ctrl+c"), key_at(1, "ctrl+v")]);
        backfill_launcher(&mut decks, launcher());
        assert_eq!(decks.len(), 2);
        assert_eq!(decks[1].id, "launcher");
        // The user's own keys are untouched, and the jump is appended PAST them.
        assert_eq!(decks[0].pages[0].keys.len(), 3);
        assert_eq!(decks[0].pages[0].keys[0].action.as_deref(), Some("ctrl+c"));
        let added = decks[0].pages[0].keys.last().unwrap();
        assert_eq!(added.pos, 2);
        assert_eq!(added.action.as_deref(), Some("deck:launcher"));

        // Twice is once: a second launch must not stack a second Launcher or a second key.
        backfill_launcher(&mut decks, launcher());
        assert_eq!(decks.len(), 2);
        assert_eq!(decks[0].pages[0].keys.len(), 3);
    }

    #[test]
    fn backfilling_respects_what_the_user_already_did() {
        // Someone who already wired their own way there does not get a second key for it.
        let mut decks = starter(vec![key_at(0, "deck:launcher")]);
        backfill_launcher(&mut decks, launcher());
        assert_eq!(
            decks[0].pages[0].keys.len(),
            1,
            "their key is the way there"
        );
        assert_eq!(decks.len(), 2);

        // And a deck of their own under that id is never overwritten.
        let mine = Deck {
            id: "launcher".into(),
            name: "Mine".into(),
            ..Default::default()
        };
        let mut decks = vec![mine];
        backfill_launcher(&mut decks, launcher());
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, "Mine");
    }

    #[test]
    fn refreshing_the_launcher_replaces_any_legacy_launcher() {
        let mut decks = vec![generated_launcher("Old", "old")];
        assert!(replace_launcher(
            &mut decks,
            Some(generated_launcher("Recent", "recent"))
        ));
        assert_eq!(decks[0].pages[0].keys[1].label, "Recent");

        let mut custom = vec![Deck {
            id: "launcher".into(),
            name: "My launcher".into(),
            ..Default::default()
        }];
        assert!(replace_launcher(
            &mut custom,
            Some(generated_launcher("Recent", "recent"))
        ));
        assert_eq!(custom[0].name, "Launcher");
        assert_eq!(custom[0].pages[0].keys[1].label, "Recent");

        let mut deleted = Vec::new();
        assert!(replace_launcher(
            &mut deleted,
            Some(generated_launcher("Recent", "recent"))
        ));
        assert_eq!(deleted[0].id, "launcher");
    }

    /// A profile that names an icon the vocabulary does not have draws a blank square on the phone
    /// and a ⬛ in the editor, silently. That is how **81 of the 93 names in use** ended up
    /// invisible — including most of the Claude Code profile — with nothing anywhere connecting
    /// the two files.
    ///
    /// Reading the JS from a Rust test is not pretty. It is, however, the only place both halves
    /// are visible at once: the names are minted here and drawn there. (The tidy version is moving
    /// the vocabulary into `deck-tokens.json` and generating both, like the colours already are.)
    #[test]
    fn icons_cover_every_profile() {
        let js = include_str!("../../src/lib/icons.js");
        let known: Vec<&str> = js
            .lines()
            .filter_map(|line| line.trim().split_once(':'))
            .map(|(name, _)| name.trim())
            .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase()))
            .collect();
        assert!(
            known.len() > 50,
            "the vocabulary did not parse: {} names",
            known.len()
        );

        let mut missing: Vec<String> = default_profiles()
            .iter()
            .flat_map(|p| &p.buttons)
            .map(|b| b.icon.clone())
            .filter(|icon| !icon.is_empty() && !known.contains(&icon.as_str()))
            .collect();
        missing.sort();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "profiles name icons nothing can draw: {missing:?}"
        );
    }

    /// A hole in the middle of a page is a hole the user left on purpose — the jump goes after the
    /// last key, not into the first gap.
    #[test]
    fn the_jump_key_lands_past_the_last_key_not_in_a_gap() {
        let mut decks = starter(vec![key_at(0, "ctrl+c"), key_at(7, "ctrl+v")]);
        backfill_launcher(&mut decks, launcher());
        assert_eq!(decks[0].pages[0].keys.last().unwrap().pos, 8);
    }

    #[test]
    fn gmail_wins_over_chrome_and_explains_its_required_shortcuts() {
        let profiles = default_profiles();
        assert_eq!(
            profile_for(
                &profiles,
                "chrome",
                "Inbox - user@gmail.com - Gmail - Google Chrome"
            ),
            "gmail"
        );

        let gmail = profiles.iter().find(|p| p.id == "gmail").unwrap();
        let setup = gmail
            .buttons
            .iter()
            .find(|button| button.label == "Atajos Gmail")
            .expect("Gmail's letter shortcuts ship disabled and need an in-deck setup path");
        assert!(setup.recommended, "setup must be visible on the first page");
        assert_eq!(
            setup.action,
            "picker:Activar atajos=?>>tab>>enter;Abrir configuración=open:https://mail.google.com/mail/u/0/#settings/general;Ver estado=?"
        );
        assert_eq!(
            crate::engine::actions::choose(&setup.action, Some(0), None).as_deref(),
            Some("?>>tab>>enter"),
            "the question mark must remain the first complete macro step"
        );
        assert_eq!(
            crate::engine::actions::choose(&setup.action, Some(1), None).as_deref(),
            Some("open:https://mail.google.com/mail/u/0/#settings/general"),
            "Gmail settings are the keyboard-independent fallback"
        );
    }

    #[test]
    fn ai_agents_have_separate_boards_without_frozen_provider_models() {
        let profiles = default_profiles();
        assert!(
            profiles
                .iter()
                .flat_map(|profile| &profile.buttons)
                .all(|button| !button.action.eq_ignore_ascii_case("alt+F4")),
            "closing the foreground app belongs to its reserved panel, not a profile key"
        );
        let expected_core = [
            (
                "claude-code",
                [
                    "Modelo",
                    "Permisos",
                    "Compactar",
                    "Limpiar",
                    "Copiar",
                    "Pegar",
                    "Aceptar",
                    "Rechazar",
                ],
            ),
            (
                "codex-cli",
                [
                    "Modelo",
                    "Aprobaciones",
                    "Compactar",
                    "Nuevo chat",
                    "Copiar",
                    "Pegar",
                    "Aceptar",
                    "Rechazar",
                ],
            ),
            (
                "gemini-cli",
                [
                    "Modelo", "Agentes", "MCP", "Limpiar", "Copiar", "Pegar", "Aceptar", "Rechazar",
                ],
            ),
            (
                "aider",
                [
                    "Modelo",
                    "Esfuerzo",
                    "Arquitecto",
                    "Código",
                    "Deshacer",
                    "Diferencias",
                    "Aceptar",
                    "Rechazar",
                ],
            ),
            (
                "claude-desktop",
                [
                    "",
                    "Dictar",
                    "Sel. todo",
                    "Nueva línea",
                    "Copiar",
                    "Pegar",
                    "Enviar",
                    "Cancelar",
                ],
            ),
            (
                "codex-desktop",
                [
                    "Modelo",
                    "Esfuerzo -",
                    "Esfuerzo +",
                    "Velocidad",
                    "Deshacer",
                    "Revisar",
                    "Aceptar",
                    "Rechazar",
                ],
            ),
            (
                "lm-studio",
                [
                    "",
                    "",
                    "Dictar",
                    "Nueva línea",
                    "Copiar",
                    "Pegar",
                    "Enviar",
                    "Detener",
                ],
            ),
        ];
        for (id, slots) in expected_core {
            let profile = profiles
                .iter()
                .find(|p| p.id == id)
                .unwrap_or_else(|| panic!("missing AI board {id}"));
            let recommended: Vec<&Button> = profile
                .buttons
                .iter()
                .filter(|button| button.recommended)
                .collect();
            assert_eq!(
                recommended.len(),
                12,
                "{id} must fill the shared eight-slot core plus four arrows"
            );
            assert_eq!(
                recommended[..8]
                    .iter()
                    .map(|button| button.label.as_str())
                    .collect::<Vec<_>>(),
                slots,
                "{id} drifted from the shared AI-board slot map"
            );
            assert_eq!(
                recommended[8..]
                    .iter()
                    .map(|button| button.action.as_str())
                    .collect::<Vec<_>>(),
                ["up", "down", "left", "right"],
                "{id} must end in the same four-direction block"
            );
        }
        assert!(
            !profiles.iter().any(|p| p.id == "ai"),
            "the old mixed board must stay gone"
        );

        for p in profiles.iter().filter(|p| p.id != "claude-code") {
            assert!(
                p.buttons.iter().all(|b| !b.action.contains("claude-")),
                "{} freezes a Claude model into another product's board",
                p.id
            );
        }

        let codex = profiles.iter().find(|p| p.id == "codex-desktop").unwrap();
        let actions: Vec<&str> = codex.buttons.iter().map(|b| b.action.as_str()).collect();
        assert_eq!(codex.buttons.len(), 18);
        for action in [
            "enter",
            "esc",
            "ctrl+shift+m",
            "ctrl+alt+shift+i",
            "ctrl+alt+shift+k",
            "ctrl+alt+shift+f",
            "ctrl+z",
            "ctrl+shift+g",
            "tab",
            "shift+tab",
            "up",
            "down",
            "left",
            "right",
        ] {
            assert!(
                actions.contains(&action),
                "Codex Desktop is missing {action}"
            );
        }
        assert!(!actions.contains(&"alt+F4"));
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
                // Hand-authored decks are written against the reference 5×3 grid. A GENERATED deck
                // (the Launcher) is deliberately one long page instead, because repagination — not
                // a `Page` boundary the user cannot navigate to — is what cuts it for the client.
                if deck.id != "launcher" {
                    assert!(page.keys.len() <= REFERENCE_PAGE);
                }
                // `pos` IS the address the phone sends back in a `key` message: it must be the
                // index, contiguous from 0, or the host resolves a press to the wrong action.
                for (i, k) in page.keys.iter().enumerate() {
                    assert_eq!(k.pos, i, "page {} key {} has pos {}", page.id, i, k.pos);
                }
                // Every navigating key must name a page that exists in this deck.
                for k in &page.keys {
                    if matches!(k.kind, KeyKind::Folder | KeyKind::Page) {
                        let t = k
                            .target
                            .as_deref()
                            .expect("a navigating key needs a target");
                        assert!(deck.page(t).is_some(), "key targets missing page {t}");
                    }
                    if matches!(k.kind, KeyKind::Action) {
                        assert!(k.action.is_some(), "an action key needs an action");
                    }
                }
            }
        }
    }

    #[test]
    fn starter_manual_deck_contains_only_common_actions() {
        let starter = default_decks()
            .into_iter()
            .find(|deck| deck.id == "starter")
            .expect("the Manual starter deck");
        let actions: Vec<&str> = starter.pages[0]
            .keys
            .iter()
            .filter_map(|key| key.action.as_deref())
            .collect();
        assert_eq!(actions.len(), 9);
        for common in [
            "ctrl+c",
            "ctrl+v",
            "ctrl+x",
            "ctrl+z",
            "ctrl+y",
            "ctrl+a",
            "screenshot",
            "trackpad",
            "dictate",
        ] {
            assert!(actions.contains(&common), "starter is missing {common}");
        }
        for redundant in ["windows", "mode:auto", "deck:launcher"] {
            assert!(!actions.contains(&redundant));
        }
    }

    /// Manual probe against the real machine: `cargo test probe_launcher -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn probe_launcher() {
        let decks = default_decks();
        let l = decks
            .iter()
            .find(|d| d.id == "launcher")
            .expect("a launcher deck");
        for p in &l.pages {
            for k in &p.keys {
                println!(
                    "{:2} {:35} {}",
                    k.pos,
                    k.label,
                    k.action.as_deref().unwrap_or("")
                );
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
        let d = decks
            .iter()
            .find(|d| d.id == "launcher")
            .expect("a launcher deck");
        let msg: serde_json::Value = serde_json::from_str(&layout_json(
            d,
            &d.pages[0],
            Grid::new(3, 5),
            0,
            crate::i18n::Lang::Es,
            None,
        ))
        .unwrap();
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
        let k = Key {
            action: Some("ctrl+c".into()),
            kind: KeyKind::Action,
            ..Default::default()
        };
        assert_eq!(k.action_for("short"), Some("ctrl+c"));
        assert_eq!(k.action_for("long"), None);
        assert_eq!(k.action_for("double"), None);
        assert_eq!(k.action_for("nonsense"), None);
    }

    // An empty key must serialize to just its position and kind: it is sent on every layout, and
    // 15 of them per page adds up against the 64 KB frame cap.
    #[test]
    fn empty_key_serializes_minimally() {
        let json = serde_json::to_string(&Key {
            pos: 10,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"pos":10,"kind":"empty"}"#);
    }

    // The TITLE identifies the active shell (Windows Terminal changes it per tab). Real titles
    // verified on Windows 11.
    #[test]
    fn profile_by_shell_from_title() {
        let profiles = default_profiles();
        assert_eq!(
            profile_for(&profiles, "WindowsTerminal", "Windows PowerShell"),
            "shell-pwsh"
        );
        assert_eq!(
            profile_for(&profiles, "cmd", r"C:\WINDOWS\system32\cmd.exe"),
            "shell-cmd"
        );
        assert_eq!(
            profile_for(&profiles, "wsl", r"C:\WINDOWS\system32\wsl.exe"),
            "shell-bash"
        );
        assert_eq!(
            profile_for(&profiles, "WindowsTerminal", "Ubuntu"),
            "shell-bash"
        );
        // KNOWN LIMITATION: if bash exports its prompt to the title ("user@host: ~") there's no
        // distinctive word left; adding "@" as a match would hijack half the catalogue (Gmail,
        // paths with @...). Degrades to the emulator profile, not the generic one — and the user
        // can add their own match from the host UI.
        assert_eq!(
            profile_for(&profiles, "WindowsTerminal", "ricardo@DESKTOP: ~"),
            "terminal"
        );
        // No shell hint in the title -> emulator profile (used to fall through to generic because
        // "windows terminal" with a space never matched "WindowsTerminal").
        assert_eq!(profile_for(&profiles, "WindowsTerminal", ""), "terminal");
    }
}
