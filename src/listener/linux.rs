use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use evdev::{Device, EventSummary, KeyCode};

use super::{KeyEvent, KeySender};

pub fn start_listening(sender: KeySender) -> Result<()> {
    let devices = discover_keyboards()?;
    if devices.is_empty() {
        bail!("no compatible keyboard devices found in /dev/input");
    }

    let mut readers = Vec::with_capacity(devices.len());
    for (path, device) in devices {
        let sender = sender.clone();
        readers.push(thread::spawn(move || {
            if let Err(error) = read_device(&path, device, sender) {
                eprintln!("[klicky] {error:#}");
            }
        }));
    }
    drop(sender);

    for reader in readers {
        let _ = reader.join();
    }
    Ok(())
}

fn discover_keyboards() -> Result<Vec<(PathBuf, Device)>> {
    let entries = fs::read_dir("/dev/input").context("failed to read /dev/input")?;
    let mut devices = Vec::new();
    let mut permission_denied = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        if !is_event_device(&path) {
            continue;
        }

        match Device::open(&path) {
            Ok(device) if is_keyboard(&device) => devices.push((path, device)),
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::PermissionDenied => permission_denied += 1,
            Err(_) => {}
        }
    }

    if devices.is_empty() && permission_denied > 0 {
        bail!(
            "keyboard devices are not readable; grant access to /dev/input/event* and log in again"
        );
    }

    Ok(devices)
}

fn is_event_device(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("event"))
}

fn is_keyboard(device: &Device) -> bool {
    device
        .supported_keys()
        .is_some_and(|keys| keys.iter().any(|key| key_to_name(key).is_some()))
}

fn read_device(path: &Path, mut device: Device, sender: KeySender) -> Result<()> {
    loop {
        let events = device
            .fetch_events()
            .with_context(|| format!("lost input device {}", path.display()))?;

        for event in events {
            if let EventSummary::Key(_, key, value) = event.destructure() {
                if let Some(key_name) = pressed_key_name(key, value) {
                    if sender
                        .send(KeyEvent {
                            key_name,
                            timestamp: Instant::now(),
                        })
                        .is_err()
                    {
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn pressed_key_name(key: KeyCode, value: i32) -> Option<&'static str> {
    (value == 1).then(|| key_to_name(key)).flatten()
}

fn key_to_name(key: KeyCode) -> Option<&'static str> {
    Some(match key {
        KeyCode::KEY_ESC => "Escape",
        KeyCode::KEY_1 => "Num1",
        KeyCode::KEY_2 => "Num2",
        KeyCode::KEY_3 => "Num3",
        KeyCode::KEY_4 => "Num4",
        KeyCode::KEY_5 => "Num5",
        KeyCode::KEY_6 => "Num6",
        KeyCode::KEY_7 => "Num7",
        KeyCode::KEY_8 => "Num8",
        KeyCode::KEY_9 => "Num9",
        KeyCode::KEY_0 => "Num0",
        KeyCode::KEY_MINUS => "Minus",
        KeyCode::KEY_EQUAL => "Equal",
        KeyCode::KEY_BACKSPACE => "Backspace",
        KeyCode::KEY_TAB => "Tab",
        KeyCode::KEY_Q => "KeyQ",
        KeyCode::KEY_W => "KeyW",
        KeyCode::KEY_E => "KeyE",
        KeyCode::KEY_R => "KeyR",
        KeyCode::KEY_T => "KeyT",
        KeyCode::KEY_Y => "KeyY",
        KeyCode::KEY_U => "KeyU",
        KeyCode::KEY_I => "KeyI",
        KeyCode::KEY_O => "KeyO",
        KeyCode::KEY_P => "KeyP",
        KeyCode::KEY_LEFTBRACE => "LeftBracket",
        KeyCode::KEY_RIGHTBRACE => "RightBracket",
        KeyCode::KEY_ENTER => "Return",
        KeyCode::KEY_LEFTCTRL => "ControlLeft",
        KeyCode::KEY_A => "KeyA",
        KeyCode::KEY_S => "KeyS",
        KeyCode::KEY_D => "KeyD",
        KeyCode::KEY_F => "KeyF",
        KeyCode::KEY_G => "KeyG",
        KeyCode::KEY_H => "KeyH",
        KeyCode::KEY_J => "KeyJ",
        KeyCode::KEY_K => "KeyK",
        KeyCode::KEY_L => "KeyL",
        KeyCode::KEY_SEMICOLON => "SemiColon",
        KeyCode::KEY_APOSTROPHE => "Quote",
        KeyCode::KEY_GRAVE => "BackQuote",
        KeyCode::KEY_LEFTSHIFT => "ShiftLeft",
        KeyCode::KEY_BACKSLASH => "BackSlash",
        KeyCode::KEY_Z => "KeyZ",
        KeyCode::KEY_X => "KeyX",
        KeyCode::KEY_C => "KeyC",
        KeyCode::KEY_V => "KeyV",
        KeyCode::KEY_B => "KeyB",
        KeyCode::KEY_N => "KeyN",
        KeyCode::KEY_M => "KeyM",
        KeyCode::KEY_COMMA => "Comma",
        KeyCode::KEY_DOT => "Dot",
        KeyCode::KEY_SLASH => "Slash",
        KeyCode::KEY_RIGHTSHIFT => "ShiftRight",
        KeyCode::KEY_KPASTERISK => "KpMultiply",
        KeyCode::KEY_LEFTALT => "Alt",
        KeyCode::KEY_SPACE => "Space",
        KeyCode::KEY_CAPSLOCK => "CapsLock",
        KeyCode::KEY_F1 | KeyCode::KEY_MUTE => "F1",
        KeyCode::KEY_F2 | KeyCode::KEY_VOLUMEDOWN => "F2",
        KeyCode::KEY_F3 | KeyCode::KEY_VOLUMEUP => "F3",
        KeyCode::KEY_F4 | KeyCode::KEY_MICMUTE => "F4",
        KeyCode::KEY_F5 | KeyCode::KEY_BRIGHTNESSDOWN => "F5",
        KeyCode::KEY_F6 | KeyCode::KEY_BRIGHTNESSUP => "F6",
        KeyCode::KEY_F7 | KeyCode::KEY_SWITCHVIDEOMODE => "F7",
        KeyCode::KEY_F8 | KeyCode::KEY_RFKILL => "F8",
        KeyCode::KEY_F9 | KeyCode::KEY_PLAYPAUSE => "F9",
        KeyCode::KEY_F10 | KeyCode::KEY_STOPCD => "F10",
        KeyCode::KEY_F11 | KeyCode::KEY_PREVIOUSSONG => "F11",
        KeyCode::KEY_F12 | KeyCode::KEY_NEXTSONG => "F12",
        KeyCode::KEY_NUMLOCK => "NumLock",
        KeyCode::KEY_SCROLLLOCK => "ScrollLock",
        KeyCode::KEY_KP7 => "Kp7",
        KeyCode::KEY_KP8 => "Kp8",
        KeyCode::KEY_KP9 => "Kp9",
        KeyCode::KEY_KPMINUS => "KpMinus",
        KeyCode::KEY_KP4 => "Kp4",
        KeyCode::KEY_KP5 => "Kp5",
        KeyCode::KEY_KP6 => "Kp6",
        KeyCode::KEY_KPPLUS => "KpPlus",
        KeyCode::KEY_KP1 => "Kp1",
        KeyCode::KEY_KP2 => "Kp2",
        KeyCode::KEY_KP3 => "Kp3",
        KeyCode::KEY_KP0 => "Kp0",
        KeyCode::KEY_KPDOT => "KpDelete",
        KeyCode::KEY_KPENTER => "KpReturn",
        KeyCode::KEY_RIGHTCTRL => "ControlRight",
        KeyCode::KEY_KPSLASH => "KpDivide",
        KeyCode::KEY_SYSRQ => "PrintScreen",
        KeyCode::KEY_RIGHTALT => "AltGr",
        KeyCode::KEY_HOME => "Home",
        KeyCode::KEY_UP => "UpArrow",
        KeyCode::KEY_PAGEUP => "PageUp",
        KeyCode::KEY_LEFT => "LeftArrow",
        KeyCode::KEY_RIGHT => "RightArrow",
        KeyCode::KEY_END => "End",
        KeyCode::KEY_DOWN => "DownArrow",
        KeyCode::KEY_PAGEDOWN => "PageDown",
        KeyCode::KEY_INSERT => "Insert",
        KeyCode::KEY_DELETE => "Delete",
        KeyCode::KEY_PAUSE => "Pause",
        KeyCode::KEY_LEFTMETA => "MetaLeft",
        KeyCode::KEY_RIGHTMETA => "MetaRight",
        KeyCode::KEY_FN => "Function",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_representative_linux_keys() {
        assert_eq!(key_to_name(KeyCode::KEY_A), Some("KeyA"));
        assert_eq!(key_to_name(KeyCode::KEY_RIGHTCTRL), Some("ControlRight"));
        assert_eq!(key_to_name(KeyCode::KEY_KPENTER), Some("KpReturn"));
        assert_eq!(key_to_name(KeyCode::KEY_VOLUMEUP), Some("F3"));
    }

    #[test]
    fn only_initial_key_down_triggers_sound() {
        assert_eq!(pressed_key_name(KeyCode::KEY_A, 1), Some("KeyA"));
        assert_eq!(pressed_key_name(KeyCode::KEY_A, 0), None);
        assert_eq!(pressed_key_name(KeyCode::KEY_A, 2), None);
    }

    #[test]
    fn ignores_unmapped_buttons() {
        assert_eq!(key_to_name(KeyCode::BTN_LEFT), None);
    }
}
