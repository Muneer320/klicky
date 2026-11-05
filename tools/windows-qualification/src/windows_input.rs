use std::collections::HashSet;
use std::mem::size_of;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::*;
use windows::Win32::UI::WindowsAndMessaging::*;

enum Event {
    Press(Instant),
    Stop,
    Failure(String),
}

#[derive(Default, Serialize)]
pub struct Stats {
    pub initial_presses: u64,
    pub repeats_ignored: u64,
    pub dropped_events: u64,
    pub device_changes: u64,
    pub input_errors: u64,
}

struct State {
    sender: SyncSender<Event>,
    pressed: HashSet<(usize, u16, u16)>,
    stats: Stats,
}

impl State {
    fn send(&mut self, event: Event) {
        if let Err(TrySendError::Full(_)) = self.sender.try_send(event) {
            self.stats.dropped_events += 1;
        }
    }
}

pub struct Input {
    receiver: Receiver<Event>,
    window: usize,
    worker: Option<JoinHandle<Result<Stats>>>,
}

impl Input {
    pub fn start() -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(256);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || run(sender, ready_tx));
        let window = ready_rx
            .recv_timeout(Duration::from_secs(5))
            .context("Raw Input did not become ready")?
            .map_err(anyhow::Error::msg)?;
        Ok(Self {
            receiver,
            window,
            worker: Some(worker),
        })
    }

    pub fn receive(&self, timeout: Duration) -> Result<Option<Instant>> {
        match self.receiver.recv_timeout(timeout) {
            Ok(Event::Press(at)) => Ok(Some(at)),
            Ok(Event::Stop) | Err(RecvTimeoutError::Timeout) => Ok(None),
            Ok(Event::Failure(error)) => bail!("Raw Input: {error}"),
            Err(RecvTimeoutError::Disconnected) => bail!("Raw Input thread disconnected"),
        }
    }

    pub fn stop(&mut self) -> Result<Stats> {
        let Some(worker) = self.worker.take() else {
            bail!("input already stopped");
        };
        if !worker.is_finished() {
            unsafe {
                PostMessageW(
                    Some(HWND(self.window as *mut _)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                )?;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while !worker.is_finished() {
            if Instant::now() >= deadline {
                bail!("Raw Input shutdown timed out");
            }
            thread::sleep(Duration::from_millis(2));
        }
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("Raw Input thread panicked"))?
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        if self.worker.is_some() {
            let _ = self.stop();
        }
    }
}

struct Window(HWND);
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(Some(self.0)).as_bool() {
                let _ = DestroyWindow(self.0);
            }
        }
    }
}

fn run(sender: SyncSender<Event>, ready: SyncSender<Result<usize, String>>) -> Result<Stats> {
    let mut state = Box::new(State {
        sender,
        pressed: HashSet::new(),
        stats: Stats::default(),
    });
    let window = unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            hInstance: instance.into(),
            lpszClassName: w!("KlickyQualificationInput"),
            lpfnWndProc: Some(window_proc),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            bail!("cannot register qualification window class");
        }
        let window = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class.lpszClassName,
            w!("Klicky qualification"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            Some((&mut *state as *mut State).cast()),
        )?;
        let devices = [RAWINPUTDEVICE {
            usUsagePage: 0x01,
            usUsage: 0x06,
            dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
            hwndTarget: window,
        }];
        if let Err(error) = RegisterRawInputDevices(&devices, size_of::<RAWINPUTDEVICE>() as u32) {
            let _ = DestroyWindow(window);
            let _ = ready.send(Err(error.to_string()));
            return Err(error.into());
        }
        window
    };
    let window_guard = Window(window);
    if ready.send(Ok(window.0 as usize)).is_err() {
        bail!("qualification caller left before input readiness");
    }
    let mut message = MSG::default();
    loop {
        let status = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
        if status == -1 {
            bail!("Raw Input message loop failed");
        }
        if status == 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    // Window callbacks must finish before any State fields are moved/dropped.
    drop(window_guard);
    Ok(state.stats)
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // No Rust panic may cross the OS callback boundary.
    let result = std::panic::catch_unwind(|| unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut State;
        if !pointer.is_null() {
            let state = &mut *pointer;
            if message == WM_INPUT {
                let at = Instant::now();
                if let Err(error) = raw_event(state, lparam, at) {
                    state.stats.input_errors += 1;
                    state.send(Event::Failure(error.to_string()));
                }
            } else if message == WM_INPUT_DEVICE_CHANGE {
                // Clear per-device pressed state rather than retaining lost breaks.
                let removed = lparam.0 as usize;
                state.pressed.retain(|(device, _, _)| *device != removed);
                state.stats.device_changes += 1;
            }
        }
        match message {
            WM_CLOSE => {
                let _ = DestroyWindow(window);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    });
    result.unwrap_or_else(|_| unsafe {
        PostQuitMessage(1);
        DefWindowProcW(window, message, wparam, lparam)
    })
}

unsafe fn raw_event(state: &mut State, lparam: LPARAM, at: Instant) -> Result<()> {
    let mut raw = RAWINPUT::default();
    let mut size = size_of::<RAWINPUT>() as u32;
    let received = unsafe {
        GetRawInputData(
            HRAWINPUT(lparam.0 as *mut _),
            RID_INPUT,
            Some((&mut raw as *mut RAWINPUT).cast()),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if received == u32::MAX {
        bail!("GetRawInputData failed");
    }
    if raw.header.dwType != RIM_TYPEKEYBOARD.0 {
        return Ok(());
    }
    if received < (size_of::<RAWINPUTHEADER>() + size_of::<RAWKEYBOARD>()) as u32 {
        bail!("short keyboard packet");
    }
    let keyboard = unsafe { raw.data.keyboard };
    key_transition(state, raw.header.hDevice.0 as usize, &keyboard, at);
    Ok(())
}

fn key_transition(state: &mut State, device: usize, keyboard: &RAWKEYBOARD, at: Instant) {
    // Observe only the declared test trigger and exit control. Never translate text.
    if keyboard.VKey == 0x1b {
        if keyboard.Flags & RI_KEY_BREAK as u16 == 0 {
            state.send(Event::Stop);
        }
        return;
    }
    if keyboard.VKey != 0x20 || keyboard.MakeCode == 0xff {
        return;
    }
    let identity = (
        device,
        keyboard.MakeCode,
        keyboard.Flags & (RI_KEY_E0 | RI_KEY_E1) as u16,
    );
    if keyboard.Flags & RI_KEY_BREAK as u16 != 0 {
        state.pressed.remove(&identity);
    } else if state.pressed.insert(identity) {
        state.stats.initial_presses += 1;
        state.send(Event::Press(at));
    } else {
        state.stats.repeats_ignored += 1;
    }
}
