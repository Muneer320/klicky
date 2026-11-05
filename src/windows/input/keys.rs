pub(super) struct Mapped {
    pub name: &'static str,
    pub identity: u32,
    pub pulse: bool,
}

pub(super) fn map(make: u16, flags: u16, virtual_key: u16) -> Option<Mapped> {
    if make == 0xff || virtual_key >= 0xff {
        return None;
    }
    let media = match virtual_key {
        0xad => Some("F1"),
        0xae => Some("F2"),
        0xaf => Some("F3"),
        0xb3 => Some("F9"),
        0xb2 => Some("F10"),
        0xb1 => Some("F11"),
        0xb0 => Some("F12"),
        _ => None,
    };
    if let Some(name) = media {
        return Some(Mapped {
            name,
            identity: 0x1_0000 | u32::from(virtual_key),
            pulse: false,
        });
    }
    // Pause is a make-only pulse on ordinary Windows keyboards. Its E1 Control
    // prefix is not another Control press; avoid a permanently held Pause entry.
    if virtual_key == 0x13 {
        return Some(Mapped {
            name: "Pause",
            identity: 0x2_0000,
            pulse: true,
        });
    }
    if flags & 4 != 0 || make == 0 || make > 0x7f {
        return None;
    }
    let extended = flags & 2 != 0;
    let name = if extended {
        match make {
            0x1c => "KpReturn",
            0x1d => "ControlRight",
            0x35 => "KpDivide",
            0x37 => "PrintScreen",
            0x38 => "AltGr",
            0x45 if virtual_key == 0x90 => "NumLock",
            0x47 => "Home",
            0x48 => "UpArrow",
            0x49 => "PageUp",
            0x4b => "LeftArrow",
            0x4d => "RightArrow",
            0x4f => "End",
            0x50 => "DownArrow",
            0x51 => "PageDown",
            0x52 => "Insert",
            0x53 => "Delete",
            0x5b => "MetaLeft",
            0x5c => "MetaRight",
            0x5d => "Function",
            _ => return None,
        }
    } else {
        match make {
            0x01 => "Escape",
            0x02 => "Num1",
            0x03 => "Num2",
            0x04 => "Num3",
            0x05 => "Num4",
            0x06 => "Num5",
            0x07 => "Num6",
            0x08 => "Num7",
            0x09 => "Num8",
            0x0a => "Num9",
            0x0b => "Num0",
            0x0c => "Minus",
            0x0d => "Equal",
            0x0e => "Backspace",
            0x0f => "Tab",
            0x10 => "KeyQ",
            0x11 => "KeyW",
            0x12 => "KeyE",
            0x13 => "KeyR",
            0x14 => "KeyT",
            0x15 => "KeyY",
            0x16 => "KeyU",
            0x17 => "KeyI",
            0x18 => "KeyO",
            0x19 => "KeyP",
            0x1a => "LeftBracket",
            0x1b => "RightBracket",
            0x1c => "Return",
            0x1d => "ControlLeft",
            0x1e => "KeyA",
            0x1f => "KeyS",
            0x20 => "KeyD",
            0x21 => "KeyF",
            0x22 => "KeyG",
            0x23 => "KeyH",
            0x24 => "KeyJ",
            0x25 => "KeyK",
            0x26 => "KeyL",
            0x27 => "SemiColon",
            0x28 => "Quote",
            0x29 => "BackQuote",
            0x2a => "ShiftLeft",
            0x2b => "BackSlash",
            0x2c => "KeyZ",
            0x2d => "KeyX",
            0x2e => "KeyC",
            0x2f => "KeyV",
            0x30 => "KeyB",
            0x31 => "KeyN",
            0x32 => "KeyM",
            0x33 => "Comma",
            0x34 => "Dot",
            0x35 => "Slash",
            0x36 => "ShiftRight",
            0x37 => "KpMultiply",
            0x38 => "Alt",
            0x39 => "Space",
            0x3a => "CapsLock",
            0x3b => "F1",
            0x3c => "F2",
            0x3d => "F3",
            0x3e => "F4",
            0x3f => "F5",
            0x40 => "F6",
            0x41 => "F7",
            0x42 => "F8",
            0x43 => "F9",
            0x44 => "F10",
            0x45 => "NumLock",
            0x46 => "ScrollLock",
            0x47 => "Kp7",
            0x48 => "Kp8",
            0x49 => "Kp9",
            0x4a => "KpMinus",
            0x4b => "Kp4",
            0x4c => "Kp5",
            0x4d => "Kp6",
            0x4e => "KpPlus",
            0x4f => "Kp1",
            0x50 => "Kp2",
            0x51 => "Kp3",
            0x52 => "Kp0",
            0x53 => "KpDelete",
            0x57 => "F11",
            0x58 => "F12",
            // ISO extra and JIS Ro/Yen positions explicitly reuse BackSlash.
            0x56 | 0x73 | 0x7d => "BackSlash",
            _ => return None,
        }
    };
    Some(Mapped {
        name,
        identity: u32::from(make) | (u32::from(extended) << 8),
        pulse: false,
    })
}

pub(super) fn modifier_virtual_key(make: u16, flags: u16, virtual_key: u16) -> usize {
    match make {
        0x2a => 0xa0,
        0x36 => 0xa1,
        0x1d => {
            if flags & 2 == 0 {
                0xa2
            } else {
                0xa3
            }
        }
        0x38 => {
            if flags & 2 == 0 {
                0xa4
            } else {
                0xa5
            }
        }
        _ => usize::from(virtual_key.min(255)),
    }
}
