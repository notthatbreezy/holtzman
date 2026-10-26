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

/// Where the mouse is, relative to the game window.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct CursorPos {
    /// 0.0 = left edge of the game window, 1.0 = right edge.
    pub x: f64,
    /// 0.0 = top edge of the game window, 1.0 = bottom edge.
    pub y: f64,
    pub x_px: i32,
    pub y_px: i32,
    pub window_width: i32,
    pub window_height: i32,
}

/// Something that can press keys, click, and set the clipboard.
pub trait KeySender: Send + Sync {
    fn key_down(&self, key: ScanCode) -> Result<()>;
    fn key_up(&self, key: ScanCode) -> Result<()>;
    fn set_clipboard(&self, text: &str) -> Result<()>;
    /// Bring the game window to the foreground. Keystrokes go to whatever
    /// window has focus, so this matters.
    fn focus_game(&self) -> Result<()>;
    /// Move the mouse to a point given as fractions of the game window.
    fn move_mouse(&self, x: f64, y: f64) -> Result<()>;
    fn left_button(&self, down: bool) -> Result<()>;
    /// Current mouse position relative to the game window.
    fn cursor(&self) -> Result<CursorPos>;
}

/// Move to (x, y) in game-window fractions and left-click.
pub async fn click(sender: &dyn KeySender, x: f64, y: f64, hold: Duration) -> Result<()> {
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        anyhow::bail!("click target ({x}, {y}) is outside the game window");
    }
    sender.move_mouse(x, y)?;
    // Give the UI a moment to register hover before pressing.
    tokio::time::sleep(Duration::from_millis(120)).await;
    sender.left_button(true)?;
    tokio::time::sleep(hold).await;
    sender.left_button(false)?;
    Ok(())
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
    fn move_mouse(&self, x: f64, y: f64) -> Result<()> {
        self.push(format!("move {x:.3},{y:.3}"));
        Ok(())
    }
    fn left_button(&self, down: bool) -> Result<()> {
        self.push(if down { "click down" } else { "click up" }.into());
        Ok(())
    }
    fn cursor(&self) -> Result<CursorPos> {
        Ok(CursorPos {
            x: 0.5,
            y: 0.5,
            x_px: 960,
            y_px: 540,
            window_width: 1920,
            window_height: 1080,
        })
    }
}

/// The real sender for the current platform, or a recorder in dry-run mode.
pub fn platform_sender(dry_run: bool) -> Arc<dyn KeySender> {
    if dry_run {
        return Arc::new(Recorder::default());
    }
    #[cfg(windows)]
    {
        Arc::new(windows::WindowsSender::new())
    }
    #[cfg(not(windows))]
    {
        tracing::warn!("not on Windows: keypresses will only be logged (dry run)");
        Arc::new(Recorder::default())
    }
}

#[cfg(windows)]
pub(crate) mod windows {
    use anyhow::{Result, anyhow, bail};
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
            KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
            MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
        },
        WindowsAndMessaging::{FindWindowW, GetForegroundWindow, SetForegroundWindow},
    };
    use windows_sys::Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::Gdi::ClientToScreen,
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            WindowsAndMessaging::{GetClientRect, GetCursorPos, SetCursorPos},
        },
    };

    use crate::keys::ScanCode;

    /// Window class of the Elite Dangerous client.
    const GAME_WINDOW_CLASS: &str = "FrontierDevelopmentsAppWinClass";

    pub struct WindowsSender;

    impl WindowsSender {
        pub fn new() -> Self {
            // Work in physical pixels so cursor and window coordinates agree
            // on scaled (125%/150%) displays.
            // SAFETY: process-wide setting; fails harmlessly if already set.
            unsafe {
                SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            }
            Self
        }
    }

    pub(crate) fn game_window() -> Result<HWND> {
        let class = wide(GAME_WINDOW_CLASS);
        // SAFETY: null-terminated UTF-16 class name; null window name.
        let hwnd = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
        if hwnd.is_null() {
            bail!("Elite Dangerous window not found; is the game running?");
        }
        Ok(hwnd)
    }

    /// Screen-space origin and size of the game window's client area.
    pub(crate) fn client_area(hwnd: HWND) -> Result<(i32, i32, i32, i32)> {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut origin = POINT { x: 0, y: 0 };
        // SAFETY: valid window handle and out-pointers to initialized structs.
        unsafe {
            if GetClientRect(hwnd, &mut rect) == 0 || ClientToScreen(hwnd, &mut origin) == 0 {
                bail!("could not read the game window's size");
            }
        }
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        if w <= 0 || h <= 0 {
            bail!("game window has no size (minimized?)");
        }
        Ok((origin.x, origin.y, w, h))
    }

    fn send_mouse(flags: u32) -> Result<()> {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        // SAFETY: one fully initialized INPUT, correct size.
        let sent = unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
        if sent != 1 {
            bail!("SendInput (mouse) failed");
        }
        Ok(())
    }

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
            let hwnd = game_window()?;
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

        fn move_mouse(&self, x: f64, y: f64) -> Result<()> {
            let (ox, oy, w, h) = client_area(game_window()?)?;
            let px = ox + (x * f64::from(w)).round() as i32;
            let py = oy + (y * f64::from(h)).round() as i32;
            // SAFETY: plain Win32 call.
            if unsafe { SetCursorPos(px, py) } == 0 {
                bail!("SetCursorPos failed");
            }
            // A tiny relative wiggle so the game sees real mouse movement and
            // updates its hover state.
            let wiggle = |dx: i32| -> Result<()> {
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx,
                            dy: 0,
                            mouseData: 0,
                            dwFlags: MOUSEEVENTF_MOVE,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };
                // SAFETY: one fully initialized INPUT, correct size.
                unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
                Ok(())
            };
            wiggle(1)?;
            wiggle(-1)?;
            Ok(())
        }

        fn left_button(&self, down: bool) -> Result<()> {
            send_mouse(if down {
                MOUSEEVENTF_LEFTDOWN
            } else {
                MOUSEEVENTF_LEFTUP
            })
        }

        fn cursor(&self) -> Result<super::CursorPos> {
            let (ox, oy, w, h) = client_area(game_window()?)?;
            let mut p = POINT { x: 0, y: 0 };
            // SAFETY: out-pointer to an initialized POINT.
            if unsafe { GetCursorPos(&mut p) } == 0 {
                bail!("GetCursorPos failed");
            }
            let (x_px, y_px) = (p.x - ox, p.y - oy);
            Ok(super::CursorPos {
                x: f64::from(x_px) / f64::from(w),
                y: f64::from(y_px) / f64::from(h),
                x_px,
                y_px,
                window_width: w,
                window_height: h,
            })
        }
    }
}
