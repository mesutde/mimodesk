//! Bidirectional clipboard sync (text + images) over Iroh uni-streams.

use anyhow::Result;
use arboard::{Clipboard, ImageData};
use iroh::endpoint::Connection;
use serde_json::json;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const TAG_CLIP: &[u8; 8] = b"MDCLIP\0\0";

static LAST_TEXT: Mutex<Option<String>> = Mutex::new(None);
static LAST_IMG_HASH: Mutex<Option<u64>> = Mutex::new(None);

fn hash_bytes(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Poll local clipboard and push changes to the peer.
pub fn spawn_clipboard_sender(conn: Connection) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if !try_send_clipboard(&conn).await {
                break;
            }
        }
    });
}

async fn try_send_clipboard(conn: &Connection) -> bool {
    let Ok(mut clip) = Clipboard::new() else {
        return true;
    };

    if let Ok(text) = clip.get_text() {
        if !text.is_empty() {
            let changed = match LAST_TEXT.lock() {
                Ok(last) => last.as_deref() != Some(text.as_str()),
                Err(_) => false,
            };
            if changed {
                if let Ok(mut last) = LAST_TEXT.lock() {
                    *last = Some(text.clone());
                }
                let payload = text.into_bytes();
                if let Ok(mut send) = conn.open_uni().await {
                    if send.write_all(TAG_CLIP).await.is_err() {
                        return false;
                    }
                    if send.write_all(&[0u8]).await.is_err() {
                        return false;
                    }
                    let _ = send
                        .write_all(&(payload.len() as u32).to_le_bytes())
                        .await;
                    if send.write_all(&payload).await.is_err() {
                        return false;
                    }
                    // Finish explicitly so the peer never sees a RESET
                    // instead of the payload.
                    let _ = send.finish();
                } else {
                    return false;
                }
            }
            return true;
        }
    }

    if let Ok(img) = clip.get_image() {
        let w = img.width as u32;
        let h = img.height as u32;
        let bytes = img.bytes.into_owned();
        let hash = hash_bytes(&bytes);
        let changed = match LAST_IMG_HASH.lock() {
            Ok(last) => *last != Some(hash),
            Err(_) => false,
        };
        if changed {
            if let Ok(mut last) = LAST_IMG_HASH.lock() {
                *last = Some(hash);
            }
            if let Ok(mut send) = conn.open_uni().await {
                if send.write_all(TAG_CLIP).await.is_err() {
                    return false;
                }
                if send.write_all(&[1u8]).await.is_err() {
                    return false;
                }
                let _ = send.write_all(&w.to_le_bytes()).await;
                let _ = send.write_all(&h.to_le_bytes()).await;
                let _ = send.write_all(&(bytes.len() as u32).to_le_bytes()).await;
                if send.write_all(&bytes).await.is_err() {
                    return false;
                }
                let _ = send.finish();
            } else {
                return false;
            }
        }
    }
    true
}

pub fn apply_clip_text(text: String) -> Result<()> {
    if let Ok(mut clip) = Clipboard::new() {
        let _ = clip.set_text(text.clone());
    }
    if let Ok(mut last) = LAST_TEXT.lock() {
        *last = Some(text);
    }
    Ok(())
}

pub fn apply_clip_image(w: u32, h: u32, rgba: Vec<u8>) -> Result<()> {
    if let Ok(mut clip) = Clipboard::new() {
        let _ = clip.set_image(ImageData {
            width: w as usize,
            height: h as usize,
            bytes: rgba.into(),
        });
    }
    Ok(())
}

pub async fn handle_clip_body(
    recv: &mut iroh::endpoint::RecvStream,
    app: &AppHandle,
) -> Result<()> {
    let mut kind = [0u8; 1];
    recv.read_exact(&mut kind).await?;
    match kind[0] {
        0 => {
            let mut lb = [0u8; 4];
            recv.read_exact(&mut lb).await?;
            let len = u32::from_le_bytes(lb) as usize;
            if len > 8 * 1024 * 1024 {
                anyhow::bail!("clipboard text too large");
            }
            let mut buf = vec![0u8; len];
            recv.read_exact(&mut buf).await?;
            let text = String::from_utf8_lossy(&buf).to_string();
            apply_clip_text(text.clone())?;
            let preview: String = text.chars().take(100).collect();
            let _ = app.emit(
                "clipboard",
                json!({ "kind": "text", "preview": preview }),
            );
        }
        1 => {
            let mut w = [0u8; 4];
            let mut h = [0u8; 4];
            let mut lb = [0u8; 4];
            recv.read_exact(&mut w).await?;
            recv.read_exact(&mut h).await?;
            recv.read_exact(&mut lb).await?;
            let width = u32::from_le_bytes(w);
            let height = u32::from_le_bytes(h);
            let len = u32::from_le_bytes(lb) as usize;
            if len == 0 || len > 64 * 1024 * 1024 {
                anyhow::bail!("clipboard image invalid");
            }
            let mut buf = vec![0u8; len];
            recv.read_exact(&mut buf).await?;
            apply_clip_image(width, height, buf)?;
            let _ = app.emit(
                "clipboard",
                json!({ "kind": "image", "width": width, "height": height }),
            );
        }
        _ => anyhow::bail!("unknown clipboard kind"),
    }
    Ok(())
}
