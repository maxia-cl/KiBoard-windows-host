//! Live state consulted by `engine::layout`: `integrations::obs` writes to it, `layout_for` reads
//! it on every 500ms poll so the phone's buttons show live state (REC on, mic muted...) without
//! adding new messages to the host<->phone protocol.
use std::sync::{Mutex, OnceLock};

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
