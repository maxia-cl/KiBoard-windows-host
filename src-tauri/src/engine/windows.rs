//! The open-window switcher (protocol/README.md §4.3). In v1 this was a list in a bottom sheet;
//! in v2 it is another key pad — same grid, same key shape, same pagination as a deck.
//!
//! The ordering is the whole value: Alt+Tab is useful because the window you want is almost always
//! the one you used last, so the host keeps a most-recently-used list fed by the 500 ms poll that
//! already runs for auto mode.
use std::sync::{Mutex, OnceLock};

use serde_json::json;

use crate::engine::deck::Grid;
use crate::platform;

/// A window as the platform layer reports it, before ordering.
pub(crate) struct Win {
    pub(crate) id: isize,
    pub(crate) title: String,
    /// Full path to the owning exe; the label is its file stem, the icon is extracted from it.
    pub(crate) exe: String,
    pub(crate) minimized: bool,
}

/// Foreground history, most recent first. Bounded: a long session touches hundreds of windows and
/// only the top of this list is ever read.
static MRU: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();
const MRU_CAP: usize = 64;

fn mru() -> &'static Mutex<Vec<isize>> {
    MRU.get_or_init(|| Mutex::new(Vec::new()))
}

/// Records that `id` is in the foreground right now. Called by the auto-mode poll; cheap enough to
/// run at 2 Hz because the common case is "already at the front, do nothing".
pub fn touch(id: isize) {
    if id == 0 {
        return;
    }
    let mut list = mru().lock().unwrap();
    if list.first() == Some(&id) {
        return;
    }
    list.retain(|&x| x != id);
    list.insert(0, id);
    list.truncate(MRU_CAP);
}

/// Orders windows for the switcher. Pure, so the rules are testable without a desktop.
///
/// 1. The current window sits at 0, flagged `current`: it is a reference point, not a useful
///    destination — the interesting jump starts at 1, exactly like Alt+Tab.
/// 2. Then everything else in MRU order.
/// 3. Windows never brought to the front (started and untouched) go last, in the Z-order the
///    platform reported them in.
/// 4. `minimized` is a flag to paint dimmed, NOT a reason to sort lower.
pub(crate) fn order(raw: Vec<Win>, mru_ids: &[isize], current: isize) -> Vec<Win> {
    let rank = |id: isize| mru_ids.iter().position(|&x| x == id);
    let mut out: Vec<Win> = Vec::with_capacity(raw.len());
    let mut rest: Vec<Win> = Vec::with_capacity(raw.len());
    for w in raw {
        if w.id == current {
            out.push(w);
        } else {
            rest.push(w);
        }
    }
    // Stable sort: windows with no MRU entry keep their incoming Z-order relative to each other.
    rest.sort_by_key(|w| rank(w.id).unwrap_or(usize::MAX));
    out.append(&mut rest);
    out
}

/// A `windows` message for one page of the client's grid — the same shape as a `layout`, so the
/// phone reuses the key pad as is.
pub fn windows_json(grid: Grid, page: usize) -> String {
    let current = platform::foreground_window();
    let ordered = {
        let ids = mru().lock().unwrap().clone();
        order(platform::list_windows(), &ids, current)
    };
    // The full grid on purpose: the switcher is its own surface and draws nothing of its own
    // in the reserved cells, so leaving two holes there would just look like a bug.
    let size = grid.size();
    let pages = ordered.len().div_ceil(size).max(1);
    let page = page.min(pages - 1);

    let keys: Vec<_> = ordered
        .iter()
        .skip(page * size)
        .take(size)
        .enumerate()
        .map(|(pos, w)| {
            // A packaged (UWP) window is owned by ApplicationFrameHost.exe, so naming it after its
            // executable labels Notepad, Photos and Settings alike — all "ApplicationFrameHost",
            // all with the same generic icon. The AUMID it advertises is its real identity (the
            // one the taskbar groups by), and F4's catalogue turns that into the name and tile the
            // Start menu shows. Desktop apps advertise no AUMID and keep the executable's name.
            // Resolved ONCE per window: this is a COM call, and the catalogue has ~80 entries.
            let aumid = platform::window_aumid(w.id);
            let catalogued = crate::platform::apps::app_for_window(&w.exe, &aumid);
            let label = catalogued.map(|a| a.name.clone()).unwrap_or_else(|| {
                std::path::Path::new(&w.exe)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| w.title.clone())
            });
            let mut state = json!({});
            if w.id == current {
                state["current"] = json!(true);
            }
            if w.minimized {
                state["minimized"] = json!(true);
            }
            // A `data:` URI, not bare base64. The phone decodes `image` the same way for every
            // key it draws, and that decoder requires the prefix — without it the switcher's
            // icons silently came out blank. Absent rather than empty when there is no icon: an
            // empty data URI decodes to zero bytes and draws a broken image, not nothing.
            // Use the same high-resolution shell artwork as Launcher for desktop and packaged
            // apps alike. Falling straight to the executable here magnified a 32 px Codex/Chrome
            // ICO frame across a high-density phone.
            let icon = crate::platform::apps::icon_for_window(&w.exe, &aumid);
            json!({
                "pos": pos, "id": w.id, "label": label, "sub": w.title,
                "image": (!icon.is_empty()).then(|| format!("data:image/png;base64,{icon}")),
                "state": state,
            })
        })
        .collect();

    json!({
        "v": 2, "type": "windows",
        "grid": { "rows": grid.rows, "cols": grid.cols },
        "page": page, "pages": pages,
        "keys": keys,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(id: isize) -> Win {
        Win {
            id,
            title: format!("t{id}"),
            exe: format!("C:\\a{id}.exe"),
            minimized: false,
        }
    }
    fn ids(v: &[Win]) -> Vec<isize> {
        v.iter().map(|x| x.id).collect()
    }

    #[test]
    fn current_leads_then_most_recent() {
        let raw = vec![w(1), w(2), w(3), w(4)];
        // Used 3, then 2. 1 is in the foreground now; 4 was never touched.
        let out = order(raw, &[1, 3, 2], 1);
        assert_eq!(ids(&out), vec![1, 3, 2, 4]);
    }

    // Nothing in the MRU yet (host just started): the switcher must still be usable, falling back
    // to the platform's Z-order instead of an arbitrary shuffle.
    #[test]
    fn untouched_windows_keep_z_order() {
        let out = order(vec![w(7), w(8), w(9)], &[], 0);
        assert_eq!(ids(&out), vec![7, 8, 9]);
    }

    // A minimized window is painted dimmed but must NOT fall down the list: it is often exactly
    // the one being restored.
    #[test]
    fn minimized_does_not_sink() {
        let mut raw = vec![w(1), w(2)];
        raw[1].minimized = true;
        let out = order(raw, &[2, 1], 0);
        assert_eq!(ids(&out), vec![2, 1]);
    }

    // The MRU can name windows that have since been closed; they must not resurrect or shift the
    // ordering of the ones that are still open.
    #[test]
    fn closed_windows_in_the_mru_are_ignored() {
        let out = order(vec![w(1), w(2)], &[99, 2, 98, 1], 0);
        assert_eq!(ids(&out), vec![2, 1]);
    }

    #[test]
    fn touch_moves_to_front_without_duplicating() {
        let mut list = vec![3, 2, 1];
        // Same logic as `touch`, on a local list so the test does not touch global state.
        let bump = |list: &mut Vec<isize>, id: isize| {
            list.retain(|&x| x != id);
            list.insert(0, id);
        };
        bump(&mut list, 1);
        bump(&mut list, 1);
        assert_eq!(list, vec![1, 3, 2]);
    }
}
