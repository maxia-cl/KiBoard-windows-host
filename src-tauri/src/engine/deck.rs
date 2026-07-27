//! Manual mode: repaginates a deck onto whatever grid the client declared, and resolves a
//! (page, pos) press back to the key that owns it (protocol/README.md §4.1, §4.2).
//!
//! Decks are authored on the reference 5×3 grid. A client with a different grid — a tablet, a
//! folding phone, a future 4×2 device — gets the SAME keys in the same order, just cut into
//! different pages. Nothing in the stored deck depends on the client.
use serde_json::json;

use crate::config::{Deck, Key, KeyKind, Page};

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
    if pos >= grid.size() {
        return None;
    }
    let flat = at_page.checked_mul(grid.size())?.checked_add(pos)?;
    page.keys.iter().find(|k| k.pos == flat && k.kind != KeyKind::Empty)
}

/// A `layout` message for one page of a deck (protocol/README.md §4.1).
pub(crate) fn layout_json(deck: &Deck, page: &Page, grid: Grid, at_page: usize) -> String {
    let total = pages(page, grid);
    let at_page = at_page.min(total - 1);
    json!({
        "v": 2,
        "type": "layout",
        "mode": "manual",
        "source": { "kind": "deck", "id": deck.id, "name": deck.name, "page": page.id },
        "grid": { "rows": grid.rows, "cols": grid.cols },
        "page": at_page,
        "pages": total,
        "keys": keys_for(page, grid, at_page),
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

    // A client cannot make the host allocate an absurd layout by lying in `hello`.
    #[test]
    fn client_grid_is_clamped() {
        assert_eq!(Grid::new(0, 0).size(), 1);
        assert_eq!(Grid::new(9999, 9999), Grid::new(MAX_SIDE, MAX_SIDE));
    }
}
