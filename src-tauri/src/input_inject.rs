//! Inject remote control events into the host OS.

use anyhow::Result;
use enigo::{
    Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings,
};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum InputEvent {
    #[serde(rename = "move")]
    Move { x: i32, y: i32 },
    #[serde(rename = "down")]
    Down { btn: String },
    #[serde(rename = "up")]
    Up { btn: String },
    #[serde(rename = "wheel")]
    Wheel { dy: i32 },
    #[serde(rename = "key")]
    Key {
        key: String,
        down: bool,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        shift: bool,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        meta: bool,
    },
    #[serde(rename = "text")]
    Text { s: String },
}

static ENIGO: Mutex<Option<Enigo>> = Mutex::new(None);

fn with_enigo(f: impl FnOnce(&mut Enigo)) -> Result<()> {
    let mut guard = ENIGO
        .lock()
        .map_err(|_| anyhow::anyhow!("input lock poisoned"))?;
    if guard.is_none() {
        *guard = Some(Enigo::new(&Settings::default())?);
    }
    let enigo = guard.as_mut().unwrap();
    f(enigo);
    Ok(())
}

fn button_of(name: &str) -> Button {
    match name.to_ascii_lowercase().as_str() {
        "right" => Button::Right,
        "middle" => Button::Middle,
        _ => Button::Left,
    }
}

fn key_of(name: &str) -> Option<Key> {
    let k = match name {
        "Control" | "control" | "Ctrl" => Key::Control,
        "Shift" | "shift" => Key::Shift,
        "Alt" | "alt" | "AltGraph" => Key::Alt,
        "Meta" | "meta" | "Super" | "OS" => Key::Meta,
        "Enter" | "enter" | "Return" => Key::Return,
        "Backspace" | "backspace" => Key::Backspace,
        "Tab" | "tab" => Key::Tab,
        "Escape" | "escape" | "Esc" => Key::Escape,
        " " | "Spacebar" | "Space" | "space" => Key::Space,
        "ArrowUp" => Key::UpArrow,
        "ArrowDown" => Key::DownArrow,
        "ArrowLeft" => Key::LeftArrow,
        "ArrowRight" => Key::RightArrow,
        "Delete" | "Del" => Key::Delete,
        "Insert" => Key::Insert,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "CapsLock" => Key::CapsLock,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,
        _ => return None,
    };
    Some(k)
}

/// Map a single printable character to enigo Key when possible (a–z, 0–9, symbols).
fn char_key(c: char) -> Option<Key> {
    match c {
        'a'..='z' | 'A'..='Z' | '0'..='9' => Some(Key::Unicode(c.to_ascii_lowercase())),
        '-' | '=' | '[' | ']' | '\\' | ';' | '\'' | '`' | ',' | '.' | '/' => {
            Some(Key::Unicode(c))
        }
        _ => None,
    }
}

fn press_mods(e: &mut Enigo, ctrl: bool, shift: bool, alt: bool, meta: bool) {
    if ctrl {
        let _ = e.key(Key::Control, Direction::Press);
    }
    if alt {
        let _ = e.key(Key::Alt, Direction::Press);
    }
    if shift {
        let _ = e.key(Key::Shift, Direction::Press);
    }
    if meta {
        let _ = e.key(Key::Meta, Direction::Press);
    }
}

fn release_mods(e: &mut Enigo, ctrl: bool, shift: bool, alt: bool, meta: bool) {
    if meta {
        let _ = e.key(Key::Meta, Direction::Release);
    }
    if shift {
        let _ = e.key(Key::Shift, Direction::Release);
    }
    if alt {
        let _ = e.key(Key::Alt, Direction::Release);
    }
    if ctrl {
        let _ = e.key(Key::Control, Direction::Release);
    }
}

pub fn inject(ev: InputEvent) -> Result<()> {
    match ev {
        InputEvent::Move { x, y } => with_enigo(|e| {
            let _ = e.move_mouse(x, y, Coordinate::Abs);
        })?,
        InputEvent::Down { btn } => with_enigo(|e| {
            let _ = e.button(button_of(&btn), Direction::Press);
        })?,
        InputEvent::Up { btn } => with_enigo(|e| {
            let _ = e.button(button_of(&btn), Direction::Release);
        })?,
        InputEvent::Wheel { dy } => with_enigo(|e| {
            let _ = e.scroll(dy, Axis::Vertical);
        })?,
        InputEvent::Text { s } => {
            if !s.is_empty() {
                with_enigo(|e| {
                    let _ = e.text(&s);
                })?;
            }
        }
        InputEvent::Key {
            key,
            down,
            ctrl,
            shift,
            alt,
            meta,
        } => {
            let dir = if down {
                Direction::Press
            } else {
                Direction::Release
            };

            // Pure modifier press/release
            if let Some(k) = key_of(&key) {
                if matches!(
                    k,
                    Key::Control | Key::Shift | Key::Alt | Key::Meta
                ) {
                    with_enigo(|e| {
                        let _ = e.key(k, dir);
                    })?;
                    return Ok(());
                }
            }

            // Special / function / arrows — with modifiers for shortcuts (Ctrl+C etc.)
            if let Some(k) = key_of(&key) {
                with_enigo(|e| {
                    if down {
                        press_mods(e, ctrl, shift, alt, meta);
                        let _ = e.key(k, Direction::Press);
                    } else {
                        let _ = e.key(k, Direction::Release);
                        release_mods(e, ctrl, shift, alt, meta);
                    }
                })?;
                return Ok(());
            }

            // Single character (letter/digit/symbol)
            if key.chars().count() == 1 {
                let c = key.chars().next().unwrap();
                with_enigo(|e| {
                    if down {
                        // Prefer Unicode text for reliable typing (covers Shift symbols).
                        if ctrl || alt || meta {
                            // Shortcut like Ctrl+A: use layout key + modifiers.
                            if let Some(ck) = char_key(c) {
                                press_mods(e, ctrl, shift, alt, meta);
                                let _ = e.key(ck, Direction::Press);
                            } else {
                                press_mods(e, ctrl, shift, alt, meta);
                                let _ = e.text(&key);
                            }
                        } else {
                            let _ = e.text(&key);
                        }
                    } else {
                        if ctrl || alt || meta {
                            if let Some(ck) = char_key(c) {
                                let _ = e.key(ck, Direction::Release);
                            }
                            release_mods(e, ctrl, shift, alt, meta);
                        }
                    }
                })?;
            }
        }
    }
    Ok(())
}
