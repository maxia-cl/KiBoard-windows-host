//! obs-websocket v5 client (built into OBS >= 28, port 4455). Permanent connection with
//! reconnection; the live state (recording/streaming/mic/scenes) lives in
//! `engine::state::obs_state()` — `layout_for` reads it on every poll (500ms), so the phone's
//! buttons show live state without adding new messages to the host<->phone protocol.
//! ponytail: fixed port 4455 (OBS' default); make it configurable if a user ever asks.
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

use crate::engine::state::obs_state;

static OBS_TX: OnceLock<tokio::sync::mpsc::UnboundedSender<String>> = OnceLock::new();
static OBS_REQ_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn set_sender(tx: tokio::sync::mpsc::UnboundedSender<String>) {
    let _ = OBS_TX.set(tx);
}

/// Maps an "obs:<cmd>" action to the obs-websocket request. Pure, so it's testable.
pub fn obs_request_for(
    cmd: &str,
    mic_name: &str,
) -> Result<(&'static str, serde_json::Value), &'static str> {
    match cmd {
        "record" => Ok(("ToggleRecord", json!({}))),
        "stream" => Ok(("ToggleStream", json!({}))),
        "replay" => Ok(("SaveReplayBuffer", json!({}))),
        "replaybuffer" => Ok(("ToggleReplayBuffer", json!({}))),
        "mic" => {
            if mic_name.is_empty() {
                return Err("obs_sin_mic");
            }
            Ok(("ToggleInputMute", json!({ "inputName": mic_name })))
        }
        s => match s.strip_prefix("scene:") {
            Some(name) if !name.is_empty() => {
                Ok(("SetCurrentProgramScene", json!({ "sceneName": name })))
            }
            _ => Err("obs_accion_desconocida"),
        },
    }
}

/// Queues a request to OBS (fire and forget: the result comes back as a state event).
fn obs_send(request_type: &str, data: serde_json::Value) -> Result<(), &'static str> {
    if !obs_state().lock().unwrap().connected {
        return Err("obs_desconectado");
    }
    let id = OBS_REQ_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let msg = json!({
        "op": 6,
        "d": { "requestType": request_type, "requestId": id.to_string(), "requestData": data }
    })
    .to_string();
    OBS_TX
        .get()
        .ok_or("obs_desconectado")?
        .send(msg)
        .map_err(|_| "obs_desconectado")
}

pub fn obs_action(cmd: &str) -> Result<(), &'static str> {
    let mic = obs_state().lock().unwrap().mic_name.clone();
    let (rtype, data) = obs_request_for(cmd, &mic)?;
    obs_send(rtype, data)
}

/// v5 auth: b64(sha256(b64(sha256(password+salt)) + challenge)).
fn obs_auth(password: &str, salt: &str, challenge: &str) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let b64 = base64::engine::general_purpose::STANDARD;
    let secret = b64.encode(Sha256::digest(format!("{password}{salt}")));
    b64.encode(Sha256::digest(format!("{secret}{challenge}")))
}

/// Permanent loop: connects, serves, and on disconnect clears the state and retries after 5s.
pub async fn obs_client_loop(mut rx: tokio::sync::mpsc::UnboundedReceiver<String>) {
    loop {
        // Discard commands queued while we were down (avoids ghost toggles on reconnect).
        while rx.try_recv().is_ok() {}
        let _ = obs_serve(&mut rx).await;
        *obs_state().lock().unwrap() = crate::engine::state::ObsState::default();
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn obs_serve(rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>) -> Result<(), ()> {
    use futures_util::{SinkExt, StreamExt};
    let (ws, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:4455")
        .await
        .map_err(|_| ())?;
    let (mut write, mut read) = ws.split();

    // Hello (op 0) -> Identify (op 1) with auth if the server requires it.
    let hello = obs_next_json(&mut read).await.ok_or(())?;
    let mut identify = json!({
        "rpcVersion": 1,
        // Scenes(4) | Inputs(8) | Outputs(64): scenes, mic mute, recording/streaming status.
        "eventSubscriptions": 76
    });
    if let Some(auth) = hello["d"].get("authentication") {
        let password = crate::config::config().lock().unwrap().obs_password.clone();
        let salt = auth["salt"].as_str().unwrap_or("");
        let challenge = auth["challenge"].as_str().unwrap_or("");
        identify["authentication"] = json!(obs_auth(&password, salt, challenge));
    }
    write
        .send(Message::text(json!({"op": 1, "d": identify}).to_string()))
        .await
        .map_err(|_| ())?;
    // Identified (op 2); with a wrong password OBS closes the socket and we fall to the retry.
    let identified = obs_next_json(&mut read).await.ok_or(())?;
    if identified["op"].as_i64() != Some(2) {
        return Err(());
    }
    obs_state().lock().unwrap().connected = true;

    // Initial state (the op 7 responses fill it in).
    for req in [
        "GetSceneList",
        "GetSpecialInputs",
        "GetRecordStatus",
        "GetStreamStatus",
        "GetReplayBufferStatus",
    ] {
        let _ = obs_send(req, json!({}));
    }

    loop {
        tokio::select! {
            cmd = rx.recv() => {
                let Some(cmd) = cmd else { return Err(()) };
                write.send(Message::text(cmd)).await.map_err(|_| ())?;
            }
            msg = read.next() => {
                let Some(Ok(msg)) = msg else { return Err(()) };
                if msg.is_close() { return Err(()); }
                if !msg.is_text() { continue; }
                let txt = msg.into_text().map(|t| t.to_string()).unwrap_or_default();
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                    obs_apply(&v);
                }
            }
        }
    }
}

async fn obs_next_json<S>(read: &mut S) -> Option<serde_json::Value>
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    use futures_util::StreamExt;
    loop {
        let msg = read.next().await?.ok()?;
        if msg.is_close() {
            return None;
        }
        if !msg.is_text() {
            continue;
        }
        let txt = msg.into_text().ok()?.to_string();
        if let Ok(v) = serde_json::from_str(&txt) {
            return Some(v);
        }
    }
}

/// Applies an event (op 5) or response (op 7) from OBS to the shared state.
fn obs_apply(v: &serde_json::Value) {
    // Follow-up request to send AFTER releasing the lock (obs_send also takes it).
    let mut followup: Option<(&'static str, serde_json::Value)> = None;
    {
        let mut st = obs_state().lock().unwrap();
        match v["op"].as_i64() {
            Some(5) => {
                let d = &v["d"]["eventData"];
                match v["d"]["eventType"].as_str().unwrap_or("") {
                    "RecordStateChanged" => {
                        st.recording = d["outputActive"].as_bool().unwrap_or(st.recording)
                    }
                    "StreamStateChanged" => {
                        st.streaming = d["outputActive"].as_bool().unwrap_or(st.streaming)
                    }
                    "ReplayBufferStateChanged" => {
                        st.replay_active = d["outputActive"].as_bool().unwrap_or(st.replay_active)
                    }
                    "InputMuteStateChanged" => {
                        if d["inputName"].as_str() == Some(st.mic_name.as_str()) {
                            st.mic_muted = d["inputMuted"].as_bool().unwrap_or(st.mic_muted);
                        }
                    }
                    "CurrentProgramSceneChanged" => {
                        st.current_scene = d["sceneName"].as_str().unwrap_or("").to_string();
                    }
                    "SceneListChanged" => st.scenes = obs_scene_names(&d["scenes"]),
                    _ => {}
                }
            }
            Some(7) => {
                let d = &v["d"]["responseData"];
                match v["d"]["requestType"].as_str().unwrap_or("") {
                    "GetSceneList" => {
                        st.scenes = obs_scene_names(&d["scenes"]);
                        st.current_scene = d["currentProgramSceneName"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                    }
                    "GetSpecialInputs" => {
                        st.mic_name = ["mic1", "mic2", "mic3", "mic4"]
                            .iter()
                            .find_map(|k| d[k].as_str().filter(|s| !s.is_empty()))
                            .unwrap_or("")
                            .to_string();
                        if st.mic_name.is_empty() {
                            // No global mic: look for a microphone input among the scenes.
                            followup = Some(("GetInputList", json!({})));
                        } else {
                            followup = Some(("GetInputMute", json!({ "inputName": st.mic_name })));
                        }
                    }
                    "GetInputList" => {
                        // Fallback: first microphone capture input (wasapi_input_capture).
                        st.mic_name = d["inputs"]
                            .as_array()
                            .and_then(|a| {
                                a.iter().find(|i| {
                                    i["inputKind"]
                                        .as_str()
                                        .unwrap_or("")
                                        .contains("input_capture")
                                })
                            })
                            .and_then(|i| i["inputName"].as_str())
                            .unwrap_or("")
                            .to_string();
                        if !st.mic_name.is_empty() {
                            followup = Some(("GetInputMute", json!({ "inputName": st.mic_name })));
                        }
                    }
                    "GetRecordStatus" => {
                        st.recording = d["outputActive"].as_bool().unwrap_or(false)
                    }
                    "GetStreamStatus" => {
                        st.streaming = d["outputActive"].as_bool().unwrap_or(false)
                    }
                    "GetReplayBufferStatus" => {
                        st.replay_active = d["outputActive"].as_bool().unwrap_or(false)
                    }
                    "GetInputMute" => st.mic_muted = d["inputMuted"].as_bool().unwrap_or(false),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if let Some((rtype, data)) = followup {
        let _ = obs_send(rtype, data);
    }
}

/// GetSceneList returns scenes bottom-to-top -> reverse it to match OBS' own UI order.
fn obs_scene_names(scenes: &serde_json::Value) -> Vec<String> {
    scenes
        .as_array()
        .map(|a| {
            a.iter()
                .rev()
                .filter_map(|s| s["sceneName"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::obs_request_for;

    #[test]
    fn maps_obs_actions() {
        assert_eq!(obs_request_for("record", "").unwrap().0, "ToggleRecord");
        assert_eq!(obs_request_for("stream", "").unwrap().0, "ToggleStream");
        assert_eq!(obs_request_for("replay", "").unwrap().0, "SaveReplayBuffer");
        let (t, d) = obs_request_for("scene:Gameplay", "").unwrap();
        assert_eq!(t, "SetCurrentProgramScene");
        assert_eq!(d["sceneName"], "Gameplay");
        let (t, d) = obs_request_for("mic", "Mic/Aux").unwrap();
        assert_eq!(t, "ToggleInputMute");
        assert_eq!(d["inputName"], "Mic/Aux");
        assert!(obs_request_for("mic", "").is_err()); // no mic detected
        assert!(obs_request_for("scene:", "").is_err());
        assert!(obs_request_for("nope", "").is_err());
    }
}
