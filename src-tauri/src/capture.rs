//! Screen capture → JPEG with quality presets.

use anyhow::Result;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum QualityLevel {
    Low,
    Medium,
    High,
}

impl QualityLevel {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Low,
            2 => Self::High,
            _ => Self::Medium,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
        }
    }

    pub fn max_width(self) -> u32 {
        match self {
            Self::Low => 960,
            Self::Medium => 1600,
            Self::High => 1920,
        }
    }

    pub fn jpeg_quality(self) -> u8 {
        match self {
            Self::Low => 50,
            Self::Medium => 75,
            Self::High => 90,
        }
    }

    pub fn interval_ms(self) -> u64 {
        match self {
            // Faster updates for control feel; bandwidth protected by JPEG quality.
            Self::Low => 120,
            Self::Medium => 70,
            Self::High => 40,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Düşük",
            Self::Medium => "Orta",
            Self::High => "Yüksek",
        }
    }
}

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub native_width: u32,
    pub native_height: u32,
    pub jpeg: Vec<u8>,
    /// Resolved monitor index actually captured (0xFF request → primary).
    pub monitor: u8,
    /// Persistent device id of the captured monitor (followed across frames).
    pub monitor_id: String,
    /// Captured monitor's origin in virtual-screen coordinates, so the viewer
    /// and input injection can map between frame pixels and OS coordinates.
    pub origin_x: i32,
    pub origin_y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub index: u8,
    /// Persistent OS device name (e.g. `\\.\DISPLAY2`). Stable across
    /// re-enumerations while the layout is unchanged; the viewer matches by
    /// this id so an index never silently points at another physical screen.
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

/// Enumerate monitors in a deterministic spatial order (left-to-right,
/// top-to-bottom) so an index means the same screen on every call.
fn all_sorted() -> Result<Vec<xcap::Monitor>> {
    let mut mons = xcap::Monitor::all()?;
    mons.sort_by_key(|m| (m.x(), m.y()));
    Ok(mons)
}

/// All attached monitors in stable spatial order.
pub fn list_monitors() -> Vec<MonitorInfo> {
    match all_sorted() {
        Ok(mons) => mons
            .iter()
            .enumerate()
            .map(|(i, m)| MonitorInfo {
                index: i.min(255) as u8,
                id: m.name().to_string(),
                name: m.name().to_string(),
                width: m.width(),
                height: m.height(),
                primary: m.is_primary(),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// 0xFF = auto (primary monitor). Out-of-range indexes fall back to primary.
fn resolve_index(mons: &[xcap::Monitor], index: u8) -> usize {
    if index != 0xFF && (index as usize) < mons.len() {
        return index as usize;
    }
    mons.iter().position(|m| m.is_primary()).unwrap_or(0)
}

/// Resolved monitor index + persistent id + origin for an input mapper.
/// Never fails: falls back to the primary monitor (or (0, "", 0, 0) with no
/// screens at all).
pub fn monitor_origin(index: u8) -> (u8, String, i32, i32) {
    match all_sorted() {
        Ok(mons) if !mons.is_empty() => {
            let idx = resolve_index(&mons, index);
            let m = &mons[idx];
            (idx.min(255) as u8, m.name().to_string(), m.x(), m.y())
        }
        _ => (0, String::new(), 0, 0),
    }
}

/// Pick which enumerated monitor to capture. When the viewer follows a
/// specific screen (`follow_id` set by an explicit switch), stick to that
/// physical monitor by device id; otherwise resolve the requested index
/// (0xFF = primary, tracked dynamically).
pub fn resolve_follow(mons: &[xcap::Monitor], requested: u8, follow_id: &str) -> usize {
    if requested != 0xFF && !follow_id.is_empty() {
        if let Some(pos) = mons.iter().position(|m| m.name() == follow_id) {
            return pos;
        }
    }
    resolve_index(mons, requested)
}

pub fn capture_jpeg_monitor(
    quality: QualityLevel,
    monitor: u8,
    follow_id: &str,
) -> Result<Frame> {
    let monitors = all_sorted()?;
    if monitors.is_empty() {
        anyhow::bail!("ekran bulunamadı");
    }
    let idx = resolve_follow(&monitors, monitor, follow_id);
    let mon = &monitors[idx];

    let mut rgba = mon.capture_image()?;
    let native_width = rgba.width();
    let native_height = rgba.height();
    // Draw the real cursor into the frame (xcap captures without it), so the
    // viewer sees the original pointer whatever its shape is.
    // Origin = this monitor's position, so multi-monitor offsets are correct.
    draw_cursor_into_rgba(&mut rgba, mon.x(), mon.y());
    let max_w = quality.max_width();
    let mut width = native_width;
    let mut height = native_height;

    if width > max_w {
        let new_h = ((height as u64 * max_w as u64) / width as u64).max(1) as u32;
        rgba = image::imageops::resize(&rgba, max_w, new_h, FilterType::Triangle);
        width = rgba.width();
        height = rgba.height();
    }

    let rgb: Vec<u8> = rgba
        .as_raw()
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();

    let mut jpeg = Vec::with_capacity(width as usize * height as usize / 3);
    let mut enc = JpegEncoder::new_with_quality(&mut jpeg, quality.jpeg_quality());
    enc.encode(&rgb, width, height, image::ExtendedColorType::Rgb8)?;

    Ok(Frame {
        width,
        height,
        native_width,
        native_height,
        jpeg,
        monitor: idx.min(255) as u8,
        monitor_id: mon.name().to_string(),
        origin_x: mon.x(),
        origin_y: mon.y(),
    })
}

/// Blend the current OS cursor bitmap over the captured frame at its real
/// screen position. `origin_*` is the captured monitor's position in
/// virtual-screen coordinates (from xcap), so multi-monitor offsets are right.
/// No-op when the cursor is hidden or has no color bitmap
/// (e.g. monochrome cursors) — the viewer still shows a CSS arrow there.
#[cfg(target_os = "windows")]
fn draw_cursor_into_rgba(rgba: &mut image::RgbaImage, origin_x: i32, origin_y: i32) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, HICON, ICONINFO,
    };

    unsafe {
        let mut ci: CURSORINFO = std::mem::zeroed();
        ci.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
        if GetCursorInfo(&mut ci).is_err() {
            return;
        }
        if ci.flags.0 & CURSOR_SHOWING.0 == 0 || ci.hCursor.0.is_null() {
            return;
        }

        let mut ii: ICONINFO = std::mem::zeroed();
        if GetIconInfo(HICON(ci.hCursor.0), &mut ii).is_err() {
            return;
        }
        if ii.hbmColor.0.is_null() {
            // Monochrome (mask-only) cursor — leave it to the CSS arrow.
            if !ii.hbmMask.0.is_null() {
                let _ = DeleteObject(ii.hbmMask);
            }
            return;
        }

        let mut bmp: BITMAP = std::mem::zeroed();
        let got = GetObjectW(
            ii.hbmColor,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut _),
        );
        if got == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
            let _ = DeleteObject(ii.hbmColor);
            if !ii.hbmMask.0.is_null() {
                let _ = DeleteObject(ii.hbmMask);
            }
            return;
        }
        let (cw, ch) = (bmp.bmWidth as u32, bmp.bmHeight as u32);
        if cw > 256 || ch > 256 {
            let _ = DeleteObject(ii.hbmColor);
            if !ii.hbmMask.0.is_null() {
                let _ = DeleteObject(ii.hbmMask);
            }
            return;
        }

        let hdc = GetDC(HWND(std::ptr::null_mut()));
        if hdc.0.is_null() {
            let _ = DeleteObject(ii.hbmColor);
            if !ii.hbmMask.0.is_null() {
                let _ = DeleteObject(ii.hbmMask);
            }
            return;
        }
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = cw as i32;
        bmi.bmiHeader.biHeight = -(ch as i32); // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        let mut pixels = vec![0u8; (cw * ch * 4) as usize];
        let lines = GetDIBits(
            hdc,
            ii.hbmColor,
            0,
            ch,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(HWND(std::ptr::null_mut()), hdc);
        let _ = DeleteObject(ii.hbmColor);
        if !ii.hbmMask.0.is_null() {
            let _ = DeleteObject(ii.hbmMask);
        }
        if lines == 0 {
            return;
        }

        // Skip cursors without usable alpha (mask-only style); the CSS arrow
        // already marks the position.
        if pixels.chunks_exact(4).all(|p| p[3] == 0) {
            return;
        }

        let dest_x = ci.ptScreenPos.x - origin_x - ii.xHotspot as i32;
        let dest_y = ci.ptScreenPos.y - origin_y - ii.yHotspot as i32;
        let (img_w, img_h) = (rgba.width() as i32, rgba.height() as i32);
        // BGRA bytes from GetDIBits → alpha-blend onto the RGBA frame.
        for cy in 0..ch as i32 {
            let dy = dest_y + cy;
            if dy < 0 || dy >= img_h {
                continue;
            }
            for cx in 0..cw as i32 {
                let dx = dest_x + cx;
                if dx < 0 || dx >= img_w {
                    continue;
                }
                let s = ((cy as u32 * cw + cx as u32) * 4) as usize;
                let (b, g, r, a) = (
                    pixels[s] as u32,
                    pixels[s + 1] as u32,
                    pixels[s + 2] as u32,
                    pixels[s + 3] as u32,
                );
                if a == 0 {
                    continue;
                }
                let px = rgba.get_pixel_mut(dx as u32, dy as u32);
                let inv = 255 - a;
                px[0] = ((r * a + px[0] as u32 * inv) / 255) as u8;
                px[1] = ((g * a + px[1] as u32 * inv) / 255) as u8;
                px[2] = ((b * a + px[2] as u32 * inv) / 255) as u8;
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn draw_cursor_into_rgba(_rgba: &mut image::RgbaImage, _ox: i32, _oy: i32) {}
