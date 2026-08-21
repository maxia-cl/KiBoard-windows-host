//! v2 pairing (protocol/README.md §2): a six-digit code shown on the PC, one token per device,
//! individually revocable. Replaces v1's single shared token handed out by QR.
use std::sync::{Mutex, OnceLock};

use crate::config::{config, new_token, Device};

const CODE_TTL_SECS: u64 = 120;
const MAX_FAILED_CODES: u32 = 3;
const LOCKOUT_SECS: u64 = 5 * 60;

/// One pairing attempt in flight: the code shown on the PC, waiting for `pair_confirm`.
struct PendingPairing {
    code: String,
    expires_at: u64,
    device_name: String,
    platform: String,
    failed_attempts: u32,
    locked_until: Option<u64>,
}

static PENDING: OnceLock<Mutex<Option<PendingPairing>>> = OnceLock::new();
fn pending() -> &'static Mutex<Option<PendingPairing>> {
    PENDING.get_or_init(|| Mutex::new(None))
}

fn new_code() -> String {
    use rand::Rng;
    format!("{:06}", rand::thread_rng().gen_range(0..1_000_000))
}

/// A phone opened a WS connection and asked to pair. Rejects if the host has pairing closed, or
/// if a previous attempt on this same connection is locked out. Otherwise generates a fresh code
/// and remembers it for the PC's UI to display.
pub(crate) fn start(device_name: &str, platform: &str) -> Result<(String, u64), &'static str> {
    if !config().lock().unwrap().pairing_open {
        return Err("pairing_closed");
    }
    let now = crate::now_ts();
    let mut slot = pending().lock().unwrap();
    if let Some(p) = slot.as_ref() {
        if let Some(until) = p.locked_until {
            if now < until {
                return Err("rate_limited");
            }
        }
    }
    let code = new_code();
    // Also logged (not just shown in the host UI's pairing panel): useful when running headless
    // during development, and a normal thing to have in the console for a security-relevant event.
    eprintln!("KiBoard: \"{device_name}\" wants to connect — code {code}");
    *slot = Some(PendingPairing {
        code,
        expires_at: now + CODE_TTL_SECS,
        device_name: device_name.to_string(),
        platform: platform.to_string(),
        failed_attempts: 0,
        locked_until: None,
    });
    Ok((slot.as_ref().unwrap().code.clone(), CODE_TTL_SECS))
}

/// The phone typed (or scanned) a code back. On success, issues and stores a fresh per-device
/// token and clears the pending attempt. Single-use: a spent or expired code never validates
/// again, and three wrong guesses locks new attempts out for 5 minutes.
pub(crate) fn confirm(code: &str) -> Result<Device, &'static str> {
    let now = crate::now_ts();
    let mut slot = pending().lock().unwrap();
    let Some(p) = slot.as_mut() else {
        return Err("bad_code");
    };
    if let Some(until) = p.locked_until {
        if now < until {
            return Err("rate_limited");
        }
    }
    if now > p.expires_at {
        *slot = None;
        return Err("bad_code");
    }
    if p.code != code {
        p.failed_attempts += 1;
        if p.failed_attempts >= MAX_FAILED_CODES {
            p.locked_until = Some(now + LOCKOUT_SECS);
        }
        return Err("bad_code");
    }
    let device = Device {
        device_id: new_token()[..16].to_string(),
        name: p.device_name.clone(),
        platform: p.platform.clone(),
        token: new_token(),
        last_seen: now,
    };
    *slot = None;
    let mut cfg = config().lock().unwrap();
    cfg.devices
        .retain(|d| d.name != device.name || d.platform != device.platform);
    cfg.devices.push(device.clone());
    cfg.save();
    Ok(device)
}

/// Current pending code for the host UI to display (e.g. "«Pixel 8» wants to connect: 418203").
/// None once it's confirmed, expired, or was never requested.
pub(crate) fn pending_status() -> Option<(String, String, u64)> {
    let now = crate::now_ts();
    let mut slot = pending().lock().unwrap();
    match slot.as_ref() {
        Some(p) if now <= p.expires_at => {
            Some((p.device_name.clone(), p.code.clone(), p.expires_at - now))
        }
        _ => {
            *slot = None;
            None
        }
    }
}

/// Reconnecting device: checks its stored per-device token and bumps `last_seen`.
pub(crate) fn authenticate(device_id: &str, token: &str) -> Result<(), &'static str> {
    let mut cfg = config().lock().unwrap();
    let Some(d) = cfg.devices.iter_mut().find(|d| d.device_id == device_id) else {
        return Err("revoked");
    };
    if d.token != token {
        return Err("invalid_token");
    }
    d.last_seen = crate::now_ts();
    cfg.save();
    Ok(())
}

/// Picks the best LAN IP so the phone can reach the PC.
/// Avoids loopback/APIPA and prioritizes home networks (192.168 > 10 > 172) over virtual adapters.
pub(crate) fn best_lan_ip() -> String {
    use std::net::IpAddr;
    // The right IP is the one on the interface the PC uses to REACH the network (the one the
    // router sees), not the first 192.168.x in the list: virtual adapters (WSL/Hyper-V/hotspot)
    // also use 192.168.x and used to poison the QR (2026-07-14 incident: QR with a virtual
    // 192.168.224.1). A UDP connect sends no packet at all; it just asks the OS which interface
    // it would use.
    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    // Fallback with no network/internet route: the usual prefix heuristic.
    let mut v4s: Vec<std::net::Ipv4Addr> = local_ip_address::list_afinet_netifas()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, ip)| match ip {
            IpAddr::V4(v4) => Some(v4),
            _ => None,
        })
        .filter(|v4| !v4.is_loopback() && !v4.is_link_local())
        .collect();
    v4s.sort_by_key(|v4| {
        let o = v4.octets();
        if o[0] == 192 && o[1] == 168 {
            0
        } else if o[0] == 10 {
            1
        } else if o[0] == 172 {
            2
        } else {
            3
        }
    });
    v4s.first()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "127.0.0.1".into())
}

pub(crate) fn qr_svg(data: &str) -> String {
    use qrcode::{render::svg, QrCode};
    match QrCode::new(data.as_bytes()) {
        Ok(code) => code
            .render::<svg::Color>()
            .min_dimensions(220, 220)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::new_code;

    #[test]
    fn codes_are_six_digits() {
        for _ in 0..50 {
            let c = new_code();
            assert_eq!(c.len(), 6, "code {c:?} isn't 6 chars");
            assert!(
                c.chars().all(|ch| ch.is_ascii_digit()),
                "code {c:?} has a non-digit"
            );
        }
    }
}
