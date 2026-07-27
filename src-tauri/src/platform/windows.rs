//! Windows implementation of the platform surface: icon extraction, UI Automation, process-tree
//! inspection, Core Audio volume, window enumeration/focus.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Extracts the executable's icon as base64 PNG. Empty string if it can't be done.
/// ponytail: uses the .exe's real icon instead of bundling logos; covers any app.
pub fn extract_icon_b64(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let p = path.to_string();
    let b64 = std::panic::catch_unwind(move || windows_icons::get_icon_base64_by_path(&p))
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    // In case it came as a data URI ("data:image/png;base64,...").
    b64.rsplit(',').next().unwrap_or("").to_string()
}

/// Which shell is running INSIDE the foreground window. Returns the profile id
/// ("shell-pwsh" | "shell-cmd" | "shell-bash"). See `super::pick_shell` for the matching rule.
pub fn detect_shell_kind(root_pid: u32, title: &str) -> Option<&'static str> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    if root_pid == 0 {
        return None;
    }
    // (pid, parent_pid, exe) of every process.
    let mut procs: Vec<(u32, u32, String)> = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                let exe = String::from_utf16_lossy(&entry.szExeFile[..end]).to_lowercase();
                procs.push((entry.th32ProcessID, entry.th32ParentProcessID, exe));
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
    }
    super::pick_shell(&procs, root_pid, title)
}

/// Which `uia:` actions are disabled right now in the foreground window's control (so the phone
/// can hide them). Empty if the profile has no uia buttons -> zero cost for normal apps.
pub fn uia_disabled_actions(
    buttons: &[(usize, String, String, String, bool, bool)],
) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    use uiautomation::types::Handle;
    use uiautomation::UIAutomation;
    let mut out = HashSet::new();
    // (full action, first step's name) of every uia button.
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

/// Locates an element of the foreground window by name (partial match) and clicks it.
/// ponytail: searches from the root and takes the first match; enough because the target app is
/// in the foreground.
pub fn invoke_uia(name: &str) -> Result<(), &'static str> {
    use uiautomation::types::Handle;
    use uiautomation::UIAutomation;
    let automation = UIAutomation::new().map_err(|_| "uia_init")?;
    let desktop = automation.get_root_element().map_err(|_| "uia_root")?;
    // Scope to the foreground window (small tree -> fast). If not found there (a popup menu/
    // flyout can live in another window), retry from the desktop.
    let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    let fg = automation.element_from_handle(Handle::from(hwnd.0 as isize)).ok();
    // Candidates by root: exact name first ("Rectangle" doesn't collide with "Rounded
    // rectangle"); partial match otherwise.
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
    // Activate via UIA pattern, NOT a mouse click (el.click() would move the cursor).
    // Several apps duplicate the name: a container (GridViewItem, Legacy only) and the real
    // button (Toggle/Invoke). Prefer the candidate that exposes an actionable pattern.
    use uiautomation::patterns::{
        UIInvokePattern, UILegacyIAccessiblePattern, UISelectionItemPattern, UITogglePattern,
    };
    // Prefer ENABLED candidates (Paint disables "Resize"/"Crop" depending on context: invoking a
    // disabled control "succeeds" but does nothing -> would look broken).
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
    // No enabled actionable candidate: MSAA "default action" (doesn't move the mouse).
    for el in &candidates {
        if el.is_enabled().unwrap_or(true) {
            if let Ok(p) = el.get_pattern::<UILegacyIAccessiblePattern>() {
                return p.do_default_action().map_err(|_| "uia_legacy");
            }
        }
    }
    // Only disabled matches: tell the phone instead of silently doing nothing.
    if saw_disabled {
        return Err("uia_disabled");
    }
    candidates[0].click().map_err(|_| "uia_click") // ponytail: last resort; this one does move the mouse
}

/// Runs `f` with the default output device's IAudioEndpointVolume.
fn with_endpoint_volume<T>(
    f: impl FnOnce(&windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume) -> windows::core::Result<T>,
) -> Option<T> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    unsafe {
        // ponytail: COM init/uninit per call; at 2Hz the cost is irrelevant.
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

/// (volume 0-100, muted) of the default output device.
pub fn system_volume() -> Option<(u8, bool)> {
    with_endpoint_volume(|v| unsafe {
        let level = v.GetMasterVolumeLevelScalar()?;
        let muted = v.GetMute()?.as_bool();
        Ok(((level * 100.0).round() as u8, muted))
    })
}

/// Sets the master volume (0.0-1.0) and unmutes (moving the slider means "I want to hear it").
pub fn set_system_volume(level: f32) -> Result<(), &'static str> {
    with_endpoint_volume(|v| unsafe {
        v.SetMasterVolumeLevelScalar(level, std::ptr::null())?;
        v.SetMute(false, std::ptr::null())
    })
    .ok_or("sin_audio")
}

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
    // "Cloaked" windows (suspended UWP: Settings, Store...): Windows lists them as visible but
    // they are NOT on screen — they're the duplicate ghosts in the switcher.
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
    // Tool windows (floating palettes, overlays) don't show up in Windows' own alt-tab: skip.
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

/// Path of the executable that owns a window (to pull its icon).
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

/// Icon by exe path, cached (extracting it is expensive and icons don't change).
pub fn icon_cached(path: &str) -> String {
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

/// Lists open app windows (`windows` JSON), with each exe's real icon.
pub fn list_windows_json() -> String {
    use serde_json::json;
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

/// Brings a window to the foreground (restores it if minimized).
/// Windows blocks focus-stealing from a background process; we work around it with a tap of
/// Alt + AttachThreadInput (standard technique) to force a real foreground switch.
pub fn focus_window(id: isize) {
    use enigo::{
        Direction::{Press, Release},
        Enigo, Key, Keyboard, Settings,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };
    // Holding Alt during the switch dodges Windows' foreground lock.
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

/// Is there a process whose name is in `names` (case-insensitive)? Used to tell "OBS closed"
/// apart from "OBS open but its WebSocket server is off".
pub fn process_running(names: &[&str]) -> bool {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return false;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                let exe = String::from_utf16_lossy(&entry.szExeFile[..end]).to_lowercase();
                if names.iter().any(|n| exe == n.to_lowercase()) {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
        found
    }
}

/// Takes a screenshot of the primary monitor and saves it under Pictures/KiBoard.
pub fn take_screenshot() -> Result<(), &'static str> {
    use xcap::Monitor;
    let monitor = Monitor::all().map_err(|_| "internal")?.into_iter().next().ok_or("no_monitor")?;
    let img = monitor.capture_image().map_err(|_| "internal")?;
    let dir = dirs::picture_dir().unwrap_or(std::env::temp_dir()).join("KiBoard");
    std::fs::create_dir_all(&dir).map_err(|_| "internal")?;
    let path = dir.join(format!("screenshot-{}.png", crate::now_ts()));
    img.save(&path).map_err(|_| "internal")?;
    // Audible cue: a screenshot can be triggered remotely from the phone; a sound tells whoever
    // is in front of the PC that one was taken (UX + anti silent-abuse).
    unsafe {
        let _ = windows::Win32::System::Diagnostics::Debug::MessageBeep(
            windows::Win32::UI::WindowsAndMessaging::MB_OK,
        );
    }
    Ok(())
}
