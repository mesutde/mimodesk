//! LAN beacon + responder so a 10-digit short ID can resolve without a server.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const LAN_PORT: u16 = 47777;

fn peer_cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let map = load_peers_from_disk();
        Mutex::new(map)
    })
}

fn peers_path() -> std::path::PathBuf {
    let base = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    base.join("MimoDesk").join("peers.json")
}

fn load_peers_from_disk() -> HashMap<String, String> {
    std::fs::read_to_string(peers_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_peers_to_disk(map: &HashMap<String, String>) {
    let path = peers_path();
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    if let Ok(json) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(path, json);
    }
}

pub fn remember_peer(short_id: &str, endpoint_id: &str) {
    if short_id.is_empty() || endpoint_id.is_empty() || short_id.len() != 10 {
        return;
    }
    if let Ok(mut map) = peer_cache().lock() {
        if map.get(short_id) != Some(&endpoint_id.to_string()) {
            map.insert(short_id.to_string(), endpoint_id.to_string());
            save_peers_to_disk(&map);
        }
    }
}

pub fn cached_peer(short_id: &str) -> Option<String> {
    peer_cache().lock().ok()?.get(short_id).cloned()
}

/// Host advertises and answers lookups on a fixed LAN port.
pub fn spawn_lan(short_id: String, endpoint_id: String) -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();
    std::thread::spawn(move || {
        let Ok(socket) = bind_host() else {
            tracing::warn!("lan bind failed on {LAN_PORT} — kısa ID LAN keşfi kapalı");
            return;
        };
        let _ = socket.set_read_timeout(Some(Duration::from_millis(200)));
        let hello = format!("PDHELLO|{short_id}|{endpoint_id}");
        let dest = SocketAddr::from((Ipv4Addr::BROADCAST, LAN_PORT));
        let mut last_hello = Instant::now() - Duration::from_secs(10);
        let mut buf = [0u8; 512];

        remember_peer(&short_id, &endpoint_id);

        while flag.load(Ordering::SeqCst) {
            if last_hello.elapsed() >= Duration::from_millis(600) {
                let _ = socket.send_to(hello.as_bytes(), dest);
                let _ = socket.send_to(
                    hello.as_bytes(),
                    SocketAddr::from((Ipv4Addr::LOCALHOST, LAN_PORT)),
                );
                last_hello = Instant::now();
            }
            if let Ok((n, from)) = socket.recv_from(&mut buf) {
                let text = String::from_utf8_lossy(&buf[..n]).trim().to_string();
                if let Some(rest) = text.strip_prefix("PDLOOKUP|") {
                    if rest == short_id {
                        let reply = format!("PDFOUND|{short_id}|{endpoint_id}");
                        let _ = socket.send_to(reply.as_bytes(), from);
                    }
                } else if let Some(rest) = text.strip_prefix("PDHELLO|") {
                    let mut parts = rest.split('|');
                    let sid = parts.next().unwrap_or("");
                    let ep = parts.next().unwrap_or("");
                    if !sid.is_empty() && !ep.is_empty() {
                        remember_peer(sid, ep);
                    }
                }
            }
        }
    });
    running
}

/// Resolve a 10-digit short ID. Uses cache, then LAN query from an ephemeral port.
pub fn lookup_short_id(short_id: &str, timeout: Duration) -> Option<String> {
    if let Some(ep) = cached_peer(short_id) {
        return Some(ep);
    }

    // Client must NOT bind the host port — ephemeral socket avoids clash with local host.
    let socket = match UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("lookup socket failed: {e}");
            return cached_peer(short_id);
        }
    };
    let _ = socket.set_broadcast(true);
    let _ = socket.set_read_timeout(Some(Duration::from_millis(100)));

    let query = format!("PDLOOKUP|{short_id}");
    let bcast = SocketAddr::from((Ipv4Addr::BROADCAST, LAN_PORT));
    let _ = socket.send_to(query.as_bytes(), bcast);
    // Common private ranges (Windows broadcast can be picky).
    for third in [0u8, 1, 2] {
        let subnet = SocketAddr::from((Ipv4Addr::new(192, 168, third, 255), LAN_PORT));
        let _ = socket.send_to(query.as_bytes(), subnet);
    }
    let _ = socket.send_to(
        query.as_bytes(),
        SocketAddr::from((Ipv4Addr::LOCALHOST, LAN_PORT)),
    );

    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 512];
    while Instant::now() < deadline {
        if let Ok((n, _)) = socket.recv_from(&mut buf) {
            let text = String::from_utf8_lossy(&buf[..n]).trim().to_string();
            for prefix in ["PDFOUND|", "PDHELLO|"] {
                if let Some(rest) = text.strip_prefix(prefix) {
                    let mut parts = rest.split('|');
                    let sid = parts.next().unwrap_or("");
                    let ep = parts.next().unwrap_or("");
                    if !ep.is_empty() {
                        remember_peer(sid, ep);
                        if sid == short_id {
                            return Some(ep.to_string());
                        }
                    }
                }
            }
        }
        if let Some(ep) = cached_peer(short_id) {
            return Some(ep);
        }
    }
    cached_peer(short_id)
}

fn bind_host() -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, LAN_PORT)))?;
    socket.set_broadcast(true)?;
    Ok(socket)
}

/// Normalize user input: digits only, expect 10.
pub fn normalize_short_id(input: &str) -> Option<String> {
    let digits: String = input.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 10 {
        Some(digits)
    } else {
        None
    }
}

/// Display as 61-79-62-45-67
pub fn format_short_id(id: &str) -> String {
    id.as_bytes()
        .chunks(2)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect::<Vec<_>>()
        .join("-")
}

/// 10-digit AnyDesk-style ID from endpoint id.
pub fn short_id_from_endpoint(endpoint_hex: &str) -> String {
    let clean: String = endpoint_hex
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let mut n: u64 = 0;
    for (i, b) in clean.bytes().take(12).enumerate() {
        n = n.wrapping_mul(31).wrapping_add(b as u64 + i as u64);
    }
    format!("{:010}", n % 10_000_000_000u64)
}
