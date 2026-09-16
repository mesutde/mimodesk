mod access;
mod capture;
mod clipboard;
mod cursor_kind;
mod input_inject;
mod lan;
mod p2p;
mod session;

use capture::QualityLevel;
use input_inject::InputEvent;
use p2p::{DirEntry, LinkInfo, PeerState, SessionStatus};
use serde::Serialize;
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::RwLock;

pub struct AppState {
    pub peer: Arc<RwLock<PeerState>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeskInfo {
    address: String,
    raw_address: String,
    alias: String,
    status: SessionStatus,
    relay: String,
    listening: bool,
    last_error: Option<String>,
    quality: String,
    link: Option<LinkInfo>,
    session: u64,
    remote_version: Option<String>,
}

fn quality_label(q: QualityLevel) -> String {
    q.label().to_string()
}

fn snapshot(peer: &PeerState) -> DeskInfo {
    DeskInfo {
        address: peer.address_display(),
        raw_address: peer.address_raw(),
        alias: peer.alias.clone(),
        status: peer.status.clone(),
        relay: peer.relay_label(),
        listening: peer.listening,
        last_error: peer.last_error.clone(),
        quality: quality_label(peer.quality),
        link: peer.link.clone(),
        session: peer.session_gen,
        remote_version: peer.remote_version.clone(),
    }
}

#[tauri::command]
async fn get_desk_info(state: State<'_, AppState>) -> Result<DeskInfo, String> {
    let peer = state.peer.read().await;
    Ok(snapshot(&peer))
}

#[tauri::command]
async fn start_host(state: State<'_, AppState>) -> Result<DeskInfo, String> {
    let mut peer = state.peer.write().await;
    peer.start_host().await.map_err(|e| e.to_string())?;
    Ok(snapshot(&peer))
}

#[tauri::command]
async fn connect_remote(
    address: String,
    password: Option<String>,
    view_only: Option<bool>,
    state: State<'_, AppState>,
) -> Result<DeskInfo, String> {
    let mut peer = state.peer.write().await;
    peer.connect_remote(address, password, view_only.unwrap_or(false))
        .await
        .map_err(|e| e.to_string())?;
    Ok(snapshot(&peer))
}

#[tauri::command]
async fn disconnect(state: State<'_, AppState>) -> Result<(), String> {
    let mut peer = state.peer.write().await;
    peer.disconnect();
    Ok(())
}

#[tauri::command]
async fn set_alias(alias: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut peer = state.peer.write().await;
    peer.alias = alias;
    Ok(())
}

#[tauri::command]
async fn send_input(payload: InputEvent, state: State<'_, AppState>) -> Result<(), String> {
    let mut peer = state.peer.write().await;
    peer.send_input(payload).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_quality(level: String, state: State<'_, AppState>) -> Result<(), String> {
    let q = match level.as_str() {
        "low" => QualityLevel::Low,
        "medium" => QualityLevel::Medium,
        "high" => QualityLevel::High,
        other => return Err(format!("bilinmeyen kalite: {other}")),
    };
    let mut peer = state.peer.write().await;
    peer.set_quality(q);
    Ok(())
}

#[tauri::command]
async fn get_remote_monitors(
    state: State<'_, AppState>,
) -> Result<Vec<capture::MonitorInfo>, String> {
    let mut peer = state.peer.write().await;
    peer.list_remote_monitors().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_monitor(index: u8, state: State<'_, AppState>) -> Result<(), String> {
    let mut peer = state.peer.write().await;
    peer.set_monitor(index).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_file(
    path: String,
    dest_dir: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut peer = state.peer.write().await;
    peer.send_file(path, dest_dir.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_local_dir(path: String) -> Result<serde_json::Value, String> {
    let p = if path.trim().is_empty() {
        std::env::var("USERPROFILE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
    } else {
        std::path::PathBuf::from(path.trim())
    };
    let mut entries = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&p) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let meta = e.metadata().ok();
            entries.push(serde_json::json!({
                "name": name,
                "isDir": meta.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                "size": meta.map(|m| m.len()).unwrap_or(0),
            }));
        }
    }
    entries.sort_by(|a, b| {
        let ad = a["isDir"].as_bool().unwrap_or(false);
        let bd = b["isDir"].as_bool().unwrap_or(false);
        bd.cmp(&ad).then_with(|| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .cmp(&b["name"].as_str().unwrap_or("").to_lowercase())
        })
    });
    Ok(serde_json::json!({ "path": p.display().to_string(), "entries": entries }))
}

#[tauri::command]
async fn list_remote_dir(path: String, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let mut peer = state.peer.write().await;
    let (dir, entries) = peer
        .list_remote_dir(path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "path": dir,
        "entries": entries.iter().map(|e| serde_json::json!({
            "name": e.name,
            "isDir": e.is_dir,
            "size": e.size,
        })).collect::<Vec<_>>(),
    }))
}

#[tauri::command]
async fn pull_file(
    remote_path: String,
    local_dest: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut peer = state.peer.write().await;
    peer.pull_file(remote_path, local_dest)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn regenerate_id(state: State<'_, AppState>) -> Result<DeskInfo, String> {
    let mut peer = state.peer.write().await;
    peer.regenerate_id().await.map_err(|e| e.to_string())?;
    Ok(snapshot(&peer))
}

#[tauri::command]
async fn answer_connection(
    peer: String,
    allow: bool,
    always: bool,
) -> Result<(), String> {
    access::answer_connection(&peer, allow, always);
    Ok(())
}

#[tauri::command]
async fn set_access_password(password: Option<String>) -> Result<(), String> {
    access::set_password(password);
    Ok(())
}

#[tauri::command]
async fn set_require_approval(require: bool) -> Result<(), String> {
    access::set_require_approval(require);
    Ok(())
}

#[tauri::command]
async fn get_access_settings() -> Result<access::AccessSettings, String> {
    Ok(access::load())
}

/// Exit early when another MimoDesk instance is already running. Two
/// processes share one endpoint identity, so incoming sessions would land on
/// a random instance (flaky video/input). The OS drops the file handle on
/// crash, so a stale lock can never block a restart.
#[cfg(target_os = "windows")]
fn ensure_single_instance() {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::GetLastError;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_MODE, OPEN_ALWAYS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONWARNING, MB_OK, MESSAGEBOX_STYLE,
    };
    use windows::core::w;

    const ERROR_SHARING_VIOLATION: u32 = 32;
    // GENERIC_READ | GENERIC_WRITE (CreateFileW takes raw u32 here).
    const RW_ACCESS: u32 = 0x80000000 | 0x40000000;

    let dir = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("MimoDesk");
    let _ = std::fs::create_dir_all(&dir);
    let path: Vec<u16> = dir
        .join("instance.lock")
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let access = RW_ACCESS;
    let handle = unsafe {
        CreateFileW(
            windows::core::PCWSTR(path.as_ptr()),
            access,
            FILE_SHARE_MODE(0), // no sharing: second open fails while held
            None,
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };
    match handle {
        Ok(h) => {
            // Kept for the process lifetime; the OS releases it on crash.
            Box::leak(Box::new(h));
        }
        Err(_) => {
            if unsafe { GetLastError().0 } == ERROR_SHARING_VIOLATION {
                unsafe {
                    MessageBoxW(
                        None,
                        w!("MimoDesk zaten çalışıyor. Önce diğer pencereyi kapatın."),
                        w!("MimoDesk"),
                        MESSAGEBOX_STYLE(MB_OK.0 | MB_ICONWARNING.0),
                    );
                }
                std::process::exit(0);
            }
            // Best effort: some other error — continue without the lock.
            tracing::warn!("single-instance lock unavailable, continuing");
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_single_instance() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    ensure_single_instance();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let peer = PeerState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            peer: Arc::new(RwLock::new(peer)),
        })
        .invoke_handler(tauri::generate_handler![
            get_desk_info,
            start_host,
            connect_remote,
            disconnect,
            set_alias,
            send_input,
            set_quality,
            send_file,
            regenerate_id,
            list_local_dir,
            list_remote_dir,
            pull_file,
            get_remote_monitors,
            set_monitor,
            answer_connection,
            set_access_password,
            set_require_approval,
            get_access_settings
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let state = handle.state::<AppState>();
            let peer = state.peer.clone();
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                {
                    let mut p = peer.write().await;
                    p.set_app_handle(app_handle.clone());
                }
                if let Err(e) = {
                    let mut p = peer.write().await;
                    p.start_host().await
                } {
                    tracing::warn!("auto host start failed: {e}");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running MimoDesk");
}
