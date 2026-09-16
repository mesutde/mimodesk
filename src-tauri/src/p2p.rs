//! P2P host/client with screen video + input inject. Host PC is the server.

use crate::capture::{self, QualityLevel};
use crate::clipboard::{self, TAG_CLIP};
use crate::input_inject::{inject, InputEvent};
use crate::lan;
use anyhow::{anyhow, Result};
use base64::Engine;
use iroh::endpoint::presets;
use iroh::endpoint::{Connection, RelayMode, RecvStream, SendStream};
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const PEERDESK_ALPN: &[u8] = b"mimodesk/control/0";
/// App protocol version. Bumped on wire-incompatible changes; the handshake
/// carries it so mismatched exes warn instead of failing mysteriously.
pub const PEERDESK_VERSION: &str = env!("CARGO_PKG_VERSION");
const TAG_CTRL: &[u8; 8] = b"MDCTRL\0\0";
const TAG_VIDEO: &[u8; 8] = b"MDVIDEO\0";
const TAG_INPUT: &[u8; 8] = b"MDINPUT\0";
const TAG_PING: &[u8; 8] = b"MDPING\0\0";
const TAG_FILE: &[u8; 8] = b"MDFFILE\0";
const TAG_DIR: &[u8; 8] = b"MDDIR\0\0\0";
const TAG_PULL: &[u8; 8] = b"MDPULL\0\0";
const TAG_MONITORS: &[u8; 8] = b"MDMONS\0\0";

/// Shared last RTT so the video loop can adapt when the link degrades.
static LAST_RTT_MS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    Offline,
    Listening,
    Connecting,
    Connected,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkInfo {
    pub mode: String,
    pub rtt_ms: Option<u32>,
    pub suggested_quality: String,
}

/// Per-viewer state shared between the video loop and input injection.
///
/// The viewer sends coordinates relative to the *captured* monitor
/// (0..width), while the OS injects at *absolute* virtual-screen coordinates.
/// The video loop publishes the actually-captured monitor + origin every
/// frame, so mouse clicks land on the right monitor even after live switches
/// or mid-session layout changes.
#[derive(Default)]
struct ViewState {
    /// Resolved monitor index the latest frame was captured from.
    active_monitor: AtomicU8,
    /// Its origin in virtual-screen coordinates (may be negative).
    origin_x: AtomicI32,
    origin_y: AtomicI32,
}

#[derive(Debug, Clone)]
struct HostHandler {
    app: AppHandle,
    short_id: String,
}

impl ProtocolHandler for HostHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote = connection.remote_id();
        tracing::info!("incoming session from {remote}");

        // Per-viewer mapping between captured-frame pixels and absolute OS
        // coordinates (multi-monitor input offset). Published by video_loop.
        let view = Arc::new(ViewState::default());
        // View-only applies to THIS connection only (never a global mute:
        // a stale entry used to block the peer's input forever).
        let conn_view_only = Arc::new(AtomicBool::new(false));
        // At most one live video encoder per connection: a second TAG_VIDEO
        // aborts the previous loop so frames can never interleave.
        let mut video_task: Option<tauri::async_runtime::JoinHandle<()>> = None;

        // Host → client clipboard (user copies on remote PC, pastes on controller).
        clipboard::spawn_clipboard_sender(connection.clone());

        loop {
            tokio::select! {
                bi = connection.accept_bi() => {
                    let (mut send, mut recv) = match bi {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                    let mut tag = [0u8; 8];
                    if recv.read_exact(&mut tag).await.is_err() {
                        break;
                    }
                    if &tag == TAG_CTRL {
                        // Client hello: password + viewOnly + alias
                        let mut lb = [0u8; 4];
                        let mut hello = serde_json::Value::Null;
                        if recv.read_exact(&mut lb).await.is_ok() {
                            let n = u32::from_le_bytes(lb) as usize;
                            if n > 0 && n < 4096 {
                                let mut buf = vec![0u8; n];
                                if recv.read_exact(&mut buf).await.is_ok() {
                                    hello = serde_json::from_slice(&buf)
                                        .unwrap_or(serde_json::Value::Null);
                                }
                            }
                        }
                        let pw = hello
                            .get("password")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let view_only = hello
                            .get("viewOnly")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let peer_hex = remote.to_string();
                        let alias = hello
                            .get("alias")
                            .and_then(|a| a.as_str())
                            .unwrap_or("")
                            .to_string();

                        // Notify host UI first so the user can Allow/Deny.
                        let _ = self.app.emit(
                            "incoming-connection",
                            serde_json::json!({
                                "peer": peer_hex,
                                "alias": alias,
                                "viewOnly": view_only,
                            }),
                        );

                        let mut err: Option<String> = None;
                        if let Err(e) = crate::access::check_password(pw.as_deref()) {
                            err = Some(e);
                        } else {
                            let ok = crate::access::wait_approval(
                                &peer_hex,
                                std::time::Duration::from_secs(45),
                            )
                            .await;
                            if !ok {
                                err = Some("DENIED".into());
                            }
                        }

                        if view_only && err.is_none() {
                            conn_view_only.store(true, Ordering::SeqCst);
                        }

                        let reply = if let Some(e) = err {
                            serde_json::json!({ "ok": false, "error": e })
                        } else {
                            serde_json::json!({
                                "ok": true,
                                "shortId": self.short_id,
                                "viewOnly": view_only,
                                "version": PEERDESK_VERSION,
                            })
                        };
                        let bytes = reply.to_string();
                        let bl = bytes.as_bytes();
                        let _ = send.write_all(&(bl.len() as u32).to_le_bytes()).await;
                        let _ = send.write_all(bl).await;
                        let _ = send.finish();
                    } else if &tag == TAG_PING {
                        let mut buf = [0u8; 8];
                        let _ = recv.read_exact(&mut buf).await;
                        let _ = send.write_all(&buf).await;
                        let _ = send.finish();
                    } else if &tag == TAG_VIDEO {
                        // Quality byte follows the tag, then the desired monitor
                        // index (client writes both at open). The send half stays
                        // open afterwards for live monitor-switch bytes.
                        let mut q = [0u8; 1];
                        let quality = if recv.read_exact(&mut q).await.is_ok() {
                            QualityLevel::from_u8(q[0])
                        } else {
                            QualityLevel::High
                        };
                        let mut m = [0u8; 1];
                        let initial_monitor = if recv.read_exact(&mut m).await.is_ok() {
                            m[0]
                        } else {
                            0xFF
                        };
                        let app = self.app.clone();
                        let view = view.clone();
                        // Single encoder per connection: abort a previous
                        // video loop before starting a new one.
                        if let Some(h) = video_task.take() {
                            h.abort();
                        }
                        video_task = Some(tauri::async_runtime::spawn(async move {
                            if let Err(e) = video_loop(
                                &mut send,
                                &mut recv,
                                quality,
                                initial_monitor,
                                view,
                            )
                            .await
                            {
                                tracing::debug!("video stream ended: {e}");
                            }
                            let _ = app.emit("remote-status", serde_json::json!({ "video": false }));
                        }));
                    } else if &tag == TAG_MONITORS {
                        let list = capture::list_monitors();
                        let payload =
                            serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());
                        let bytes = payload.as_bytes();
                        let _ = send
                            .write_all(&(bytes.len() as u32).to_le_bytes())
                            .await;
                        let _ = send.write_all(bytes).await;
                        let _ = send.finish();
                    } else if &tag == TAG_FILE {
                        let app = self.app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = receive_file_stream(&mut send, &mut recv, &app).await {
                                tracing::warn!("file receive: {e}");
                                let _ = app.emit(
                                    "file-error",
                                    serde_json::json!({ "error": e.to_string() }),
                                );
                            }
                        });
                    } else if &tag == TAG_DIR {
                        let _ = handle_dir_list(&mut send, &mut recv).await;
                    } else if &tag == TAG_PULL {
                        let app = self.app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = handle_pull_file(&mut send, &mut recv, &app).await {
                                tracing::warn!("pull: {e}");
                            }
                        });
                    }
                }
                uni = connection.accept_uni() => {
                    let mut recv = match uni {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                    let mut tag = [0u8; 8];
                    if recv.read_exact(&mut tag).await.is_err() {
                        break;
                    }
                    if &tag == TAG_INPUT {
                        if conn_view_only.load(Ordering::SeqCst) {
                            // Drain payload without injecting.
                            let mut lenb = [0u8; 4];
                            let _ = recv.read_exact(&mut lenb).await;
                            continue;
                        }
                        if let Err(e) = handle_input_stream(&mut recv, &view).await {
                            tracing::debug!("input stream error: {e}");
                        }
                    } else if &tag == TAG_CLIP {
                        if let Err(e) = clipboard::handle_clip_body(&mut recv, &self.app).await {
                            tracing::warn!("clipboard recv: {e}");
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Publish desired monitor + its virtual-screen origin so input maps correctly.
fn publish_monitor(view: &ViewState, idx: u8) -> String {
    use std::sync::atomic::Ordering;
    let (resolved, id, ox, oy) = capture::monitor_origin(idx);
    view.active_monitor.store(resolved, Ordering::Relaxed);
    view.origin_x.store(ox, Ordering::Relaxed);
    view.origin_y.store(oy, Ordering::Relaxed);
    if idx == 0xFF {
        String::new()
    } else {
        id
    }
}

/// Run one capture off the async runtime so a slow/stuck DXGI grab cannot
/// stall the connection (input, monitor-switch bytes, keep-alives).
async fn capture_frame_async(
    quality: QualityLevel,
    monitor: u8,
    follow_id: String,
) -> Result<capture::Frame> {
    let joined = tokio::time::timeout(
        Duration::from_millis(2500),
        tokio::task::spawn_blocking(move || {
            capture::capture_jpeg_monitor(quality, monitor, &follow_id)
        }),
    )
    .await
    .map_err(|_| anyhow!("monitör yakalama zaman aşımı"))?;
    match joined {
        Ok(res) => res,
        Err(e) => Err(anyhow!("yakalama işi iptal: {e}")),
    }
}

async fn video_loop(
    send: &mut SendStream,
    recv: &mut RecvStream,
    quality: QualityLevel,
    mut monitor: u8,
    view: Arc<ViewState>,
) -> Result<()> {
    use std::sync::atomic::Ordering;
    let mut tick = tokio::time::interval(Duration::from_millis(quality.interval_ms()));
    // Debounce cursor-kind changes: only forward a new kind after seeing it
    // on consecutive frames so the viewer's CSS cursor doesn't flicker.
    let mut cursor_reported: u8 = 0;
    let mut cursor_pending: u8 = 0;
    let mut cursor_pending_count: u8 = 0;
    // Live monitor-switch commands from the viewer (1 byte = monitor index,
    // 0xFF = primary). Finished when the client drops its send half.
    let mut cmd_closed = false;
    let mut cmd = [0u8; 1];
    // Physical monitor the viewer follows (empty = track primary/index
    // dynamically). Survives OS re-enumeration mid-session.
    let mut follow_id = String::new();
    // Last monitor that produced a frame. Used to auto-revert when a switch
    // target keeps failing — otherwise the host freezes on a dead screen and
    // input (already remapped to the new origin) no longer matches the image.
    let mut last_ok_monitor: u8 = 0xFF;
    let mut last_ok_follow = String::new();
    let mut fail_streak: u8 = 0;
    // Publish the default origin immediately so input arriving before the
    // first frame is already mapped (matters when primary is not at 0,0).
    {
        let _ = publish_monitor(&view, 0xFF);
    }
    if monitor != 0xFF {
        follow_id = publish_monitor(&view, monitor);
    }

    loop {
        tokio::select! {
            // Prefer pending monitor-switch bytes over the next capture tick
            // so a switch is never starved behind a slow frame.
            biased;
            res = recv.read(&mut cmd), if !cmd_closed => {
                match res {
                    Ok(Some(1)) => {
                        if monitor != cmd[0] {
                            tracing::info!("viewer switched to monitor {}", cmd[0]);
                            fail_streak = 0;
                            monitor = cmd[0];
                            follow_id = publish_monitor(&view, monitor);
                        }
                    }
                    _ => {
                        // Client went away or old client that closes the
                        // stream right after the handshake.
                        cmd_closed = true;
                    }
                }
            }
            _ = tick.tick() => {
        // Adaptive: high RTT → lower quality & slower FPS so input stays responsive.
        let rtt = LAST_RTT_MS.load(Ordering::Relaxed);
        let effective = if rtt >= 120 {
            QualityLevel::Low
        } else if rtt >= 70 {
            QualityLevel::Medium
        } else {
            quality
        };
        let frame = match capture_frame_async(effective, monitor, follow_id.clone()).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("capture failed: {e}");
                fail_streak = fail_streak.saturating_add(1);
                // After several consecutive failures, leave the dead target:
                // first the previous good screen, else primary. Keeps the
                // session usable instead of freezing on a black frame.
                if fail_streak >= 6 {
                    let restore = if last_ok_monitor != monitor {
                        last_ok_monitor
                    } else if monitor != 0xFF {
                        0xFF
                    } else {
                        monitor
                    };
                    if restore != monitor {
                        tracing::warn!(
                            "monitor {monitor} capture failing — reverting to {restore}"
                        );
                        monitor = restore;
                        let published = publish_monitor(&view, restore);
                        follow_id = if restore == last_ok_monitor && !last_ok_follow.is_empty() {
                            last_ok_follow.clone()
                        } else {
                            published
                        };
                        fail_streak = 0;
                    }
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
        };
        fail_streak = 0;
        // Stick to the physical screen while a specific monitor is selected;
        // track the primary dynamically in auto mode.
        if monitor == 0xFF {
            follow_id.clear();
        } else {
            follow_id = frame.monitor_id.clone();
        }
        last_ok_monitor = frame.monitor;
        last_ok_follow = if monitor == 0xFF {
            String::new()
        } else {
            frame.monitor_id.clone()
        };
        // Publish what this frame actually shows so input injection can map
        // frame pixels → absolute OS coordinates.
        view.active_monitor.store(frame.monitor, Ordering::Relaxed);
        view.origin_x.store(frame.origin_x, Ordering::Relaxed);
        view.origin_y.store(frame.origin_y, Ordering::Relaxed);
        let mut header = [0u8; 24];
        header[0..4].copy_from_slice(&frame.width.to_le_bytes());
        header[4..8].copy_from_slice(&frame.height.to_le_bytes());
        header[8..12].copy_from_slice(&frame.native_width.to_le_bytes());
        header[12..16].copy_from_slice(&frame.native_height.to_le_bytes());
        header[16..20].copy_from_slice(&(frame.jpeg.len() as u32).to_le_bytes());
        let raw_cursor = crate::cursor_kind::current_cursor_kind();
        if raw_cursor == cursor_reported {
            cursor_pending = raw_cursor;
            cursor_pending_count = 0;
        } else if raw_cursor == cursor_pending {
            cursor_pending_count = cursor_pending_count.saturating_add(1);
            if cursor_pending_count >= 2 {
                cursor_reported = raw_cursor;
            }
        } else {
            cursor_pending = raw_cursor;
            cursor_pending_count = 0;
        }
        header[20] = cursor_reported;
        // Report the actually-captured monitor (resolves 0xFF → primary),
        // so the viewer shows where the frames really come from.
        header[21] = frame.monitor;
        if send.write_all(&header).await.is_err() {
            break;
        }
        if send.write_all(&frame.jpeg).await.is_err() {
            break;
        }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

async fn handle_dir_list(send: &mut SendStream, recv: &mut RecvStream) -> Result<()> {
    let mut lb = [0u8; 4];
    recv.read_exact(&mut lb).await?;
    let len = u32::from_le_bytes(lb) as usize;
    let mut path_buf = vec![0u8; len.min(2048)];
    if len > 0 {
        recv.read_exact(&mut path_buf).await?;
    }
    let path = if path_buf.is_empty() {
        default_home_dir()
    } else {
        std::path::PathBuf::from(String::from_utf8_lossy(&path_buf).to_string())
    };

    let mut entries = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&path) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let meta = e.metadata().ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = meta.map(|m| m.len()).unwrap_or(0);
            entries.push(DirEntry {
                name,
                is_dir,
                size,
            });
        }
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let payload = serde_json::json!({
        "path": path.display().to_string(),
        "entries": entries,
    })
    .to_string();
    let bytes = payload.as_bytes();
    send.write_all(&(bytes.len() as u32).to_le_bytes()).await?;
    send.write_all(bytes).await?;
    let _ = send.finish();
    Ok(())
}

async fn handle_pull_file(
    send: &mut SendStream,
    recv: &mut RecvStream,
    app: &AppHandle,
) -> Result<()> {
    use std::io::Read;
    let mut lb = [0u8; 4];
    recv.read_exact(&mut lb).await?;
    let len = u32::from_le_bytes(lb) as usize;
    if len == 0 || len > 2048 {
        anyhow::bail!("bad pull path");
    }
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await?;
    let src = std::path::PathBuf::from(String::from_utf8_lossy(&buf).to_string());
    if !src.is_file() {
        anyhow::bail!("not a file: {}", src.display());
    }
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file.bin".into());
    let mut file = std::fs::File::open(&src)?;
    let size = file.metadata()?.len();
    let nb = name.as_bytes();
    send.write_all(&(nb.len() as u32).to_le_bytes()).await?;
    send.write_all(nb).await?;
    send.write_all(&size.to_le_bytes()).await?;
    let mut chunk = vec![0u8; 64 * 1024];
    let mut sent = 0u64;
    loop {
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        send.write_all(&chunk[..n]).await?;
        sent += n as u64;
        let _ = app.emit(
            "file-progress",
            serde_json::json!({ "sent": sent, "total": size, "name": name }),
        );
    }
    let _ = send.finish();
    Ok(())
}

fn default_home_dir() -> std::path::PathBuf {
    std::env::var("USERPROFILE")
        .map(std::path::PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(std::path::PathBuf::from))
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

async fn handle_input_stream(recv: &mut RecvStream, view: &ViewState) -> Result<()> {
    loop {
        let mut lenb = [0u8; 4];
        if recv.read_exact(&mut lenb).await.is_err() {
            break;
        }
        let len = u32::from_le_bytes(lenb) as usize;
        if len == 0 || len > 64 * 1024 {
            break;
        }
        let mut buf = vec![0u8; len];
        if recv.read_exact(&mut buf).await.is_err() {
            break;
        }
        match serde_json::from_slice::<InputEvent>(&buf) {
            Ok(ev) => {
                // Mouse coordinates arrive relative to the captured monitor;
                // shift them to absolute virtual-screen coordinates so input
                // lands on the monitor the viewer is actually watching.
                let ev = match ev {
                    InputEvent::Move { x, y } => InputEvent::Move {
                        x: x.saturating_add(view.origin_x.load(Ordering::Relaxed)),
                        y: y.saturating_add(view.origin_y.load(Ordering::Relaxed)),
                    },
                    other => other,
                };
                if let Err(e) = inject(ev) {
                    tracing::warn!("inject failed: {e}");
                }
            }
            Err(e) => tracing::warn!("bad input payload: {e}"),
        }
    }
    Ok(())
}

async fn receive_file_stream(
    send: &mut SendStream,
    recv: &mut RecvStream,
    app: &AppHandle,
) -> Result<()> {
    let mut name_len_b = [0u8; 4];
    recv.read_exact(&mut name_len_b).await?;
    let name_len = u32::from_le_bytes(name_len_b) as usize;
    if name_len == 0 || name_len > 512 {
        anyhow::bail!("geçersiz dosya adı");
    }
    let mut name_buf = vec![0u8; name_len];
    recv.read_exact(&mut name_buf).await?;
    let name = String::from_utf8_lossy(&name_buf).to_string();
    let safe_name = std::path::Path::new(&name)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file.bin".into());

    // Optional destination directory from the sender (empty = default).
    let mut dest_len_b = [0u8; 4];
    recv.read_exact(&mut dest_len_b).await?;
    let dest_len = u32::from_le_bytes(dest_len_b) as usize;
    let mut dest_dir_str = String::new();
    if dest_len > 0 && dest_len <= 1024 {
        let mut db = vec![0u8; dest_len];
        recv.read_exact(&mut db).await?;
        dest_dir_str = String::from_utf8_lossy(&db).to_string();
    }

    let mut size_b = [0u8; 8];
    recv.read_exact(&mut size_b).await?;
    let size = u64::from_le_bytes(size_b);

    let dir = if !dest_dir_str.is_empty() {
        let p = std::path::PathBuf::from(&dest_dir_str);
        if p.is_absolute() {
            p
        } else {
            downloads_dir()?.join("MimoDesk")
        }
    } else {
        downloads_dir()?.join("MimoDesk")
    };
    std::fs::create_dir_all(&dir)?;
    let mut dest = dir.join(&safe_name);
    if dest.exists() {
        let stem = dest.file_stem().map(|s| s.to_string_lossy().to_string());
        let ext = dest.extension().map(|s| s.to_string_lossy().to_string());
        dest = dir.join(format!(
            "{}_{}{}",
            stem.unwrap_or_else(|| "file".into()),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            ext.map(|e| format!(".{e}")).unwrap_or_default()
        ));
    }

    let mut file = std::fs::File::create(&dest)?;
    use std::io::Write;
    let mut remaining = size;
    let mut buf = vec![0u8; 64 * 1024];
    let mut got = 0u64;
    while remaining > 0 {
        let n = match recv.read(&mut buf).await? {
            Some(n) if n > 0 => n,
            Some(_) | None => break,
        };
        file.write_all(&buf[..n])?;
        remaining = remaining.saturating_sub(n as u64);
        got += n as u64;
        let _ = app.emit(
            "file-received-progress",
            serde_json::json!({ "received": got, "total": size, "name": safe_name }),
        );
    }
    let _ = send.write_all(b"OK").await;
    let _ = send.finish();
    let _ = app.emit(
        "file-received",
        serde_json::json!({ "name": safe_name, "path": dest.display().to_string() }),
    );
    // Best-effort: open the folder so the user sees the file.
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(dest.parent().unwrap_or(std::path::Path::new(".")))
            .spawn();
    }
    Ok(())
}

fn downloads_dir() -> Result<std::path::PathBuf> {
    if let Ok(user) = std::env::var("USERPROFILE") {
        return Ok(std::path::PathBuf::from(user).join("Downloads"));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Ok(std::path::PathBuf::from(home).join("Downloads"));
    }
    Ok(std::path::PathBuf::from("."))
}

async fn read_video_client(
    mut recv: RecvStream,
    app: AppHandle,
    gen: u64,
    gen_watch: Arc<AtomicU64>,
) {
    loop {
        // A newer session took over: stop emitting so stale frames can never
        // interleave with the current session's (different monitor, etc.).
        if gen_watch.load(Ordering::Relaxed) != gen {
            tracing::info!("stale video reader (gen {gen}) exiting");
            break;
        }
        let mut header = [0u8; 24];
        if recv.read_exact(&mut header).await.is_err() {
            break;
        }
        let width = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let height = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let native_width = u32::from_le_bytes(header[8..12].try_into().unwrap());
        let native_height = u32::from_le_bytes(header[12..16].try_into().unwrap());
        let len = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;
        let cursor = header[20];
        let monitor = header[21];
        if len == 0 || len > 12 * 1024 * 1024 {
            break;
        }
        let mut jpeg = vec![0u8; len];
        if recv.read_exact(&mut jpeg).await.is_err() {
            break;
        }
        let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
        let payload = serde_json::json!({
            "width": width,
            "height": height,
            "nativeWidth": native_width,
            "nativeHeight": native_height,
            "cursor": cursor,
            "monitor": monitor,
            "jpeg": b64,
        });
        if app.emit("remote-frame", payload).is_err() {
            break;
        }
    }
}

async fn measure_rtt(conn: &Connection) -> Option<u32> {
    // One probe — heavier sampling starves the input stream when RTT is high.
    let (mut send, mut recv) = conn.open_bi().await.ok()?;
    let t0 = Instant::now();
    let mut payload = [0u8; 8];
    payload[0..4].copy_from_slice(&1u32.to_le_bytes());
    if send.write_all(TAG_PING).await.is_err() {
        return None;
    }
    if send.write_all(&payload).await.is_err() {
        return None;
    }
    let _ = send.finish();
    let mut buf = [0u8; 8];
    if recv.read_exact(&mut buf).await.is_err() {
        return None;
    }
    Some(t0.elapsed().as_millis() as u32)
}

fn store_rtt(rtt: u32) {
    use std::sync::atomic::Ordering;
    LAST_RTT_MS.store(rtt, Ordering::Relaxed);
}

fn suggest_quality(rtt_ms: u32) -> QualityLevel {
    if rtt_ms <= 40 {
        QualityLevel::High
    } else if rtt_ms <= 100 {
        QualityLevel::Medium
    } else {
        QualityLevel::Low
    }
}

pub struct PeerState {
    pub alias: String,
    pub status: SessionStatus,
    pub listening: bool,
    pub address_id: Option<EndpointId>,
    pub short_id: String,
    pub remote: Option<EndpointId>,
    pub last_error: Option<String>,
    pub link: Option<LinkInfo>,
    pub quality: QualityLevel,
    endpoint: Option<Endpoint>,
    router: Option<Router>,
    connection: Option<Connection>,
    app: Option<AppHandle>,
    lan_flags: Vec<Arc<AtomicBool>>,
    /// Desired remote monitor (0xFF = host primary). Sent at video open and
    /// on every switch; the send half is kept open for live switching.
    monitor: u8,
    video_send: Option<SendStream>,
    /// Bumped on every connect/disconnect. Stale async tasks (frame readers,
    /// ping loops) check it and exit instead of polluting the new session.
    pub session_gen: u64,
    /// session_gen value the stored video_send belongs to.
    video_session_gen: u64,
    /// Shared with spawned session tasks so they can detect staleness.
    gen_watch: Arc<AtomicU64>,
    /// Peer's app version from the handshake (mismatch → warn, not fail).
    pub remote_version: Option<String>,
}

impl PeerState {
    pub fn new() -> Self {
        Self {
            alias: hostname_fallback(),
            status: SessionStatus::Offline,
            listening: false,
            address_id: None,
            short_id: String::new(),
            remote: None,
            last_error: None,
            link: None,
            quality: QualityLevel::High,
            endpoint: None,
            router: None,
            connection: None,
            app: None,
            lan_flags: Vec::new(),
            monitor: 0xFF,
            video_send: None,
            session_gen: 0,
            video_session_gen: 0,
            gen_watch: Arc::new(AtomicU64::new(0)),
            remote_version: None,
        }
    }

    pub fn set_app_handle(&mut self, app: AppHandle) {
        self.app = Some(app);
    }

    pub fn set_quality(&mut self, level: QualityLevel) {
        self.quality = level;
    }

    /// Switch the remote monitor instantly: one byte over the already-open
    /// video stream, no reconnect. Persists for future reconnects.
    /// Refuses when the stored stream belongs to an older session, so a
    /// switch can never land in the wrong pipeline.
    pub async fn set_monitor(&mut self, index: u8) -> Result<()> {
        self.monitor = index;
        match self.video_send.as_mut() {
            Some(send) if self.video_session_gen == self.session_gen => {
                send.write_all(&[index])
                    .await
                    .map_err(|e| anyhow!("monitör değiştirilemedi: {e}"))?;
                Ok(())
            }
            Some(_) => {
                self.video_send = None;
                anyhow::bail!("Oturum yenilendi — pencereyi kapatıp tekrar bağlanın.")
            }
            None => anyhow::bail!("Oturum yok — önce bağlanın."),
        }
    }

    /// Ask the connected host for its monitor list.
    pub async fn list_remote_monitors(&mut self) -> Result<Vec<capture::MonitorInfo>> {
        let conn = self
            .connection
            .clone()
            .ok_or_else(|| anyhow!("oturum yok"))?;
        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .map_err(|e| anyhow!("monitör kanalı açılamadı: {e}"))?;
        send.write_all(TAG_MONITORS).await?;
        let _ = send.finish();
        let mut lb = [0u8; 4];
        recv.read_exact(&mut lb)
            .await
            .map_err(|e| anyhow!("monitör listesi okunamadı: {e}"))?;
        let len = u32::from_le_bytes(lb) as usize;
        if len == 0 || len > 8192 {
            anyhow::bail!("monitör listesi geçersiz");
        }
        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf)
            .await
            .map_err(|e| anyhow!("monitör listesi okunamadı: {e}"))?;
        let list: Vec<capture::MonitorInfo> =
            serde_json::from_slice(&buf).map_err(|e| anyhow!("monitör listesi bozuk: {e}"))?;
        Ok(list)
    }

    /// Wipe identity and bind a new endpoint (new short ID + full address).
    pub async fn regenerate_id(&mut self) -> Result<()> {
        let path = data_dir().join("identity.key");
        let _ = std::fs::remove_file(&path);
        self.stop_lan();
        if let Some(router) = self.router.take() {
            let _ = router.shutdown().await;
        }
        if let Some(conn) = self.connection.take() {
            conn.close(0u32.into(), b"id-regenerated");
        }
        self.endpoint = None;
        self.router = None;
        self.listening = false;
        self.connection = None;
        self.status = SessionStatus::Offline;
        self.start_host().await
    }

    pub fn address_display(&self) -> String {
        if self.short_id.is_empty() {
            "—".into()
        } else {
            lan::format_short_id(&self.short_id)
        }
    }

    pub fn short_id_raw(&self) -> String {
        self.short_id.clone()
    }

    pub fn address_raw(&self) -> String {
        self.address_id
            .map(|id| id.to_string())
            .unwrap_or_default()
    }

    pub fn relay_label(&self) -> String {
        "Iroh · VPS yok".into()
    }

    fn stop_lan(&mut self) {
        for f in self.lan_flags.drain(..) {
            f.store(false, Ordering::SeqCst);
        }
    }

    pub async fn start_host(&mut self) -> Result<()> {
        if self.listening && self.endpoint.is_some() {
            return Ok(());
        }

        let app = self
            .app
            .clone()
            .ok_or_else(|| anyhow!("app handle yok"))?;

        if let Some(router) = self.router.take() {
            let _ = router.shutdown().await;
        }
        self.stop_lan();
        self.router = None;
        self.endpoint = None;

        // Persistent identity so short ID stays stable across restarts.
        let secret = load_or_create_secret(&data_dir().join("identity.key"))?;
        let endpoint = Endpoint::builder(presets::N0)
            .alpns(vec![PEERDESK_ALPN.to_vec()])
            .secret_key(secret)
            .relay_mode(RelayMode::Default)
            .bind()
            .await?;

        let endpoint_id = endpoint.id();
        let id_hex = endpoint_id.to_string();
        self.short_id = lan::short_id_from_endpoint(&id_hex);

        let router = Router::builder(endpoint.clone())
            .accept(
                PEERDESK_ALPN,
                HostHandler {
                    app: app.clone(),
                    short_id: self.short_id.clone(),
                },
            )
            .spawn();
        self.router = Some(router);

        self.lan_flags
            .push(lan::spawn_lan(self.short_id.clone(), id_hex.clone()));
        lan::remember_peer(&self.short_id, &id_hex);

        self.address_id = Some(endpoint_id);
        self.listening = true;
        self.status = SessionStatus::Listening;
        self.last_error = None;
        self.endpoint = Some(endpoint);

        tracing::info!("host listening short-id={}", self.short_id);
        Ok(())
    }

    pub async fn connect_remote(
        &mut self,
        address: String,
        password: Option<String>,
        view_only: bool,
    ) -> Result<()> {
        let trimmed = address.trim().replace(' ', "");
        let endpoint_id = self.resolve_target(&trimmed).await?;

        if self.endpoint.is_none() {
            self.start_host().await?;
        }
        let endpoint = self
            .endpoint
            .clone()
            .ok_or_else(|| anyhow!("endpoint hazır değil"))?;
        let app = self
            .app
            .clone()
            .ok_or_else(|| anyhow!("app handle yok"))?;

        // Single-flight session: close any previous connection first so two
        // video pipelines (stale + new) can never run side by side.
        if let Some(old) = self.connection.take() {
            tracing::info!("closing previous session before new connect");
            old.close(0u32.into(), b"replaced-by-new-session");
        }
        self.video_send = None;
        self.remote_version = None;
        // Fresh quality baseline per connect (the old downgrade never carried
        // over — it only adapted down and stayed there forever).
        self.quality = QualityLevel::High;
        self.session_gen = self.session_gen.wrapping_add(1);
        self.gen_watch
            .store(self.session_gen, Ordering::SeqCst);
        let gen = self.session_gen;

        self.status = SessionStatus::Connecting;
        self.remote = Some(endpoint_id);

        let addr = EndpointAddr::from(endpoint_id);
        let conn = match endpoint.connect(addr, PEERDESK_ALPN).await {
            Ok(c) => c,
            Err(e) => {
                self.status = SessionStatus::Error;
                self.last_error = Some(e.to_string());
                return Err(anyhow!(e));
            }
        };

        // Control handshake — host replies with short ID + auth result.
        match conn.open_bi().await {
            Ok((mut send, mut recv)) => {
                let hello = serde_json::json!({
                    "password": password.unwrap_or_default(),
                    "viewOnly": view_only,
                    "alias": self.alias,
                })
                .to_string();
                let hb = hello.as_bytes();
                let _ = send.write_all(TAG_CTRL).await;
                let _ = send.write_all(&(hb.len() as u32).to_le_bytes()).await;
                let _ = send.write_all(hb).await;
                let _ = send.finish();
                let mut lb = [0u8; 4];
                if recv.read_exact(&mut lb).await.is_ok() {
                    let len = u32::from_le_bytes(lb) as usize;
                    if len > 0 && len < 4096 {
                        let mut buf = vec![0u8; len];
                        if recv.read_exact(&mut buf).await.is_ok() {
                            if let Ok(v) =
                                serde_json::from_slice::<serde_json::Value>(&buf)
                            {
                                if v.get("ok").and_then(|o| o.as_bool()) == Some(false) {
                                    let e = v
                                        .get("error")
                                        .and_then(|e| e.as_str())
                                        .unwrap_or("DENIED")
                                        .to_string();
                                    self.status = SessionStatus::Error;
                                    let msg = match e.as_str() {
                                        "WRONG_PASSWORD" => {
                                            "Yanlış şifre — host erişim şifresi gerekli."
                                                .to_string()
                                        }
                                        "DENIED" => "Bağlantı host tarafından reddedildi."
                                            .to_string(),
                                        other => other.to_string(),
                                    };
                                    self.last_error = Some(msg.clone());
                                    return Err(anyhow!(msg));
                                }
                                if let Some(sid) =
                                    v.get("shortId").and_then(|s| s.as_str())
                                {
                                    lan::remember_peer(sid, &endpoint_id.to_string());
                                }
                                if let Some(rv) =
                                    v.get("version").and_then(|s| s.as_str())
                                {
                                    self.remote_version = Some(rv.to_string());
                                    if rv != PEERDESK_VERSION {
                                        tracing::warn!(
                                            "peer app version {rv} != ours {PEERDESK_VERSION} — update both sides"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                self.status = SessionStatus::Error;
                self.last_error = Some(e.to_string());
                return Err(anyhow!(e));
            }
        }

        let rtt = measure_rtt(&conn).await;
        if let Some(r) = rtt {
            store_rtt(r);
        }
        let suggested = rtt.map(suggest_quality).unwrap_or(QualityLevel::High);
        let mode = if rtt.is_some() && rtt.unwrap_or(999) <= 35 {
            "doğrudan (P2P)"
        } else {
            "Iroh (hole punch / relay)"
        };
        self.link = Some(LinkInfo {
            mode: mode.into(),
            rtt_ms: rtt,
            suggested_quality: suggested.label().into(),
        });

        // Prefer faster path suggestion when quality is still default High and RTT is bad.
        if rtt.map(|r| r > 100).unwrap_or(false) && self.quality == QualityLevel::High {
            self.quality = suggested;
        }

        // Start remote video with selected quality.
        // The send half stays open in `video_send` so the viewer can switch
        // monitors live by writing a single index byte (no reconnect needed).
        match conn.open_bi().await {
            Ok((mut send, recv)) => {
                let _ = send.write_all(TAG_VIDEO).await;
                let _ = send.write_all(&[self.quality.as_u8()]).await;
                let _ = send.write_all(&[self.monitor]).await;
                self.video_send = Some(send);
                self.video_session_gen = self.session_gen;
                let app2 = app.clone();
                let gen_watch = self.gen_watch.clone();
                tracing::info!(
                    "video stream opened (gen {gen}, monitor {})",
                    self.monitor
                );
                tauri::async_runtime::spawn(async move {
                    read_video_client(recv, app2, gen, gen_watch).await;
                });
            }
            Err(e) => {
                self.status = SessionStatus::Error;
                self.last_error = Some(format!("video stream: {e}"));
                return Err(anyhow!(e));
            }
        }

        self.connection = Some(conn.clone());
        self.status = SessionStatus::Connected;
        self.last_error = None;

        // Clipboard both ways + accept host-initiated streams.
        clipboard::spawn_clipboard_sender(conn.clone());
        {
            let accept_conn = conn.clone();
            let accept_app = app.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::select! {
                        uni = accept_conn.accept_uni() => {
                            let Ok(mut recv) = uni else { break };
                            let mut tag = [0u8; 8];
                            if recv.read_exact(&mut tag).await.is_err() { break; }
                            if &tag == TAG_CLIP {
                                let _ = clipboard::handle_clip_body(&mut recv, &accept_app).await;
                            }
                        }
                        bi = accept_conn.accept_bi() => {
                            let Ok((_send, mut recv)) = bi else { break };
                            let mut tag = [0u8; 8];
                            if recv.read_exact(&mut tag).await.is_err() { break; }
                            if &tag == TAG_CLIP {
                                let _ = clipboard::handle_clip_body(&mut recv, &accept_app).await;
                            }
                        }
                    }
                }
            });
        }

        let _ = app.emit(
            "remote-status",
            serde_json::json!({
                "video": true,
                "remote": endpoint_id.to_string(),
                "link": self.link,
            }),
        );

        // Light ping every 3s. Heavy probing starves input when the link is bad.
        let ping_conn = conn;
        let ping_app = app.clone();
        let ping_watch = self.gen_watch.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(3)).await;
                if ping_watch.load(Ordering::Relaxed) != gen {
                    break;
                }
                match measure_rtt(&ping_conn).await {
                    Some(rtt) => {
                        store_rtt(rtt);
                        let _ = ping_app.emit("ping", serde_json::json!({ "rtt": rtt }));
                    }
                    None => break,
                }
            }
        });
        Ok(())
    }

    async fn resolve_target(&self, input: &str) -> Result<EndpointId> {
        let cleaned: String = input
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();

        if let Some(sid) = lan::normalize_short_id(input) {
            // 1) Disk cache (works after one successful full-address connect)
            if let Some(ep) = lan::cached_peer(&sid) {
                if let Ok(id) = EndpointId::from_str(&ep) {
                    return Ok(id);
                }
            }
            // 2) LAN UDP discovery
            let sid2 = sid.clone();
            let resolved = tokio::task::spawn_blocking(move || {
                lan::lookup_short_id(&sid2, Duration::from_millis(2500))
            })
            .await
            .ok()
            .flatten();
            if let Some(ep) = resolved {
                return EndpointId::from_str(&ep)
                    .map_err(|e| anyhow!("Kısa ID çözümlendi ama geçersiz endpoint: {e}"));
            }
            anyhow::bail!(
                "Kısa ID ({}) çözülemedi. İlk bağlantıda tam adres kullanın; sonra kısa ID hatırlanır. Aynı ağda UDP {}/firewall kontrolü.",
                lan::format_short_id(&sid),
                lan::LAN_PORT
            );
        }

        EndpointId::from_str(&cleaned).map_err(|e| anyhow!("Geçersiz MimoDesk adresi: {e}"))
    }

    pub async fn send_file(&mut self, path: String, dest_dir: String) -> Result<String> {
        use std::io::Read;
        if self.connection.is_none() {
            anyhow::bail!("Oturum yok — önce bağlanın.");
        }
        let conn = self
            .connection
            .clone()
            .ok_or_else(|| anyhow!("oturum yok"))?;
        let app = self.app.clone();

        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .map_err(|e| anyhow!("Dosya kanalı açılamadı: {e}"))?;
        send.write_all(TAG_FILE).await?;

        let path_buf = std::path::PathBuf::from(&path);
        let name = path_buf
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "file.bin".into());
        let name_bytes = name.as_bytes();
        send.write_all(&(name_bytes.len() as u32).to_le_bytes()).await?;
        send.write_all(name_bytes).await?;

        let dest_bytes = dest_dir.as_bytes();
        send.write_all(&(dest_bytes.len() as u32).to_le_bytes()).await?;
        if !dest_bytes.is_empty() {
            send.write_all(dest_bytes).await?;
        }

        let mut file = std::fs::File::open(&path_buf)?;
        let size = file.metadata()?.len();
        send.write_all(&size.to_le_bytes()).await?;

        let mut buf = vec![0u8; 64 * 1024];
        let mut sent = 0u64;
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            send.write_all(&buf[..n]).await?;
            sent += n as u64;
            if let Some(app) = &app {
                let _ = app.emit(
                    "file-progress",
                    serde_json::json!({ "sent": sent, "total": size, "name": name }),
                );
            }
        }
        let _ = send.finish();
        let mut ack = [0u8; 2];
        let _ = recv.read_exact(&mut ack).await;
        Ok(name)
    }

    pub async fn list_remote_dir(&mut self, path: String) -> Result<(String, Vec<DirEntry>)> {
        let conn = self
            .connection
            .clone()
            .ok_or_else(|| anyhow!("oturum yok"))?;
        let (mut send, mut recv) = conn.open_bi().await?;
        send.write_all(TAG_DIR).await?;
        let pb = path.as_bytes();
        send.write_all(&(pb.len() as u32).to_le_bytes()).await?;
        if !pb.is_empty() {
            send.write_all(pb).await?;
        }
        let mut lb = [0u8; 4];
        recv.read_exact(&mut lb).await?;
        let len = u32::from_le_bytes(lb) as usize;
        if len == 0 || len > 8 * 1024 * 1024 {
            anyhow::bail!("dir list empty");
        }
        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf).await?;
        let v: serde_json::Value = serde_json::from_slice(&buf)?;
        let dir = v
            .get("path")
            .and_then(|p| p.as_str())
            .unwrap_or_default()
            .to_string();
        let entries: Vec<DirEntry> =
            serde_json::from_value(v.get("entries").cloned().unwrap_or_default())?;
        Ok((dir, entries))
    }

    /// Host streams a file; we save it under `local_dest`.
    pub async fn pull_file(&mut self, remote_path: String, local_dest: String) -> Result<String> {
        use std::io::Write;
        let conn = self
            .connection
            .clone()
            .ok_or_else(|| anyhow!("oturum yok"))?;
        let app = self.app.clone();
        let (mut send, mut recv) = conn.open_bi().await?;
        send.write_all(TAG_PULL).await?;
        let pb = remote_path.as_bytes();
        send.write_all(&(pb.len() as u32).to_le_bytes()).await?;
        send.write_all(pb).await?;

        let mut nlb = [0u8; 4];
        recv.read_exact(&mut nlb).await?;
        let nlen = u32::from_le_bytes(nlb) as usize;
        let mut nbuf = vec![0u8; nlen.min(512)];
        recv.read_exact(&mut nbuf).await?;
        let name = String::from_utf8_lossy(&nbuf).to_string();
        let mut sb = [0u8; 8];
        recv.read_exact(&mut sb).await?;
        let size = u64::from_le_bytes(sb);

        let dest_dir = if local_dest.trim().is_empty() {
            downloads_dir()?.join("MimoDesk")
        } else {
            std::path::PathBuf::from(local_dest.trim())
        };
        std::fs::create_dir_all(&dest_dir)?;
        let mut dest = dest_dir.join(&name);
        if dest.exists() {
            dest = dest_dir.join(format!(
                "{}_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                name
            ));
        }
        let mut file = std::fs::File::create(&dest)?;
        let mut remaining = size;
        let mut buf = vec![0u8; 64 * 1024];
        let mut got = 0u64;
        while remaining > 0 {
            let n = match recv.read(&mut buf).await? {
                Some(n) if n > 0 => n,
                _ => break,
            };
            file.write_all(&buf[..n])?;
            remaining = remaining.saturating_sub(n as u64);
            got += n as u64;
            if let Some(app) = &app {
                let _ = app.emit(
                    "file-progress",
                    serde_json::json!({ "received": got, "total": size, "name": name }),
                );
            }
        }
        if let Some(app) = &app {
            let _ = app.emit(
                "file-received",
                serde_json::json!({ "name": name, "path": dest.display().to_string() }),
            );
        }
        Ok(dest.display().to_string())
    }

    pub async fn send_input(&mut self, payload: InputEvent) -> Result<()> {
        let conn = self
            .connection
            .clone()
            .ok_or_else(|| anyhow!("oturum yok"))?;
        let mut send = conn.open_uni().await?;
        send.write_all(TAG_INPUT).await?;
        let json = serde_json::to_vec(&payload)?;
        send.write_all(&(json.len() as u32).to_le_bytes()).await?;
        send.write_all(&json).await?;
        // Finish explicitly: a dropped-but-unfinished stream may read as
        // RESET on the peer and the event would be silently lost.
        let _ = send.finish();
        Ok(())
    }

    pub fn disconnect(&mut self) {
        // Bump the generation first so stale readers/pingers exit even before
        // the QUIC close propagates.
        self.session_gen = self.session_gen.wrapping_add(1);
        self.gen_watch
            .store(self.session_gen, Ordering::SeqCst);
        // Drop the video command stream first so a stale monitor byte can
        // never leak into the next session.
        self.video_send = None;
        if let Some(conn) = self.connection.take() {
            // Hard-close QUIC so video/input/ping tasks die immediately.
            conn.close(0u32.into(), b"session-ended");
        }
        self.remote = None;
        self.link = None;
        self.status = if self.listening {
            SessionStatus::Listening
        } else {
            SessionStatus::Offline
        };
        if let Some(app) = &self.app {
            let _ = app.emit(
                "remote-status",
                serde_json::json!({ "video": false, "disconnected": true }),
            );
        }
        tracing::info!("session disconnected");
    }
}

fn load_or_create_secret(path: &std::path::Path) -> Result<SecretKey> {
    const SECRET_LEN: usize = 32;
    if let Ok(bytes) = std::fs::read(path) {
        if bytes.len() == SECRET_LEN {
            let mut arr = [0u8; SECRET_LEN];
            arr.copy_from_slice(&bytes);
            return Ok(SecretKey::from_bytes(&arr));
        }
    }
    let secret = SecretKey::generate();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, secret.to_bytes());
    Ok(secret)
}

fn data_dir() -> std::path::PathBuf {
    std::env::var("APPDATA")
        .map(|b| std::path::PathBuf::from(b).join("MimoDesk"))
        .unwrap_or_else(|_| std::path::PathBuf::from(".mimodesk"))
}

fn hostname_fallback() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "MimoDesk".into())
}
