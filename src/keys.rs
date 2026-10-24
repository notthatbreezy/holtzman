//! Elite Dangerous key names → PC/AT scan codes (set 1).
//!
//! Elite's `.binds` files name keyboard keys like `Key_M` or `Key_LeftShift`.
//! The game reads input via DirectInput/raw input, which ignores virtual-key
//! injection, so we inject hardware scan codes with `KEYEVENTF_SCANCODE`.

/// A physical key as the game sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanCode {
    pub code: u16,
    /// Extended keys (arrows, right ctrl/alt, numpad enter, ...) need the
    /// `KEYEVENTF_EXTENDEDKEY` flag.
    pub extended: bool,
}

const fn sc(code: u16) -> ScanCode {
    ScanCode {
        code,
        extended: false,
    }
}

const fn ext(code: u16) -> ScanCode {
    ScanCode {
        code,
        extended: true,
    }
}

/// Look up an Elite key name (e.g. `Key_M`, `Key_Enter`). Case-insensitive,
/// and the `Key_` prefix is optional.
pub fn scan_code(name: &str) -> Option<ScanCode> {
    let n = name.trim();
    let n = n
        .strip_prefix("Key_")
        .or_else(|| n.strip_prefix("key_"))
        .unwrap_or(n)
        .to_ascii_lowercase();

    let code = match n.as_str() {
        "escape" | "esc" => sc(0x01),
        "1" => sc(0x02),
        "2" => sc(0x03),
        "3" => sc(0x04),
        "4" => sc(0x05),
        "5" => sc(0x06),
        "6" => sc(0x07),
        "7" => sc(0x08),
        "8" => sc(0x09),
        "9" => sc(0x0A),
        "0" => sc(0x0B),
        "minus" => sc(0x0C),
        "equals" => sc(0x0D),
        "backspace" => sc(0x0E),
        "tab" => sc(0x0F),
        "q" => sc(0x10),
        "w" => sc(0x11),
        "e" => sc(0x12),
        "r" => sc(0x13),
        "t" => sc(0x14),
        "y" => sc(0x15),
        "u" => sc(0x16),
        "i" => sc(0x17),
        "o" => sc(0x18),
        "p" => sc(0x19),
        "leftbracket" => sc(0x1A),
        "rightbracket" => sc(0x1B),
        "enter" | "return" => sc(0x1C),
        "leftcontrol" | "ctrl" | "control" => sc(0x1D),
        "a" => sc(0x1E),
        "s" => sc(0x1F),
        "d" => sc(0x20),
        "f" => sc(0x21),
        "g" => sc(0x22),
        "h" => sc(0x23),
        "j" => sc(0x24),
        "k" => sc(0x25),
        "l" => sc(0x26),
        "semicolon" => sc(0x27),
        "apostrophe" => sc(0x28),
        "grave" => sc(0x29),
        "leftshift" | "shift" => sc(0x2A),
        "backslash" => sc(0x2B),
        "z" => sc(0x2C),
        "x" => sc(0x2D),
        "c" => sc(0x2E),
        "v" => sc(0x2F),
        "b" => sc(0x30),
        "n" => sc(0x31),
        "m" => sc(0x32),
        "comma" => sc(0x33),
        "period" => sc(0x34),
        "slash" => sc(0x35),
        "rightshift" => sc(0x36),
        "numpad_multiply" => sc(0x37),
        "leftalt" | "alt" => sc(0x38),
        "space" => sc(0x39),
        "capslock" => sc(0x3A),
        "f1" => sc(0x3B),
        "f2" => sc(0x3C),
        "f3" => sc(0x3D),
        "f4" => sc(0x3E),
        "f5" => sc(0x3F),
        "f6" => sc(0x40),
        "f7" => sc(0x41),
        "f8" => sc(0x42),
        "f9" => sc(0x43),
        "f10" => sc(0x44),
        "numlock" => sc(0x45),
        "scrolllock" => sc(0x46),
        "numpad_7" => sc(0x47),
        "numpad_8" => sc(0x48),
        "numpad_9" => sc(0x49),
        "numpad_subtract" => sc(0x4A),
        "numpad_4" => sc(0x4B),
        "numpad_5" => sc(0x4C),
        "numpad_6" => sc(0x4D),
        "numpad_add" => sc(0x4E),
        "numpad_1" => sc(0x4F),
        "numpad_2" => sc(0x50),
        "numpad_3" => sc(0x51),
        "numpad_0" => sc(0x52),
        "numpad_decimal" => sc(0x53),
        "f11" => sc(0x57),
        "f12" => sc(0x58),
        "numpad_enter" => ext(0x1C),
        "rightcontrol" => ext(0x1D),
        "numpad_divide" => ext(0x35),
        "rightalt" => ext(0x38),
        "home" => ext(0x47),
        "uparrow" | "up" => ext(0x48),
        "pageup" => ext(0x49),
        "leftarrow" | "left" => ext(0x4B),
        "rightarrow" | "right" => ext(0x4D),
        "end" => ext(0x4F),
        "downarrow" | "down" => ext(0x50),
        "pagedown" => ext(0x51),
        "insert" => ext(0x52),
        "delete" => ext(0x53),
        _ => return None,
    };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_up_common_keys() {
        assert_eq!(scan_code("Key_M"), Some(sc(0x32)));
        assert_eq!(scan_code("Key_Enter"), Some(sc(0x1C)));
        assert_eq!(scan_code("key_v"), Some(sc(0x2F)));
        assert_eq!(scan_code("LeftControl"), Some(sc(0x1D)));
        assert_eq!(scan_code("Key_UpArrow"), Some(ext(0x48)));
        assert_eq!(scan_code("Key_Numpad_Enter"), Some(ext(0x1C)));
        assert_eq!(scan_code("Key_Nope"), None);
    }
}
