//! Remote input injection.
//!
//! Mouse coordinates are normalized 0..=1 over the virtual desktop captured by
//! `rv-capture`; they are mapped to Windows absolute coordinates with the
//! `VIRTUALDESK` flag so multi-monitor layouts stay aligned.
//!
//! Keyboard events carry DOM `KeyboardEvent.code` strings, which are stable
//! across layouts, and are translated to Windows virtual-key codes. IME text
//! travels separately as `InputEvent::Text` and is injected as Unicode.

use rv_proto::InputEvent;
use tracing::warn;

#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("input injection is only implemented on Windows in this build")]
    Unsupported,
    #[error("unknown key code: {0}")]
    UnknownKey(String),
    #[error("SendInput failed with Win32 error {0}")]
    SendInput(u32),
}

/// Applies one protocol input event.
pub fn apply(event: &InputEvent) -> Result<(), InputError> {
    match event {
        InputEvent::MouseMove { x, y } => mouse_move_normalized(*x, *y),
        InputEvent::MouseButton { button, pressed } => mouse_button(*button, *pressed),
        InputEvent::Wheel { delta_x, delta_y } => wheel(*delta_x, *delta_y),
        InputEvent::Key { code, pressed } => {
            let vk = vk_for_dom_code(code).ok_or_else(|| InputError::UnknownKey(code.clone()))?;
            key_event(vk, *pressed)
        }
        InputEvent::Text { text } => unicode_text(text),
        InputEvent::Clipboard { .. } => Ok(()),
    }
}

/// Maps a DOM `KeyboardEvent.code` to a Windows virtual-key code.
pub fn vk_for_dom_code(code: &str) -> Option<u16> {
    let vk = match code {
        "KeyA" => 0x41,
        "KeyB" => 0x42,
        "KeyC" => 0x43,
        "KeyD" => 0x44,
        "KeyE" => 0x45,
        "KeyF" => 0x46,
        "KeyG" => 0x47,
        "KeyH" => 0x48,
        "KeyI" => 0x49,
        "KeyJ" => 0x4A,
        "KeyK" => 0x4B,
        "KeyL" => 0x4C,
        "KeyM" => 0x4D,
        "KeyN" => 0x4E,
        "KeyO" => 0x4F,
        "KeyP" => 0x50,
        "KeyQ" => 0x51,
        "KeyR" => 0x52,
        "KeyS" => 0x53,
        "KeyT" => 0x54,
        "KeyU" => 0x55,
        "KeyV" => 0x56,
        "KeyW" => 0x57,
        "KeyX" => 0x58,
        "KeyY" => 0x59,
        "KeyZ" => 0x5A,
        "Digit0" => 0x30,
        "Digit1" => 0x31,
        "Digit2" => 0x32,
        "Digit3" => 0x33,
        "Digit4" => 0x34,
        "Digit5" => 0x35,
        "Digit6" => 0x36,
        "Digit7" => 0x37,
        "Digit8" => 0x38,
        "Digit9" => 0x39,
        "F1" => 0x70,
        "F2" => 0x71,
        "F3" => 0x72,
        "F4" => 0x73,
        "F5" => 0x74,
        "F6" => 0x75,
        "F7" => 0x76,
        "F8" => 0x77,
        "F9" => 0x78,
        "F10" => 0x79,
        "F11" => 0x7A,
        "F12" => 0x7B,
        "Escape" => 0x1B,
        "Enter" | "NumpadEnter" => 0x0D,
        "Tab" => 0x09,
        "Space" => 0x20,
        "Backspace" => 0x08,
        "Delete" => 0x2E,
        "Insert" => 0x2D,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "ShiftLeft" | "ShiftRight" => 0x10,
        "ControlLeft" | "ControlRight" => 0x11,
        "AltLeft" | "AltRight" => 0x12,
        "MetaLeft" | "MetaRight" => 0x5B,
        "CapsLock" => 0x14,
        "NumLock" => 0x90,
        "ScrollLock" => 0x91,
        "PrintScreen" => 0x2C,
        "Pause" => 0x13,
        "ContextMenu" => 0x5D,
        "Minus" => 0xBD,
        "Equal" => 0xBB,
        "BracketLeft" => 0xDB,
        "BracketRight" => 0xDD,
        "Backslash" => 0xDC,
        "Semicolon" => 0xBA,
        "Quote" => 0xDE,
        "Backquote" => 0xC0,
        "Comma" => 0xBC,
        "Period" => 0xBE,
        "Slash" => 0xBF,
        "IntlBackslash" => 0xE2,
        "Numpad0" => 0x60,
        "Numpad1" => 0x61,
        "Numpad2" => 0x62,
        "Numpad3" => 0x63,
        "Numpad4" => 0x64,
        "Numpad5" => 0x65,
        "Numpad6" => 0x66,
        "Numpad7" => 0x67,
        "Numpad8" => 0x68,
        "Numpad9" => 0x69,
        "NumpadMultiply" => 0x6A,
        "NumpadAdd" => 0x6B,
        "NumpadSubtract" => 0x6D,
        "NumpadDecimal" => 0x6E,
        "NumpadDivide" => 0x6F,
        _ => return None,
    };
    Some(vk)
}

#[cfg(windows)]
mod platform {
    use std::mem::{size_of, zeroed};

    use rv_proto::MouseButton;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOUSEEVENTF_ABSOLUTE,
        MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
        MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT,
        MapVirtualKeyW, SendInput,
    };

    const XBUTTON1: u32 = 0x0001;
    const XBUTTON2: u32 = 0x0002;

    use super::InputError;

    fn send(input: INPUT) -> Result<(), InputError> {
        let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
        if sent != 1 {
            return Err(InputError::SendInput(unsafe {
                windows_sys::Win32::Foundation::GetLastError()
            }));
        }
        Ok(())
    }

    fn send_mouse(dx: i32, dy: i32, mouse_data: u32, flags: u32) -> Result<(), InputError> {
        let mut input: INPUT = unsafe { zeroed() };
        input.r#type = INPUT_MOUSE;
        input.Anonymous = INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: mouse_data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        };
        send(input)
    }

    pub fn mouse_move_normalized(x: f64, y: f64) -> Result<(), InputError> {
        let x = (x.clamp(0.0, 1.0) * 65535.0).round() as i32;
        let y = (y.clamp(0.0, 1.0) * 65535.0).round() as i32;
        send_mouse(
            x,
            y,
            0,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        )
    }

    pub fn mouse_button(button: MouseButton, pressed: bool) -> Result<(), InputError> {
        let (down, up, data) = match button {
            MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
            MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
            MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
            MouseButton::Back => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, XBUTTON1),
            MouseButton::Forward => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, XBUTTON2),
        };
        send_mouse(0, 0, data, if pressed { down } else { up })
    }

    /// `delta_x`/`delta_y` are wheel notches (one notch = 120 units).
    pub fn wheel(delta_x: f64, delta_y: f64) -> Result<(), InputError> {
        let vertical = (delta_y * 120.0).round() as i32;
        if vertical != 0 {
            send_mouse(0, 0, vertical as u32, MOUSEEVENTF_WHEEL)?;
        }
        let horizontal = (delta_x * 120.0).round() as i32;
        if horizontal != 0 {
            send_mouse(0, 0, horizontal as u32, MOUSEEVENTF_HWHEEL)?;
        }
        Ok(())
    }

    fn is_extended(vk: u16) -> bool {
        matches!(vk, 0x21..=0x2E | 0x5B | 0x5C | 0x5D | 0x6F | 0x90)
    }

    pub fn key_event(vk: u16, pressed: bool) -> Result<(), InputError> {
        let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
        let mut flags = 0u32;
        if !pressed {
            flags |= KEYEVENTF_KEYUP;
        }
        if is_extended(vk) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        let mut input: INPUT = unsafe { zeroed() };
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous = INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        };
        send(input)
    }

    pub fn unicode_text(text: &str) -> Result<(), InputError> {
        for unit in text.encode_utf16() {
            for pressed in [true, false] {
                let mut flags = KEYEVENTF_UNICODE;
                if !pressed {
                    flags |= KEYEVENTF_KEYUP;
                }
                let mut input: INPUT = unsafe { zeroed() };
                input.r#type = INPUT_KEYBOARD;
                input.Anonymous = INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: 0,
                        wScan: unit,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                };
                send(input)?;
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod platform {
    use rv_proto::MouseButton;

    use super::InputError;

    pub fn mouse_move_normalized(_x: f64, _y: f64) -> Result<(), InputError> {
        Err(InputError::Unsupported)
    }
    pub fn mouse_button(_button: MouseButton, _pressed: bool) -> Result<(), InputError> {
        Err(InputError::Unsupported)
    }
    pub fn wheel(_dx: f64, _dy: f64) -> Result<(), InputError> {
        Err(InputError::Unsupported)
    }
    pub fn key_event(_vk: u16, _pressed: bool) -> Result<(), InputError> {
        Err(InputError::Unsupported)
    }
    pub fn unicode_text(_text: &str) -> Result<(), InputError> {
        Err(InputError::Unsupported)
    }
}

pub use platform::{key_event, mouse_button, mouse_move_normalized, unicode_text, wheel};

/// Convenience wrapper used by the session engine.
pub fn inject(event: &InputEvent) {
    if let Err(error) = apply(event) {
        warn!(?error, "input injection failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_digits_and_navigation() {
        assert_eq!(vk_for_dom_code("KeyA"), Some(0x41));
        assert_eq!(vk_for_dom_code("KeyZ"), Some(0x5A));
        assert_eq!(vk_for_dom_code("Digit0"), Some(0x30));
        assert_eq!(vk_for_dom_code("ArrowLeft"), Some(0x25));
        assert_eq!(vk_for_dom_code("ControlLeft"), Some(0x11));
        assert_eq!(vk_for_dom_code("F12"), Some(0x7B));
        assert_eq!(vk_for_dom_code("NumpadDivide"), Some(0x6F));
        assert_eq!(vk_for_dom_code("NotAKey"), None);
    }
}
