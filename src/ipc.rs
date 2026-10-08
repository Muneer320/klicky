use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config;

#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    Stop,
    Switch { pack: String },
    Volume { level: f32 },
    Status,
}

pub struct Request {
    pub command: Command,
    stream: UnixStream,
}

#[derive(Serialize, Deserialize)]
struct Response {
    pid: i32,
}

impl Request {
    pub fn acknowledge(&mut self) -> Result<()> {
        let response = serde_json::to_string(&Response {
            pid: std::process::id() as i32,
        })?;
        // A response means the daemon loop accepted the request, not that it exited.
        writeln!(self.stream, "{response}")?;
        Ok(())
    }
}

pub fn start_server(sender: Sender<Request>) -> Result<()> {
    let sock_path = config::socket_path();
    // The lifecycle owner has already reclaimed any stale socket under its lock.
    let listener = bind_listener(&sock_path)?;

    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            if stream.set_nonblocking(true).is_err() {
                continue;
            }
            let deadline = Instant::now() + Duration::from_millis(200);
            if let Ok(line) = read_line(&mut stream, deadline) {
                if let Ok(command) = serde_json::from_slice::<Command>(&line) {
                    if sender.send(Request { command, stream }).is_err() {
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

fn bind_listener(path: &Path) -> Result<UnixListener> {
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub fn send_command(cmd: &Command) -> Result<()> {
    let sock_path = config::socket_path();
    let mut stream = UnixStream::connect(&sock_path)?;
    let msg = serde_json::to_string(cmd)?;
    writeln!(stream, "{}", msg)?;
    Ok(())
}

/// Nonblocking connect and I/O share one deadline, including a full accept queue.
fn connect(path: &Path, deadline: Instant) -> Result<UnixStream> {
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    let bytes = path.as_os_str().as_bytes();
    if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        bail!("invalid Unix socket path");
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (destination, source) in address.sun_path.iter_mut().zip(bytes) {
        *destination = *source as libc::c_char;
    }
    #[cfg(target_os = "macos")]
    {
        address.sun_len = std::mem::size_of_val(&address) as u8;
    }
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // Ownership transfers immediately, so every error path closes the descriptor.
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error().into());
    }
    stream.set_nonblocking(true)?;
    let result = unsafe {
        libc::connect(
            fd,
            &address as *const _ as *const libc::sockaddr,
            std::mem::size_of_val(&address) as libc::socklen_t,
        )
    };
    if result < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(error.into());
        }
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .context("IPC connect timed out")?;
            let mut descriptor = libc::pollfd {
                fd: stream.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            let result = unsafe {
                libc::poll(
                    &mut descriptor,
                    1,
                    remaining.as_millis().max(1).min(i32::MAX as u128) as i32,
                )
            };
            if result > 0 {
                if let Some(error) = stream.take_error()? {
                    return Err(error.into());
                }
                break;
            }
            if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return Err(io::Error::last_os_error().into());
            }
        }
    }
    Ok(stream)
}

fn pause(deadline: Instant) -> Result<()> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .context("IPC timed out")?;
    thread::sleep(remaining.min(Duration::from_millis(2)));
    Ok(())
}

fn read_line(stream: &mut UnixStream, deadline: Instant) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    loop {
        if Instant::now() >= deadline {
            bail!("IPC response timed out");
        }
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => bail!("IPC connection closed before a complete message"),
            Ok(_) if byte[0] == b'\n' => return Ok(line),
            Ok(_) => {
                line.push(byte[0]);
                if line.len() > 4096 {
                    bail!("IPC message too large");
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => pause(deadline)?,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

pub fn request(path: &Path, command: &Command, deadline: Instant) -> Result<i32> {
    let mut stream = connect(path, deadline)?;
    let message = format!("{}\n", serde_json::to_string(command)?);
    let mut bytes = message.as_bytes();
    while !bytes.is_empty() {
        if Instant::now() >= deadline {
            bail!("IPC write timed out");
        }
        match stream.write(bytes) {
            Ok(0) => bail!("IPC connection closed while sending request"),
            Ok(count) => bytes = &bytes[count..],
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => pause(deadline)?,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let response: Response = serde_json::from_slice(&read_line(&mut stream, deadline)?)?;
    if response.pid <= 0 {
        bail!("invalid daemon PID in IPC response");
    }
    Ok(response.pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn incomplete_messages_are_bounded_and_disconnects_are_errors() -> Result<()> {
        let (mut client, server) = UnixStream::pair()?;
        client.set_nonblocking(true)?;
        assert!(read_line(&mut client, Instant::now()).is_err());
        drop(server);
        assert!(read_line(&mut client, Instant::now() + Duration::from_secs(1)).is_err());
        Ok(())
    }

    #[test]
    fn lifecycle_protocol_requires_valid_acknowledgment() -> Result<()> {
        for reply in [
            "not json\n",
            "{\"pid\":0}\n",
            "{\"pid\":-1}\n",
            "",
            "{\"pid\":123}\n",
        ] {
            let directory = std::env::temp_dir().join(format!(
                "klicky-protocol-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&directory)?;
            let path = directory.join("socket");
            let listener = bind_listener(&path)?;
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_nonblocking(true).unwrap();
                let line = read_line(&mut stream, Instant::now() + Duration::from_secs(1)).unwrap();
                assert!(matches!(
                    serde_json::from_slice::<Command>(&line).unwrap(),
                    Command::Status
                ));
                stream.write_all(reply.as_bytes()).unwrap();
            });
            let result = request(
                &path,
                &Command::Status,
                Instant::now() + Duration::from_secs(1),
            );
            server.join().unwrap();
            assert_eq!(result.is_ok(), reply == "{\"pid\":123}\n");
            std::fs::remove_dir_all(directory)?;
        }
        Ok(())
    }

    #[test]
    fn socket_is_accessible_only_by_its_owner() -> Result<()> {
        let directory = std::env::temp_dir().join(format!("klicky-ipc-{}", std::process::id()));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("klicky.sock");

        let listener = bind_listener(&path)?;
        let mode = std::fs::metadata(&path)?.permissions().mode() & 0o777;
        drop(listener);
        std::fs::remove_dir_all(directory)?;

        assert_eq!(mode, 0o600);
        Ok(())
    }
}
