//! OS-specific capabilities behind a stable function surface: active-window icons, UI Automation,
//! process-tree shell detection, Core Audio volume, window enumeration/focus. `windows` holds the
//! real Windows implementation; `stub` is an empty implementation so the crate compiles on other
//! targets (docs/implementation-plan.md §2). Neither macOS nor Linux backends exist yet — see the
//! cross-platform decision in that plan.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::*;

/// Which shell is running INSIDE the foreground window, given its process tree. Pure (no OS
/// calls), so it compiles and tests identically on every target — only gathering the process
/// list (`detect_shell_kind`) is platform-specific.
///
/// Why the window title isn't enough: Windows Terminal changes it per tab, but it LIES the
/// moment you run another shell inside (a `wsl` inside a pwsh tab leaves the title saying
/// "Windows PowerShell", and we'd send it `ls -Force` meant for bash). This looks at the real
/// process tree instead: the shell is a descendant of the window's process
/// (WindowsTerminal -> OpenConsole -> pwsh).
///
/// Rule: the DEEPEST shell in the tree wins. If you run `wsl` from inside PowerShell, you're
/// typing into bash (real case: WindowsTerminal -> powershell.exe -> wsl.exe, title still
/// saying "Windows PowerShell"). Ties at the same depth (two tabs with different shells) are
/// broken by the title, which does reflect the active tab.
///
/// Known limitation: a nested shell living in a BACKGROUND tab wins by depth even though it
/// isn't the active tab — the process tree doesn't expose which tab is in front.
pub fn pick_shell(procs: &[(u32, u32, String)], root_pid: u32, title: &str) -> Option<&'static str> {
    fn kind_of(exe: &str) -> Option<&'static str> {
        match exe {
            "powershell.exe" | "pwsh.exe" => Some("shell-pwsh"),
            "cmd.exe" => Some("shell-cmd"),
            // wsl.exe/ubuntu.exe are Windows processes even though the shell lives in the VM.
            "bash.exe" | "wsl.exe" | "ubuntu.exe" | "debian.exe" | "zsh.exe" | "fish.exe" => {
                Some("shell-bash")
            }
            _ => None,
        }
    }
    // Breadth-first, tracking DEPTH (the window's own process counts — a bare console IS the shell).
    let mut queue: Vec<(u32, u32)> = vec![(root_pid, 0)];
    let mut seen: Vec<u32> = vec![root_pid];
    let mut found: Vec<(&'static str, u32)> = Vec::new(); // (shell, depth)
    let mut i = 0;
    while i < queue.len() && i < 400 {
        let (pid, depth) = queue[i];
        i += 1;
        for (p, parent, exe) in procs {
            if *p == pid {
                if let Some(k) = kind_of(exe) {
                    found.push((k, depth));
                }
            }
            if *parent == pid && !seen.contains(p) {
                seen.push(*p);
                queue.push((*p, depth + 1));
            }
        }
    }
    let maxd = found.iter().map(|(_, d)| *d).max()?;
    let mut deepest: Vec<&'static str> = found
        .iter()
        .filter(|(_, d)| *d == maxd)
        .map(|(k, _)| *k)
        .collect();
    deepest.dedup();
    deepest.sort_unstable();
    deepest.dedup();
    if deepest.len() == 1 {
        return Some(deepest[0]);
    }
    // Tie (e.g. one pwsh tab and one bash tab): the title breaks it, since it does reflect the
    // active tab. "@"/"~" are safe hints here because we're only choosing AMONG shells we already
    // know run in this window (not matched against the whole catalogue).
    let t = title.to_lowercase();
    let hint = |k: &str| -> bool {
        match k {
            "shell-bash" => ["wsl", "ubuntu", "debian", "bash", "mingw", "@", "~"]
                .iter()
                .any(|s| t.contains(s)),
            "shell-pwsh" => t.contains("powershell") || t.contains("pwsh"),
            "shell-cmd" => {
                t.contains("cmd") || t.contains("símbolo del sistema") || t.contains("command prompt")
            }
            _ => false,
        }
    };
    let matches: Vec<&&str> = deepest.iter().filter(|k| hint(k)).collect();
    if matches.len() == 1 { Some(*matches[0]) } else { None }
}

#[cfg(test)]
mod tests {
    use super::pick_shell;

    fn procs(v: &[(u32, u32, &str)]) -> Vec<(u32, u32, String)> {
        v.iter().map(|(a, b, c)| (*a, *b, c.to_string())).collect()
    }

    /// Real case captured on the user's machine: `wsl` running INSIDE the PowerShell tab, with
    /// the title still saying "Windows PowerShell". The deepest shell wins.
    #[test]
    fn wsl_nested_in_powershell_wins_by_depth() {
        let p = procs(&[
            (12656, 1, "windowsterminal.exe"),
            (20816, 12656, "openconsole.exe"),
            (12516, 12656, "powershell.exe"),
            (15792, 12516, "wsl.exe"),
            (14856, 15792, "wsl.exe"),
        ]);
        assert_eq!(pick_shell(&p, 12656, "Windows PowerShell"), Some("shell-bash"));
    }

    /// Two tabs with different shells: same depth -> the title (which does reflect the active
    /// tab) breaks the tie.
    #[test]
    fn two_tabs_break_tie_by_title() {
        let p = procs(&[
            (100, 1, "windowsterminal.exe"),
            (200, 100, "powershell.exe"),
            (300, 100, "wsl.exe"),
        ]);
        assert_eq!(pick_shell(&p, 100, "Windows PowerShell"), Some("shell-pwsh"));
        assert_eq!(pick_shell(&p, 100, "ricardo@DESKTOP: ~"), Some("shell-bash"));
        assert_eq!(pick_shell(&p, 100, "Ubuntu"), Some("shell-bash"));
        assert_eq!(pick_shell(&p, 100, "no hints here"), None); // don't guess
    }

    /// A bare console: the window itself IS the shell.
    #[test]
    fn bare_console() {
        let p = procs(&[(500, 1, "cmd.exe")]);
        assert_eq!(pick_shell(&p, 500, ""), Some("shell-cmd"));
        let p2 = procs(&[(600, 1, "wsl.exe")]);
        assert_eq!(pick_shell(&p2, 600, ""), Some("shell-bash"));
        // A non-terminal window must not invent a shell.
        let p3 = procs(&[(700, 1, "notepad.exe")]);
        assert_eq!(pick_shell(&p3, 700, ""), None);
    }

    /// Manual probe against live processes:
    /// `PID=1234 cargo test probe_shell -- --nocapture --ignored`
    #[test]
    #[ignore]
    fn probe_shell() {
        let pid: u32 = std::env::var("PID").unwrap().parse().unwrap();
        let title = std::env::var("TITLE").unwrap_or_default();
        println!("PID {pid} -> {:?}", super::detect_shell_kind(pid, &title));
    }
}
