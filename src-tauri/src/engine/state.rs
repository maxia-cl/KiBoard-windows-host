//! Live state consulted by `engine::layout`: `integrations::obs` writes to it, `layout_for` reads
//! it on every 500ms poll so the phone's buttons show live state (REC on, mic muted...) without
//! adding new messages to the host<->phone protocol.
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Codex Desktop keeps the selected speed on the active thread, not in the global config. The
/// appends are visible in the thread's JSONL as `thread_settings_applied` events (`priority` is
/// Fast, `default` is Standard). A small cache means the 500 ms layout poll only reads the new tail
/// of the active session instead of repeatedly loading a multi-megabyte conversation.
pub fn codex_fast_mode() -> bool {
    let root = codex_root();
    let Some(root) = root else { return false };

    if let Some(session) = newest_session(&root.join("sessions")) {
        let len = std::fs::metadata(&session).map(|m| m.len()).unwrap_or(0);
        let mut cache = codex_speed_cache().lock().unwrap();
        let changed_session = cache.session.as_ref() != Some(&session) || len < cache.len;
        let tier = if changed_session {
            read_latest_service_tier(&session, None)
        } else if len > cache.len {
            read_latest_service_tier(&session, Some(cache.len.saturating_sub(1)))
        } else {
            None
        };
        if changed_session || tier.is_some() {
            cache.fast = tier;
        }
        cache.session = Some(session);
        cache.len = len;
        if let Some(fast) = cache.fast {
            return fast;
        }
    }

    // Older Codex builds used the top-level config value. Keep it as a fallback for machines
    // without Desktop session files and for a newly created session before its first settings event.
    std::fs::read_to_string(root.join("config.toml"))
        .ok()
        .is_some_and(|text| service_tier_is_fast(&text))
}

/// Mirrors a successful Speed-key press immediately and persists the chosen tier as Codex's global
/// default. Desktop's shortcut only changes the active thread; without updating `config.toml`, a
/// new thread after a PC restart silently returns to Standard. The active thread event remains the
/// live authority while this value supplies the next session's initial state.
pub fn codex_fast_mode_pressed(was_fast: bool) -> std::io::Result<()> {
    let fast = !was_fast;
    codex_speed_cache()
        .lock()
        .unwrap()
        .set_after_press(was_fast);
    let root = codex_root().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "Codex home is unavailable")
    })?;
    persist_service_tier(&root.join("config.toml"), fast)
}

fn codex_root() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
}

/// Rewrites only the top-level setting. A plugin or MCP table may legitimately contain a field
/// with the same name and must remain untouched, along with every comment and unrelated setting.
fn config_with_service_tier(text: &str, fast: bool) -> String {
    let value = if fast { "priority" } else { "default" };
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        let trimmed = content.trim();
        if trimmed.starts_with('[') {
            break;
        }
        if trimmed
            .split_once('=')
            .is_some_and(|(name, _)| name.trim() == "service_tier")
        {
            let indent = &content[..content.len() - content.trim_start().len()];
            let ending = &line[content.len()..];
            let mut updated = String::with_capacity(text.len() + 8);
            updated.push_str(&text[..offset]);
            updated.push_str(indent);
            updated.push_str("service_tier = \"");
            updated.push_str(value);
            updated.push('"');
            updated.push_str(ending);
            updated.push_str(&text[offset + line.len()..]);
            return updated;
        }
        offset += line.len();
    }

    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    format!("service_tier = \"{value}\"{newline}{text}")
}

fn persist_service_tier(path: &Path, fast: bool) -> std::io::Result<()> {
    let original = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let updated = config_with_service_tier(&original, fast);
    if updated == original {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    file.write_all(updated.as_bytes())?;
    file.sync_all()
}

#[derive(Default)]
struct CodexSpeedCache {
    session: Option<PathBuf>,
    len: u64,
    fast: Option<bool>,
}

impl CodexSpeedCache {
    fn set_after_press(&mut self, was_fast: bool) {
        self.fast = Some(!was_fast);
    }
}

static CODEX_SPEED: OnceLock<Mutex<CodexSpeedCache>> = OnceLock::new();

fn codex_speed_cache() -> &'static Mutex<CodexSpeedCache> {
    CODEX_SPEED.get_or_init(|| Mutex::new(CodexSpeedCache::default()))
}

fn newest_session(root: &Path) -> Option<PathBuf> {
    fn visit(dir: &Path, depth: usize, best: &mut Option<(std::time::SystemTime, PathBuf)>) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                visit(&path, depth - 1, best);
            } else if path.extension().is_some_and(|ext| ext == "jsonl") {
                let modified = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                if best.as_ref().is_none_or(|(at, _)| modified > *at) {
                    *best = Some((modified, path));
                }
            }
        }
    }

    let mut best = None;
    visit(root, 5, &mut best);
    best.map(|(_, path)| path)
}

fn read_latest_service_tier(path: &Path, from: Option<u64>) -> Option<bool> {
    let mut file = std::fs::File::open(path).ok()?;
    if let Some(from) = from {
        file.seek(SeekFrom::Start(from)).ok()?;
    }
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    text.lines().rev().find_map(service_tier_from_event)
}

fn service_tier_from_event(line: &str) -> Option<bool> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let payload = value.get("payload")?;
    (payload.get("type")?.as_str()? == "thread_settings_applied").then_some(())?;
    let tier = payload
        .get("thread_settings")?
        .get("service_tier")?
        .as_str()?;
    Some(matches!(tier, "priority" | "fast"))
}

fn service_tier_is_fast(text: &str) -> bool {
    // `service_tier` is a top-level Codex setting. Stop at the first table so a plugin or MCP
    // server with a similarly named field cannot paint the global Fast indicator.
    text.lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with('['))
        .filter_map(|line| line.split_once('='))
        .find(|(name, _)| name.trim() == "service_tier")
        .is_some_and(|(_, value)| {
            matches!(value.trim().trim_matches(['\'', '"']), "fast" | "priority")
        })
}

#[derive(Default, Clone)]
pub struct ObsState {
    pub connected: bool,
    pub recording: bool,
    pub streaming: bool,
    pub replay_active: bool, // replay buffer active
    pub mic_muted: bool,
    pub mic_name: String, // first mic from GetSpecialInputs
    pub scenes: Vec<String>,
    pub current_scene: String,
}

static OBS_STATE: OnceLock<Mutex<ObsState>> = OnceLock::new();
pub fn obs_state() -> &'static Mutex<ObsState> {
    OBS_STATE.get_or_init(|| Mutex::new(ObsState::default()))
}

#[cfg(test)]
mod tests {
    use super::{
        config_with_service_tier, persist_service_tier, service_tier_from_event,
        service_tier_is_fast, CodexSpeedCache,
    };

    #[test]
    fn a_successful_speed_press_is_visible_before_codex_writes_its_event() {
        let mut cache = CodexSpeedCache {
            fast: Some(true),
            ..CodexSpeedCache::default()
        };

        // The event may already have updated the cache before SendInput returns. The transition
        // must still be based on the state captured before the shortcut, not on this new value.
        cache.set_after_press(false);
        assert_eq!(cache.fast, Some(true));

        cache.fast = Some(false);
        cache.set_after_press(true);
        assert_eq!(cache.fast, Some(false));
    }

    #[test]
    fn codex_fast_state_only_reads_the_top_level_service_tier() {
        assert!(service_tier_is_fast(
            "model = \"gpt-5\"\nservice_tier = \"fast\"\n"
        ));
        assert!(service_tier_is_fast("service_tier = \"priority\"\n"));
        assert!(!service_tier_is_fast("service_tier = \"default\"\n"));
        assert!(!service_tier_is_fast("[plugin]\nservice_tier = \"fast\"\n"));
    }

    #[test]
    fn speed_choice_becomes_the_next_codex_sessions_default() {
        let config = "model = \"gpt-5\"\nservice_tier = \"default\"\n\n[plugin]\nservice_tier = \"plugin-owned\"\n";
        assert_eq!(
            config_with_service_tier(config, true),
            "model = \"gpt-5\"\nservice_tier = \"priority\"\n\n[plugin]\nservice_tier = \"plugin-owned\"\n"
        );
        assert_eq!(
            config_with_service_tier("model = \"gpt-5\"\r\n", false),
            "service_tier = \"default\"\r\nmodel = \"gpt-5\"\r\n"
        );
    }

    #[test]
    fn persisted_speed_survives_reopening_the_config_file() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("kiboard-speed-{}-{unique}", std::process::id()));
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "model = \"gpt-5\"\nservice_tier = \"default\"\n").unwrap();

        persist_service_tier(&path, true).unwrap();
        assert!(service_tier_is_fast(
            &std::fs::read_to_string(&path).unwrap()
        ));

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn codex_desktop_speed_comes_from_thread_settings_events() {
        let event = |tier| {
            format!(
                r#"{{"payload":{{"type":"thread_settings_applied","thread_settings":{{"service_tier":"{tier}"}}}}}}"#
            )
        };
        assert_eq!(service_tier_from_event(&event("priority")), Some(true));
        assert_eq!(service_tier_from_event(&event("fast")), Some(true));
        assert_eq!(service_tier_from_event(&event("default")), Some(false));
        assert_eq!(
            service_tier_from_event(r#"{"payload":{"type":"other"}}"#),
            None
        );
    }
}
