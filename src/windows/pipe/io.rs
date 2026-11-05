use std::cell::UnsafeCell;
use std::os::windows::io::{AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::rc::Rc;
use std::time::Instant;

use ::windows::Win32::Foundation::{
    ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE, WAIT_FAILED,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use ::windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use ::windows::Win32::System::Pipes::ConnectNamedPipe;
use ::windows::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForMultipleObjects};
use ::windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

use crate::windows::control::{ControlError, Result};

/// Own every kernel-referenced address until completion, including cancellation.
/// Only worker threads may drop a pending operation: draining can block in a
/// faulty driver. The supervisor bounds its join, retaining worker/owner lifetime.
struct Operation<'a> {
    pipe: BorrowedHandle<'a>,
    event: OwnedHandle,
    // Do not create Rust references to memory the kernel may still be updating.
    overlapped: Rc<UnsafeCell<OVERLAPPED>>,
    buffer: Vec<u8>,
    pending: bool,
    connect: bool,
}

impl<'a> Operation<'a> {
    fn new(pipe: BorrowedHandle<'a>, buffer: Vec<u8>, connect: bool) -> Result<Self> {
        let event = unsafe { CreateEventW(None, true, false, None)? };
        let event = unsafe { OwnedHandle::from_raw_handle(event.0) };
        let overlapped = Rc::new(UnsafeCell::new(OVERLAPPED {
            hEvent: HANDLE(event.as_raw_handle()),
            ..Default::default()
        }));
        Ok(Self {
            pipe,
            event,
            overlapped,
            buffer,
            pending: false,
            connect,
        })
    }

    fn started(&mut self, result: ::windows::core::Result<()>) -> Result<()> {
        match result {
            Ok(()) => self.pending = true,
            Err(error) if error.code() == ERROR_IO_PENDING.to_hresult() => self.pending = true,
            Err(error) if self.connect && error.code() == ERROR_PIPE_CONNECTED.to_hresult() => {
                self.pending = false
            }
            Err(error) => {
                self.pending = false;
                return Err(error.into());
            }
        }
        Ok(())
    }

    fn complete(&mut self, wait: bool) -> ::windows::core::Result<u32> {
        let mut transferred = 0;
        let result = unsafe {
            GetOverlappedResult(
                HANDLE(self.pipe.as_raw_handle()),
                self.overlapped.get(),
                &mut transferred,
                wait,
            )
        };
        if result
            .as_ref()
            .err()
            .is_none_or(|error| error.code() != ERROR_IO_INCOMPLETE.to_hresult())
        {
            self.pending = false;
        }
        result?;
        // ConnectNamedPipe does not define the transferred-byte output.
        Ok(if self.connect { 0 } else { transferred })
    }

    fn wait(&mut self, deadline: Option<Instant>, stop: Option<BorrowedHandle<'_>>) -> Result<u32> {
        if !self.pending {
            return Ok(0);
        }
        let event = HANDLE(self.event.as_raw_handle());
        let handles = match stop {
            Some(stop) => vec![HANDLE(stop.as_raw_handle()), event],
            None => vec![event],
        };
        loop {
            let timeout = match deadline {
                Some(deadline) => wait_milliseconds(deadline),
                None => u32::MAX,
            };
            match unsafe { WaitForMultipleObjects(&handles, false, timeout) } {
                result if result == WAIT_OBJECT_0 && stop.is_some() => {
                    return Err(ControlError::Stopped)
                }
                result if result.0 == WAIT_OBJECT_0.0 + u32::from(stop.is_some()) => {
                    return Ok(self.complete(false)?);
                }
                WAIT_TIMEOUT if deadline.is_some_and(|at| Instant::now() >= at) => {
                    return Err(ControlError::Timeout);
                }
                WAIT_TIMEOUT => continue,
                WAIT_FAILED => return Err(::windows::core::Error::from_thread().into()),
                _ => return Err(ControlError::InvalidWait),
            }
        }
    }
}

impl Drop for Operation<'_> {
    fn drop(&mut self) {
        if !self.pending {
            return;
        }
        // Cancellation requests are not completion. Never free these buffers on
        // a request timeout, ERROR_NOT_FOUND race, or an unconfirmed cancellation.
        unsafe {
            let _ = CancelIoEx(
                HANDLE(self.pipe.as_raw_handle()),
                Some(self.overlapped.get()),
            );
        }
        while self.pending {
            let _ = self.complete(true);
            if self.pending {
                unsafe {
                    let _ = ResetEvent(HANDLE(self.event.as_raw_handle()));
                }
            }
        }
    }
}

pub(crate) fn connect(pipe: BorrowedHandle<'_>, stop: BorrowedHandle<'_>) -> Result<()> {
    let mut operation = Operation::new(pipe, Vec::new(), true)?;
    operation.pending = true;
    let result = unsafe {
        ConnectNamedPipe(
            HANDLE(pipe.as_raw_handle()),
            Some(operation.overlapped.get()),
        )
    };
    operation.started(result)?;
    operation.wait(None, Some(stop))?;
    Ok(())
}

pub(crate) fn read(
    pipe: BorrowedHandle<'_>,
    deadline: Instant,
    stop: Option<BorrowedHandle<'_>>,
) -> Result<Vec<u8>> {
    let mut operation = Operation::new(pipe, vec![0; 1024], false)?;
    operation.pending = true;
    let result = unsafe {
        ReadFile(
            HANDLE(pipe.as_raw_handle()),
            Some(&mut operation.buffer),
            None,
            Some(operation.overlapped.get()),
        )
    };
    operation.started(result)?;
    let length = operation.wait(Some(deadline), stop)? as usize;
    operation.buffer.truncate(length);
    Ok(std::mem::take(&mut operation.buffer))
}

pub(crate) fn write_all(
    pipe: BorrowedHandle<'_>,
    mut bytes: &[u8],
    deadline: Instant,
    stop: Option<BorrowedHandle<'_>>,
) -> Result<()> {
    while !bytes.is_empty() {
        if Instant::now() >= deadline {
            return Err(ControlError::Timeout);
        }
        let mut operation = Operation::new(pipe, bytes.to_vec(), false)?;
        operation.pending = true;
        let result = unsafe {
            WriteFile(
                HANDLE(pipe.as_raw_handle()),
                Some(&operation.buffer),
                None,
                Some(operation.overlapped.get()),
            )
        };
        operation.started(result)?;
        let written = operation.wait(Some(deadline), stop)? as usize;
        if written == 0 {
            return Err(ControlError::Disconnected);
        }
        bytes = &bytes[written..];
    }
    Ok(())
}

pub(crate) fn wait_milliseconds(deadline: Instant) -> u32 {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        0
    } else {
        remaining.as_millis().clamp(1, 50) as u32
    }
}
