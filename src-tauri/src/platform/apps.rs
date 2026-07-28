//! The app catalogue (F4): what is installed, and how to launch / focus / close it.
//!
//! Enumeration goes through `Get-StartApps`, which IS the AppsFolder listing — the same set the
//! Start menu shows, UWP included, already localized.
//! ponytail: the COM enumeration route (IKnownFolderManager + IEnumShellItems) is ~150 lines of
//! unsafe for the identical list; upgrade if the one-off ~700 ms boot cost ever matters.

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
}

/// Does a window belong to app `id`, given the executable behind it and the AUMID it advertises?
///
/// Pure, so it tests on any target. Two identities, because neither one covers both worlds:
/// a packaged (UWP) app is only recognisable by its AUMID — every one of them runs behind
/// `ApplicationFrameHost.exe` — while a desktop app usually advertises no AUMID at all and is
/// matched by executable name.
pub fn matches(id: &str, win_exe: &str, win_aumid: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    if id.contains('!') {
        return !win_aumid.is_empty() && win_aumid.eq_ignore_ascii_case(id);
    }
    !win_exe.is_empty() && !stem(id).is_empty() && stem(id) == stem(win_exe)
}

/// Lowercased file name without extension: `C:\W\System32\cleanmgr.exe` -> `cleanmgr`.
fn stem(path: &str) -> String {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path).to_ascii_lowercase();
    name.strip_suffix(".exe").unwrap_or(&name).to_string()
}

/// Every open window of `id`. Empty means the app is not running.
/// The AUMID lookup is one COM call per window, so it only runs for AUMID targets.
fn windows_of(id: &str) -> Vec<isize> {
    let by_aumid = id.contains('!');
    crate::platform::list_windows()
        .into_iter()
        .filter(|w| {
            let aumid =
                if by_aumid { crate::platform::window_aumid(w.id) } else { String::new() };
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
            let aumid =
                if need_aumid { crate::platform::window_aumid(w.id) } else { String::new() };
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
    match windows_of(id).first() {
        Some(hwnd) => {
            crate::platform::focus_window(*hwnd);
            Ok(())
        }
        None => activate(id),
    }
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
                "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
                 Get-StartApps | ConvertTo-Json -Compress",
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
                // .msc / .chm / uninstallers are Start-menu entries too; a key pad wants apps.
                if name.is_empty() || (!id.contains('!') && !id.to_ascii_lowercase().ends_with(".exe")) {
                    return None;
                }
                let exe = resolve_exe(&id);
                Some(App { id, name, exe })
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
        let Some(rest) = id.strip_prefix('{') else { return String::new() };
        let Some((guid, rel)) = rest.split_once("}\\") else { return String::new() };
        let Ok(guid) = windows::core::GUID::try_from(guid) else { return String::new() };
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
            let Ok(hbmp) = factory.GetImage(SIZE { cx: TILE_PX, cy: TILE_PX }, SIIGBF_ICONONLY)
            else {
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
            GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
            BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
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
            let un = |c: u8| if a == 0 { 0 } else { ((c as u32 * 255) / a as u32).min(255) as u8 };
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
    use super::matches;

    #[test]
    fn known_folder_appid_matches_its_exe() {
        let id = r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\cleanmgr.exe";
        assert!(matches(id, r"C:\WINDOWS\system32\cleanmgr.exe", ""));
        assert!(!matches(id, r"C:\WINDOWS\system32\notepad.exe", ""));
        assert!(!matches(id, "", ""));
    }

    #[test]
    fn absolute_path_matches_regardless_of_case_or_folder() {
        let id = r"C:\Program Files\Foo\Foo.exe";
        assert!(matches(id, r"c:\program files\foo\FOO.EXE", ""));
        assert!(!matches(id, r"C:\Program Files\Bar\Bar.exe", ""));
    }

    /// Every packaged app runs behind ApplicationFrameHost, so the executable proves nothing —
    /// only the AUMID the window advertises does.
    #[test]
    fn packaged_app_matches_by_aumid_only() {
        let id = "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";
        let host = r"C:\WINDOWS\system32\ApplicationFrameHost.exe";
        assert!(matches(id, host, "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"));
        assert!(matches(id, host, "microsoft.windowscalculator_8wekyb3d8bbwe!app"));
        // Another packaged app behind the same host must not be mistaken for it.
        assert!(!matches(id, host, "Microsoft.WindowsCamera_8wekyb3d8bbwe!App"));
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
