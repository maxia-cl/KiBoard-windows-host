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
    let Some(face) = key.toggle.take() else { return };
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
}

impl Grid {
    /// Builds a grid from what a client CLAIMS. Untrusted: `hello` arrives before the token is
    /// checked, and `rows * cols` sizes every allocation below. 0 would divide by zero and 10000
    /// would let one frame ask for a 100M-key layout, so both ends are clamped.
    pub(crate) fn new(rows: usize, cols: usize) -> Grid {
        Grid { rows: rows.clamp(1, MAX_SIDE), cols: cols.clamp(1, MAX_SIDE) }
    }
    pub(crate) fn size(self) -> usize {
        self.rows * self.cols
    }
}

impl Default for Grid {
    /// The reference device, used when a client omits `grid`.
    fn default() -> Grid {
        Grid { rows: 3, cols: 5 }
    }
}

/// No real key pad is bigger than this per side. Also caps the per-page allocation.
const MAX_SIDE: usize = 12;

/// Flattens a page into a dense, hole-preserving vector indexed by `pos`.
///
/// The stored keys are normally already dense and in order, but `config.json` is a plain file a
/// user can edit: duplicate or out-of-range positions must not shift every later key by one (which
/// would make the phone execute the neighbour of what was pressed). Last writer wins per slot.
fn dense(page: &Page) -> Vec<Key> {
    let len = page.keys.iter().map(|k| k.pos + 1).max().unwrap_or(0);
    let len = len.min(MAX_SIDE * MAX_SIDE * 8);
    let mut out: Vec<Key> = (0..len).map(|pos| Key { pos, ..Default::default() }).collect();
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
    dense(page).len().div_ceil(grid.size()).max(1)
}

/// The keys for one page of `grid`, re-addressed to that page: `pos` comes back as 0..grid.size(),
/// which is exactly what the phone sends back in a `key` message. Short pages are padded with
/// empties so the client always receives a full grid.
pub(crate) fn keys_for(page: &Page, grid: Grid, want: usize) -> Vec<Key> {
    let all = dense(page);
    let size = grid.size();
    let start = want * size;
    (0..size)
        .map(|i| match all.get(start + i) {
            Some(k) => Key { pos: i, ..k.clone() },
            None => Key { pos: i, ..Default::default() },
        })
        .collect()
}

/// Inverse of `keys_for`: the stored key a client meant by (page, pos).
///
/// This is the security boundary from §4.2 — the phone sends a POSITION, never an action, so an
/// authenticated client cannot ask for something that is not on the layout it was given. Anything
/// off the grid, past the end, or landing on a hole resolves to `None` (`no_such_key`).
pub(crate) fn resolve(page: &Page, grid: Grid, at_page: usize, pos: usize) -> Option<&Key> {
    let at = flat(grid, at_page, pos)?;
    page.keys.iter().find(|k| k.pos == at && k.kind != KeyKind::Empty)
}

/// The key's absolute address in the page behind a (client page, client pos) pair. Grid-dependent
/// going in, grid-independent coming out — which is what makes it usable as a toggle's identity.
pub(crate) fn flat(grid: Grid, at_page: usize, pos: usize) -> Option<usize> {
    if pos >= grid.size() {
        return None;
    }
    at_page.checked_mul(grid.size())?.checked_add(pos)
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
                return Err(format!("deck \"{}\" repeats the page id \"{}\"", deck.id, page.id));
            }
            pages.push(&page.id);
        }
        for page in &deck.pages {
            let mut at: Vec<usize> = Vec::new();
            for key in &page.keys {
                // `pos` IS the address the phone sends back: two keys on one position means a
                // press resolves to whichever the host happens to find first.
                if at.contains(&key.pos) {
                    return Err(format!("two keys share position {} on page \"{}\"", key.pos, page.id));
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
    let live = crate::platform::apps::running(
        &ids.iter().map(|(_, id)| id.clone()).collect::<Vec<_>>(),
    );
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
pub(crate) fn layout_json(deck: &Deck, page: &Page, grid: Grid, at_page: usize) -> String {
    let total = pages(page, grid);
    let at_page = at_page.min(total - 1);
    let mut keys = keys_for(page, grid, at_page);
    // Faces first: `decorate_apps` looks at `action`, so a toggle whose ON face launches a
    // different app has to have swapped it in before the icon and the running dot are attached.
    for (i, key) in keys.iter_mut().enumerate() {
        let absolute = at_page * grid.size() + i;
        wear_face(key, is_on(&addr(&deck.id, &page.id, absolute)));
    }
    decorate_apps(&mut keys);
    // Auto mode has translated its labels since v1; decks never did, so a Chinese phone was shown
    // "Ventanas" and "Mazos". `tr` returns anything it does not know unchanged, which is exactly
    // right for a user's own label and for the OS-localized app names on the Launcher deck.
    for k in &mut keys {
        if !k.label.is_empty() {
            k.label = crate::i18n::tr(&k.label).to_string();
        }
    }
    json!({
        "v": 2,
        "type": "layout",
        "mode": "manual",
        "source": { "kind": "deck", "id": deck.id, "name": deck.name, "page": page.id },
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
                Key { pos: 0, label: "a".into(), action: Some("a".into()), kind: KeyKind::Action, ..Default::default() },
                Key { pos: 3, label: "d".into(), action: Some("d".into()), kind: KeyKind::Action, ..Default::default() },
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
            let key = l.pages[0].keys.iter().find(|k| k.pos == 1).expect("an app key");
            assert!(key.action.as_deref().unwrap_or("").starts_with("launch:"));
            assert!(key.hold.as_deref().unwrap_or("").starts_with("focus:"));
        }
    }

    /// Every rule `validate` enforces is one the phone would otherwise hit as "nothing happens".
    #[test]
    fn validate_rejects_what_the_phone_could_not_resolve() {
        let key = |pos: usize, kind: KeyKind| Key { pos, kind, ..Default::default() };
        let act = |pos: usize| Key {
            pos,
            action: Some("ctrl+c".into()),
            kind: KeyKind::Action,
            ..Default::default()
        };
        let deck = |pages: Vec<Page>| Deck { id: "d".into(), pages, ..Default::default() };
        let page = |id: &str, keys: Vec<Key>| Page { id: id.into(), keys, ..Default::default() };

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
        assert!(validate(&[deck(vec![page("p0", vec![])]), deck(vec![page("p0", vec![])])]).is_err());
        // An empty key carries neither action nor target, and that is exactly what it is for.
        assert!(validate(&[deck(vec![page("p0", vec![key(0, KeyKind::Empty)])])]).is_ok());
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
        let off: Key = serde_json::from_str(r#"{"pos":0,"kind":"action","toggle":null}"#).expect("null toggle");
        assert!(off.toggle.is_none());

        let grid = Grid::new(2, 3); // six per page, so pos 6 is the first key of client page 1
        let off = layout_json(&deck, page, grid, 1);
        assert!(off.contains("Mute") && !off.contains("Unmute"));
        assert!(off.contains("\"on\":false"));
        assert!(!off.contains("toggle"), "the spare face must not reach the phone");

        // What a press would run, and what it does to the face.
        let key = resolve(page, grid, 1, 0).expect("the key");
        assert_eq!(showing_action(key, is_on(&at)), Some("vol:mute"));
        assert!(flip(&at));
        assert_eq!(showing_action(key, is_on(&at)), Some("vol:unmute"));

        let on = layout_json(&deck, page, grid, 1);
        assert!(on.contains("Unmute") && on.contains("\"on\":true"));

        // A phone with a different grid sees the SAME face: the address is absolute, so a
        // 1x7 client is not looking at a key that a 2x3 client flipped somewhere else.
        let wide = layout_json(&deck, page, Grid::new(1, 7), 0);
        assert!(wide.contains("Unmute"));
        flip(&at);
    }

    // A client cannot make the host allocate an absurd layout by lying in `hello`.
    #[test]
    fn client_grid_is_clamped() {
        assert_eq!(Grid::new(0, 0).size(), 1);
        assert_eq!(Grid::new(9999, 9999), Grid::new(MAX_SIDE, MAX_SIDE));
    }
}
