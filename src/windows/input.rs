//! Passive physical-position keyboard input. No character conversion or key log.
mod keys;

use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ::windows::core::w;
use ::windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, WAIT_FAILED, WAIT_OBJECT_0,
    WAIT_TIMEOUT, WPARAM,
};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::SystemInformation::GetTickCount64;
use ::windows::Win32::System::Threading::{
    CreateEventW, ResetEvent, SetEvent, WaitForSingleObject,
};
use ::windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use ::windows::Win32::UI::Input::*;
use ::windows::Win32::UI::WindowsAndMessaging::*;
use anyhow::{bail, Result};
use crossbeam_channel::{bounded, Receiver, Sender};

static CAPTURE_OWNER: AtomicBool = AtomicBool::new(false);
const MAX_DEVICES: usize = 32;
pub const KEY_CAPACITY: usize = 256;
pub const MAX_KEY_AGE: Duration = Duration::from_millis(30);

pub struct Press {
    pub key: crate::KeyEvent,
    pub epoch: u64,
    pub observed_at: Instant,
}

pub enum Status {
    Ready,
    Stopped(std::result::Result<(), String>),
}

#[derive(Default)]
pub struct Counters {
    pub sent: AtomicU64,
    pub repeats: AtomicU64,
    pub overflow: AtomicU64,
    pub stale: AtomicU64,
    pub device_changes: AtomicU64,
    pub overruns: AtomicU64,
}

struct Control {
    changed: OwnedHandle,
    stop: AtomicBool,
    enabled: AtomicBool,
    reset: AtomicBool,
    epoch: AtomicU64,
    counters: Arc<Counters>,
}

pub struct Input {
    control: Arc<Control>,
    events: Receiver<Press>,
    status: Receiver<Status>,
    worker: Option<JoinHandle<()>>,
}

impl Input {
    /// Starts disabled; readiness arrives through status(). No caller HWND posts.
    pub fn spawn() -> Result<Self> {
        if CAPTURE_OWNER
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            bail!("this process already owns a Raw Input receiver");
        }
        let lease = RegistrationLease;
        let changed = unsafe { CreateEventW(None, true, false, None)? };
        let control = Arc::new(Control {
            changed: unsafe { OwnedHandle::from_raw_handle(changed.0) },
            stop: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
            reset: AtomicBool::new(true),
            epoch: AtomicU64::new(1),
            counters: Arc::new(Counters::default()),
        });
        let (sender, events) = bounded(KEY_CAPACITY);
        let (status_tx, status) = bounded(2);
        let shared = Arc::clone(&control);
        let worker = thread::Builder::new()
            .name("klicky-input".into())
            .spawn(move || {
                let _lease = lease;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(shared, sender, &status_tx)
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Raw Input thread panicked")));
                let _ = status_tx.try_send(Status::Stopped(
                    result.map_err(|error| format!("{error:#}")),
                ));
            })?;
        Ok(Self {
            control,
            events,
            status,
            worker: Some(worker),
        })
    }

    pub fn events(&self) -> Receiver<Press> {
        self.events.clone()
    }
    pub fn status(&self) -> Receiver<Status> {
        self.status.clone()
    }
    pub fn counters(&self) -> Arc<Counters> {
        Arc::clone(&self.control.counters)
    }
    pub fn epoch(&self) -> u64 {
        self.control.epoch.load(Ordering::Acquire)
    }
    pub fn has_exited(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<()> {
        if self.control.enabled.swap(enabled, Ordering::AcqRel) != enabled {
            self.control.epoch.fetch_add(1, Ordering::AcqRel);
            self.wake()?;
        }
        Ok(())
    }

    /// Resynchronize after lock, suspend or session transitions with lost breaks.
    pub fn reset(&self) -> Result<()> {
        self.control.reset.store(true, Ordering::Release);
        self.control.epoch.fetch_add(1, Ordering::AcqRel);
        self.wake()
    }

    /// Drop queued presses after a pack commit without reseeding held keys.
    pub fn discard_pending(&self) -> Result<()> {
        self.control.epoch.fetch_add(1, Ordering::AcqRel);
        self.wake()
    }

    fn wake(&self) -> Result<()> {
        unsafe { SetEvent(HANDLE(self.control.changed.as_raw_handle()))? };
        Ok(())
    }

    pub fn request_stop(&self) -> Result<()> {
        self.control.enabled.store(false, Ordering::Release);
        self.control.stop.store(true, Ordering::Release);
        self.wake()
    }

    pub fn shutdown(&mut self, deadline: Instant) -> Result<()> {
        self.request_stop()?;
        let Some(worker) = self.worker.as_ref() else {
            return Ok(());
        };
        loop {
            match unsafe {
                WaitForSingleObject(
                    HANDLE(worker.as_raw_handle()),
                    super::pipe::io::wait_milliseconds(deadline),
                )
            } {
                WAIT_OBJECT_0 => break,
                WAIT_TIMEOUT if Instant::now() >= deadline => bail!("Raw Input shutdown timed out"),
                WAIT_TIMEOUT => (),
                WAIT_FAILED => return Err(::windows::core::Error::from_thread().into()),
                _ => bail!("unexpected Raw Input thread wait result"),
            }
        }
        self.worker
            .take()
            .expect("observed input worker")
            .join()
            .map_err(|_| anyhow::anyhow!("Raw Input worker panicked"))?;
        Ok(())
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        let _ = self.request_stop();
    }
}
struct RegistrationLease;
impl Drop for RegistrationLease {
    fn drop(&mut self) {
        CAPTURE_OWNER.store(false, Ordering::Release);
    }
}

struct State {
    control: Arc<Control>,
    sender: Sender<Press>,
    pressed: HashMap<usize, HashSet<u32>>,
    blocked: [bool; 256],
    epoch: u64,
    enabled: bool,
    fence: Option<(Instant, u32)>,
    failure: Option<String>,
    observed_at: Instant,
}

impl State {
    fn apply_control(&mut self) {
        let epoch = self.control.epoch.load(Ordering::Acquire);
        if epoch != self.epoch || self.control.reset.load(Ordering::Acquire) {
            self.epoch = epoch;
            self.enabled = self.control.enabled.load(Ordering::Acquire);
            if self.control.reset.swap(false, Ordering::AcqRel) {
                self.pressed.clear();
                for (key, blocked) in self.blocked.iter_mut().enumerate() {
                    *blocked = unsafe { GetAsyncKeyState(key as i32) } & i16::MIN != 0;
                }
            }
            self.fence = self.enabled.then(|| {
                (
                    Instant::now() + Duration::from_millis(100),
                    unsafe { GetTickCount64() } as u32,
                )
            });
        }
    }

    fn packet(&mut self, device: usize, make: u16, flags: u16, virtual_key: u16, tick: u32) {
        if make == 0xff {
            self.pressed.remove(&device);
            self.control
                .counters
                .overruns
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let Some(mapped) = keys::map(make, flags, virtual_key) else {
            return;
        };
        let vk = keys::modifier_virtual_key(make, flags, virtual_key);
        if flags & 1 != 0 {
            self.blocked[vk] = false;
            if let Some(pressed) = self.pressed.get_mut(&device) {
                pressed.remove(&mapped.identity);
            }
            return;
        }
        if !mapped.pulse {
            if !self.pressed.contains_key(&device) {
                if self.pressed.len() >= MAX_DEVICES {
                    self.control
                        .counters
                        .overflow
                        .fetch_add(1, Ordering::Relaxed);
                    return;
                }
                self.pressed.insert(device, HashSet::with_capacity(128));
            }
            if !self
                .pressed
                .get_mut(&device)
                .expect("tracked device")
                .insert(mapped.identity)
            {
                self.control
                    .counters
                    .repeats
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        if self.blocked[vk]
            || !self.enabled
            || !self.control.enabled.load(Ordering::Acquire)
            || self.control.stop.load(Ordering::Acquire)
            || self.epoch != self.control.epoch.load(Ordering::Acquire)
        {
            return;
        }
        let age = Duration::from_millis(u64::from(
            (unsafe { GetTickCount64() } as u32).wrapping_sub(tick),
        ));
        let fenced = self.fence.is_some_and(|(until, after)| {
            Instant::now() < until && (tick.wrapping_sub(after) as i32) < 0
        });
        if age > MAX_KEY_AGE || fenced {
            self.control.counters.stale.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let timestamp = Instant::now().checked_sub(age).unwrap_or_else(Instant::now);
        let event = Press {
            key: crate::KeyEvent {
                key_name: mapped.name,
                timestamp,
            },
            epoch: self.epoch,
            observed_at: self.observed_at,
        };
        if self.sender.try_send(event).is_err() {
            self.control
                .counters
                .overflow
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.control.counters.sent.fetch_add(1, Ordering::Relaxed);
        }
    }
}

struct Window {
    handle: HWND,
    registered: bool,
}
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            if self.registered {
                let _ = RegisterRawInputDevices(
                    &[RAWINPUTDEVICE {
                        usUsagePage: 1,
                        usUsage: 6,
                        dwFlags: RIDEV_REMOVE,
                        hwndTarget: HWND::default(),
                    }],
                    size_of::<RAWINPUTDEVICE>() as u32,
                );
            }
            let _ = DestroyWindow(self.handle);
        }
    }
}

fn run(control: Arc<Control>, sender: Sender<Press>, status: &Sender<Status>) -> Result<()> {
    if control.stop.load(Ordering::Acquire) {
        return Ok(());
    }
    let mut state = Box::new(State {
        control,
        sender,
        pressed: HashMap::new(),
        blocked: [false; 256],
        epoch: 0,
        enabled: false,
        fence: None,
        failure: None,
        observed_at: Instant::now(),
    });
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            hInstance: instance.into(),
            lpfnWndProc: Some(window_proc),
            lpszClassName: w!("KlickyRawInput"),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            let error = ::windows::core::Error::from_thread();
            if error.code() != ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
                return Err(error.into());
            }
        }
        let handle = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class.lpszClassName,
            w!("Klicky input"),
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
        let mut window = Window {
            handle,
            registered: false,
        };
        RegisterRawInputDevices(
            &[RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 6,
                dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                hwndTarget: handle,
            }],
            size_of::<RAWINPUTDEVICE>() as u32,
        )?;
        window.registered = true;
        state.apply_control();
        status.try_send(Status::Ready).ok();
        let event = HANDLE(state.control.changed.as_raw_handle());
        loop {
            if state.control.stop.load(Ordering::Acquire) {
                break;
            }
            state.apply_control();
            match MsgWaitForMultipleObjectsEx(
                Some(&[event]),
                u32::MAX,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            ) {
                WAIT_OBJECT_0 => {
                    ResetEvent(event)?;
                    state.apply_control();
                }
                result if result.0 == WAIT_OBJECT_0.0 + 1 => {
                    let mut message = MSG::default();
                    for _ in 0..128 {
                        if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                            break;
                        }
                        if message.message == WM_QUIT {
                            return Ok(());
                        }
                        DispatchMessageW(&message);
                        if state.control.stop.load(Ordering::Acquire) {
                            break;
                        }
                    }
                }
                WAIT_FAILED => return Err(::windows::core::Error::from_thread().into()),
                _ => bail!("unexpected Raw Input message wait result"),
            }
        }
        if let Some(error) = state.failure.take() {
            bail!(error);
        }
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let observed_at = Instant::now();
    if message == WM_NCCREATE {
        let creation = &*(lparam.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(window, GWLP_USERDATA, creation.lpCreateParams as isize);
        return LRESULT(1);
    }
    if message == WM_NCDESTROY {
        SetWindowLongPtrW(window, GWLP_USERDATA, 0);
    }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut State;
    if !pointer.is_null() && (message == WM_INPUT || message == WM_INPUT_DEVICE_CHANGE) {
        let state = &mut *pointer;
        state.observed_at = observed_at;
        state.apply_control();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            if message == WM_INPUT_DEVICE_CHANGE {
                state
                    .control
                    .counters
                    .device_changes
                    .fetch_add(1, Ordering::Relaxed);
                if wparam.0 == GIDC_REMOVAL as usize {
                    state.pressed.remove(&(lparam.0 as usize));
                }
            } else {
                let mut size = 0;
                let input = HRAWINPUT(lparam.0 as *mut _);
                if GetRawInputData(
                    input,
                    RID_INPUT,
                    None,
                    &mut size,
                    size_of::<RAWINPUTHEADER>() as u32,
                ) == u32::MAX
                {
                    bail!("Raw Input size query failed");
                }
                if size < (size_of::<RAWINPUTHEADER>() + size_of::<RAWKEYBOARD>()) as u32
                    || size > size_of::<RAWINPUT>() as u32
                {
                    return Ok(());
                }
                let mut raw = RAWINPUT::default();
                let copied = GetRawInputData(
                    input,
                    RID_INPUT,
                    Some((&mut raw as *mut RAWINPUT).cast()),
                    &mut size,
                    size_of::<RAWINPUTHEADER>() as u32,
                );
                if copied == u32::MAX {
                    bail!("Raw Input packet read failed");
                }
                if copied < (size_of::<RAWINPUTHEADER>() + size_of::<RAWKEYBOARD>()) as u32
                    || raw.header.dwType != RIM_TYPEKEYBOARD.0
                {
                    return Ok(());
                }
                let key = raw.data.keyboard;
                state.packet(
                    raw.header.hDevice.0 as usize,
                    key.MakeCode,
                    key.Flags,
                    key.VKey,
                    GetMessageTime() as u32,
                );
            }
            Ok(())
        }));
        let error = match result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(format!("{error:#}")),
            Err(_) => Some("Raw Input callback panicked".into()),
        };
        if let Some(error) = error {
            state.failure = Some(error);
            state.control.enabled.store(false, Ordering::Release);
            state.control.stop.store(true, Ordering::Release);
            let _ = SetEvent(HANDLE(state.control.changed.as_raw_handle()));
        }
    }
    // Release the state borrow before native dispatch/cleanup can reenter.
    DefWindowProcW(window, message, wparam, lparam)
}
