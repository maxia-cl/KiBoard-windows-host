//! v1 pairing: one shared token, handed out by QR. Per-device tokens and the six-digit code flow
//! are F1 work (docs/implementation-plan.md) — this module keeps today's behavior only.
use crate::config::config;

/// Checks the phone's token against the host's, and records the device name on first sight.
/// Returns whether the session is now authenticated.
pub fn authenticate(token: &str, device: &str) -> bool {
    let mut cfg = config().lock().unwrap();
    if !token.is_empty() && token == cfg.token {
        if !cfg.paired.contains(&device.to_string()) {
            cfg.paired.push(device.to_string());
            cfg.save();
        }
        true
    } else {
        false
    }
}

/// Picks the best LAN IP so the phone can reach the PC.
/// Avoids loopback/APIPA and prioritizes home networks (192.168 > 10 > 172) over virtual adapters.
pub fn best_lan_ip() -> String {
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
    v4s.first().map(|v| v.to_string()).unwrap_or_else(|| "127.0.0.1".into())
}

pub fn qr_svg(data: &str) -> String {
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
