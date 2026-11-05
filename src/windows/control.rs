//! Bounded Windows control transport. Blocking client calls belong on workers.
use std::fmt;
use std::os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ::windows::core::PCWSTR;
use ::windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY,
    ERROR_PIPE_NOT_CONNECTED, ERROR_SEM_TIMEOUT, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use ::windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_OVERLAPPED, FILE_SHARE_MODE, OPEN_EXISTING,
};
use ::windows::Win32::System::Pipes::{DisconnectNamedPipe, WaitNamedPipeW};
use ::windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};

use super::pipe::{io, PipeAddress, PipePool, CLIENT_ACCESS};
use super::protocol::{
    Command, ErrorCode, FrameBuffer, Outcome, ProtocolError, Request, Response, PROTOCOL_MAJOR,
};
use super::ServerProcess;

pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
pub const CONTROL_CAPACITY: usize = 32;
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);
const RECEIPT: [u8; 1] = [6];
pub type Result<T> = std::result::Result<T, ControlError>;

#[derive(Debug)]
pub enum ControlError {
    /// Only the pipe is absent. Offline writes must additionally acquire ownership.
    Absent,
    Busy,
    PermissionDenied,
    Timeout,
    Disconnected,
    Stopped,
    InvalidWait,
    ControllerClosed,
    WorkerPanicked,
    Protocol(ProtocolError),
    Peer(anyhow::Error),
    Native(::windows::core::Error),
    Io(std::io::Error),
    Remote {
        code: ErrorCode,
        message: String,
    },
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => f.write_str("Windows control pipe is absent"),
            Self::Busy => f.write_str("Windows control pipe is busy"),
            Self::PermissionDenied => f.write_str("Windows control access denied"),
            Self::Timeout => {
                f.write_str("Windows control deadline expired; outcome may be unknown")
            }
            Self::Disconnected => f.write_str("Windows control peer disconnected"),
            Self::Stopped => f.write_str("Windows control transport is stopping"),
            Self::InvalidWait => f.write_str("unexpected Windows control wait result"),
            Self::ControllerClosed => f.write_str("Windows engine control channel closed"),
            Self::WorkerPanicked => f.write_str("Windows control worker panicked"),
            Self::Protocol(error) => write!(f, "{error}"),
            Self::Peer(error) => write!(f, "Windows server verification failed: {error}"),
            Self::Native(error) => write!(f, "Windows control I/O: {error}"),
            Self::Io(error) => write!(f, "Windows control worker: {error}"),
            Self::Remote { code, message } => write!(f, "Windows engine {code:?}: {message}"),
        }
    }
}

impl std::error::Error for ControlError {}
impl From<ProtocolError> for ControlError {
    fn from(error: ProtocolError) -> Self {
        Self::Protocol(error)
    }
}
impl From<::windows::core::Error> for ControlError {
    fn from(error: ::windows::core::Error) -> Self {
        if error.code() == ERROR_ACCESS_DENIED.to_hresult() {
            Self::PermissionDenied
        } else {
            Self::Native(error)
        }
    }
}

pub struct PendingCommand {
    pub request: Request,
    pub deadline: Instant,
    reply: Sender<Outcome>,
    delivery: Option<Receiver<()>>,
}

impl PendingCommand {
    pub fn local(command: Command, deadline: Instant) -> Result<(Self, Receiver<Outcome>)> {
        let request = Request::new(request_id(), command);
        request.validate()?;
        let (reply, receiver) = bounded(1);
        Ok((
            Self {
                request,
                deadline,
                reply,
                delivery: None,
            },
            receiver,
        ))
    }

    /// A remote receipt channel is returned when queued. Its disconnection is not
    /// delivery. Local requests and vanished receivers return None. Neither queue
    /// acceptance nor receipt confirms process exit.
    pub fn respond(self, outcome: Outcome) -> Option<Receiver<()>> {
        if self.reply.try_send(outcome).is_ok() {
            self.delivery
        } else {
            None
        }
    }
}

/// Workers retain the pool and lease until every pending operation has drained.
/// Drop requests shutdown; only shutdown() confirms worker exit, not process exit.
pub struct Server {
    pool: Arc<PipePool>,
    stop: Arc<OwnedHandle>,
    broadcast: Option<Sender<()>>,
    commands: Sender<PendingCommand>,
    workers: Vec<JoinHandle<()>>,
}

impl Server {
    pub fn start(
        pool: PipePool,
    ) -> Result<(Self, Receiver<PendingCommand>, Receiver<ControlError>)> {
        let count = pool.handles().len();
        let pool = Arc::new(pool);
        let stop = unsafe { CreateEventW(None, true, false, None)? };
        let stop = Arc::new(unsafe { OwnedHandle::from_raw_handle(stop.0) });
        let (broadcast, stopped) = bounded::<()>(0);
        let (commands, receiver) = bounded(CONTROL_CAPACITY);
        let (failure_tx, failures) = bounded(count);
        let mut server = Self {
            pool,
            stop,
            broadcast: Some(broadcast),
            commands,
            workers: Vec::with_capacity(count),
        };
        for index in 0..count {
            let pool = Arc::clone(&server.pool);
            let stop = Arc::clone(&server.stop);
            let stopped = stopped.clone();
            let commands = server.commands.clone();
            let failures = failure_tx.clone();
            let worker = thread::Builder::new()
                .name(format!("klicky-control-{index}"))
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        serve(&pool, index, &stop, &stopped, &commands)
                    }))
                    .unwrap_or(Err(ControlError::WorkerPanicked));
                    if let Err(error) = result {
                        let _ = failures.try_send(error);
                    }
                })
                .map_err(ControlError::Io)?;
            server.workers.push(worker);
        }
        Ok((server, receiver, failures))
    }

    pub fn commands(&self) -> Sender<PendingCommand> {
        self.commands.clone()
    }

    pub fn shutdown(&mut self, deadline: Instant) -> Result<()> {
        self.request_shutdown()?;
        while let Some(worker) = self.workers.last() {
            loop {
                match unsafe {
                    WaitForSingleObject(
                        HANDLE(worker.as_raw_handle()),
                        io::wait_milliseconds(deadline),
                    )
                } {
                    WAIT_OBJECT_0 => break,
                    WAIT_TIMEOUT if Instant::now() >= deadline => {
                        return Err(ControlError::Timeout)
                    }
                    WAIT_TIMEOUT => continue,
                    WAIT_FAILED => return Err(::windows::core::Error::from_thread().into()),
                    _ => return Err(ControlError::InvalidWait),
                }
            }
            if self.workers.pop().expect("observed worker").join().is_err() {
                return Err(ControlError::WorkerPanicked);
            }
        }
        Ok(())
    }

    pub fn has_exited(&self) -> bool {
        self.workers.iter().all(JoinHandle::is_finished)
    }

    pub fn request_shutdown(&mut self) -> Result<()> {
        self.broadcast.take();
        unsafe { SetEvent(HANDLE(self.stop.as_raw_handle()))? };
        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.request_shutdown();
    }
}

fn serve(
    pool: &PipePool,
    index: usize,
    stop: &OwnedHandle,
    stopped: &Receiver<()>,
    commands: &Sender<PendingCommand>,
) -> Result<()> {
    let pipe = pool.handles().nth(index).expect("fixed pool worker index");
    loop {
        if matches!(
            stopped.try_recv(),
            Err(crossbeam_channel::TryRecvError::Disconnected)
        ) {
            return Ok(());
        }
        match io::connect(pipe, stop.as_handle()) {
            Ok(()) => (),
            Err(ControlError::Stopped) => return Ok(()),
            Err(error) if peer_closed(&error) => {
                unsafe {
                    let _ = DisconnectNamedPipe(HANDLE(pipe.as_raw_handle()));
                }
                continue;
            }
            Err(error) => return Err(error),
        }
        let result = exchange(pool, pipe, stop, stopped, commands);
        unsafe {
            let _ = DisconnectNamedPipe(HANDLE(pipe.as_raw_handle()));
        }
        match result {
            Err(ControlError::Stopped) => return Ok(()),
            Err(ControlError::ControllerClosed) => return Err(ControlError::ControllerClosed),
            _ => (), // Malformed, disconnected and stalled clients only lose their own connection.
        }
    }
}

fn exchange(
    pool: &PipePool,
    pipe: std::os::windows::io::BorrowedHandle<'_>,
    stop: &OwnedHandle,
    stopped: &Receiver<()>,
    commands: &Sender<PendingCommand>,
) -> Result<()> {
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    let bytes = read_frame(
        pipe,
        FrameBuffer::request(),
        deadline,
        Some(stop.as_handle()),
    )?;
    let mut delivery = None;
    let (request_id, outcome) = match Request::decode(&bytes) {
        Ok(request) => {
            let request_id = request.request_id;
            let (reply, received) = bounded(1);
            let (delivered, receipt) = bounded(1);
            delivery = Some(delivered);
            match commands.try_send(PendingCommand {
                request,
                deadline,
                reply,
                delivery: Some(receipt),
            }) {
                Ok(()) => (),
                Err(TrySendError::Full(_)) => {
                    return send_outcome(
                        pool,
                        pipe,
                        request_id,
                        failure(ErrorCode::Busy, "engine control queue is full"),
                        deadline,
                        stop,
                    )
                }
                Err(TrySendError::Disconnected(_)) => return Err(ControlError::ControllerClosed),
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let outcome = crossbeam_channel::select_biased! {
                recv(stopped) -> _ => return Err(ControlError::Stopped),
                recv(received) -> value => value.unwrap_or_else(|_| failure(ErrorCode::Unavailable,"engine did not supply a result")),
                default(remaining) => return Err(ControlError::Timeout),
            };
            (request_id, outcome)
        }
        Err(ProtocolError::ProtocolMismatch { request_id, .. }) => (
            request_id,
            failure(
                ErrorCode::ProtocolMismatch,
                "control protocol major is incompatible",
            ),
        ),
        Err(error) => return Err(error.into()),
    };
    send_outcome(pool, pipe, request_id, outcome, deadline, stop)?;
    if let Some(delivery) = delivery {
        let _ = delivery.try_send(());
    }
    Ok(())
}

fn send_outcome(
    pool: &PipePool,
    pipe: std::os::windows::io::BorrowedHandle<'_>,
    request_id: u64,
    outcome: Outcome,
    deadline: Instant,
    stop: &OwnedHandle,
) -> Result<()> {
    let mut response = Response {
        protocol: PROTOCOL_MAJOR,
        request_id,
        instance: pool.identity().clone(),
        outcome,
    };
    let frame = match response.encode() {
        Ok(frame) => frame,
        Err(_) => {
            response.outcome = failure(ErrorCode::Internal, "engine supplied an invalid result");
            response.encode()?
        }
    };
    io::write_all(pipe, &frame, deadline, Some(stop.as_handle()))?;
    // Write completion only means buffered bytes, not client consumption. Avoid
    // blocking FlushFileBuffers: require the client's bounded, validated receipt.
    if io::read(pipe, deadline, Some(stop.as_handle()))? != RECEIPT {
        return Err(ControlError::Protocol(ProtocolError::TrailingData));
    }
    Ok(())
}

fn read_frame(
    pipe: std::os::windows::io::BorrowedHandle<'_>,
    mut buffer: FrameBuffer,
    deadline: Instant,
    stop: Option<std::os::windows::io::BorrowedHandle<'_>>,
) -> Result<Vec<u8>> {
    loop {
        if Instant::now() >= deadline {
            return Err(ControlError::Timeout);
        }
        let chunk = io::read(pipe, deadline, stop)?;
        if chunk.is_empty() {
            return Err(ControlError::Disconnected);
        }
        if buffer.feed(&chunk)? {
            return Ok(buffer.finish()?);
        }
    }
}

pub struct Reply {
    pub response: Response,
    pub process: ServerProcess,
}

pub struct Client {
    address: PipeAddress,
}
impl Client {
    pub fn current() -> Result<Self> {
        Ok(Self {
            address: PipeAddress::current().map_err(ControlError::Peer)?,
        })
    }

    /// Read-only handshake first; verify logon/process before sending a mutation.
    /// Keep on a worker: cancellation drains retain kernel-owned buffers safely.
    pub fn command(&self, command: Command, deadline: Instant) -> Result<Reply> {
        let status = Request::new(request_id(), Command::Status {});
        let pipe = self.connect(deadline)?;
        let response = self.exchange(&pipe, &status, deadline)?;
        response.validate_for(&status, &response.instance)?;
        let process = ServerProcess::verify(pipe.as_handle(), &response.instance)
            .map_err(ControlError::Peer)?;
        let identity = process.identity().clone();
        io::write_all(pipe.as_handle(), &RECEIPT, deadline, None)?;
        check_remote(&response)?;
        drop(pipe);
        if matches!(command, Command::Status {}) {
            return Ok(Reply { response, process });
        }
        let request = Request::new(request_id(), command);
        let pipe = self.connect(deadline)?;
        let _verified =
            ServerProcess::verify(pipe.as_handle(), &identity).map_err(ControlError::Peer)?;
        let response = self.exchange(&pipe, &request, deadline)?;
        response.validate_for(&request, &identity)?;
        io::write_all(pipe.as_handle(), &RECEIPT, deadline, None)?;
        check_remote(&response)?;
        Ok(Reply { response, process })
    }

    pub fn stop(&self, deadline: Instant) -> Result<()> {
        let reply = self.command(Command::Stop {}, deadline)?;
        if !reply
            .process
            .wait_until(deadline)
            .map_err(ControlError::Peer)?
        {
            return Err(ControlError::Timeout);
        }
        Ok(())
    }

    fn exchange(
        &self,
        pipe: &OwnedHandle,
        request: &Request,
        deadline: Instant,
    ) -> Result<Response> {
        io::write_all(pipe.as_handle(), &request.encode()?, deadline, None)?;
        let frame = read_frame(pipe.as_handle(), FrameBuffer::response(), deadline, None)?;
        Ok(Response::decode(&frame)?)
    }

    fn connect(&self, deadline: Instant) -> Result<OwnedHandle> {
        loop {
            if Instant::now() >= deadline {
                return Err(ControlError::Timeout);
            }
            let result = unsafe {
                CreateFileW(
                    PCWSTR(self.address.name().as_ptr()),
                    CLIENT_ACCESS,
                    FILE_SHARE_MODE::default(),
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | FILE_FLAGS_AND_ATTRIBUTES(0x0010_0000),
                    None,
                )
            };
            match result {
                Ok(handle) => return Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) }),
                Err(error) if error.code() == ERROR_FILE_NOT_FOUND.to_hresult() => {
                    return Err(ControlError::Absent)
                }
                Err(error) if error.code() == ERROR_PIPE_BUSY.to_hresult() => {
                    let available = unsafe {
                        WaitNamedPipeW(
                            PCWSTR(self.address.name().as_ptr()),
                            io::wait_milliseconds(deadline),
                        )
                    };
                    if !available.as_bool() {
                        let error = ::windows::core::Error::from_thread();
                        if error.code() != ERROR_SEM_TIMEOUT.to_hresult()
                            && error.code() != ERROR_FILE_NOT_FOUND.to_hresult()
                        {
                            return Err(error.into());
                        }
                    }
                    if Instant::now() >= deadline {
                        return Err(ControlError::Busy);
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

fn request_id() -> u64 {
    loop {
        let value = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
        if value != 0 {
            return value;
        }
    }
}
fn failure(code: ErrorCode, message: &str) -> Outcome {
    Outcome::Failure {
        code,
        message: message.to_owned(),
    }
}
fn check_remote(response: &Response) -> Result<()> {
    if let Outcome::Failure { code, message } = &response.outcome {
        return Err(ControlError::Remote {
            code: *code,
            message: message.clone(),
        });
    }
    Ok(())
}
fn peer_closed(error: &ControlError) -> bool {
    matches!(error, ControlError::Native(error) if [ERROR_BROKEN_PIPE,ERROR_NO_DATA,ERROR_PIPE_NOT_CONNECTED].iter().any(|code|error.code()==code.to_hresult()))
}
