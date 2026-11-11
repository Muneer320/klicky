//! Native tray frontend. Filesystem work and command waits stay off its callbacks.
mod startup_worker;
use std::cell::RefCell;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ::windows::core::{w, PCWSTR};
use ::windows::Win32::Foundation::*;
use ::windows::Win32::System::Console::{
    GetConsoleCP, SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_C_EVENT,
};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Threading::{CreateEventW, ResetEvent, SetEvent, INFINITE};
use ::windows::Win32::UI::Shell::*;
use ::windows::Win32::UI::WindowsAndMessaging::*;
use anyhow::{bail, Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};

use super::paths::same_name;
use super::protocol::{Command, EngineState, Outcome};
use super::runtime::{Controller, Host, Options, View};
use super::session::Session;
use super::startup::Status as StartupStatus;
use super::Paths;
use startup_worker::{Request as StartupRequest, Worker as StartupWorker};

const TRAY_MESSAGE: u32 = WM_APP + 1;
// shellapi.h defines this expression; the generated Windows bindings omit it.
const KEY_SELECT: u32 = NIN_SELECT | 1;
const MENU_TIMER: usize = 1;
const ICON_TIMER: usize = 2;
const REPLY_TIMER: usize = 3;
const PACK_BASE: u32 = 1000;
const VOLUME_BASE: u32 = 100;
const PAUSE: u32 = 1;
const RESUME: u32 = 2;
const RETRY: u32 = 3;
const QUIT: u32 = 4;
const REFRESH: u32 = 5;
const LOGIN_ENABLE: u32 = 6;
const LOGIN_DISABLE: u32 = 7;
const LOGIN_CHECK: u32 = 8;
const LOGIN_SETTINGS: u32 = 9;

/// Run in the interactive user's process. Capture starts only after WTS verification.
pub fn run(options: Options) -> Result<()> {
    let paths = Paths::discover()?;
    super::bundle::hold_for_process(&paths)?;
    let mut host = Host::spawn(paths.clone(), options)?;
    let controller = host.controller();
    let result = run_ui(paths, controller.clone());
    controller.request_stop();
    // Even a failed frontend cannot abandon workers while releasing the process lease.
    // Keep the process alive until the actual native coordinator thread has exited.
    let shutdown = loop {
        let joined = host.join(Instant::now() + Duration::from_millis(250));
        if host.shutdown_complete() {
            break joined;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    if let Some(report) = controller.take_view()?.diagnostics {
        println!("{}", serde_json::to_string(&report)?);
    }
    result.and(shutdown)
}

pub fn run_entry() -> Result<()> {
    use clap::Parser;
    #[derive(Parser)]
    #[command(
        name = "klicky-desktop",
        version,
        about = "Klicky notification-area app"
    )]
    struct Args {
        #[arg(long)]
        paused: bool,
        #[arg(long)]
        benchmark: bool,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=3600))]
        seconds: Option<u64>,
    }
    let args = Args::parse();
    run(Options {
        paused: args.paused,
        benchmark: args.benchmark,
        duration: args.seconds.map(Duration::from_secs),
    })
}

pub fn show_error(error: &anyhow::Error) {
    // An updater may own the bundle between CreateProcess and the reader lease.
    // Exit promptly instead of pinning the executable behind an error dialog.
    if super::bundle::is_busy(error) {
        return;
    }
    let message = wide(&format!("{error:#}"));
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            w!("Klicky"),
            MB_OK | MB_ICONERROR,
        );
    }
}

struct Metadata {
    event: OwnedHandle,
    latest: Mutex<Update>,
}
#[derive(Default)]
struct Update {
    packs: Option<std::result::Result<Vec<String>, String>>,
}
struct Worker {
    work: Sender<()>,
    results: Arc<Metadata>,
}
impl Worker {
    fn spawn(paths: Paths) -> Result<Self> {
        let (work, queue) = bounded(1);
        let event = unsafe { CreateEventW(None, true, false, None)? };
        let results = Arc::new(Metadata {
            event: unsafe { OwnedHandle::from_raw_handle(event.0) },
            latest: Mutex::new(Update::default()),
        });
        let mailbox = Arc::clone(&results);
        std::thread::Builder::new()
            .name("klicky-ui-work".into())
            .spawn(move || {
                while queue.recv().is_ok() {
                    let update = Update {
                        packs: Some(
                            paths
                                .available_sound_packs()
                                .map_err(|error| format!("{error:#}")),
                        ),
                    };
                    let mut latest = mailbox
                        .latest
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    if update.packs.is_some() {
                        latest.packs = update.packs;
                    }
                    unsafe {
                        let _ = SetEvent(HANDLE(mailbox.event.as_raw_handle()));
                    }
                }
            })?;
        work.try_send(())?;
        Ok(Self { work, results })
    }
    fn take(&self) -> Result<Update> {
        let mut latest = self
            .results
            .latest
            .lock()
            .map_err(|_| anyhow::anyhow!("UI result lock poisoned"))?;
        unsafe { ResetEvent(HANDLE(self.results.event.as_raw_handle()))? };
        Ok(std::mem::take(&mut *latest))
    }
}

struct Pending {
    reply: Receiver<Outcome>,
    deadline: Instant,
    mutation: bool,
}

struct State {
    controller: Controller,
    session: Session,
    worker: Worker,
    startup: StartupWorker,
    startup_status: Option<StartupStatus>,
    startup_error: Option<String>,
    view: View,
    packs: Vec<String>,
    packs_error: Option<String>,
    operation_error: Option<String>,
    pending: Vec<Pending>,
    refreshing: bool,
    menu_open: bool,
    taskbar_created: u32,
    icon_added: bool,
    icon_failure: Option<(Instant, String)>,
    fatal: Option<String>,
}

struct Window {
    controller: Controller,
    hwnd: HWND,
    state: Option<Box<RefCell<State>>>,
    instance: HINSTANCE,
}
impl Drop for Window {
    fn drop(&mut self) {
        self.controller.request_stop();
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(self.hwnd));
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            self.state.take(); // unregister WTS before destroying the window
            let _ = DestroyWindow(self.hwnd);
            let _ = UnregisterClassW(w!("Klicky.Desktop.v1"), Some(self.instance));
        }
    }
}

fn run_ui(paths: Paths, controller: Controller) -> Result<()> {
    let instance = HINSTANCE(unsafe { GetModuleHandleW(None)? }.0);
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: w!("Klicky.Desktop.v1"),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(::windows::core::Error::from_thread().into());
    }
    let hwnd = match unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.lpszClassName,
            w!("Klicky"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    } {
        Ok(hwnd) => hwnd,
        Err(error) => {
            unsafe {
                let _ = UnregisterClassW(class.lpszClassName, Some(instance));
            }
            return Err(error.into());
        }
    };
    let mut window = Window {
        controller: controller.clone(),
        hwnd,
        state: None,
        instance,
    };
    let session = Session::register(hwnd, paths.session_id())?;
    let worker = Worker::spawn(paths.clone())?;
    let startup = StartupWorker::spawn(paths)?;
    let state = Box::new(RefCell::new(State {
        controller: controller.clone(),
        session,
        worker,
        startup,
        startup_status: None,
        startup_error: None,
        view: controller.take_view()?,
        packs: Vec::new(),
        packs_error: None,
        operation_error: None,
        pending: Vec::new(),
        refreshing: true,
        menu_open: false,
        taskbar_created: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
        icon_added: false,
        icon_failure: None,
        fatal: None,
    }));
    let state_pointer = (&*state as *const RefCell<State>) as isize;
    window.state = Some(state);
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_pointer);
    }
    if state_ref(hwnd).borrow().taskbar_created == 0 {
        bail!("Cannot register Explorer restart notification");
    }
    let _console = ConsoleGuard::install(controller.clone())?;
    controller.set_desktop_active(state_ref(hwnd).borrow().session.active());
    update_icon(hwnd)?;
    let metadata_event = {
        let state = state_ref(hwnd).borrow();
        HANDLE(state.worker.results.event.as_raw_handle())
    };
    let initial_handles = [
        HANDLE(controller.changed_handle().as_raw_handle()),
        metadata_event,
    ];
    let mut message = MSG::default();
    loop {
        update_view(hwnd)?;
        poll_startup(hwnd)?;
        {
            let state = state_ref(hwnd).borrow();
            if state.view.finished && state.startup.shutdown_complete() {
                if let Some(error) = &state.fatal {
                    bail!("{error}");
                }
                return Ok(());
            }
            if state.view.finished || state.view.snapshot.state == EngineState::Stopping {
                state.startup.request_stop();
            }
        }
        let mut handles = initial_handles.to_vec();
        {
            let state = state_ref(hwnd).borrow();
            handles.push(state.startup.changed_handle());
            if let Some(thread) = state.startup.thread_handle() {
                handles.push(thread);
            }
        }
        let result = unsafe {
            MsgWaitForMultipleObjectsEx(Some(&handles), INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        if result == WAIT_FAILED {
            return Err(::windows::core::Error::from_thread().into());
        }
        if result.0 == WAIT_OBJECT_0.0 + 1 {
            take_metadata(hwnd)?;
        }
        // Revisit the host even if another app continuously posts window messages.
        for _ in 0..256 {
            if !unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                break;
            }
            if message.message == WM_QUIT {
                controller.request_stop();
                continue;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}

fn poll_startup(hwnd: HWND) -> Result<()> {
    let mut state = state_ref(hwnd).borrow_mut();
    if let Some(update) = state.startup.take()? {
        if let Some(status) = update.status {
            match status {
                Ok(status) => {
                    state.startup_status = Some(status);
                    state.startup_error = None;
                }
                Err(error) => {
                    state.startup_status = None;
                    state.startup_error = Some(error);
                }
            }
        }
        if let Some(error) = update.error {
            state.startup_error = Some(error);
        }
    }
    if state.startup.thread_handle().is_some() {
        let stopping = state.startup.stopping();
        match state.startup.try_join() {
            Ok(true) if !stopping => {
                state.fatal = Some("Login startup worker stopped unexpectedly".into());
                state.controller.request_stop();
            }
            Err(error) => {
                state.fatal = Some(format!("{error:#}"));
                state.controller.request_stop();
            }
            _ => (),
        }
    }
    Ok(())
}

// The box belongs to Window, stays pinned through the message loop, and is detached
// before destruction. RefCell prevents accidental aliasing across native reentry.
fn state_ref(hwnd: HWND) -> &'static RefCell<State> {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    assert_ne!(pointer, 0, "window state unavailable");
    unsafe { &*(pointer as *const RefCell<State>) }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if GetWindowLongPtrW(hwnd, GWLP_USERDATA) == 0 {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatch(hwnd, message, wparam, lparam)
    })) {
        Ok(Some(result)) => result,
        Ok(None) => DefWindowProcW(hwnd, message, wparam, lparam),
        Err(_) => {
            if let Ok(mut state) = state_ref(hwnd).try_borrow_mut() {
                state.fatal = Some("Windows frontend callback failed; stopping".into());
                state.controller.request_stop();
            }
            LRESULT(0)
        }
    }
}

fn dispatch(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let taskbar = state_ref(hwnd).borrow().taskbar_created;
    if message == taskbar {
        state_ref(hwnd).borrow_mut().icon_added = false;
        if let Err(error) = update_icon(hwnd) {
            fail(hwnd, error);
        }
        return Some(LRESULT(0));
    }
    match message {
        WM_CLOSE => state_ref(hwnd).borrow().controller.request_stop(),
        WM_QUERYENDSESSION => {
            let controller = {
                let mut state = state_ref(hwnd).borrow_mut();
                state.session.ending(true);
                state.controller.clone()
            };
            controller.set_desktop_active(false);
            return Some(LRESULT(1));
        }
        WM_ENDSESSION => {
            let (controller, active) = {
                let mut state = state_ref(hwnd).borrow_mut();
                state.session.ending(wparam.0 != 0);
                (state.controller.clone(), state.session.active())
            };
            if wparam.0 != 0 {
                controller.request_stop();
            } else {
                controller.set_desktop_active(active);
            }
        }
        WM_WTSSESSION_CHANGE | WM_POWERBROADCAST => {
            let (controller, active) = {
                let mut state = state_ref(hwnd).borrow_mut();
                let changed = if message == WM_WTSSESSION_CHANGE {
                    state.session.change(wparam.0 as u32, lparam.0 as u32)
                } else {
                    state.session.power(wparam.0 as u32)
                };
                if !changed {
                    return Some(LRESULT(1));
                }
                (state.controller.clone(), state.session.active())
            };
            controller.set_desktop_active(active);
            return Some(LRESULT(1));
        }
        WM_TIMER if wparam.0 == MENU_TIMER => {
            if let Err(error) = update_view(hwnd) {
                fail(hwnd, error);
            }
            if state_ref(hwnd).borrow().view.finished {
                unsafe {
                    let _ = EndMenu();
                }
            }
        }
        WM_TIMER if wparam.0 == ICON_TIMER => {
            let expired = {
                let state = state_ref(hwnd).borrow();
                state
                    .icon_failure
                    .as_ref()
                    .filter(|(at, _)| at.elapsed() >= Duration::from_secs(15))
                    .map(|(_, error)| error.clone())
            };
            if let Some(error) = expired {
                unsafe {
                    let _ = KillTimer(Some(hwnd), ICON_TIMER);
                }
                fail(
                    hwnd,
                    anyhow::anyhow!("Notification area unavailable: {error}"),
                );
            } else if let Err(error) = update_icon(hwnd) {
                fail(hwnd, error);
            }
        }
        WM_TIMER if wparam.0 == REPLY_TIMER => {
            poll_replies(hwnd);
        }
        TRAY_MESSAGE => {
            let event = (lparam.0 as u32) & 0xffff;
            if event == WM_CONTEXTMENU || event == NIN_SELECT || event == KEY_SELECT {
                if let Err(error) = show_menu(hwnd) {
                    state_ref(hwnd).borrow_mut().operation_error = Some(format!("{error:#}"));
                }
            }
        }
        _ => return None,
    }
    Some(LRESULT(0))
}

fn fail(hwnd: HWND, error: anyhow::Error) {
    let mut state = state_ref(hwnd).borrow_mut();
    state.fatal = Some(format!("{error:#}"));
    state.controller.request_stop();
}

fn update_view(hwnd: HWND) -> Result<()> {
    let controller = state_ref(hwnd).borrow().controller.clone();
    let view = controller.take_view()?;
    let changed = {
        let mut state = state_ref(hwnd).borrow_mut();
        let changed = state.view != view;
        state.view = view;
        changed
    };
    if changed {
        update_icon(hwnd)?;
    }
    Ok(())
}

fn take_metadata(hwnd: HWND) -> Result<()> {
    let mut state = state_ref(hwnd).borrow_mut();
    let update = state.worker.take()?;
    if let Some(packs) = update.packs {
        state.refreshing = false;
        match packs {
            Ok(packs) => {
                state.packs = packs;
                state.packs_error = None;
            }
            Err(error) => state.packs_error = Some(error),
        }
    }
    Ok(())
}

fn poll_replies(hwnd: HWND) {
    let empty = {
        let mut state = state_ref(hwnd).borrow_mut();
        let mut error = None;
        state
            .pending
            .retain(|pending| match pending.reply.try_recv() {
                Ok(Outcome::Success { .. }) => false,
                Ok(Outcome::UpdatePrepared { .. }) => {
                    error = Some("Unexpected installer preparation response".into());
                    false
                }
                Ok(Outcome::Diagnostics { .. }) => {
                    error = Some("Unexpected diagnostics response".into());
                    false
                }
                Ok(Outcome::Failure { message, .. }) => {
                    error = Some(message);
                    false
                }
                Err(TryRecvError::Disconnected) => {
                    error = Some("Command receipt disconnected; outcome is unknown".into());
                    false
                }
                Err(TryRecvError::Empty) if Instant::now() >= pending.deadline => {
                    error = Some("Command confirmation timed out; outcome is unknown".into());
                    false
                }
                Err(TryRecvError::Empty) => true,
            });
        if error.is_some() {
            state.operation_error = error;
        }
        state.pending.is_empty()
    };
    if empty {
        unsafe {
            let _ = KillTimer(Some(hwnd), REPLY_TIMER);
        }
    }
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    }
}
fn update_icon(hwnd: HWND) -> Result<()> {
    match try_icon(hwnd) {
        Ok(()) => {
            state_ref(hwnd).borrow_mut().icon_failure = None;
            unsafe {
                let _ = KillTimer(Some(hwnd), ICON_TIMER);
            }
        }
        Err(error) => {
            let first = {
                let mut state = state_ref(hwnd).borrow_mut();
                state.icon_added = false;
                let first = state.icon_failure.is_none();
                state
                    .icon_failure
                    .get_or_insert_with(|| (Instant::now(), format!("{error:#}")));
                first
            };
            // Explorer can be absent briefly during login or a taskbar restart.
            if first && unsafe { SetTimer(Some(hwnd), ICON_TIMER, 1000, None) } == 0 {
                bail!("Cannot supervise notification-area recovery");
            }
        }
    }
    Ok(())
}
fn try_icon(hwnd: HWND) -> Result<()> {
    let (added, tip) = {
        let state = state_ref(hwnd).borrow();
        (
            state.icon_added,
            format!(
                "Klicky: {} | {} | {:.0}%",
                label(state.view.snapshot.state),
                state.view.snapshot.sound_pack,
                state.view.snapshot.volume * 100.0
            ),
        )
    };
    let mut data = icon_data(hwnd);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    data.uCallbackMessage = TRAY_MESSAGE;
    // Shared icon handles loaded from this module live for the process lifetime.
    data.hIcon = unsafe { LoadIconW(Some(GetModuleHandleW(None)?.into()), PCWSTR(101usize as _))? };
    copy_utf16(&tip, &mut data.szTip);
    if !unsafe { Shell_NotifyIconW(if added { NIM_MODIFY } else { NIM_ADD }, &data) }.as_bool() {
        bail!("Cannot display the Klicky notification icon");
    }
    if !added {
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if !unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) }.as_bool() {
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            }
            bail!("Cannot enable accessible tray notifications");
        }
        state_ref(hwnd).borrow_mut().icon_added = true;
    }
    Ok(())
}

struct Menu(HMENU);
impl Menu {
    fn new() -> Result<Self> {
        Ok(Self(unsafe { CreatePopupMenu()? }))
    }
    fn item(&self, id: u32, text: &str, flags: MENU_ITEM_FLAGS) -> Result<()> {
        let text = wide(&text.replace('&', "&&"));
        unsafe {
            AppendMenuW(
                self.0,
                flags | MF_STRING,
                id as usize,
                PCWSTR(text.as_ptr()),
            )?;
        }
        Ok(())
    }
    fn submenu(&self, text: &str, menu: Self, flags: MENU_ITEM_FLAGS) -> Result<()> {
        let text = wide(text);
        unsafe {
            AppendMenuW(
                self.0,
                MF_POPUP | flags,
                menu.0 .0 as usize,
                PCWSTR(text.as_ptr()),
            )?;
        }
        std::mem::forget(menu); // DestroyMenu(parent) owns successfully attached submenus.
        Ok(())
    }
}
impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}

fn show_menu(hwnd: HWND) -> Result<()> {
    // Snapshot and release every Rust borrow before TrackPopupMenuEx's nested loop.
    let (view, packs, error, busy, menu_open, refreshing, login_status, login_busy) = {
        let state = state_ref(hwnd).borrow();
        (
            state.view.clone(),
            state.packs.clone(),
            state
                .operation_error
                .clone()
                .or(state.view.last_error.clone())
                .or(state.startup_error.clone())
                .or(state.packs_error.clone()),
            state.pending.iter().any(|pending| pending.mutation) || state.view.busy,
            state.menu_open,
            state.refreshing,
            state.startup_status,
            state.startup.busy(),
        )
    };
    if menu_open {
        return Ok(());
    }
    let menu = Menu::new()?;
    menu.item(
        0,
        &format!("Klicky: {}", label(view.snapshot.state)),
        MF_GRAYED,
    )?;
    if view.finished {
        menu.item(0, "Finishing startup work before closing", MF_GRAYED)?;
    }
    if let Some(error) = error {
        menu.item(0, &menu_text(&error), MF_GRAYED)?;
    }
    let disabled = if busy || view.snapshot.state == EngineState::Stopping {
        MF_GRAYED
    } else {
        MF_ENABLED
    };
    let pack_menu = Menu::new()?;
    for (index, name) in packs.iter().enumerate() {
        let checked = if same_name(name, &view.snapshot.sound_pack)? {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };
        pack_menu.item(PACK_BASE + index as u32, name, checked)?;
    }
    if packs.is_empty() {
        pack_menu.item(0, "No packs loaded", MF_GRAYED)?;
    }
    menu.submenu("Sound pack", pack_menu, disabled)?;
    let volume = Menu::new()?;
    for step in 0..=10 {
        let checked = if (view.snapshot.volume * 10.0 - step as f32).abs() < 0.01 {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };
        volume.item(VOLUME_BASE + step, &format!("{}%", step * 10), checked)?;
    }
    menu.submenu("Volume", volume, disabled)?;
    let live_controls = if view.snapshot.state == EngineState::Stopping {
        MF_GRAYED
    } else {
        MF_ENABLED
    };
    menu.item(
        PAUSE,
        "Pause",
        if view.paused {
            MF_GRAYED
        } else {
            live_controls
        },
    )?;
    menu.item(
        RESUME,
        "Resume",
        if view.paused {
            live_controls
        } else {
            MF_GRAYED
        },
    )?;
    menu.item(RETRY, "Retry audio", live_controls)?;
    menu.item(
        REFRESH,
        "Refresh sound packs",
        if refreshing { MF_GRAYED } else { MF_ENABLED },
    )?;
    let login = Menu::new()?;
    let login_label = if login_busy {
        "Checking or updating startup..."
    } else {
        match login_status {
            Some(StartupStatus::Absent) => "Not added to startup",
            Some(StartupStatus::Registered) => "Added; Windows controls startup approval",
            Some(StartupStatus::Foreign) => "Different or edited startup entry; preserved",
            Some(StartupStatus::DisabledByUser) => "Disabled in Windows Startup settings",
            Some(StartupStatus::DisabledByPolicy) => "Disabled by Windows policy",
            Some(StartupStatus::EnabledByPolicy) => "Enabled by Windows policy",
            None => "Startup status unavailable",
        }
    };
    login.item(0, login_label, MF_GRAYED)?;
    let login_disabled =
        login_busy || view.snapshot.state == EngineState::Stopping || view.finished;
    let conflict = matches!(
        login_status,
        Some(
            StartupStatus::Foreign
                | StartupStatus::DisabledByUser
                | StartupStatus::DisabledByPolicy
                | StartupStatus::EnabledByPolicy
        )
    );
    login.item(
        LOGIN_ENABLE,
        "Add Klicky to startup",
        if login_disabled || conflict || login_status == Some(StartupStatus::Registered) {
            MF_GRAYED
        } else {
            MF_ENABLED
        },
    )?;
    login.item(
        LOGIN_DISABLE,
        "Remove Klicky from startup",
        if login_disabled || conflict || login_status == Some(StartupStatus::Absent) {
            MF_GRAYED
        } else {
            MF_ENABLED
        },
    )?;
    login.item(
        LOGIN_CHECK,
        "Refresh startup status",
        if login_disabled {
            MF_GRAYED
        } else {
            MF_ENABLED
        },
    )?;
    login.item(
        LOGIN_SETTINGS,
        "Windows startup settings",
        if login_disabled {
            MF_GRAYED
        } else {
            MF_ENABLED
        },
    )?;
    menu.submenu("Start with Windows", login, MF_ENABLED)?;
    menu.item(QUIT, "Quit Klicky", MF_ENABLED)?;
    let mut point = POINT::default();
    unsafe {
        GetCursorPos(&mut point)?;
    }
    state_ref(hwnd).borrow_mut().menu_open = true;
    // Only while the native menu owns a nested loop: ensure remote Stop closes it.
    if unsafe { SetTimer(Some(hwnd), MENU_TIMER, 100, None) } == 0 {
        state_ref(hwnd).borrow_mut().menu_open = false;
        bail!("Cannot supervise the tray menu");
    }
    let selected = unsafe {
        let _ = SetForegroundWindow(hwnd);
        TrackPopupMenuEx(
            menu.0,
            (TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON).0,
            point.x,
            point.y,
            hwnd,
            None,
        )
        .0 as u32
    };
    unsafe {
        let _ = KillTimer(Some(hwnd), MENU_TIMER);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
    }
    state_ref(hwnd).borrow_mut().menu_open = false;
    let login_request = match selected {
        LOGIN_ENABLE => Some(StartupRequest::Set(true)),
        LOGIN_DISABLE => Some(StartupRequest::Set(false)),
        LOGIN_CHECK => Some(StartupRequest::Check),
        LOGIN_SETTINGS => Some(StartupRequest::Settings),
        _ => None,
    };
    if let Some(request) = login_request {
        let mut state = state_ref(hwnd).borrow_mut();
        state.startup.submit(request)?;
        state.startup_error = None;
        return Ok(());
    }
    if selected == REFRESH {
        let mut state = state_ref(hwnd).borrow_mut();
        if !state.refreshing {
            state
                .worker
                .work
                .try_send(())
                .map_err(|_| anyhow::anyhow!("Tray metadata worker is unavailable"))?;
            state.refreshing = true;
        }
        return Ok(());
    }
    let command = match selected {
        0 => return Ok(()),
        QUIT => {
            state_ref(hwnd).borrow().controller.request_stop();
            return Ok(());
        }
        PAUSE => Command::Pause { paused: true },
        RESUME => Command::Pause { paused: false },
        RETRY => Command::RetryAudio {},
        id if (VOLUME_BASE..=VOLUME_BASE + 10).contains(&id) => Command::Volume {
            level: (id - VOLUME_BASE) as f32 / 10.0,
        },
        id if id >= PACK_BASE => {
            let pack = packs
                .get((id - PACK_BASE) as usize)
                .context("Unknown pack menu item")?;
            Command::Switch { pack: pack.clone() }
        }
        _ => return Ok(()),
    };
    submit(hwnd, command)
}

fn submit(hwnd: HWND, command: Command) -> Result<()> {
    let mutation = matches!(command, Command::Switch { .. } | Command::Volume { .. });
    let mut state = state_ref(hwnd).borrow_mut();
    if state.pending.len() == 4 {
        bail!("Tray command queue is busy");
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    // Submission is nonblocking. Pause/Retry reach the engine immediately, even
    // while an earlier pack change or metadata filesystem read remains outstanding.
    let reply = state.controller.command(command, deadline)?;
    state.pending.push(Pending {
        reply,
        deadline,
        mutation,
    });
    state.operation_error = None;
    drop(state);
    // A low-frequency timer exists only during user-requested command receipts.
    // It also runs inside the menu's nested loop; no permanent GUI polling.
    if unsafe { SetTimer(Some(hwnd), REPLY_TIMER, 100, None) } == 0 {
        fail(
            hwnd,
            anyhow::anyhow!("Cannot supervise command confirmation; stopping"),
        );
    }
    Ok(())
}

fn label(state: EngineState) -> &'static str {
    match state {
        EngineState::Starting => "Starting",
        EngineState::Ready => "Ready",
        EngineState::Paused => "Paused",
        EngineState::WaitingForAudio => "Waiting for audio",
        EngineState::Recovering => "Recovering audio",
        EngineState::Failed => "Failed",
        EngineState::Stopping => "Stopping",
    }
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn copy_utf16(text: &str, target: &mut [u16]) {
    let mut used = 0;
    for character in text.chars() {
        let count = character.len_utf16();
        if used + count >= target.len() {
            break;
        }
        character.encode_utf16(&mut target[used..used + count]);
        used += count;
    }
    target[used] = 0;
}
fn menu_text(text: &str) -> String {
    text.chars()
        .take(180)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

static CONSOLE: OnceLock<Mutex<Option<Controller>>> = OnceLock::new();
struct ConsoleGuard(bool);
impl ConsoleGuard {
    fn install(controller: Controller) -> Result<Self> {
        if unsafe { GetConsoleCP() } == 0 {
            return Ok(Self(false));
        }
        let cell = CONSOLE.get_or_init(|| Mutex::new(None));
        *cell
            .lock()
            .map_err(|_| anyhow::anyhow!("Console controller lock poisoned"))? = Some(controller);
        if let Err(error) = unsafe { SetConsoleCtrlHandler(Some(console_handler), true) } {
            *cell.lock().unwrap_or_else(|error| error.into_inner()) = None;
            return Err(error.into());
        }
        Ok(Self(true))
    }
}
impl Drop for ConsoleGuard {
    fn drop(&mut self) {
        if !self.0 {
            return;
        }
        unsafe {
            let _ = SetConsoleCtrlHandler(Some(console_handler), false);
        }
        if let Some(cell) = CONSOLE.get() {
            *cell.lock().unwrap_or_else(|error| error.into_inner()) = None;
        }
    }
}
unsafe extern "system" fn console_handler(event: u32) -> ::windows::core::BOOL {
    if event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT {
        if let Some(cell) = CONSOLE.get() {
            if let Ok(guard) = cell.try_lock() {
                if let Some(controller) = guard.as_ref() {
                    controller.request_stop();
                    return TRUE;
                }
            }
        }
    }
    FALSE
}
