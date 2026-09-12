//! 按键名解析：把 `"ctrl"`、`"PageDown"`、`"F5"`、`"cmd+shift+s"` 之类
//! 的字符串转换成 enigo 的 [`Key`]。
//!
//! 注意：enigo 的 `Key` 枚举里有大量变体是按平台 `#[cfg]` 门控的
//! （例如 `Key::A`、`Key::Num0`、`Key::Insert` 只在 Windows / Linux 存在）。
//! 因此这里**字母与数字一律映射成 `Key::Unicode`**，保证三平台通用；
//! 平台专属按键则分别放在 `non_mac_key()` / `windows_key()` / `unix_key()` / `mac_only_key()` 里。

use enigo::Key;

use crate::error::{DeskError, DeskResult};

/// 解析单个按键名
pub fn parse_key(name: &str) -> DeskResult<Key> {
    let raw = name.trim();
    if raw.is_empty() {
        return Err(DeskError::InvalidArgument("按键名不能为空".into()));
    }

    // 单字符直接按字符处理（'a' / '1' / '-' / '=' / '[' ...）
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() == 1 {
        return Ok(Key::Unicode(chars[0]));
    }

    // 归一化：忽略空格、'-'、'_'，转小写
    let norm: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect();

    if let Some(k) = common_key(&norm)
        .or_else(|| non_mac_key(&norm))
        .or_else(|| windows_key(&norm))
        .or_else(|| unix_key(&norm))
        .or_else(|| mac_only_key(&norm))
    {
        return Ok(k);
    }

    Err(DeskError::InvalidArgument(format!(
        "无法识别的按键名 `{raw}`。支持示例：a / 1 / F5 / Enter / Esc / Tab / Space / Backspace / \
         Delete / Home / End / PageUp / PageDown / Up / Down / Left / Right / CapsLock / \
         Ctrl / Alt / Shift / Meta(cmd) / Numpad0-9 / VolumeUp / MediaPlayPause / BrightnessUp(macOS)"
    )))
}

/// 三平台通用按键
fn common_key(norm: &str) -> Option<Key> {
    // 数字键 → Unicode（Num0..Num9 只在 Windows 存在）
    if norm.len() == 1 && norm.chars().all(|c| c.is_ascii_digit()) {
        return norm.chars().next().map(Key::Unicode);
    }
    // 功能键 F1-F20 三平台通用
    if let Some(rest) = norm.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u8>() {
            if let Some(k) = f_key(n) {
                return Some(k);
            }
        }
    }

    Some(match norm {
        // 修饰键
        "ctrl" | "control" => Key::Control,
        "alt" | "option" | "opt" => Key::Alt,
        "shift" => Key::Shift,
        "meta" | "cmd" | "command" | "super" | "win" | "windows" | "gui" => Key::Meta,
        "lcontrol" => Key::LControl,
        "rcontrol" => Key::RControl,
        "lshift" => Key::LShift,
        "rshift" => Key::RShift,
        // 编辑 / 导航
        "enter" | "return" | "cr" => Key::Return,
        "tab" => Key::Tab,
        "esc" | "escape" => Key::Escape,
        "space" | "spacebar" => Key::Space,
        "backspace" | "bksp" | "bs" => Key::Backspace,
        "delete" | "del" | "forwarddelete" => Key::Delete,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" | "prior" => Key::PageUp,
        "pagedown" | "pgdn" | "next" => Key::PageDown,
        "up" | "uparrow" | "arrowup" => Key::UpArrow,
        "down" | "downarrow" | "arrowdown" => Key::DownArrow,
        "left" | "leftarrow" | "arrowleft" => Key::LeftArrow,
        "right" | "rightarrow" | "arrowright" => Key::RightArrow,
        "capslock" | "caps" => Key::CapsLock,
        // 符号（名字形式）
        "minus" | "hyphen" | "dash" => Key::Unicode('-'),
        "equal" | "equals" => Key::Unicode('='),
        "plus" => Key::Unicode('+'),
        "comma" => Key::Unicode(','),
        "period" | "dot" | "fullstop" => Key::Unicode('.'),
        "semicolon" => Key::Unicode(';'),
        "quote" | "apostrophe" | "singlequote" => Key::Unicode('\''),
        "grave" | "backtick" | "backquote" => Key::Unicode('`'),
        "backslash" => Key::Unicode('\\'),
        "slash" | "forwardslash" => Key::Unicode('/'),
        "leftbracket" | "bracketleft" => Key::Unicode('['),
        "rightbracket" | "bracketright" => Key::Unicode(']'),
        // 音量 / 媒体
        "volumeup" | "volup" => Key::VolumeUp,
        "volumedown" | "voldown" => Key::VolumeDown,
        "volumemute" | "mute" => Key::VolumeMute,
        "playpause" | "mediaplaypause" => Key::MediaPlayPause,
        "nexttrack" | "medianexttrack" => Key::MediaNextTrack,
        "prevtrack" | "mediaprevtrack" => Key::MediaPrevTrack,
        // 小键盘
        "numpad0" => Key::Numpad0,
        "numpad1" => Key::Numpad1,
        "numpad2" => Key::Numpad2,
        "numpad3" => Key::Numpad3,
        "numpad4" => Key::Numpad4,
        "numpad5" => Key::Numpad5,
        "numpad6" => Key::Numpad6,
        "numpad7" => Key::Numpad7,
        "numpad8" => Key::Numpad8,
        "numpad9" => Key::Numpad9,
        "numpadadd" | "add" => Key::Add,
        "numpadsubtract" | "subtract" => Key::Subtract,
        "numpadmultiply" | "multiply" => Key::Multiply,
        "numpaddivide" | "divide" => Key::Divide,
        "numpaddecimal" | "decimal" => Key::Decimal,
        "numpadenter" => Key::Return,
        "help" => Key::Help,
        _ => return None,
    })
}

fn f_key(n: u8) -> Option<Key> {
    Some(match n {
        1 => Key::F1,
        2 => Key::F2,
        3 => Key::F3,
        4 => Key::F4,
        5 => Key::F5,
        6 => Key::F6,
        7 => Key::F7,
        8 => Key::F8,
        9 => Key::F9,
        10 => Key::F10,
        11 => Key::F11,
        12 => Key::F12,
        13 => Key::F13,
        14 => Key::F14,
        15 => Key::F15,
        16 => Key::F16,
        17 => Key::F17,
        18 => Key::F18,
        19 => Key::F19,
        20 => Key::F20,
        _ => return None,
    })
}

/// Windows / Linux 共有（enigo 中标注为 `any(windows, all(unix, not(macos)))`）
#[cfg(not(target_os = "macos"))]
fn non_mac_key(norm: &str) -> Option<Key> {
    Some(match norm {
        "insert" | "ins" => Key::Insert,
        "numlock" => Key::Numlock,
        "pause" => Key::Pause,
        "printscreen" | "prtsc" | "printscr" | "print" => Key::PrintScr,
        "clear" => Key::Clear,
        "stop" | "mediastop" => Key::MediaStop,
        "f21" => Key::F21,
        "f22" => Key::F22,
        "f23" => Key::F23,
        "f24" => Key::F24,
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
fn non_mac_key(_norm: &str) -> Option<Key> {
    None
}

/// 仅 Windows
#[cfg(target_os = "windows")]
fn windows_key(norm: &str) -> Option<Key> {
    Some(match norm {
        "apps" | "menu" | "contextmenu" => Key::Apps,
        _ => return None,
    })
}

#[cfg(not(target_os = "windows"))]
fn windows_key(_norm: &str) -> Option<Key> {
    None
}

/// 仅 Linux / BSD（非 macOS 的 unix）
#[cfg(all(unix, not(target_os = "macos")))]
fn unix_key(norm: &str) -> Option<Key> {
    Some(match norm {
        "scrolllock" => Key::ScrollLock,
        "micmute" => Key::MicMute,
        _ => return None,
    })
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn unix_key(_norm: &str) -> Option<Key> {
    None
}

/// macOS 独有
#[cfg(target_os = "macos")]
fn mac_only_key(norm: &str) -> Option<Key> {
    Some(match norm {
        "brightnessup" => Key::BrightnessUp,
        "brightnessdown" => Key::BrightnessDown,
        "eject" => Key::Eject,
        "missioncontrol" | "f3mission" => Key::MissionControl,
        "launchpad" => Key::Launchpad,
        "illuminationup" => Key::IlluminationUp,
        "illuminationdown" => Key::IlluminationDown,
        "illuminationtoggle" => Key::IlluminationToggle,
        "contrastup" => Key::ContrastUp,
        "contrastdown" => Key::ContrastDown,
        "power" => Key::Power,
        _ => return None,
    })
}

#[cfg(not(target_os = "macos"))]
fn mac_only_key(_norm: &str) -> Option<Key> {
    None
}

/// 解析一个或多个按键。
///
/// 每一项既可以是一个按键名，也可以是 `ctrl+shift+s` 这样的组合；
/// 返回的按键会被视为**同一个组合**一起按下。
pub fn parse_key_group(specs: &[String]) -> DeskResult<Vec<Key>> {
    let mut out = Vec::new();
    for spec in specs {
        let spec = spec.trim();
        if spec.is_empty() {
            continue;
        }
        // `+` 本身就是一个按键
        if spec == "+" {
            out.push(Key::Unicode('+'));
            continue;
        }
        for part in spec.split('+') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            out.push(parse_key(part)?);
        }
    }
    if out.is_empty() {
        return Err(DeskError::InvalidArgument(
            "没有解析出任何按键，请检查 `keys` 参数".into(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_chars() {
        assert!(matches!(parse_key("a").unwrap(), Key::Unicode('a')));
        assert!(matches!(parse_key("Z").unwrap(), Key::Unicode('Z')));
        assert!(matches!(parse_key("5").unwrap(), Key::Unicode('5')));
        assert!(matches!(parse_key("-").unwrap(), Key::Unicode('-')));
        assert!(matches!(parse_key("/").unwrap(), Key::Unicode('/')));
    }

    #[test]
    fn named() {
        assert!(matches!(parse_key("Enter").unwrap(), Key::Return));
        assert!(matches!(parse_key("return").unwrap(), Key::Return));
        assert!(matches!(parse_key("page up").unwrap(), Key::PageUp));
        assert!(matches!(parse_key("PAGE_DOWN").unwrap(), Key::PageDown));
        assert!(matches!(parse_key("ctrl").unwrap(), Key::Control));
        assert!(matches!(parse_key("Cmd").unwrap(), Key::Meta));
        assert!(matches!(parse_key("F11").unwrap(), Key::F11));
        assert!(matches!(parse_key("f20").unwrap(), Key::F20));
        assert!(matches!(parse_key("numpad-3").unwrap(), Key::Numpad3));
        assert!(matches!(parse_key("Esc").unwrap(), Key::Escape));
        assert!(matches!(parse_key("space").unwrap(), Key::Space));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn platform_keys_win_linux() {
        assert!(matches!(parse_key("Insert").unwrap(), Key::Insert));
        assert!(matches!(parse_key("PrintScreen").unwrap(), Key::PrintScr));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn platform_keys_macos() {
        assert!(matches!(parse_key("BrightnessUp").unwrap(), Key::BrightnessUp));
        // 这些键在 macOS 上不可用，应给出明确错误
        assert!(parse_key("Insert").is_err());
        assert!(parse_key("PrintScreen").is_err());
    }

    #[test]
    fn combos() {
        let keys = parse_key_group(&["ctrl+shift+s".to_string()]).unwrap();
        assert_eq!(keys.len(), 3);
        assert!(matches!(keys[0], Key::Control));
        assert!(matches!(keys[1], Key::Shift));
        assert!(matches!(keys[2], Key::Unicode('s')));

        let keys = parse_key_group(&["Cmd".to_string(), "Space".to_string()]).unwrap();
        assert_eq!(keys.len(), 2);
        assert!(matches!(keys[0], Key::Meta));
        assert!(matches!(keys[1], Key::Space));
    }

    #[test]
    fn rejects_unknown() {
        assert!(parse_key("nosuchkey").is_err());
        assert!(parse_key("").is_err());
        assert!(parse_key_group(&[]).is_err());
    }
}
