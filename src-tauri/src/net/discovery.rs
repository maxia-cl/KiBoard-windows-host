//! mDNS advertisement (`_kiboard._tcp.local.`, protocol/README.md §1). The approach (the
//! `mdns-sd` crate, `enable_addr_auto()` so the advertisement follows DHCP address changes on its
//! own) is harvested from the `nervous-swirles-1ed28b` worktree, already validated against real
//! phone-side discovery there — adapted here to the v2 TXT record shape (presence only, no
//! secret ever travels over multicast).
use std::sync::{Mutex, OnceLock};

use crate::config::config;
use crate::net::ws::WS_PORT;

static DAEMON: OnceLock<Option<mdns_sd::ServiceDaemon>> = OnceLock::new();
static REGISTERED: Mutex<Option<String>> = Mutex::new(None);

fn daemon() -> Option<&'static mdns_sd::ServiceDaemon> {
    DAEMON
        .get_or_init(|| mdns_sd::ServiceDaemon::new().ok())
        .as_ref()
}

/// (Re)advertises the host. Call at startup and whenever a TXT field changes (pairing
/// opened/closed, mode switched). Safe to call repeatedly: it retires the previous registration
/// first, so there's never more than one instance advertised at a time.
pub(crate) fn advertise(mode: &str) {
    let Some(daemon) = daemon() else {
        eprintln!("KiBoard: mDNS unavailable, no automatic discovery");
        return;
    };
    let (host_id, pairing_open) = {
        let cfg = config().lock().unwrap();
        (cfg.host_id.clone(), cfg.pairing_open)
    };

    let instance = format!("KiBoard-{host_id}");
    let hostname = format!("kiboard-{host_id}.local.");
    let props = [
        ("v", "2"),
        ("name", crate::HOST_NAME),
        ("id", host_id.as_str()),
        ("os", "win"),
        ("mode", mode),
        ("pair", if pairing_open { "open" } else { "closed" }),
        // §1: which scheme to use. A client that had to guess would find a failed TLS handshake
        // against a plaintext port indistinguishable from a host that is simply not there.
        ("tls", "1"),
    ];

    if let Some(prev) = REGISTERED.lock().unwrap().take() {
        let _ = daemon.unregister(&prev);
    }
    match mdns_sd::ServiceInfo::new(
        "_kiboard._tcp.local.",
        &instance,
        &hostname,
        "",
        WS_PORT,
        &props[..],
    )
    .map(|i| i.enable_addr_auto())
    {
        Ok(info) => {
            let fullname = info.get_fullname().to_string();
            match daemon.register(info) {
                Ok(()) => {
                    *REGISTERED.lock().unwrap() = Some(fullname);
                    eprintln!("KiBoard: advertised over mDNS as {instance}");
                }
                Err(e) => eprintln!("KiBoard: mDNS advertise failed: {e}"),
            }
        }
        Err(e) => eprintln!("KiBoard: mDNS advertise failed: {e}"),
    }
}
