//! Access control: password, one-time approval, view-only sessions.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessSettings {
    pub password: Option<String>,
    pub require_approval: bool,
    pub always_allow: Vec<String>,
}

impl Default for AccessSettings {
    fn default() -> Self {
        // Require an explicit Allow by default (RustDesk-style).
        Self {
            password: None,
            require_approval: true,
            always_allow: Vec::new(),
        }
    }
}

fn settings_path() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("MimoDesk").join("access.json")
}

fn store() -> &'static Mutex<AccessSettings> {
    static S: OnceLock<Mutex<AccessSettings>> = OnceLock::new();
    S.get_or_init(|| {
        let mut s: AccessSettings = std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        // Older builds defaulted require_approval=false; opt those installs in.
        let raw: serde_json::Value = std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let has_version = raw.get("schema").and_then(|v| v.as_u64()).unwrap_or(0) >= 2;
        if !has_version {
            s.require_approval = true;
            let _ = save_to_disk(&s);
        }
        Mutex::new(s)
    })
}

fn save_to_disk(s: &AccessSettings) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let mut v = serde_json::to_value(s).unwrap_or_default();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("schema".into(), serde_json::json!(2));
    }
    std::fs::write(path, serde_json::to_string_pretty(&v)?)
}

pub fn load() -> AccessSettings {
    store().lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn save(s: AccessSettings) {
    if let Ok(mut g) = store().lock() {
        *g = s.clone();
    }
    let _ = save_to_disk(&s);
}

pub fn set_password(pw: Option<String>) {
    let mut s = load();
    s.password = pw.filter(|p| !p.trim().is_empty()).map(|p| p.trim().to_string());
    save(s);
}

pub fn set_require_approval(v: bool) {
    let mut s = load();
    s.require_approval = v;
    save(s);
}

pub fn check_password(client_pw: Option<&str>) -> Result<(), String> {
    let s = load();
    match &s.password {
        None => Ok(()),
        Some(expected) => {
            let got = client_pw.unwrap_or("");
            if got == expected {
                Ok(())
            } else {
                Err("WRONG_PASSWORD".into())
            }
        }
    }
}

pub fn is_always_allowed(peer_hex: &str) -> bool {
    load().always_allow.iter().any(|p| p == peer_hex)
}

pub fn remember_always_allow(peer_hex: &str) {
    let mut s = load();
    if !s.always_allow.iter().any(|p| p == peer_hex) {
        s.always_allow.push(peer_hex.to_string());
        save(s);
    }
}

/// Pending approval waiters: peer_id → oneshot sender.
type Pending = Mutex<std::collections::HashMap<String, oneshot::Sender<bool>>>;

fn pending() -> &'static Pending {
    static P: OnceLock<Pending> = OnceLock::new();
    P.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// Host UI answers via `answer_connection`.
pub async fn wait_approval(peer_hex: &str, timeout: Duration) -> bool {
    let s = load();
    if !s.require_approval || is_always_allowed(peer_hex) {
        return true;
    }
    let (tx, rx) = oneshot::channel();
    if let Ok(mut map) = pending().lock() {
        map.insert(peer_hex.to_string(), tx);
    }
    matches!(tokio::time::timeout(timeout, rx).await, Ok(Ok(true)))
}

pub fn answer_connection(peer_hex: &str, allow: bool, always: bool) {
    if always && allow {
        remember_always_allow(peer_hex);
    }
    if let Ok(mut map) = pending().lock() {
        if let Some(tx) = map.remove(peer_hex) {
            let _ = tx.send(allow);
        }
    }
}
