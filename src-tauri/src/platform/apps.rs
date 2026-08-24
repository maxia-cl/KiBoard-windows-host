//! The app catalogue (F4): what is installed, and how to launch / focus / close it.
//!
//! Enumeration goes through `Get-StartApps`, which IS the AppsFolder listing — the same set the
//! Start menu shows, UWP included, already localized.
//! ponytail: the COM enumeration route (IKnownFolderManager + IEnumShellItems) is ~150 lines of
//! unsafe for the identical list; upgrade if the one-off ~700 ms boot cost ever matters.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// One entry of the catalogue. `id` is what a `launch:` / `focus:` / `kill:` key stores.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct App {
    /// AUMID (`Microsoft.WindowsNotepad_8wekyb3d8bbwe!App`), known-folder AppID
    /// (`{GUID}\rel\app.exe`) or an absolute path.
    pub id: String,
    pub name: String,
    /// Absolute path when it can be resolved. Empty for packaged (UWP) entries, which have no
    /// executable of their own — `icon` gets theirs from the shell instead.
    pub exe: String,
    /// Windows' best-effort last execution time, used only to bootstrap the local recent list.
    /// It is deliberately absent from the editor's catalogue JSON.
    #[serde(skip)]
    pub(crate) last_opened: u64,
}

/// Thirty rolling days, not "this calendar month": an app used 29 days ago remains useful today.
const RECENT_WINDOW_MS: u64 = 30 * 24 * 60 * 60 * 1000;

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn recent_path() -> std::path::PathBuf {
    crate::config::config_dir().join("launcher-recent.json")
}

fn save_recent(values: &HashMap<String, u64>) {
    if let Ok(json) = serde_json::to_string_pretty(values) {
        let _ = std::fs::write(recent_path(), json);
    }
}

/// Loads KiBoard's own foreground history, then bootstraps it from Windows UserAssist and apps
/// that are open right now. UserAssist is useful for the first run; the local file is authoritative
/// afterwards because Windows does not record every launch path consistently.
fn load_recent() -> HashMap<String, u64> {
    let mut values: HashMap<String, u64> = std::fs::read_to_string(recent_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let apps = catalogue();
    for app in apps {
        if app.last_opened > values.get(&app.id).copied().unwrap_or(0) {
            values.insert(app.id.clone(), app.last_opened);
        }
    }
    // First run should not produce an almost-empty launcher while the user already has their daily
    // tools on screen. Open windows are indisputably recent and can be gathered in one pass.
    let ids: Vec<String> = apps.iter().map(|a| a.id.clone()).collect();
    let now = now_millis();
    for (app, open) in apps.iter().zip(running(&ids)) {
        if open {
            values.insert(app.id.clone(), now);
        }
    }
    let cutoff = now.saturating_sub(RECENT_WINDOW_MS);
    values.retain(|_, used| *used >= cutoff && *used <= now.saturating_add(24 * 60 * 60 * 1000));
    save_recent(&values);
    values
}

fn recent() -> &'static Mutex<HashMap<String, u64>> {
    static RECENT: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    RECENT.get_or_init(|| Mutex::new(load_recent()))
}

fn ranked_recent<'a>(apps: &'a [App], values: &HashMap<String, u64>, now: u64) -> Vec<&'a App> {
    let cutoff = now.saturating_sub(RECENT_WINDOW_MS);
    let mut rows: Vec<(&App, u64)> = apps
        .iter()
        .filter_map(|app| {
            let used = values.get(&app.id).copied()?;
            (used >= cutoff && used <= now.saturating_add(24 * 60 * 60 * 1000))
                .then_some((app, used))
        })
        .collect();
    rows.sort_by(|(a, at), (b, bt)| {
        bt.cmp(at)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    rows.into_iter().map(|(app, _)| app).collect()
}

/// Catalogue entries used during the rolling month, newest first.
pub fn recent_catalogue() -> Vec<&'static App> {
    let apps = catalogue();
    ranked_recent(apps, &recent().lock().unwrap(), now_millis())
}

fn touch_recent_id(id: &str) -> bool {
    if !catalogue().iter().any(|app| app.id == id) {
        return false;
    }
    let mut values = recent().lock().unwrap();
    let now = now_millis();
    values.insert(id.to_string(), now);
    let cutoff = now.saturating_sub(RECENT_WINDOW_MS);
    values.retain(|_, used| *used >= cutoff);
    save_recent(&values);
    true
}

/// Records the app owning a foreground window. Returns true only when it resolved to a catalogue
/// entry, so callers know whether the generated Launcher may need to be reordered.
pub fn touch_recent_window(exe: &str, aumid: &str) -> bool {
    let Some(id) = catalogue()
        .iter()
        .find(|app| matches(&app.id, exe, aumid))
        .map(|app| app.id.clone())
    else {
        return false;
    };
    touch_recent_id(&id)
}

/// Does a window belong to app `id`, given the executable behind it and the AUMID it advertises?
///
/// Pure, so it tests on any target. A desktop app is matched by executable name. A packaged (UWP)
/// one has TWO shapes, and the second is why this was reporting apps as closed with their window
/// open: the old kind runs behind `ApplicationFrameHost.exe` and advertises its AUMID, while a
/// modern one (Notepad 11, WhatsApp, Claude) owns its window and advertises nothing — for those,
/// the identity is the package family in the `WindowsApps` path.
pub fn matches(id: &str, win_exe: &str, win_aumid: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    if let Some((family, _)) = id.split_once('!') {
        // The AUMID when the window publishes one — the old shape, where a packaged app's window
        // belongs to ApplicationFrameHost.exe and carries PKEY_AppUserModel_ID (Calculator still
        // does this).
        if !win_aumid.is_empty() {
            return win_aumid.eq_ignore_ascii_case(id);
        }
        // Modern packaged apps own their window and publish NO AUMID at all: Notepad 11, WhatsApp
        // and Claude all report an empty one. Their identity is still in the path — every one runs
        // out of `WindowsApps\<name>_<version>_<arch>__<publisher>` — and name+publisher IS the
        // package family, which is the half of the AUMID before the `!`. Matching that is not the
        // executable-name guess this deliberately avoids: it is the same package, by identity.
        //
        // Without it `state.running` was false for every modern Store app with its window open, so
        // `launch:` opened a second copy and `focus:`/`kill:` answered `not_running`.
        return package_family(win_exe).is_some_and(|f| f.eq_ignore_ascii_case(family));
    }
    !win_exe.is_empty() && !stem(id).is_empty() && stem(id) == stem(win_exe)
}

/// `…\WindowsApps\Microsoft.WindowsNotepad_11.2605.34.0_x64__8wekyb3d8bbwe\Notepad.exe`
/// -> `Microsoft.WindowsNotepad_8wekyb3d8bbwe`, the package family name. None for anything that
/// does not live under `WindowsApps`.
fn package_family(win_exe: &str) -> Option<String> {
    let rest = win_exe
        .split_once("\\WindowsApps\\")
        .or_else(|| win_exe.split_once("/WindowsApps/"))?
        .1;
    let folder = rest.split(['\\', '/']).next()?;
    let name = folder.split('_').next()?;
    // The publisher id is last: the folder ends `..._<arch>__<publisher>`, and the empty piece
    // between the double underscore is why this takes the last NON-EMPTY one.
    let publisher = folder.rsplit('_').find(|p| !p.is_empty())?;
    if name.is_empty() || publisher == name {
        return None;
    }
    Some(format!("{name}_{publisher}"))
}

/// Lowercased file name without extension: `C:\W\System32\cleanmgr.exe` -> `cleanmgr`.
fn stem(path: &str) -> String {
    let name = path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();
    name.strip_suffix(".exe").unwrap_or(&name).to_string()
}

/// Every open window of `id`. Empty means the app is not running.
/// The AUMID lookup is one COM call per window, so it only runs for AUMID targets.
fn windows_of(id: &str) -> Vec<isize> {
    let by_aumid = id.contains('!');
    crate::platform::list_windows()
        .into_iter()
        .filter(|w| {
            let aumid = if by_aumid {
                crate::platform::window_aumid(w.id)
            } else {
                String::new()
            };
            matches(id, &w.exe, &aumid)
        })
        .map(|w| w.id)
        .collect()
}

/// The executable behind a catalogue id, or "".
fn exe_of(id: &str) -> &'static str {
    catalogue()
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.exe.as_str())
        .unwrap_or("")
}

/// The app's real icon as base64 PNG, or "" — what a `launch:` key shows instead of a glyph.
///
/// Two sources, because a packaged app has no executable to pull an icon out of: a desktop app
/// goes through the exe's own icon, everything else asks the shell for the tile it draws in the
/// Start menu. Both are cached; extracting an icon is expensive and icons do not change.
pub fn icon(id: &str) -> String {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    let exe = exe_of(id);
    if !exe.is_empty() {
        return crate::platform::icon_cached(exe);
    }
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(b) = cache.lock().unwrap().get(id) {
        return b.clone();
    }
    let b = imp::shell_tile_b64(id);
    cache.lock().unwrap().insert(id.to_string(), b.clone());
    b
}

/// Which of `ids` have at least one open window, in ONE pass over the window list — a layout can
/// carry fifteen `launch:` keys and enumerating the desktop once per key would be absurd.
pub fn running(ids: &[String]) -> Vec<bool> {
    let need_aumid = ids.iter().any(|i| i.contains('!'));
    let wins: Vec<(String, String)> = crate::platform::list_windows()
        .into_iter()
        .map(|w| {
            let aumid = if need_aumid {
                crate::platform::window_aumid(w.id)
            } else {
                String::new()
            };
            (w.exe, aumid)
        })
        .collect();
    ids.iter()
        .map(|id| wins.iter().any(|(exe, aumid)| matches(id, exe, aumid)))
        .collect()
}

/// `launch:<id>` — focuses a running instance instead of duplicating it (protocol §3, table of
/// actions), otherwise activates the catalogue entry.
pub fn launch(id: &str) -> Result<(), &'static str> {
    let result = match windows_of(id).first() {
        Some(hwnd) => {
            crate::platform::focus_window(*hwnd);
            Ok(())
        }
        None => activate(id),
    };
    // A Launcher press has an exact identity, so it can update the order immediately instead of
    // waiting for the foreground poll. The poll remains necessary for apps opened elsewhere.
    if result.is_ok() && touch_recent_id(id) {
        crate::config::refresh_launcher();
    }
    result
}

/// `focus:<id>` — never launches.
pub fn focus(id: &str) -> Result<(), &'static str> {
    let hwnd = *windows_of(id).first().ok_or("not_running")?;
    crate::platform::focus_window(hwnd);
    Ok(())
}

/// `kill:<id>` — asks every window of the app to close (WM_CLOSE), the same as clicking the ✕, so
/// the app can still prompt to save.
/// ponytail: no TerminateProcess fallback; a hung app stays for the Task Manager.
pub fn close(id: &str) -> Result<(), &'static str> {
    let hit = windows_of(id);
    if hit.is_empty() {
        return Err("not_running");
    }
    for h in hit {
        crate::platform::close_window(h);
    }
    Ok(())
}

#[cfg(windows)]
mod imp {
    use super::App;
    use std::sync::OnceLock;

    /// Hides the console window PowerShell would otherwise flash (CREATE_NO_WINDOW).
    const NO_WINDOW: u32 = 0x0800_0000;

    /// `Get-StartApps` supplies the launchable catalogue but no usage date. UserAssist is Windows'
    /// per-user GUI execution history; its value names are ROT13 and its FILETIME starts at byte
    /// 60 on current Windows. Failure to read it simply leaves `LastOpened` at zero — KiBoard's own
    /// foreground history takes over from the first run onward.
    const CATALOGUE_SCRIPT: &str = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$usage = @{}
$root = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist'
foreach ($key in Get-ChildItem "$root\*\Count" -ErrorAction SilentlyContinue) {
  $props = Get-ItemProperty -LiteralPath $key.PSPath
  foreach ($prop in $props.PSObject.Properties) {
    $bytes = $prop.Value
    if ($prop.Name -like 'PS*' -or $bytes -isnot [byte[]] -or $bytes.Length -lt 68) { continue }
    $fileTime = [BitConverter]::ToInt64($bytes, 60)
    if ($fileTime -le 0) { continue }
    $decoded = -join ($prop.Name.ToCharArray() | ForEach-Object {
      $n = [int]$_
      if (($n -ge 65 -and $n -le 77) -or ($n -ge 97 -and $n -le 109)) { [char]($n + 13) }
      elseif (($n -ge 78 -and $n -le 90) -or ($n -ge 110 -and $n -le 122)) { [char]($n - 13) }
      else { $_ }
    })
    try { $millis = [DateTimeOffset]::new([DateTime]::FromFileTimeUtc($fileTime)).ToUnixTimeMilliseconds() }
    catch { continue }
    if (-not $usage.ContainsKey($decoded) -or $millis -gt $usage[$decoded]) { $usage[$decoded] = $millis }
  }
}
@(Get-StartApps | ForEach-Object {
  $last = 0
  if ($usage.ContainsKey($_.AppID)) { $last = $usage[$_.AppID] }
  [pscustomobject]@{ Name = $_.Name; AppID = $_.AppID; LastOpened = $last }
}) | ConvertTo-Json -Compress
"#;

    /// Every Start-menu entry, resolved and filtered, computed once per run.
    /// ponytail: no invalidation — installing an app mid-session needs a host restart to show up.
    pub fn catalogue() -> &'static [App] {
        static CACHE: OnceLock<Vec<App>> = OnceLock::new();
        CACHE.get_or_init(build)
    }

    fn build() -> Vec<App> {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                // Without the encoding line PowerShell writes the console's OEM codepage and every
                // accented app name ("Administración de equipos") arrives as invalid UTF-8, which
                // takes the WHOLE catalogue down with it.
                CATALOGUE_SCRIPT,
            ])
            .creation_flags(NO_WINDOW)
            .output();
        let Ok(out) = out else { return Vec::new() };
        let parsed: Vec<serde_json::Value> =
            serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap_or_default();
        let mut apps: Vec<App> = parsed
            .into_iter()
            .filter_map(|v| {
                let id = v["AppID"].as_str()?.to_string();
                let name = v["Name"].as_str()?.trim().to_string();
                // .msc / .chm / web links are Start-menu entries too; a key pad wants apps. Plain
                // aliases such as `Chrome` and `MSEdge` are valid AppsFolder ids and must survive.
                let lower = id.to_ascii_lowercase();
                let alias = !id.contains(['\\', '/']) && !id.contains(':');
                if name.is_empty() || !(id.contains('!') || lower.ends_with(".exe") || alias) {
                    return None;
                }
                let exe = resolve_exe(&id);
                let last_opened = v["LastOpened"].as_u64().unwrap_or(0);
                Some(App {
                    id,
                    name,
                    exe,
                    last_opened,
                })
            })
            .collect();
        apps.sort_by_key(|a| a.name.to_lowercase());
        apps.dedup_by(|a, b| a.id == b.id);
        apps
    }

    /// `{1AC14E77-…}\cleanmgr.exe` -> `C:\WINDOWS\system32\cleanmgr.exe`. The AppID's leading brace
    /// group is a KNOWNFOLDERID, so one `SHGetKnownFolderPath` resolves any of them without a table
    /// of GUIDs to keep up to date. Absolute paths pass through; AUMIDs have no exe.
    fn resolve_exe(id: &str) -> String {
        use windows::Win32::UI::Shell::{SHGetKnownFolderPath, KNOWN_FOLDER_FLAG};
        if std::path::Path::new(id).is_absolute() {
            return id.to_string();
        }
        let Some(rest) = id.strip_prefix('{') else {
            return String::new();
        };
        let Some((guid, rel)) = rest.split_once("}\\") else {
            return String::new();
        };
        let Ok(guid) = windows::core::GUID::try_from(guid) else {
            return String::new();
        };
        unsafe {
            let Ok(p) = SHGetKnownFolderPath(&guid, KNOWN_FOLDER_FLAG(0), None) else {
                return String::new();
            };
            let base = p.to_string().unwrap_or_default();
            windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
            if base.is_empty() {
                return String::new();
            }
            format!("{base}\\{rel}")
        }
    }

    /// Side of a shell tile in pixels. The phone draws keys around 113 pt, so 96 is already more
    /// than it needs and every packaged app ships an asset at least this big.
    const TILE_PX: i32 = 96;

    /// The icon the Start menu draws for a catalogue entry, as base64 PNG.
    ///
    /// This is the only source for a packaged app: there is no executable to pull an icon out of,
    /// and `C:\Program Files\WindowsApps` denies even traversal, so reading the package's own
    /// assets is not an option. `IShellItemImageFactory` over `shell:AppsFolder\<AppID>` asks the
    /// shell for the same bitmap it draws itself, ACLs and all.
    pub fn shell_tile_b64(id: &str) -> String {
        use windows::Win32::Foundation::SIZE;
        use windows::Win32::Graphics::Gdi::{DeleteObject, HGDIOBJ};
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Shell::{
            IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY,
        };
        unsafe {
            // Harmless if this thread is already initialized (or initialized in another mode):
            // the shell call below is what actually reports failure.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let path: Vec<u16> = format!("shell:AppsFolder\\{id}\0").encode_utf16().collect();
            let factory: IShellItemImageFactory =
                match SHCreateItemFromParsingName(windows::core::PCWSTR(path.as_ptr()), None) {
                    Ok(f) => f,
                    Err(_) => return String::new(),
                };
            // ICONONLY, never a document thumbnail: a launcher key must show what the app IS, not
            // a preview of whatever it last opened.
            let Ok(hbmp) = factory.GetImage(
                SIZE {
                    cx: TILE_PX,
                    cy: TILE_PX,
                },
                SIIGBF_ICONONLY,
            ) else {
                return String::new();
            };
            let out = bitmap_to_png_b64(hbmp);
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            out
        }
    }

    /// HBITMAP -> base64 PNG. The shell hands back a 32-bit top-down DIB in BGRA with
    /// PREMULTIPLIED alpha, so the channels are both reordered and undone here.
    unsafe fn bitmap_to_png_b64(hbmp: windows::Win32::Graphics::Gdi::HBITMAP) -> String {
        use base64::Engine;
        use windows::Win32::Graphics::Gdi::{
            GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
            DIB_RGB_COLORS, HGDIOBJ,
        };
        let mut bm = BITMAP::default();
        let n = unsafe {
            GetObjectW(
                HGDIOBJ(hbmp.0),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bm as *mut _ as *mut _),
            )
        };
        if n == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            return String::new();
        }
        let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: bm.bmWidth,
                // Negative height = top-down rows, the order an image buffer wants.
                biHeight: -bm.bmHeight,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let ok = unsafe {
            let dc = GetDC(None);
            let r = GetDIBits(
                dc,
                hbmp,
                0,
                h,
                Some(buf.as_mut_ptr() as *mut _),
                &mut info,
                DIB_RGB_COLORS,
            );
            ReleaseDC(None, dc);
            r
        };
        if ok == 0 {
            return String::new();
        }
        // Some entries come back with a zeroed alpha channel (a bitmap that never had one). Taken
        // literally that is a fully transparent icon, i.e. a blank key — treat it as opaque.
        let opaque = buf.chunks_exact(4).all(|p| p[3] == 0);
        for px in buf.chunks_exact_mut(4) {
            let (b, g, r, a) = (px[0], px[1], px[2], px[3]);
            if opaque {
                px[0] = r;
                px[2] = b;
                px[3] = 255;
                continue;
            }
            // Undo the premultiplication, or every semi-transparent edge renders too dark.
            let un = |c: u8| {
                if a == 0 {
                    0
                } else {
                    ((c as u32 * 255) / a as u32).min(255) as u8
                }
            };
            px[0] = un(r);
            px[1] = un(g);
            px[2] = un(b);
        }
        let Some(img) = image::RgbaImage::from_raw(w, h, buf) else {
            return String::new();
        };
        let mut png = std::io::Cursor::new(Vec::new());
        if img.write_to(&mut png, image::ImageFormat::Png).is_err() {
            return String::new();
        }
        base64::engine::general_purpose::STANDARD.encode(png.into_inner())
    }

    /// Starts the app. `explorer shell:AppsFolder\<AppID>` is the documented activation for ANY
    /// Start-menu entry, UWP included, without needing IApplicationActivationManager.
    pub fn activate(id: &str) -> Result<(), &'static str> {
        use std::os::windows::process::CommandExt;
        // An absolute path is spawned directly, with NO arguments — that is the whole difference
        // from `run:`, which takes a command line and stays blocked until Settings can opt in.
        if std::path::Path::new(id).is_absolute() {
            if !std::path::Path::new(id).is_file() {
                return Err("not_found");
            }
            return std::process::Command::new(id)
                .creation_flags(NO_WINDOW)
                .spawn()
                .map(|_| ())
                .map_err(|_| "internal");
        }
        // No shell is involved (Command passes the argument through as one token), so an AppID
        // full of quotes is data, not injection.
        std::process::Command::new("explorer.exe")
            .arg(format!("shell:AppsFolder\\{id}"))
            .creation_flags(NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|_| "internal")
    }
}

#[cfg(not(windows))]
mod imp {
    use super::App;
    pub fn catalogue() -> &'static [App] {
        &[]
    }
    pub fn activate(_id: &str) -> Result<(), &'static str> {
        Err("unsupported_platform")
    }
    pub fn shell_tile_b64(_id: &str) -> String {
        String::new()
    }
}

pub use imp::{activate, catalogue};

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{matches, ranked_recent, App, RECENT_WINDOW_MS};

    fn app(id: &str, name: &str) -> App {
        App {
            id: id.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn recent_apps_are_filtered_to_thirty_days_and_newest_first() {
        let now = 10 * RECENT_WINDOW_MS;
        let apps = vec![
            app("old", "Old"),
            app("yesterday", "Yesterday"),
            app("today-b", "Beta"),
            app("today-a", "Alpha"),
            app("never", "Never"),
        ];
        let values = HashMap::from([
            ("old".into(), now - RECENT_WINDOW_MS - 1),
            ("yesterday".into(), now - 24 * 60 * 60 * 1000),
            ("today-b".into(), now),
            ("today-a".into(), now),
        ]);

        let ids: Vec<&str> = ranked_recent(&apps, &values, now)
            .into_iter()
            .map(|app| app.id.as_str())
            .collect();
        assert_eq!(ids, ["today-a", "today-b", "yesterday"]);
    }

    #[test]
    fn known_folder_appid_matches_its_exe() {
        let id = r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\cleanmgr.exe";
        assert!(matches(id, r"C:\WINDOWS\system32\cleanmgr.exe", ""));
        assert!(!matches(id, r"C:\WINDOWS\system32\notepad.exe", ""));
        assert!(!matches(id, "", ""));
    }

    /// Real strings, read off this machine with Notepad, WhatsApp and Calculator open. Modern
    /// packaged apps publish no AUMID on their window at all, which is why `state.running` said
    /// false with the window right there.
    #[test]
    fn packaged_app_matches_by_package_family_when_the_window_has_no_aumid() {
        let notepad = "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App";
        let win = r"C:\Program Files\WindowsApps\Microsoft.WindowsNotepad_11.2605.34.0_x64__8wekyb3d8bbwe\Notepad\Notepad.exe";
        assert!(
            matches(notepad, win, ""),
            "same package, no AUMID on the window"
        );

        // The old shape still wins when it is there: Calculator's window belongs to
        // ApplicationFrameHost and carries the AUMID.
        let calc = "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";
        assert!(matches(
            calc,
            r"C:\Windows\System32\ApplicationFrameHost.exe",
            calc
        ));

        // A different package of the same publisher must NOT match, and neither must a desktop app
        // that merely happens to be called Notepad.
        assert!(!matches(
            notepad,
            r"C:\Program Files\WindowsApps\Microsoft.WindowsCalculator_11.0_x64__8wekyb3d8bbwe\Calc.exe",
            ""
        ));
        assert!(!matches(notepad, r"C:\Windows\System32\notepad.exe", ""));

        // Double underscore before the publisher id, and a publisher that is not the last thing
        // before it — the two shapes the folder name actually comes in.
        let whats = "5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App";
        assert!(matches(
            whats,
            r"C:\Program Files\WindowsApps\5319275A.WhatsAppDesktop_2.2629.100.0_x64__cv1g1gvanyjgm\WhatsApp.Root.exe",
            ""
        ));
    }

    #[test]
    fn absolute_path_matches_regardless_of_case_or_folder() {
        let id = r"C:\Program Files\Foo\Foo.exe";
        assert!(matches(id, r"c:\program files\foo\FOO.EXE", ""));
        assert!(!matches(id, r"C:\Program Files\Bar\Bar.exe", ""));
        assert!(matches(
            "Chrome",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            ""
        ));
    }

    /// Every packaged app runs behind ApplicationFrameHost, so the executable proves nothing —
    /// only the AUMID the window advertises does.
    #[test]
    fn packaged_app_matches_by_aumid_only() {
        let id = "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";
        let host = r"C:\WINDOWS\system32\ApplicationFrameHost.exe";
        assert!(matches(
            id,
            host,
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
        ));
        assert!(matches(
            id,
            host,
            "microsoft.windowscalculator_8wekyb3d8bbwe!app"
        ));
        // Another packaged app behind the same host must not be mistaken for it.
        assert!(!matches(
            id,
            host,
            "Microsoft.WindowsCamera_8wekyb3d8bbwe!App"
        ));
        assert!(!matches(id, host, ""));
    }

    /// An AUMID key must never fall back to executable matching: `focus:` reporting the wrong
    /// window would send the press to somebody else's app.
    #[test]
    fn aumid_never_falls_back_to_the_executable() {
        assert!(!matches("Foo.Bar_x!App", r"C:\a\Foo.Bar.exe", ""));
    }

    /// Manual probe against the real machine:
    /// `cargo test probe_catalogue -- --nocapture --ignored`
    #[test]
    #[ignore]
    fn probe_catalogue() {
        for a in super::catalogue().iter().take(40) {
            println!("{:60} {:50} {}", a.name, a.id, a.exe);
        }
        println!("total: {}", super::catalogue().len());
    }

    /// Manual round trip against the real desktop — launches, then focuses, then closes:
    /// `APP=Microsoft.WindowsCalculator_8wekyb3d8bbwe!App cargo test probe_launch -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn probe_launch() {
        let id = std::env::var("APP").unwrap();
        println!("launch -> {:?}", super::launch(&id));
        std::thread::sleep(std::time::Duration::from_secs(3));
        println!("focus  -> {:?}", super::focus(&id));
        println!("kill   -> {:?}", super::close(&id));
    }
}
