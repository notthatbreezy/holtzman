//! Synthesized keyboard input.
//!
//! On Windows this uses `SendInput` with hardware scan codes, which is what
//! the game listens to. Everywhere else (and in tests) a recorder stands in so
//! the step logic can be exercised without a game.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;

use crate::keys::ScanCode;

/// Something that can press keys and set the clipboard.
pub trait KeySender: Send + Sync {
    fn key_down(&self, key: ScanCode) -> Result<()>;
    fn key_up(&self, key: ScanCode) -> Result<()>;
    fn set_clipboard(&self, text: &str) -> Result<()>;
    /// Bring the game window to the foreground. Keystrokes go to whatever
    /// window has focus, so this matters.
    fn focus_game(&self) -> Result<()>;
}

/// Press a chord: modifiers down, key down, hold, key up, modifiers up.
pub async fn press(
    sender: &dyn KeySender,
    key: ScanCode,
    modifiers: &[ScanCode],
    hold: Duration,
) -> Result<()> {
    for m in modifiers {
        sender.key_down(*m)?;
    }
    if !modifiers.is_empty() {
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    sender.key_down(key)?;
    tokio::time::sleep(hold).await;
    sender.key_up(key)?;
    for m in modifiers.iter().rev() {
        sender.key_up(*m)?;
    }
    Ok(())
}

/// Records calls instead of sending them. Used off-Windows and in tests.
#[derive(Default, Clone)]
pub struct Recorder {
    pub log: Arc<Mutex<Vec<String>>>,
}

impl Recorder {
    pub fn entries(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
    fn push(&self, s: String) {
        tracing::info!(target: "holtzman::dry_run", "{s}");
        self.log.lock().unwrap().push(s);
    }
}

impl KeySender for Recorder {
    fn key_down(&self, key: ScanCode) -> Result<()> {
        self.push(format!("down {:#04x}", key.code));
        Ok(())
    }
    fn key_up(&self, key: ScanCode) -> Result<()> {
        self.push(format!("up {:#04x}", key.code));
        Ok(())
    }
    fn set_clipboard(&self, text: &str) -> Result<()> {
        self.push(format!("clipboard {text}"));
        Ok(())
    }
    fn focus_game(&self) -> Result<()> {
        self.push("focus".into());
        Ok(())
    }
}

/// The real sender for the current platform, or a recorder in dry-run mode.
pub fn platform_sender(dry_run: bool) -> Arc<dyn KeySender> {
    if dry_run {
        return Arc::new(Recorder::default());
    }
    #[cfg(windows)]
    {
        Arc::new(windows::WindowsSender)
    }
    #[cfg(not(windows))]
    {
        tracing::warn!("not on Windows: keypresses will only be logged (dry run)");
        Arc::new(Recorder::default())
    }
}

#[cfg(windows)]
mod windows {
    use anyhow::{Result, anyhow, bail};
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
            KEYEVENTF_SCANCODE, SendInput,
        },
        WindowsAndMessaging::{FindWindowW, GetForegroundWindow, SetForegroundWindow},
    };

    use crate::keys::ScanCode;

    /// Window class of the Elite Dangerous client.
    const GAME_WINDOW_CLASS: &str = "FrontierDevelopmentsAppWinClass";

    pub struct WindowsSender;

    fn send(key: ScanCode, up: bool) -> Result<()> {
        let mut flags = KEYEVENTF_SCANCODE;
        if key.extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: key.code,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        // SAFETY: one fully initialized INPUT, correct size.
        let sent = unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
        if sent != 1 {
            bail!(
                "SendInput failed (blocked by UIPI? run holtzman at the same privilege level as the game)"
            );
        }
        Ok(())
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    impl super::KeySender for WindowsSender {
        fn key_down(&self, key: ScanCode) -> Result<()> {
            send(key, false)
        }

        fn key_up(&self, key: ScanCode) -> Result<()> {
            send(key, true)
        }

        fn set_clipboard(&self, text: &str) -> Result<()> {
            let mut cb = arboard::Clipboard::new().map_err(|e| anyhow!("clipboard: {e}"))?;
            cb.set_text(text.to_string())
                .map_err(|e| anyhow!("clipboard: {e}"))
        }

        fn focus_game(&self) -> Result<()> {
            let class = wide(GAME_WINDOW_CLASS);
            // SAFETY: null-terminated UTF-16 class name; null window name.
            let hwnd = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
            if hwnd.is_null() {
                bail!("Elite Dangerous window not found; is the game running?");
            }
            // SAFETY: plain Win32 calls on a valid window handle.
            unsafe {
                if GetForegroundWindow() == hwnd {
                    return Ok(());
                }
                // Windows only lets the process that received the last input
                // event steal focus; a synthetic Alt tap satisfies that rule.
                let alt = ScanCode {
                    code: 0x38,
                    extended: false,
                };
                send(alt, false)?;
                send(alt, true)?;
                SetForegroundWindow(hwnd);
                std::thread::sleep(std::time::Duration::from_millis(150));
                if GetForegroundWindow() != hwnd {
                    bail!(
                        "could not bring the game window to the foreground; click into the game and retry"
                    );
                }
            }
            Ok(())
        }
    }
}
