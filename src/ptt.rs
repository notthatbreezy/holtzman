//! Opt-in input observation. Never consumes input, records text, or sends keys.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub inputs: Vec<String>,
    pub tail_ms: u64,
    pub max_hold_s: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            inputs: vec![],
            tail_ms: 150,
            max_hold_s: 30,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.inputs.len() > 16 || self.tail_ms > 2000 || !(1..=120).contains(&self.max_hold_s) {
            return Err("PTT limits: 16 inputs, tail <=2000 ms, max hold 1..120 s".into());
        }
        for input in &self.inputs {
            validate_input(input)?;
        }
        Ok(())
    }
}
pub fn validate_input(input: &str) -> Result<(), String> {
    let valid = matches!(input, "Mouse4" | "Mouse5" | "MouseMiddle")
        || (input.strip_prefix("Key_").is_some_and(|k| {
            (k.starts_with('F') && k[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)))
                || matches!(
                    k,
                    "Insert"
                        | "Delete"
                        | "Home"
                        | "End"
                        | "PageUp"
                        | "PageDown"
                        | "Pause"
                        | "ScrollLock"
                )
        }))
        || input.strip_prefix("Joy:").is_some_and(|s| {
            s.rsplit_once(':').is_some_and(|(name, n)| {
                !name.trim().is_empty()
                    && name.len() <= 128
                    && n.parse::<u16>().is_ok_and(|n| (1..=128).contains(&n))
            })
        });
    if valid {
        Ok(())
    } else {
        Err("Choose Mouse4/5/Middle, a function/navigation key, or Joy:<HID product name>:<1-based button>; typing keys and modifiers are forbidden".into())
    }
}

struct State {
    config: Config,
    active: Option<String>,
    down: bool,
    since: Option<Instant>,
    modified: Option<SystemTime>,
    error: Option<String>,
    capture_error: Option<String>,
    learn: Option<Instant>,
    learned: Option<String>,
    blocked: bool,
}
pub struct Reader {
    state: Mutex<State>,
    pub events: Arc<crate::events::Bus>,
}
impl Reader {
    fn snapshot_of(s: &State) -> Value {
        json!({"input":s.active,"down":s.down,"tail_ms":s.config.tail_ms,"max_hold_s":s.config.max_hold_s,"error":s.error.as_ref().or(s.capture_error.as_ref()),"learning":s.learn.is_some_and(|t|t.elapsed()<Duration::from_secs(10)),"learned":s.learned})
    }
    pub fn new(config: Config, events: Arc<crate::events::Bus>) -> Self {
        let state = State {
            config,
            active: None,
            down: false,
            since: None,
            modified: None,
            error: None,
            capture_error: None,
            learn: None,
            learned: None,
            blocked: false,
        };
        events.snapshot(json!({"ptt":Self::snapshot_of(&state)}));
        Self {
            state: Mutex::new(state),
            events,
        }
    }
    pub fn snapshot(&self) -> Value {
        Self::snapshot_of(&self.state.lock().unwrap())
    }
    pub fn capture_error(&self, error: Option<String>) {
        let mut state = self.state.lock().unwrap();
        if state.capture_error != error {
            state.capture_error = error;
            self.events.publish(
                "input",
                "snapshot",
                json!({"ptt":Self::snapshot_of(&state)}),
            );
        }
    }
    pub fn configured(&self) -> Vec<String> {
        self.state.lock().unwrap().config.inputs.clone()
    }
    pub fn select(&self, connected: &[String]) {
        let mut s = self.state.lock().unwrap();
        let active = s
            .config
            .inputs
            .iter()
            .find(|i| {
                connected.iter().any(|c| {
                    *i == c
                        || i.strip_prefix("Joy:")
                            .and_then(|i| i.rsplit_once(':'))
                            .is_some_and(|(name, _)| c == &format!("Joy:{name}"))
                })
            })
            .cloned();
        if active != s.active {
            s.active = active;
            s.down = false;
            s.since = None;
            s.blocked = false;
            self.events
                .publish("input", "snapshot", json!({"ptt":Self::snapshot_of(&s)}));
        }
    }
    pub fn edge(&self, input: &str, down: bool) {
        let mut s = self.state.lock().unwrap();
        if down
            && s.learn
                .is_some_and(|t| t.elapsed() < Duration::from_secs(10))
            && validate_input(input).is_ok()
        {
            s.learned = Some(input.into());
            s.learn = None;
        }
        if s.active.as_deref() != Some(input) {
            return;
        }
        if !down {
            s.blocked = false;
        }
        if s.down == down || down && s.blocked {
            return;
        }
        s.down = down;
        s.since = if down { Some(Instant::now()) } else { None };
        self.events
            .publish("input", "ptt", json!({"down":down,"input":input}));
    }
    pub fn learn(&self) {
        let mut s = self.state.lock().unwrap();
        s.learn = Some(Instant::now());
        s.learned = None;
    }
    pub fn learning(&self) -> bool {
        self.state
            .lock()
            .unwrap()
            .learn
            .is_some_and(|t| t.elapsed() < Duration::from_secs(10))
    }
    pub fn reload(&self, ship: &crate::server::Ship) {
        let Some(path) = &ship.config_path else {
            return;
        };
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let mut s = self.state.lock().unwrap();
        if s.down
            && s.since
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(s.config.max_hold_s))
        {
            s.down = false;
            s.since = None;
            s.blocked = true;
            self.events.publish(
                "input",
                "ptt",
                json!({"down":false,"reason":"maximum hold"}),
            );
        }
        if modified == s.modified {
            return;
        }
        s.modified = modified;
        match crate::config::Config::load(path)
            .map_err(|e| e.to_string())
            .and_then(|c| {
                c.ptt.validate()?;
                Ok(c.ptt)
            }) {
            Ok(config) => {
                s.config = config;
                s.error = None;
            }
            Err(error) => {
                s.error = Some(error);
            }
        }
        self.events
            .publish("input", "snapshot", json!({"ptt":Self::snapshot_of(&s)}));
    }
}

#[cfg(windows)]
pub fn start(reader: Arc<Reader>) {
    std::thread::spawn(move || windows::run(reader));
}
#[cfg(not(windows))]
pub fn start(_reader: Arc<Reader>) {}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        mem::size_of,
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Devices::HumanInterfaceDevice::*,
        Foundation::*,
        Storage::FileSystem::*,
        UI::{Input::*, WindowsAndMessaging::*},
    };
    static READER: std::sync::OnceLock<Arc<Reader>> = std::sync::OnceLock::new();
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    static PRODUCTS: std::sync::OnceLock<Mutex<std::collections::HashMap<usize, String>>> =
        std::sync::OnceLock::new();
    unsafe fn product(device: HANDLE) -> String {
        unsafe {
            let cache = PRODUCTS.get_or_init(Default::default);
            if let Some(name) = cache.lock().unwrap().get(&(device as usize)).cloned() {
                return name;
            }
            let mut n = 0;
            GetRawInputDeviceInfoW(device, RIDI_DEVICENAME, null_mut(), &mut n);
            let mut path = vec![0u16; n as usize + 1];
            GetRawInputDeviceInfoW(device, RIDI_DEVICENAME, path.as_mut_ptr().cast(), &mut n);
            let file = CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            );
            if file == INVALID_HANDLE_VALUE {
                return String::new();
            }
            let mut name = [0u16; 256];
            let ok = HidD_GetProductString(file, name.as_mut_ptr().cast(), 512);
            CloseHandle(file);
            let name = if ok {
                String::from_utf16_lossy(&name[..name.iter().position(|c| *c == 0).unwrap_or(256)])
            } else {
                String::new()
            };
            cache.lock().unwrap().insert(device as usize, name.clone());
            name
        }
    }
    unsafe fn connected() -> Vec<String> {
        unsafe {
            let mut n = 0;
            GetRawInputDeviceList(null_mut(), &mut n, size_of::<RAWINPUTDEVICELIST>() as u32);
            let mut devices = vec![RAWINPUTDEVICELIST::default(); n as usize];
            GetRawInputDeviceList(
                devices.as_mut_ptr(),
                &mut n,
                size_of::<RAWINPUTDEVICELIST>() as u32,
            );
            let mut names = Vec::new();
            for d in devices {
                match d.dwType {
                    RIM_TYPEMOUSE => {
                        names.extend(["Mouse4", "Mouse5", "MouseMiddle"].map(String::from))
                    }
                    RIM_TYPEKEYBOARD => {
                        for n in 1..=24 {
                            names.push(format!("Key_F{n}"));
                        }
                        for name in [
                            "Insert",
                            "Delete",
                            "Home",
                            "End",
                            "PageUp",
                            "PageDown",
                            "Pause",
                            "ScrollLock",
                        ] {
                            names.push(format!("Key_{name}"));
                        }
                    }
                    RIM_TYPEHID => {
                        let name = product(d.hDevice);
                        if !name.is_empty() {
                            names.push(format!("Joy:{name}"));
                        }
                    }
                    _ => {}
                }
            }
            names
        }
    }
    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe {
            if msg == WM_INPUT_DEVICE_CHANGE
                && let Some(cache) = PRODUCTS.get()
            {
                cache.lock().unwrap().remove(&(l as usize));
            }
            if msg == WM_INPUT
                && let Some(reader) = READER.get()
            {
                let mut n = 0;
                GetRawInputData(
                    l as HRAWINPUT,
                    RID_INPUT,
                    null_mut(),
                    &mut n,
                    size_of::<RAWINPUTHEADER>() as u32,
                );
                if n as usize >= size_of::<RAWINPUTHEADER>() && n < 1024 * 1024 {
                    let mut data = vec![0u64; (n as usize).max(size_of::<RAWINPUT>()).div_ceil(8)];
                    if GetRawInputData(
                        l as HRAWINPUT,
                        RID_INPUT,
                        data.as_mut_ptr().cast(),
                        &mut n,
                        size_of::<RAWINPUTHEADER>() as u32,
                    ) != u32::MAX
                    {
                        let raw = &*(data.as_ptr() as *const RAWINPUT);
                        match raw.header.dwType {
                            RIM_TYPEMOUSE
                                if n as usize
                                    >= size_of::<RAWINPUTHEADER>() + size_of::<RAWMOUSE>() =>
                            {
                                let flags = raw.data.mouse.Anonymous.Anonymous.usButtonFlags;
                                for (name, d, u) in [
                                    ("MouseMiddle", 0x10, 0x20),
                                    ("Mouse4", 0x40, 0x80),
                                    ("Mouse5", 0x100, 0x200),
                                ] {
                                    if flags & d != 0 {
                                        reader.edge(name, true);
                                    }
                                    if flags & u != 0 {
                                        reader.edge(name, false);
                                    }
                                }
                            }
                            RIM_TYPEKEYBOARD
                                if n as usize
                                    >= size_of::<RAWINPUTHEADER>() + size_of::<RAWKEYBOARD>() =>
                            {
                                let k = raw.data.keyboard;
                                let name = match k.VKey {
                                    0x70..=0x87 => format!("Key_F{}", k.VKey - 0x70 + 1),
                                    0x21 => "Key_PageUp".into(),
                                    0x22 => "Key_PageDown".into(),
                                    0x23 => "Key_End".into(),
                                    0x24 => "Key_Home".into(),
                                    0x2d => "Key_Insert".into(),
                                    0x2e => "Key_Delete".into(),
                                    0x13 => "Key_Pause".into(),
                                    0x91 => "Key_ScrollLock".into(),
                                    _ => String::new(),
                                };
                                if !name.is_empty() {
                                    reader.edge(&name, k.Flags & 1 == 0);
                                }
                            }
                            RIM_TYPEHID if n as usize >= size_of::<RAWINPUTHEADER>() + 8 => {
                                let name = product(raw.header.hDevice);
                                let mut length = 0;
                                GetRawInputDeviceInfoW(
                                    raw.header.hDevice,
                                    RIDI_PREPARSEDDATA,
                                    null_mut(),
                                    &mut length,
                                );
                                let mut prep = vec![0u64; (length as usize).div_ceil(8)];
                                if length > 0
                                    && GetRawInputDeviceInfoW(
                                        raw.header.hDevice,
                                        RIDI_PREPARSEDDATA,
                                        prep.as_mut_ptr().cast(),
                                        &mut length,
                                    ) != u32::MAX
                                {
                                    let hid = &raw.data.hid;
                                    let reports =
                                        (hid.dwSizeHid as usize).checked_mul(hid.dwCount as usize);
                                    let start =
                                        hid.bRawData.as_ptr() as usize - data.as_ptr() as usize;
                                    if hid.dwSizeHid > 0
                                        && reports
                                            .and_then(|len| start.checked_add(len))
                                            .is_some_and(|end| end <= n as usize)
                                    {
                                        for index in 0..hid.dwCount as usize {
                                            let report = hid
                                                .bRawData
                                                .as_ptr()
                                                .add(index * hid.dwSizeHid as usize);
                                            let mut usages = [0u16; 128];
                                            let mut count = 128;
                                            if HidP_GetUsages(
                                                HidP_Input,
                                                9,
                                                0,
                                                usages.as_mut_ptr(),
                                                &mut count,
                                                prep.as_ptr() as isize,
                                                report as *mut u8,
                                                hid.dwSizeHid,
                                            ) >= 0
                                            {
                                                for button in 1..=128 {
                                                    reader.edge(
                                                        &format!("Joy:{name}:{button}"),
                                                        usages[..count.min(128) as usize]
                                                            .contains(&button),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            DefWindowProcW(hwnd, msg, w, l)
        }
    }
    pub fn run(reader: Arc<Reader>) {
        unsafe {
            if READER.set(reader.clone()).is_err() {
                return;
            }
            let class = wide("HoltzmanPTT");
            let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null());
            let wc = WNDCLASSW {
                lpfnWndProc: Some(proc),
                lpszClassName: class.as_ptr(),
                hInstance: instance,
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 {
                reader.capture_error(Some("PTT message-window registration failed".into()));
                return;
            }
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                instance,
                null(),
            );
            if hwnd.is_null() {
                reader.capture_error(Some("PTT message window unavailable".into()));
                return;
            }
            let mut registered = Vec::<u16>::new();
            let mut last_devices = Instant::now() - Duration::from_secs(2);
            loop {
                if last_devices.elapsed() >= Duration::from_secs(1) {
                    let connected = connected();
                    reader.select(&connected);
                    last_devices = Instant::now();
                }
                let snap = reader.snapshot();
                let input = snap["input"].as_str().unwrap_or("");
                let classes: Vec<u16> = if reader.learning() {
                    vec![2, 6, 4, 5]
                } else if input.starts_with("Mouse") {
                    vec![2]
                } else if input.starts_with("Key_") {
                    vec![6]
                } else if input.starts_with("Joy:") {
                    vec![4, 5]
                } else {
                    vec![]
                };
                if classes != registered {
                    for usage in &registered {
                        let d = RAWINPUTDEVICE {
                            usUsagePage: 1,
                            usUsage: *usage,
                            dwFlags: RIDEV_REMOVE,
                            hwndTarget: null_mut(),
                        };
                        RegisterRawInputDevices(&d, 1, size_of::<RAWINPUTDEVICE>() as u32);
                    }
                    registered.clear();
                    let wanted = classes.len();
                    for usage in classes {
                        let d = RAWINPUTDEVICE {
                            usUsagePage: 1,
                            usUsage: usage,
                            dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                            hwndTarget: hwnd,
                        };
                        if RegisterRawInputDevices(&d, 1, size_of::<RAWINPUTDEVICE>() as u32) != 0 {
                            registered.push(usage);
                        }
                    }
                    reader.capture_error(if registered.len() == wanted {
                        None
                    } else {
                        Some("PTT Raw Input registration failed".into())
                    });
                }
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
