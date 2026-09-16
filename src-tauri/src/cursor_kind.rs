//! Map the OS cursor to a compact kind for the remote viewer.

/// 0 default/arrow · 1 text · 2 hand · 3 wait · 4 resize-ns · 5 resize-ew
/// 6 move · 7 cross · 8 resize-nwse · 9 resize-nesw
///
/// NOTE: the host now draws the real cursor bitmap into the captured frame
/// (see `capture.rs`), so this kind is only a hint for the local CSS cursor.
/// Busy states (WAIT / APPSTARTING) deliberately map to the normal arrow:
/// handle comparison against stock cursors is unreliable for custom/animated
/// cursors and used to leave the viewer stuck on a "loading" spinner.
#[cfg(target_os = "windows")]
pub fn current_cursor_kind() -> u8 {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursor, LoadCursorW, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM,
        IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
    };

    unsafe {
        let cur = GetCursor();
        if cur.0.is_null() {
            return 0;
        }
        // WAIT / APPSTARTING intentionally NOT mapped: the viewer would stick
        // on a "loading" cursor for custom or briefly-busy cursors. The real
        // cursor shape is drawn into the video frame itself.
        let checks: [(windows::core::PCWSTR, u8); 9] = [
            (IDC_ARROW, 0),
            (IDC_IBEAM, 1),
            (IDC_HAND, 2),
            (IDC_SIZENS, 4),
            (IDC_SIZEWE, 5),
            (IDC_SIZEALL, 6),
            (IDC_CROSS, 7),
            (IDC_SIZENWSE, 8),
            (IDC_SIZENESW, 9),
        ];
        for (id, kind) in checks {
            if let Ok(std) = LoadCursorW(None, id) {
                if cur == std {
                    return kind;
                }
            }
        }
    }
    0
}

#[cfg(not(target_os = "windows"))]
pub fn current_cursor_kind() -> u8 {
    0
}
