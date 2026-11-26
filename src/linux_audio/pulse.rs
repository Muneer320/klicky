//! PulseAudio/PipeWire-Pulse client. Native objects and callback data are accessed
//! only with the threaded-mainloop lock. Queries own their callback allocations.
use std::cell::{Cell, UnsafeCell};
use std::ffi::{c_char, c_void, CStr, CString};
use std::num::{NonZeroU16, NonZeroU32};
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::audio::{Backend, OutputDevice, OutputList, OutputPreference};
use anyhow::{bail, Context as _, Result};
use libpulse_sys as pa;
use rodio::mixer::{mixer, Mixer, MixerSource};

const TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Debug)]
pub(super) struct Error(i32);
impl Error {
    pub fn is_absent(&self) -> bool {
        self.0 == pa::PA_ERR_CONNECTIONREFUSED as i32 || self.0 == pa::PA_ERR_NOENTITY as i32
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SAFETY: pa_strerror returns a static NUL-terminated string.
        unsafe {
            write!(
                f,
                "{}",
                CStr::from_ptr(pa::pa_strerror(self.0)).to_string_lossy()
            )
        }
    }
}
impl std::error::Error for Error {}

struct Mainloop {
    ptr: NonNull<pa::pa_threaded_mainloop>,
    started: Cell<bool>,
}
struct Lock<'a>(&'a Mainloop);
impl Mainloop {
    fn lock(&self) -> Lock<'_> {
        // SAFETY: the loop remains alive through the guard, on its owner thread.
        unsafe { pa::pa_threaded_mainloop_lock(self.ptr.as_ptr()) };
        Lock(self)
    }
}
impl Drop for Lock<'_> {
    fn drop(&mut self) {
        // SAFETY: paired with lock on this thread.
        unsafe { pa::pa_threaded_mainloop_unlock(self.0.ptr.as_ptr()) };
    }
}
impl Drop for Mainloop {
    fn drop(&mut self) {
        // SAFETY: context and streams have released their Rc leases. Stop unlocked.
        unsafe {
            if self.started.get() {
                pa::pa_threaded_mainloop_stop(self.ptr.as_ptr());
            }
            pa::pa_threaded_mainloop_free(self.ptr.as_ptr());
        }
    }
}

/// Cancellation disables callbacks before their stable allocation is released,
/// including failed creation, timeout, disconnect and successful completion.
struct Query<T> {
    operation: Option<NonNull<pa::pa_operation>>,
    data: Box<UnsafeCell<T>>,
    mainloop: Rc<Mainloop>,
}
impl<T> Query<T> {
    fn new(mainloop: &Rc<Mainloop>, data: T) -> Self {
        Self {
            operation: None,
            data: Box::new(UnsafeCell::new(data)),
            mainloop: Rc::clone(mainloop),
        }
    }
    fn userdata(&self) -> *mut c_void {
        self.data.get().cast()
    }
    fn started(&mut self, operation: *mut pa::pa_operation) -> Result<()> {
        self.operation = NonNull::new(operation);
        if self.operation.is_none() {
            bail!("desktop audio query could not start");
        }
        Ok(())
    }
    fn wait(&self, connection: &Connection) -> Result<()> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let state = {
                let _lock = self.mainloop.lock();
                // SAFETY: started succeeded; Query owns the operation until Drop.
                unsafe { pa::pa_operation_get_state(self.operation.unwrap().as_ptr()) }
            };
            match state {
                pa::PA_OPERATION_DONE => return Ok(()),
                pa::PA_OPERATION_CANCELLED => bail!("desktop audio query was cancelled"),
                _ if !connection.alive() => bail!("desktop audio server disconnected"),
                _ if Instant::now() >= deadline => bail!("desktop audio query timed out"),
                _ => thread::sleep(Duration::from_millis(2)),
            }
        }
    }
}
impl<T> Drop for Query<T> {
    fn drop(&mut self) {
        let _lock = self.mainloop.lock();
        if let Some(operation) = self.operation.take() {
            // SAFETY: cancellation forbids further callbacks before data is freed.
            unsafe {
                pa::pa_operation_cancel(operation.as_ptr());
                pa::pa_operation_unref(operation.as_ptr());
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct Device {
    pub output: OutputDevice,
    rate: u32,
    channels: u8,
}
pub(super) struct Connection {
    context: NonNull<pa::pa_context>,
    mainloop: Rc<Mainloop>,
    pub dirty: Arc<AtomicBool>,
}
impl Connection {
    pub fn connect() -> Result<Self> {
        // SAFETY: checked constructors with static names and matching native API.
        let mainloop = Rc::new(Mainloop {
            ptr: NonNull::new(unsafe { pa::pa_threaded_mainloop_new() })
                .context("create Pulse mainloop")?,
            started: Cell::new(false),
        });
        let context = NonNull::new(unsafe {
            pa::pa_context_new(
                pa::pa_threaded_mainloop_get_api(mainloop.ptr.as_ptr()),
                c"Klicky".as_ptr(),
            )
        })
        .context("create Pulse context")?;
        let connection = Self {
            context,
            mainloop,
            dirty: Arc::new(AtomicBool::new(true)),
        };
        if unsafe { pa::pa_threaded_mainloop_start(connection.mainloop.ptr.as_ptr()) } < 0 {
            bail!("start Pulse mainloop");
        }
        connection.mainloop.started.set(true);
        {
            let _lock = connection.mainloop.lock();
            if unsafe {
                pa::pa_context_connect(
                    context.as_ptr(),
                    ptr::null(),
                    pa::PA_CONTEXT_NOAUTOSPAWN,
                    ptr::null(),
                )
            } < 0
            {
                return Err(connection.error()).context("connect to desktop audio server");
            }
        }
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let _lock = connection.mainloop.lock();
            match unsafe { pa::pa_context_get_state(context.as_ptr()) } {
                pa::PA_CONTEXT_READY => {
                    // Readiness and subscription creation share one lock.
                    unsafe {
                        pa::pa_context_set_subscribe_callback(
                            context.as_ptr(),
                            Some(changed),
                            Arc::as_ptr(&connection.dirty).cast_mut().cast(),
                        );
                        let operation = pa::pa_context_subscribe(
                            context.as_ptr(),
                            pa::PA_SUBSCRIPTION_MASK_SINK | pa::PA_SUBSCRIPTION_MASK_SERVER,
                            None,
                            ptr::null_mut(),
                        );
                        if operation.is_null() {
                            bail!("subscribe to desktop audio changes");
                        }
                        pa::pa_operation_unref(operation);
                    }
                    break;
                }
                pa::PA_CONTEXT_FAILED | pa::PA_CONTEXT_TERMINATED => {
                    return Err(connection.error()).context("connect to desktop audio server")
                }
                _ if Instant::now() >= deadline => bail!("desktop audio connection timed out"),
                _ => (),
            }
            drop(_lock);
            thread::sleep(Duration::from_millis(2));
        }
        Ok(connection)
    }
    // Internal native context access requires the mainloop lock.
    fn error(&self) -> Error {
        Error(unsafe { pa::pa_context_errno(self.context.as_ptr()) })
    }
    fn ready(&self) -> Result<()> {
        if unsafe { pa::pa_context_get_state(self.context.as_ptr()) } != pa::PA_CONTEXT_READY {
            bail!("desktop audio server disconnected");
        }
        Ok(())
    }
    pub fn alive(&self) -> bool {
        let _lock = self.mainloop.lock();
        self.ready().is_ok()
    }
    fn server_default(&self) -> Result<Option<String>> {
        let mut query = Query::new(&self.mainloop, Server::default());
        {
            let _lock = self.mainloop.lock();
            self.ready()?;
            let operation = unsafe {
                pa::pa_context_get_server_info(
                    self.context.as_ptr(),
                    Some(server_info),
                    query.userdata(),
                )
            };
            query.started(operation)?;
        }
        query.wait(self)?;
        let _lock = self.mainloop.lock();
        // SAFETY: serialized with the callback by the mainloop lock.
        let data = unsafe { &mut *query.data.get() };
        if !data.received {
            bail!("desktop server query failed");
        }
        Ok(data.name.take())
    }
    pub fn default_id(&self) -> Result<String> {
        self.server_default()?
            .context("no default desktop audio output")
    }
    pub fn responsive(&self) -> bool {
        self.server_default().is_ok()
    }
    fn query_devices(&self, name: Option<&CStr>) -> Result<Vec<Device>> {
        let mut query = Query::new(&self.mainloop, Devices::default());
        {
            let _lock = self.mainloop.lock();
            self.ready()?;
            let operation = unsafe {
                match name {
                    Some(name) => pa::pa_context_get_sink_info_by_name(
                        self.context.as_ptr(),
                        name.as_ptr(),
                        Some(sink_info),
                        query.userdata(),
                    ),
                    None => pa::pa_context_get_sink_info_list(
                        self.context.as_ptr(),
                        Some(sink_info),
                        query.userdata(),
                    ),
                }
            };
            query.started(operation)?;
        }
        query.wait(self)?;
        let _lock = self.mainloop.lock();
        // SAFETY: serialized with the callback by the mainloop lock.
        let data = unsafe { &mut *query.data.get() };
        if data.failed {
            bail!("desktop output query failed or exceeded its limit");
        }
        Ok(std::mem::take(&mut data.devices))
    }
    fn devices(&self) -> Result<Vec<Device>> {
        let devices = self.query_devices(None)?;
        OutputList {
            schema_version: 1,
            devices: devices.iter().map(|d| d.output.clone()).collect(),
            default_id: None,
            suggested_speakers_id: None,
        }
        .validate()?;
        Ok(devices)
    }
    pub fn outputs(&self) -> Result<OutputList> {
        let devices = self
            .devices()?
            .into_iter()
            .map(|d| d.output)
            .collect::<Vec<_>>();
        let default_id = self
            .default_id()
            .ok()
            .filter(|id| devices.iter().any(|d| d.id == *id));
        let list = OutputList {
            schema_version: 1,
            devices,
            default_id,
            suggested_speakers_id: None,
        };
        list.validate()?;
        Ok(list)
    }
    pub fn resolve(&self, preference: &OutputPreference) -> Result<Device> {
        super::validate_preference(preference)?;
        let id = match preference {
            OutputPreference::SystemDefault => self.default_id()?,
            OutputPreference::Device { id, .. } => id.clone(),
        };
        OutputPreference::Device {
            backend: Backend::PulseAudio,
            id: id.clone(),
        }
        .validate()?;
        let name = CString::new(id.as_bytes())?;
        // Playback needs only this sink, even when the bounded control-plane
        // enumeration cannot represent all of the server's other outputs.
        self.query_devices(Some(&name))
            .context("selected desktop audio output is unavailable")?
            .into_iter()
            .find(|d| d.output.id == id)
            .context("selected desktop audio output is unavailable")
    }
    pub fn open(&mut self, device: Device) -> Result<Output> {
        let channels = NonZeroU16::new(u16::from(device.channels))
            .context("audio output has zero channels")?;
        let rate = NonZeroU32::new(device.rate).context("audio output has zero rate")?;
        let spec = pa::pa_sample_spec {
            format: pa::PA_SAMPLE_FLOAT32NE,
            channels: device.channels,
            rate: device.rate,
        };
        if unsafe { pa::pa_sample_spec_valid(&spec) } == 0 {
            bail!("invalid audio sample specification");
        }
        let (mixer, samples) = mixer(channels, rate);
        let id = CString::new(device.output.id.as_bytes())?;
        let mut output = {
            let _lock = self.mainloop.lock();
            self.ready()?;
            let stream = NonNull::new(unsafe {
                pa::pa_stream_new(
                    self.context.as_ptr(),
                    c"Keyboard sounds".as_ptr(),
                    &spec,
                    ptr::null(),
                )
            })
            .context("create desktop playback stream")?;
            Output {
                stream,
                mainloop: Rc::clone(&self.mainloop),
                mixer,
                device: device.output,
                channels,
                rate,
                render: Box::new(UnsafeCell::new(Render {
                    samples,
                    failed: false,
                    frame_bytes: usize::from(device.channels) * 4,
                })),
            }
        };
        {
            let _lock = self.mainloop.lock();
            let frame_bytes = u32::from(device.channels) * 4;
            let buffer = pa::pa_buffer_attr {
                maxlength: frame_bytes * 1024,
                tlength: frame_bytes * 256,
                prebuf: 0,
                minreq: frame_bytes * 128,
                fragsize: u32::MAX,
            };
            unsafe {
                pa::pa_stream_set_write_callback(
                    output.stream.as_ptr(),
                    Some(write_samples),
                    output.render.get().cast(),
                );
                if pa::pa_stream_connect_playback(
                    output.stream.as_ptr(),
                    id.as_ptr(),
                    &buffer,
                    pa::PA_STREAM_DONT_MOVE | pa::PA_STREAM_ADJUST_LATENCY,
                    ptr::null(),
                    ptr::null_mut(),
                ) < 0
                {
                    return Err(self.error()).context("connect desktop playback stream");
                }
            }
        }
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let _lock = self.mainloop.lock();
            match unsafe { pa::pa_stream_get_state(output.stream.as_ptr()) } {
                pa::PA_STREAM_READY => break,
                pa::PA_STREAM_FAILED | pa::PA_STREAM_TERMINATED => {
                    bail!("desktop playback stream failed")
                }
                _ if Instant::now() >= deadline => {
                    bail!("desktop playback stream initialization timed out")
                }
                _ => (),
            }
            drop(_lock);
            thread::sleep(Duration::from_millis(2));
        }
        if !output.healthy() {
            bail!("desktop playback route did not match the requested output");
        }
        Ok(output)
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        let _lock = self.mainloop.lock();
        // SAFETY: the owner releases its stream first. Detach callbacks before
        // releasing context/userdata; the last Rc stops the mainloop unlocked.
        unsafe {
            pa::pa_context_set_subscribe_callback(self.context.as_ptr(), None, ptr::null_mut());
            pa::pa_context_disconnect(self.context.as_ptr());
            pa::pa_context_unref(self.context.as_ptr());
        }
    }
}

#[derive(Default)]
struct Server {
    name: Option<String>,
    received: bool,
}
#[derive(Default)]
struct Devices {
    devices: Vec<Device>,
    failed: bool,
}
// SAFETY for native callbacks: libpulse holds the mainloop lock; userdata points
// to the stable owner allocation, cancelled/detached under that lock before drop.
// Native strings and metadata are valid only for the duration of each callback.
unsafe fn text(value: *const c_char) -> Option<String> {
    if value.is_null() {
        None
    } else {
        CStr::from_ptr(value).to_str().ok().map(str::to_owned)
    }
}
extern "C" fn changed(
    _: *mut pa::pa_context,
    _: pa::pa_subscription_event_type_t,
    _: u32,
    data: *mut c_void,
) {
    unsafe { &*data.cast::<AtomicBool>() }.store(true, Ordering::Release);
}
extern "C" fn server_info(
    _: *mut pa::pa_context,
    info: *const pa::pa_server_info,
    data: *mut c_void,
) {
    if !info.is_null() {
        let query = unsafe { &mut *data.cast::<Server>() };
        query.name = unsafe { text((*info).default_sink_name) };
        query.received = true;
    }
}
extern "C" fn sink_info(
    _: *mut pa::pa_context,
    info: *const pa::pa_sink_info,
    end: i32,
    data: *mut c_void,
) {
    let query = unsafe { &mut *data.cast::<Devices>() };
    if end < 0 {
        query.failed = true;
        return;
    }
    if end > 0 {
        return;
    }
    if info.is_null() || query.devices.len() >= 64 {
        query.failed = true;
        return;
    }
    let info = unsafe { &*info };
    let Some(id) = (unsafe { text(info.name) }) else {
        query.failed = true;
        return;
    };
    query.devices.push(Device {
        output: OutputDevice {
            backend: Backend::PulseAudio,
            id,
            name: unsafe { text(info.description) }.filter(|s| !s.is_empty() && s.len() <= 2048),
        },
        channels: info.sample_spec.channels,
        rate: info.sample_spec.rate,
    });
}
struct Render {
    samples: MixerSource,
    failed: bool,
    frame_bytes: usize,
}
extern "C" fn write_samples(stream: *mut pa::pa_stream, requested: usize, data: *mut c_void) {
    let render = unsafe { &mut *data.cast::<Render>() };
    if render.failed {
        return;
    }
    // Never unwind through C. Callback work is limited to prepared PCM.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        let mut length = requested.min(16 * 1024) / render.frame_bytes * render.frame_bytes;
        if length == 0 {
            return true;
        }
        let mut buffer = ptr::null_mut();
        if pa::pa_stream_begin_write(stream, &mut buffer, &mut length) < 0 {
            return false;
        }
        if buffer.is_null() || length < render.frame_bytes {
            pa::pa_stream_cancel_write(stream);
            return false;
        }
        length = length / render.frame_bytes * render.frame_bytes;
        let bytes = std::slice::from_raw_parts_mut(buffer.cast::<u8>(), length);
        for chunk in bytes.chunks_exact_mut(4) {
            chunk.copy_from_slice(&render.samples.next().unwrap_or(0.0).to_ne_bytes());
        }
        pa::pa_stream_write(stream, buffer, length, None, 0, pa::PA_SEEK_RELATIVE) == 0
    }));
    if !matches!(result, Ok(true)) {
        render.failed = true;
    }
}
pub(super) struct Output {
    stream: NonNull<pa::pa_stream>,
    mainloop: Rc<Mainloop>,
    render: Box<UnsafeCell<Render>>,
    pub mixer: Mixer,
    pub device: OutputDevice,
    pub channels: NonZeroU16,
    pub rate: NonZeroU32,
}
impl Output {
    pub fn healthy(&mut self) -> bool {
        let _lock = self.mainloop.lock();
        unsafe {
            !(*self.render.get()).failed
                && pa::pa_stream_get_state(self.stream.as_ptr()) == pa::PA_STREAM_READY
                && pa::pa_stream_is_suspended(self.stream.as_ptr()) == 0
                && text(pa::pa_stream_get_device_name(self.stream.as_ptr()))
                    .is_some_and(|id| id == self.device.id)
        }
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        let _lock = self.mainloop.lock();
        // SAFETY: callback detachment under lock ends all access to Render.
        unsafe {
            pa::pa_stream_set_write_callback(self.stream.as_ptr(), None, ptr::null_mut());
            pa::pa_stream_disconnect(self.stream.as_ptr());
            pa::pa_stream_unref(self.stream.as_ptr());
        }
    }
}
