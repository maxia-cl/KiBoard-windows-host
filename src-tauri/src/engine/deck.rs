//! Manual mode: repaginates a deck onto whatever grid the client declared, and resolves a
//! (page, pos) press back to the key that owns it (protocol/README.md §4.1, §4.2).
//!
//! Decks are authored on the reference 5×3 grid. A client with a different grid — a tablet, a
//! folding phone, a future 4×2 device — gets the SAME keys in the same order, just cut into
//! different pages. Nothing in the stored deck depends on the client.
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use serde_json::json;

use crate::config::{Deck, Key, KeyKind, Page};

/// Which two-state keys are showing their ON face right now (protocol §3, F6).
///
/// ponytail: a `HashSet` of addresses in memory, not a field in `config.json`. A toggle is a
/// running fact, not a preference — persisting it would mean a disk write on every press, and the
/// contract already says a restart shows the OFF face. Persist it when someone misses it.
static ON: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
fn on_set() -> &'static Mutex<HashSet<String>> {
    ON.get_or_init(|| Mutex::new(HashSet::new()))
}

/// A key's identity for toggle purposes. Deliberately the ABSOLUTE position in the page, not the
/// per-grid one: two phones with different grids see the same key at different `pos`, and they
/// must agree on which face is showing.
pub(crate) fn addr(deck_id: &str, page_id: &str, pos: usize) -> String {
    format!("{deck_id}/{page_id}/{pos}")
}

pub(crate) fn is_on(addr: &str) -> bool {
    on_set().lock().unwrap().contains(addr)
}

/// Drops remembered faces that no longer belong to anything, after the decks were edited.
///
/// The set is keyed by address, so a key that loses its second state — or a page or deck that is
/// deleted — leaves its ON behind. Nothing shows it, but re-adding a toggle at that same address
/// would find it already switched on, which reads as the editor mis-saving.
///
/// Only ever runs on a SAVE. Doing it on the live preview would reset a toggle every time the
/// editor pushed a keystroke, which is the opposite of the bug.
pub(crate) fn forget_orphans(decks: &[Deck]) {
    let live = live_faces(decks);
    on_set().lock().unwrap().retain(|at| live.contains(at));
}

/// The addresses that still belong to a two-state key. Split out so the rule can be tested without
/// the global set — two tests pruning one static in parallel is a race, not a check.
fn live_faces(decks: &[Deck]) -> HashSet<String> {
    let mut live = HashSet::new();
    for deck in decks {
        for page in &deck.pages {
            for key in page.keys.iter().filter(|k| k.toggle.is_some()) {
                live.insert(addr(&deck.id, &page.id, key.pos));
            }
        }
    }
    live
}

/// Flips a key's face. Returns the new state.
pub(crate) fn flip(addr: &str) -> bool {
    let mut set = on_set().lock().unwrap();
    if set.remove(addr) {
        false
    } else {
        set.insert(addr.to_string());
        true
    }
}

/// The truth about a key whose state KiBoard does not own: OBS decides whether it is recording,
/// and a scene is on air or it is not. `None` means nothing live knows, and the remembered flip in
/// `ON` is the best answer there is.
///
/// A chained action (`obs:record>>wait:100`) deliberately falls through to `None`: the key is
/// doing more than the one thing this could speak for.
pub(crate) fn live_on(action: &str) -> Option<bool> {
    let cmd = action.strip_prefix("obs:")?;
    let obs = crate::engine::state::obs_state().lock().unwrap();
    if !obs.connected {
        return None;
    }
    match cmd {
        "record" => Some(obs.recording),
        "stream" => Some(obs.streaming),
        "replaybuffer" => Some(obs.replay_active),
        "mic" => Some(obs.mic_muted),
        // A scene key wears its ON face when it IS the scene on air.
        s => s
            .strip_prefix("scene:")
            .map(|name| obs.current_scene == name),
    }
}

/// `Some` when something outside KiBoard owns this key's on/off state. The caller needs to know
/// which it is, not just the value: a key whose state is owned elsewhere must not ALSO be
/// remembered here, or the two would fight.
pub(crate) fn live_face(key: &Key) -> Option<bool> {
    key.action.as_deref().and_then(live_on)
}

/// Whether a key's ON face is showing: reality when something live knows, memory otherwise.
fn face_is_on(key: &Key, at: &str) -> bool {
    live_face(key).unwrap_or_else(|| is_on(at))
}

/// The action a two-state key runs right now: the face that is SHOWING, which is the one the user
/// is looking at when they press. A toggle with no action of its own reuses the base one — that is
/// a key that changes appearance only.
pub(crate) fn showing_action(key: &Key, on: bool) -> Option<&str> {
    if on {
        if let Some(face) = &key.toggle {
            if let Some(action) = face.action.as_deref() {
                return Some(action);
            }
        }
    }
    key.action.as_deref()
}

/// Paints the ON face over a key and drops `toggle` so it never reaches the phone. Each field
/// overrides only itself: a toggle that sets just the label keeps the rest of the key.
fn wear_face(key: &mut Key, on: bool) {
    let Some(face) = key.toggle.take() else {
        return;
    };
    if on {
        if !face.label.is_empty() {
            key.label = face.label;
        }
        if !face.icon.is_empty() {
            key.icon = face.icon;
            // A face that names an icon means the icon, not the image the OFF face carried.
            key.image = None;
        }
        if face.image.is_some() {
            key.image = face.image;
        }
        if face.color.is_some() {
            key.color = face.color;
        }
        if let Some(action) = face.action {
            key.action = Some(action);
        }
    }
    set_state(key, "on", json!(on));
}

/// Merges one field into a key's live `state` without dropping what is already there — `running`
/// is written by `decorate_apps` and `on` by the toggle, and a key can have both.
fn set_state(key: &mut Key, field: &str, value: serde_json::Value) {
    match key.state.as_mut().and_then(|v| v.as_object_mut()) {
        Some(obj) => {
            obj.insert(field.to_string(), value);
        }
        None => key.state = Some(json!({ field: value })),
    }
}

/// The key grid a client declared in `hello`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Grid {
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    /// Cells at the END of every page the CLIENT keeps for itself (§4.1 `grid.reserve`). The phone
    /// draws the foreground app there, so the host must not paginate keys into them.
    pub(crate) reserve: usize,
}

impl Grid {
    /// Builds a grid from what a client CLAIMS. Untrusted: `hello` arrives before the token is
    /// checked, and `rows * cols` sizes every allocation below. 0 would divide by zero and 10000
    /// would let one frame ask for a 100M-key layout, so both ends are clamped.
    pub(crate) fn new(rows: usize, cols: usize) -> Grid {
        Grid {
            rows: rows.clamp(1, MAX_SIDE),
            cols: cols.clamp(1, MAX_SIDE),
            reserve: 0,
        }
    }

    /// Same grid, with `n` cells at the end left to the client. Clamped so a client cannot ask for
    /// a page with no keys on it at all — `reserve` arrives from `hello`, before the token is
    /// checked, like every other field of the grid.
    pub(crate) fn reserving(self, n: usize) -> Grid {
        Grid {
            reserve: n.min(self.size().saturating_sub(1)),
            ..self
        }
    }

    /// Every cell, including the reserved ones. What the client DRAWS.
    pub(crate) fn size(self) -> usize {
        self.rows * self.cols
    }

    /// Cells the host may put keys in. What pagination and addressing run on — the two must agree,
    /// or a press at position 12 would resolve against a page cut at 15.
    pub(crate) fn usable(self) -> usize {
        self.size().saturating_sub(self.reserve).max(1)
    }
}

impl Default for Grid {
    /// The reference device, used when a client omits `grid`.
    fn default() -> Grid {
        Grid {
            rows: 3,
            cols: 5,
            reserve: 0,
        }
    }
}

/// No real key pad is bigger than this per side. Also caps the per-page allocation.
const MAX_SIDE: usize = 12;

/// How many bytes of stored `image` a single page may hold. Three quarters of §4's 64 KB frame,
/// leaving the labels, actions and JSON scaffolding room in the same message.
const IMAGE_BUDGET: usize = 48 * 1024;

/// Flattens a page into a dense, hole-preserving vector indexed by `pos`.
///
/// The stored keys are normally already dense and in order, but `config.json` is a plain file a
/// user can edit: duplicate or out-of-range positions must not shift every later key by one (which
/// would make the phone execute the neighbour of what was pressed). Last writer wins per slot.
fn dense(page: &Page) -> Vec<Key> {
    let len = page.keys.iter().map(|k| k.pos + 1).max().unwrap_or(0);
    let len = len.min(MAX_SIDE * MAX_SIDE * 8);
    let mut out: Vec<Key> = (0..len)
        .map(|pos| Key {
            pos,
            ..Default::default()
        })
        .collect();
    for k in &page.keys {
        if let Some(slot) = out.get_mut(k.pos) {
            *slot = k.clone();
        }
    }
    out
}

/// Total pages this deck page needs on `grid`. Always at least 1, so an empty deck still renders
/// (as an empty grid) instead of leaving the phone with nothing to draw.
pub(crate) fn pages(page: &Page, grid: Grid) -> usize {
    dense(page).len().div_ceil(grid.usable()).max(1)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArrowDirection {
    Up,
    Down,
    Left,
    Right,
}

fn arrow_direction(key: &Key) -> Option<ArrowDirection> {
    match key.icon.as_str() {
        "scrollup" | "up" => Some(ArrowDirection::Up),
        "scrolldown" | "down" => Some(ArrowDirection::Down),
        "prev" | "back" | "left" => Some(ArrowDirection::Left),
        "next" | "fwdnav" | "right" => Some(ArrowDirection::Right),
        _ => None,
    }
}

/// Moves directional keys into the familiar keyboard geometry without consuming extra cells.
/// The two cells beside Up remain available to ordinary keys, so a full 5×3 board still fits on
/// one page: only the arrows' relationship is fixed, not an ornamental empty D-pad outline.
fn arrange_arrows(keys: Vec<Key>, grid: Grid) -> Vec<Key> {
    let at = |direction| {
        keys.iter()
            .position(|key| arrow_direction(key) == Some(direction))
    };
    let up = at(ArrowDirection::Up);
    let down = at(ArrowDirection::Down);
    let left = at(ArrowDirection::Left);
    let right = at(ArrowDirection::Right);
    let size = keys.len();

    let mut selected: Vec<(usize, usize)> = Vec::new();
    if let (Some(up), Some(down), Some(left), Some(right)) = (up, down, left, right) {
        let mut block = None;
        if grid.rows >= 2 && grid.cols >= 3 {
            for row in 0..grid.rows - 1 {
                for col in 0..grid.cols - 2 {
                    let targets = [
                        row * grid.cols + col + 1,
                        (row + 1) * grid.cols + col,
                        (row + 1) * grid.cols + col + 1,
                        (row + 1) * grid.cols + col + 2,
                    ];
                    if targets.iter().all(|&target| target < size) {
                        block = Some(targets);
                    }
                }
            }
        }
        if let Some([up_at, left_at, down_at, right_at]) = block {
            selected.extend([
                (up, up_at),
                (left, left_at),
                (down, down_at),
                (right, right_at),
            ]);
        }
    }

    if selected.is_empty() {
        if let (Some(up), Some(down)) = (up, down) {
            let mut pair = None;
            if grid.rows >= 2 {
                for row in 0..grid.rows - 1 {
                    for col in 0..grid.cols {
                        let targets = [row * grid.cols + col, (row + 1) * grid.cols + col];
                        if targets.iter().all(|&target| target < size) {
                            pair = Some(targets);
                        }
                    }
                }
            }
            if let Some([up_at, down_at]) = pair {
                selected.extend([(up, up_at), (down, down_at)]);
            }
        } else if let (Some(left), Some(right)) = (left, right) {
            let mut pair = None;
            if grid.cols >= 2 {
                for row in 0..grid.rows {
                    for col in 0..grid.cols - 1 {
                        let targets = [row * grid.cols + col, row * grid.cols + col + 1];
                        if targets.iter().all(|&target| target < size) {
                            pair = Some(targets);
                        }
                    }
                }
            }
            if let Some([left_at, right_at]) = pair {
                selected.extend([(left, left_at), (right, right_at)]);
            }
        }
    }

    if selected.is_empty() {
        return keys;
    }

    let mut output: Vec<Option<Key>> = (0..size).map(|_| None).collect();
    let mut remaining = Vec::with_capacity(size - selected.len());
    for (index, key) in keys.into_iter().enumerate() {
        if let Some((_, target)) = selected.iter().find(|(source, _)| *source == index) {
            output[*target] = Some(key);
        } else {
            remaining.push(key);
        }
    }
    let mut remaining = remaining.into_iter();
    output
        .into_iter()
        .map(|slot| {
            slot.unwrap_or_else(|| remaining.next().expect("arrow layout preserves capacity"))
        })
        .collect()
}

/// One page with source positions intact. Rendering re-addresses them afterwards; resolving uses
/// these original positions to execute the same key the phone saw after the arrow permutation.
fn arranged_page(page: &Page, grid: Grid, want: usize) -> Vec<Key> {
    let all = dense(page);
    let size = grid.usable();
    let Some(start) = want.checked_mul(size) else {
        return (0..size).map(|_| Key::default()).collect();
    };
    let keys = (0..size)
        .map(|i| all.get(start + i).cloned().unwrap_or_default())
        .collect();
    arrange_arrows(keys, grid)
}

/// The keys for one page of `grid`, re-addressed to that page: `pos` comes back as 0..grid.size(),
/// which is exactly what the phone sends back in a `key` message. Short pages are padded with
/// empties so the client always receives a full grid.
pub(crate) fn keys_for(page: &Page, grid: Grid, want: usize) -> Vec<Key> {
    arranged_page(page, grid, want)
        .into_iter()
        .enumerate()
        .map(|(pos, key)| Key { pos, ..key })
        .collect()
}

/// Inverse of `keys_for`: the stored key a client meant by (page, pos).
///
/// This is the security boundary from §4.2 — the phone sends a POSITION, never an action, so an
/// authenticated client cannot ask for something that is not on the layout it was given. Anything
/// off the grid, past the end, or landing on a hole resolves to `None` (`no_such_key`).
pub(crate) fn resolve(page: &Page, grid: Grid, at_page: usize, pos: usize) -> Option<&Key> {
    let shown = arranged_page(page, grid, at_page).into_iter().nth(pos)?;
    if shown.kind == KeyKind::Empty {
        return None;
    }
    page.keys
        .iter()
        .find(|key| key.pos == shown.pos && key.kind != KeyKind::Empty)
}

/// The key's absolute address in the page behind a (client page, client pos) pair. Grid-dependent
/// going in, grid-independent coming out — which is what makes it usable as a toggle's identity.
pub(crate) fn flat(grid: Grid, at_page: usize, pos: usize) -> Option<usize> {
    if pos >= grid.usable() {
        return None;
    }
    at_page.checked_mul(grid.usable())?.checked_add(pos)
}

/// Rejects decks the phone could not render or resolve. The editor is not the only writer —
/// `config.json` is a file a user can edit by hand — but it IS the one that can be told why.
///
/// Only structural rules live here, the ones that would make a press resolve to nothing:
/// action strings are NOT validated, because `run_step` already answers `unknown_action` per press
/// and a deck full of typos is still a deck the user can open and fix.
pub(crate) fn validate(decks: &[Deck]) -> Result<(), String> {
    let mut seen: Vec<&str> = Vec::new();
    for deck in decks {
        if deck.id.is_empty() {
            return Err("a deck has no id".into());
        }
        if seen.contains(&deck.id.as_str()) {
            return Err(format!("two decks share the id \"{}\"", deck.id));
        }
        seen.push(&deck.id);
        if deck.pages.is_empty() {
            return Err(format!("deck \"{}\" has no pages", deck.id));
        }
        let mut pages: Vec<&str> = Vec::new();
        for page in &deck.pages {
            if page.id.is_empty() {
                return Err(format!("a page of deck \"{}\" has no id", deck.id));
            }
            if pages.contains(&page.id.as_str()) {
                return Err(format!(
                    "deck \"{}\" repeats the page id \"{}\"",
                    deck.id, page.id
                ));
            }
            pages.push(&page.id);
        }
        for page in &deck.pages {
            let mut at: Vec<usize> = Vec::new();
            // Stored images are re-sent on every push and §4 caps a frame at 64 KB. The editor
            // downscales to 96x96 (~1-4 KB), so this only ever catches a hand-edited config.json
            // with a photo pasted into it — which would otherwise arrive slowly or not at all, and
            // nothing downstream would say why. Budgeted per PAGE, which is conservative: a frame
            // carries at most one grid-full of a page, never more.
            let stored_images: usize = page
                .keys
                .iter()
                .filter_map(|k| k.image.as_ref())
                .map(String::len)
                .sum();
            if stored_images > IMAGE_BUDGET {
                return Err(format!(
                    "page \"{}\" stores {} KB of images; a page must stay under {} KB so the layout fits one frame",
                    page.id,
                    stored_images / 1024,
                    IMAGE_BUDGET / 1024
                ));
            }
            for key in &page.keys {
                // `pos` IS the address the phone sends back: two keys on one position means a
                // press resolves to whichever the host happens to find first.
                if at.contains(&key.pos) {
                    return Err(format!(
                        "two keys share position {} on page \"{}\"",
                        key.pos, page.id
                    ));
                }
                at.push(key.pos);
                match key.kind {
                    KeyKind::Folder | KeyKind::Page => {
                        let target = key.target.as_deref().unwrap_or("");
                        if !deck.pages.iter().any(|p| p.id == target) {
                            return Err(format!(
                                "key \"{}\" points at a page that does not exist (\"{target}\")",
                                key.label
                            ));
                        }
                    }
                    KeyKind::Action => {
                        if key.action.as_deref().unwrap_or("").is_empty() {
                            return Err(format!("key \"{}\" has no action", key.label));
                        }
                    }
                    KeyKind::Empty => {}
                }
            }
        }
    }
    Ok(())
}

/// Fills in what a stored deck cannot hold: the app's real icon and whether it is running right
/// now. Kept out of `keys_for` so pagination stays pure — this is the only part that touches the
/// desktop, and it runs once per `layout` message, not once per key.
fn decorate_apps(keys: &mut [Key]) {
    let ids: Vec<(usize, String)> = keys
        .iter()
        .enumerate()
        .filter_map(|(i, k)| {
            let a = k.action.as_deref()?;
            let id = ["launch:", "focus:", "kill:"]
                .iter()
                .find_map(|v| a.strip_prefix(v))?;
            Some((i, id.trim().to_string()))
        })
        .collect();
    if ids.is_empty() {
        return;
    }
    let live =
        crate::platform::apps::running(&ids.iter().map(|(_, id)| id.clone()).collect::<Vec<_>>());
    for ((i, id), running) in ids.into_iter().zip(live) {
        let key = &mut keys[i];
        // The stored key wins: a custom image set in the editor is not overwritten by the exe's.
        if key.image.is_none() {
            let b64 = crate::platform::apps::icon(&id);
            if !b64.is_empty() {
                key.image = Some(format!("data:image/png;base64,{b64}"));
            }
        }
        set_state(key, "running", json!(running));
    }
}

/// A `layout` message for one page of a deck (protocol/README.md §4.1).
pub(crate) fn layout_json(
    deck: &Deck,
    page: &Page,
    grid: Grid,
    at_page: usize,
    lang: crate::i18n::Lang,
    app: Option<(&str, &str)>,
) -> String {
    render(deck, page, grid, at_page, lang, "layout", app)
}

/// One page either side of the one the client is on, as a §4.1 `page_preload`. `None` when there
/// is no such page — nothing exists before page 0 or after the last.
///
/// Takes a SIGNED page so "the one before" is expressible without the caller checking first, and
/// renders it without going near the session's page cursor. That is the whole point: the only way
/// for a client to ASK for a page is `set_page`, which moves that cursor, and every push the host
/// makes renders from it.
pub(crate) fn preload_json(
    deck: &Deck,
    page: &Page,
    grid: Grid,
    at_page: isize,
    lang: crate::i18n::Lang,
    app: Option<(&str, &str)>,
) -> Option<String> {
    let at_page = usize::try_from(at_page)
        .ok()
        .filter(|&p| p < pages(page, grid))?;
    Some(render(deck, page, grid, at_page, lang, "page_preload", app))
}

fn render(
    deck: &Deck,
    page: &Page,
    grid: Grid,
    at_page: usize,
    lang: crate::i18n::Lang,
    kind: &str,
    app: Option<(&str, &str)>,
) -> String {
    let total = pages(page, grid);
    let at_page = at_page.min(total - 1);
    let mut keys = keys_for(page, grid, at_page);
    // Faces first: `decorate_apps` looks at `action`, so a toggle whose ON face launches a
    // different app has to have swapped it in before the icon and the running dot are attached.
    for (i, key) in keys.iter_mut().enumerate() {
        let at = addr(&deck.id, &page.id, at_page * grid.usable() + i);
        let on = face_is_on(key, &at);
        wear_face(key, on);
    }
    decorate_apps(&mut keys);
    // Auto mode has translated its labels since v1; decks never did, so a Chinese phone was shown
    // "Ventanas" and "Mazos". `tr` returns anything it does not know unchanged, which is exactly
    // right for a user's own label and for the OS-localized app names on the Launcher deck.
    for k in &mut keys {
        if !k.label.is_empty() {
            k.label = crate::i18n::tr(lang, &k.label).to_string();
        }
    }
    json!({
        "v": 2,
        "type": kind,
        "mode": "manual",
        // §4.1: a manual deck follows nothing, but the PC still has something in front of it, and
        // the phone had no way to know what it was pressing keys at. Same two fields auto mode has
        // always carried, meaning the same thing.
        "source": {
            "kind": "deck", "id": deck.id, "name": deck.name, "page": page.id,
            "appName": app.map(|(name, _)| name),
            "appIcon": app.map(|(_, icon)| icon),
        },
        "grid": { "rows": grid.rows, "cols": grid.cols },
        "page": at_page,
        "pages": total,
        "keys": keys,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_of(n: usize) -> Page {
        Page {
            id: "p0".into(),
            name: String::new(),
            keys: (0..n)
                .map(|pos| Key {
                    pos,
                    label: format!("k{pos}"),
                    action: Some(format!("act{pos}")),
                    kind: KeyKind::Action,
                    ..Default::default()
                })
                .collect(),
        }
    }

    fn arrow_page(all_four: bool) -> Page {
        let mut page = page_of(if all_four { 12 } else { 8 });
        page.keys[1].label = "Up".into();
        page.keys[1].icon = "scrollup".into();
        page.keys[2].label = "Down".into();
        page.keys[2].icon = "scrolldown".into();
        if all_four {
            page.keys[3].label = "Left".into();
            page.keys[3].icon = "prev".into();
            page.keys[4].label = "Right".into();
            page.keys[4].icon = "next".into();
        }
        page
    }

    // §4.1 `page_preload`. What matters is the EDGES: there is nothing before page 0 or after the
    // last, and asking for one must come back empty rather than clamping onto a page that is
    // already on screen — the client would draw a copy of itself coming in behind the swipe.
    #[test]
    fn a_preload_exists_only_where_a_page_does() {
        let p = page_of(15);
        let g = Grid::new(2, 3); // 6 per page -> 3 pages
        let deck = Deck {
            id: "d".into(),
            name: "D".into(),
            pages: vec![p.clone()],
            ..Default::default()
        };
        let at = |n: isize| preload_json(&deck, &p, g, n, crate::i18n::Lang::Es, None);

        assert!(at(-1).is_none(), "nothing before the first page");
        assert!(at(3).is_none(), "nothing after the last");
        assert!(at(0).is_some() && at(1).is_some() && at(2).is_some());

        // It is the same body as a `layout` with a different type — that is what lets the client
        // parse it with the code it already has.
        let json = at(1).unwrap();
        assert!(json.contains("\"type\":\"page_preload\""));
        assert!(!json.contains("\"type\":\"layout\""));
        assert!(json.contains("\"page\":1") && json.contains("\"pages\":3"));
        assert!(
            json.contains("k6"),
            "page 1 of a 6-per-page grid starts at k6"
        );
    }

    // §4.1: a manual deck follows nothing, but the PC still has something in front of it. Auto
    // mode has carried these two fields since v1; this is the same pair meaning the same thing,
    // and a client that ignores them loses nothing — which is why `v` did not move.
    // §4.1 `grid.reserve`: the client keeps the last cells of every page, so pagination and
    // addressing must both shrink. If only one of them did, a press would resolve against a page
    // cut differently from the one that was drawn.
    #[test]
    fn reserve_shrinks_the_page_and_the_addressing_together() {
        let p = page_of(20);
        let g = Grid::new(3, 5).reserving(2);
        assert_eq!(g.size(), 15, "the client still DRAWS fifteen cells");
        assert_eq!(g.usable(), 13);

        let first = keys_for(&p, g, 0);
        assert_eq!(first.len(), 13, "a page must not fill the reserved cells");
        assert_eq!(first[12].label, "k12");

        assert_eq!(pages(&p, g), 2, "20 keys at 13 a page");
        let second = keys_for(&p, g, 1);
        assert_eq!(second[0].label, "k13", "page 2 starts where page 1 stopped");
        assert_eq!(resolve(&p, g, 1, 0).map(|k| k.label.as_str()), Some("k13"));
    }

    #[test]
    fn a_manual_layout_can_name_the_app_in_front() {
        let p = page_of(3);
        let deck = Deck {
            id: "d".into(),
            name: "D".into(),
            pages: vec![p.clone()],
            ..Default::default()
        };
        let grid = Grid::new(3, 5);

        let named = layout_json(
            &deck,
            &p,
            grid,
            0,
            crate::i18n::Lang::Es,
            Some(("Photoshop", "data:x")),
        );
        assert!(named.contains("\"appName\":\"Photoshop\""));
        assert!(named.contains("\"appIcon\":\"data:x\""));

        // And nothing in front — or a host that has not resolved one yet — says so explicitly
        // rather than inventing a name.
        let bare = layout_json(&deck, &p, grid, 0, crate::i18n::Lang::Es, None);
        assert!(bare.contains("\"appName\":null"));
    }

    // A deck authored on 5x3 must survive being shown on a smaller grid: same keys, same order,
    // more pages. This is the whole point of repagination.
    #[test]
    fn same_deck_repaginates_to_any_grid() {
        let p = page_of(15);
        assert_eq!(pages(&p, Grid::new(3, 5)), 1);
        assert_eq!(pages(&p, Grid::new(2, 3)), 3); // 6 per page -> 15 needs 3
        assert_eq!(pages(&p, Grid::new(4, 5)), 1);

        // Reading every page of the small grid back in order rebuilds the original sequence.
        let g = Grid::new(2, 3);
        let seen: Vec<String> = (0..pages(&p, g))
            .flat_map(|i| keys_for(&p, g, i))
            .filter(|k| k.kind == KeyKind::Action)
            .map(|k| k.label)
            .collect();
        assert_eq!(seen, (0..15).map(|i| format!("k{i}")).collect::<Vec<_>>());
    }

    #[test]
    fn four_arrows_are_laid_out_like_a_keyboard_without_losing_keys() {
        let page = arrow_page(true);
        let grid = Grid::new(3, 5);
        let shown = keys_for(&page, grid, 0);
        let at = |label| shown.iter().position(|key| key.label == label).unwrap();

        let up = at("Up");
        let down = at("Down");
        let left = at("Left");
        let right = at("Right");
        assert_eq!(up + grid.cols, down, "Up sits directly above Down");
        assert_eq!(left + 1, down, "Left sits immediately left of Down");
        assert_eq!(down + 1, right, "Right sits immediately right of Down");
        assert_eq!(
            shown
                .iter()
                .filter(|key| key.kind == KeyKind::Action)
                .count(),
            12
        );

        // The visual permutation is also the press permutation: every displayed key resolves to
        // its own action, never the key that occupied that cell before the arrows moved.
        for (pos, key) in shown
            .iter()
            .enumerate()
            .filter(|(_, key)| key.kind == KeyKind::Action)
        {
            assert_eq!(resolve(&page, grid, 0, pos).unwrap().action, key.action);
        }
    }

    #[test]
    fn an_up_down_pair_is_vertical_like_the_keyboard() {
        let page = arrow_page(false);
        let grid = Grid::new(3, 5);
        let shown = keys_for(&page, grid, 0);
        let up = shown.iter().position(|key| key.label == "Up").unwrap();
        let down = shown.iter().position(|key| key.label == "Down").unwrap();
        assert_eq!(up + grid.cols, down);
    }

    // Short last page is padded, so the client always draws a full grid.
    #[test]
    fn last_page_is_padded_with_empties() {
        let g = Grid::new(2, 3);
        let keys = keys_for(&page_of(8), g, 1);
        assert_eq!(keys.len(), 6);
        assert_eq!(keys[1].kind, KeyKind::Action); // pos 7 of the deck
        assert_eq!(keys[2].kind, KeyKind::Empty);
        assert!(keys.iter().enumerate().all(|(i, k)| k.pos == i));
    }

    // The press round-trip: what the phone was shown at (page, pos) is what the host executes.
    #[test]
    fn resolve_is_the_inverse_of_pagination() {
        let p = page_of(15);
        let g = Grid::new(2, 3);
        for at in 0..pages(&p, g) {
            for (pos, shown) in keys_for(&p, g, at).iter().enumerate() {
                match resolve(&p, g, at, pos) {
                    Some(k) => assert_eq!(k.label, shown.label),
                    None => assert_eq!(shown.kind, KeyKind::Empty),
                }
            }
        }
    }

    // §4.2: a position that was never on the layout must not execute anything.
    #[test]
    fn out_of_range_presses_resolve_to_nothing() {
        let p = page_of(6);
        let g = Grid::new(2, 3);
        assert!(resolve(&p, g, 0, 6).is_none()); // past the grid
        assert!(resolve(&p, g, 9, 0).is_none()); // page that does not exist
        assert!(resolve(&p, g, usize::MAX, 1).is_none()); // overflow attempt
    }

    // A hand-edited config.json with a gap must not slide the later keys up: pressing "pos 3"
    // has to hit the key labelled for pos 3, or the phone executes its neighbour.
    #[test]
    fn holes_do_not_shift_later_keys() {
        let p = Page {
            id: "p0".into(),
            name: String::new(),
            keys: vec![
                Key {
                    pos: 0,
                    label: "a".into(),
                    action: Some("a".into()),
                    kind: KeyKind::Action,
                    ..Default::default()
                },
                Key {
                    pos: 3,
                    label: "d".into(),
                    action: Some("d".into()),
                    kind: KeyKind::Action,
                    ..Default::default()
                },
            ],
        };
        let g = Grid::new(1, 4);
        let keys = keys_for(&p, g, 0);
        assert_eq!(keys[0].label, "a");
        assert_eq!(keys[1].kind, KeyKind::Empty);
        assert_eq!(keys[3].label, "d");
        assert_eq!(resolve(&p, g, 0, 3).unwrap().action.as_deref(), Some("d"));
        assert!(resolve(&p, g, 0, 1).is_none());
    }

    /// The editor receives a deck as JSON, edits it and hands it straight back to `save_decks`.
    /// Every `skip_serializing_if` on `Key` is therefore a chance to drop a field on the way out
    /// and fail to read it on the way back in — silently, and into the file the user's decks live
    /// in. A round trip through serde is the cheapest guard against that.
    #[test]
    fn a_deck_survives_the_editors_round_trip() {
        let decks = crate::config::default_decks();
        let json = serde_json::to_string(&decks).expect("decks serialize");
        let back: Vec<Deck> = serde_json::from_str(&json).expect("and deserialize");
        assert_eq!(
            serde_json::to_string(&back).unwrap(),
            json,
            "a field was lost on the way through the editor"
        );
        // Whatever came back has to still be saveable, or the editor could not save what it read.
        assert!(validate(&back).is_ok());
        // The fields that carry meaning are actually there, not merely equal-to-empty on both sides.
        let launcher = back.iter().find(|d| d.id == "launcher");
        if let Some(l) = launcher {
            let key = l.pages[0]
                .keys
                .iter()
                .find(|k| k.pos == 1)
                .expect("an app key");
            assert!(key.action.as_deref().unwrap_or("").starts_with("launch:"));
            assert!(key.hold.as_deref().unwrap_or("").starts_with("focus:"));
        }
    }

    /// Every rule `validate` enforces is one the phone would otherwise hit as "nothing happens".
    #[test]
    fn validate_rejects_what_the_phone_could_not_resolve() {
        let key = |pos: usize, kind: KeyKind| Key {
            pos,
            kind,
            ..Default::default()
        };
        let act = |pos: usize| Key {
            pos,
            action: Some("ctrl+c".into()),
            kind: KeyKind::Action,
            ..Default::default()
        };
        let deck = |pages: Vec<Page>| Deck {
            id: "d".into(),
            pages,
            ..Default::default()
        };
        let page = |id: &str, keys: Vec<Key>| Page {
            id: id.into(),
            keys,
            ..Default::default()
        };

        assert!(validate(&[deck(vec![page("p0", vec![act(0), act(1)])])]).is_ok());
        assert!(validate(&[]).is_ok(), "no decks at all is not an error");

        // Two keys on one position: a press resolves to whichever is found first.
        assert!(validate(&[deck(vec![page("p0", vec![act(0), act(0)])])]).is_err());
        // A navigating key with no destination, or one that names a page that is not there.
        assert!(validate(&[deck(vec![page("p0", vec![key(0, KeyKind::Folder)])])]).is_err());
        let dangling = Key {
            pos: 0,
            target: Some("gone".into()),
            kind: KeyKind::Page,
            ..Default::default()
        };
        assert!(validate(&[deck(vec![page("p0", vec![dangling])])]).is_err());
        // An action key with nothing to run.
        assert!(validate(&[deck(vec![page("p0", vec![key(0, KeyKind::Action)])])]).is_err());
        // Structural: no pages, repeated page id, repeated deck id.
        assert!(validate(&[deck(vec![])]).is_err());
        assert!(validate(&[deck(vec![page("p0", vec![]), page("p0", vec![])])]).is_err());
        assert!(validate(&[
            deck(vec![page("p0", vec![])]),
            deck(vec![page("p0", vec![])])
        ])
        .is_err());
        // An empty key carries neither action nor target, and that is exactly what it is for.
        assert!(validate(&[deck(vec![page("p0", vec![key(0, KeyKind::Empty)])])]).is_ok());

        // Images the editor produces are welcome; a photo pasted into config.json is not, because
        // the layout it makes would not fit one frame.
        let with_image = |pos: usize, bytes: usize| Key {
            image: Some("x".repeat(bytes)),
            ..act(pos)
        };
        assert!(validate(&[deck(vec![page(
            "p0",
            (0..15).map(|i| with_image(i, 3_000)).collect()
        )])])
        .is_ok());
        let fat = validate(&[deck(vec![page("p0", vec![with_image(0, 2_000_000)])])]);
        assert!(fat.is_err());
        assert!(
            fat.unwrap_err().contains("one frame"),
            "the error has to say why"
        );
    }

    /// The whole F6 toggle contract in one place: the face swaps, `toggle` never travels, and the
    /// address survives being read through a different grid than the one it was flipped on.
    #[test]
    fn a_two_state_key_swaps_its_face_and_never_ships_the_spare() {
        let deck = Deck {
            id: "d".into(),
            pages: vec![Page {
                id: "p0".into(),
                keys: vec![Key {
                    pos: 6,
                    label: "Mute".into(),
                    action: Some("vol:mute".into()),
                    kind: KeyKind::Action,
                    toggle: Some(crate::config::Face {
                        label: "Unmute".into(),
                        action: Some("vol:unmute".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let page = &deck.pages[0];
        let at = addr("d", "p0", 6);
        on_set().lock().unwrap().remove(&at);

        // Unticking "Second state" in the editor sends `"toggle": null`, not a missing field.
        // Rejecting that would mean a deck the user just edited can no longer be saved.
        let off: Key = serde_json::from_str(r#"{"pos":0,"kind":"action","toggle":null}"#)
            .expect("null toggle");
        assert!(off.toggle.is_none());

        let grid = Grid::new(2, 3); // six per page, so pos 6 is the first key of client page 1
        let off = layout_json(&deck, page, grid, 1, crate::i18n::Lang::Es, None);
        assert!(off.contains("Mute") && !off.contains("Unmute"));
        assert!(off.contains("\"on\":false"));
        assert!(
            !off.contains("toggle"),
            "the spare face must not reach the phone"
        );

        // What a press would run, and what it does to the face.
        let key = resolve(page, grid, 1, 0).expect("the key");
        assert_eq!(showing_action(key, is_on(&at)), Some("vol:mute"));
        assert!(flip(&at));
        assert_eq!(showing_action(key, is_on(&at)), Some("vol:unmute"));

        let on = layout_json(&deck, page, grid, 1, crate::i18n::Lang::Es, None);
        assert!(on.contains("Unmute") && on.contains("\"on\":true"));

        // A phone with a different grid sees the SAME face: the address is absolute, so a
        // 1x7 client is not looking at a key that a 2x3 client flipped somewhere else.
        let wide = layout_json(&deck, page, Grid::new(1, 7), 0, crate::i18n::Lang::Es, None);
        assert!(wide.contains("Unmute"));
        flip(&at);
    }

    /// A remembered face outliving the key it belonged to is invisible until someone puts a toggle
    /// back at that address — and then it arrives already switched on, which reads as the editor
    /// having mis-saved.
    #[test]
    fn a_face_is_forgotten_when_its_key_stops_having_two() {
        let with_toggle = |pos: usize| Key {
            pos,
            action: Some("wait:1".into()),
            kind: KeyKind::Action,
            toggle: Some(crate::config::Face {
                label: "ON".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let deck_of = |keys: Vec<Key>| {
            vec![Deck {
                id: "d".into(),
                pages: vec![Page {
                    id: "p0".into(),
                    keys,
                    ..Default::default()
                }],
                ..Default::default()
            }]
        };

        // Key 1 loses its second state; key 0 keeps its own.
        let after = live_faces(&deck_of(vec![
            with_toggle(0),
            Key {
                pos: 1,
                kind: KeyKind::Action,
                ..Default::default()
            },
        ]));
        assert!(
            after.contains(&addr("d", "p0", 0)),
            "a toggle that still exists keeps its face"
        );
        assert!(
            !after.contains(&addr("d", "p0", 1)),
            "one that does not must not come back on"
        );

        // A deck that is gone takes every face on it.
        assert!(live_faces(&[]).is_empty());
    }

    /// A key that asks OBS a question must be answered by OBS, not by what KiBoard remembers doing.
    #[test]
    fn obs_owns_the_face_of_an_obs_key() {
        // Nothing is known while OBS is not connected, so the remembered flip is all there is.
        crate::engine::state::obs_state().lock().unwrap().connected = false;
        assert_eq!(live_on("obs:record"), None);
        assert_eq!(live_on("obs:scene:Intro"), None);

        {
            let mut obs = crate::engine::state::obs_state().lock().unwrap();
            obs.connected = true;
            obs.recording = true;
            obs.mic_muted = false;
            obs.current_scene = "Intro".into();
        }
        assert_eq!(live_on("obs:record"), Some(true));
        assert_eq!(live_on("obs:mic"), Some(false));
        // A scene key is on when it is the scene on air, and only then.
        assert_eq!(live_on("obs:scene:Intro"), Some(true));
        assert_eq!(live_on("obs:scene:Outro"), Some(false));
        // Not everything is OBS's business.
        assert_eq!(live_on("launch:notepad.exe"), None);
        // A chain does more than the one thing this could speak for.
        assert_eq!(live_on("obs:record>>wait:100"), None);

        // And the face follows OBS even against a remembered flip pointing the other way.
        let key = Key {
            pos: 0,
            label: "Record".into(),
            action: Some("obs:record".into()),
            kind: KeyKind::Action,
            toggle: Some(crate::config::Face {
                label: "Stop".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let at = addr("d", "p0", 0);
        on_set().lock().unwrap().remove(&at); // memory says OFF
        assert_eq!(live_face(&key), Some(true), "OBS says recording");
        assert!(
            face_is_on(&key, &at),
            "so the ON face shows regardless of memory"
        );

        crate::engine::state::obs_state().lock().unwrap().connected = false;
    }

    // A client cannot make the host allocate an absurd layout by lying in `hello`.
    #[test]
    fn client_grid_is_clamped() {
        assert_eq!(Grid::new(0, 0).size(), 1);
        assert_eq!(Grid::new(9999, 9999), Grid::new(MAX_SIDE, MAX_SIDE));
    }
}
