//! Only the process holding the persistent lock may publish or remove runtime state.
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};

use crate::ipc::{self, Command};

pub const TIMEOUT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(10);

pub fn process_exists(pid: i32) -> bool {
    // Never pass process-group selectors (zero/negative values) to kill.
    pid > 0
        && (unsafe { libc::kill(pid, 0) } == 0
            || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

pub fn read_pid(directory: &Path) -> Result<Option<i32>> {
    match fs::read_to_string(directory.join("klicky.pid")) {
        Ok(text) => Ok(text.trim().parse().ok().filter(|pid| *pid > 0)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

struct Lock(File);

impl Drop for Lock {
    fn drop(&mut self) {
        // Explicit unlock also releases ownership if a concurrently forked child
        // still has a copy of this descriptor before its close-on-exec runs.
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

fn try_lock(directory: &Path) -> Result<Option<Lock>> {
    // Never unlink this file: replacing a locked inode would permit two owners.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join("klicky.lock"))?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        Ok(Some(Lock(file)))
    } else {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::WouldBlock {
            Ok(None)
        } else {
            Err(error.into())
        }
    }
}

fn remove_if_present(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub struct Owner {
    directory: PathBuf,
    // Drop runs cleanup before this field closes and releases the lock.
    _lock: Lock,
}

impl Owner {
    pub fn acquire(directory: &Path) -> Result<Option<Self>> {
        let Some(lock) = try_lock(directory)? else {
            return Ok(None);
        };
        if let Some(pid) = read_pid(directory)?.filter(|pid| process_exists(*pid)) {
            bail!("PID {pid} is alive without the lifecycle lock; refusing to replace its state");
        }
        let owner = Self {
            directory: directory.into(),
            _lock: lock,
        };
        // An exited/crashed predecessor cannot clean up after this owner.
        remove_if_present(&directory.join("klicky.sock"))?;
        remove_if_present(&directory.join("klicky.pid"))?;
        let path = directory.join("klicky.pid");
        fs::write(&path, std::process::id().to_string())?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        Ok(Some(owner))
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        for name in ["klicky.sock", "klicky.pid"] {
            if let Err(error) = remove_if_present(&self.directory.join(name)) {
                eprintln!("Failed to clean up {name}: {error:#}");
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct Status {
    pub process_exists: bool,
    pub responsive: bool,
}

/// A matching event-loop reply is not a keyboard or audio health check.
pub fn status(directory: &Path) -> Result<Status> {
    let Some(pid) = read_pid(directory)?.filter(|pid| process_exists(*pid)) else {
        return Ok(Status::default());
    };
    let response = ipc::request(
        &directory.join("klicky.sock"),
        &Command::Status,
        Instant::now() + Duration::from_millis(200),
    );
    let same_pid = read_pid(directory)? == Some(pid);
    let exists = same_pid && process_exists(pid);
    Ok(Status {
        process_exists: exists,
        responsive: exists && response.is_ok_and(|reply_pid| reply_pid == pid),
    })
}

pub fn responsive(directory: &Path) -> bool {
    status(directory).is_ok_and(|status| status.responsive)
}

pub fn wait_ready(directory: &Path) -> Result<()> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match ipc::request(&directory.join("klicky.sock"), &Command::Status, deadline) {
            Ok(pid) if read_pid(directory)? == Some(pid) && process_exists(pid) => return Ok(()),
            Ok(_) if Instant::now() >= deadline => {
                bail!("daemon readiness response does not match a live PID file")
            }
            Ok(_) => thread::sleep(POLL),
            Err(error) if Instant::now() >= deadline => {
                return Err(error.context("daemon did not become responsive within 3 seconds"))
            }
            Err(_) => thread::sleep(POLL),
        }
    }
}

/// Returns false for an already stopped daemon. The client never removes state.
pub fn stop(directory: &Path) -> Result<bool> {
    if !directory.try_exists()? {
        return Ok(false);
    }
    let deadline = Instant::now() + TIMEOUT;
    let mut target = None;
    loop {
        // Remember a live target even if it removes its PID before process exit.
        // A stale PID observed during startup is not a completed shutdown.
        if target.is_some_and(|pid| !process_exists(pid)) {
            return Ok(true);
        }
        if target.is_none() {
            target = read_pid(directory)?.filter(|pid| process_exists(*pid));
        }
        // Hold the lock while deciding 'already stopped' so startup cannot race
        // this observation. A held lock with no socket means starting/stopping.
        if let Some(_lock) = try_lock(directory)? {
            if target.is_none() {
                target = read_pid(directory)?.filter(|pid| process_exists(*pid));
            }
            if target.is_none() {
                return Ok(false);
            }
        }
        match ipc::request(&directory.join("klicky.sock"), &Command::Stop, deadline) {
            Ok(acknowledged_pid) => {
                if target.is_some_and(|pid| process_exists(pid) && pid != acknowledged_pid) {
                    bail!(
                        "IPC daemon PID does not match the live PID file; shutdown not confirmed"
                    );
                }
                wait_for_exit(acknowledged_pid, deadline)?;
                return Ok(true);
            }
            Err(error) => {
                // A concurrent stop may have completed during this request.
                if let Some(pid) = target {
                    if !process_exists(pid) {
                        return Ok(true);
                    }
                }
                if Instant::now() >= deadline {
                    return Err(error.context("shutdown not confirmed within 3 seconds"));
                }
                thread::sleep(POLL);
            }
        }
    }
}

fn wait_for_exit(pid: i32, deadline: Instant) -> Result<()> {
    while process_exists(pid) {
        if Instant::now() >= deadline {
            bail!("daemon PID {pid} acknowledged stop but has not exited within 3 seconds");
        }
        thread::sleep(POLL);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::UnixListener;
    use std::process::{Child, Command as ProcessCommand, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "klicky-owner-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn owner_excludes_startup_and_cleans_before_replacement() -> Result<()> {
        let directory = Directory::new();
        let owner = Owner::acquire(&directory.0)?.unwrap();
        fs::write(directory.0.join("klicky.sock"), "old socket")?;
        assert!(Owner::acquire(&directory.0)?.is_none());
        assert!(directory.0.join("klicky.pid").exists());
        drop(owner);
        assert!(!directory.0.join("klicky.pid").exists());
        assert!(!directory.0.join("klicky.sock").exists());
        assert!(directory.0.join("klicky.lock").exists());
        let replacement = Owner::acquire(&directory.0)?.unwrap();
        fs::write(directory.0.join("klicky.sock"), "new socket")?;
        assert!(Owner::acquire(&directory.0)?.is_none());
        assert_eq!(
            fs::read_to_string(directory.0.join("klicky.sock"))?,
            "new socket"
        );
        drop(replacement);
        Ok(())
    }

    #[test]
    fn stale_state_is_reclaimed_by_startup_and_errors_release_ownership() -> Result<()> {
        let directory = Directory::new();
        fs::write(directory.0.join("klicky.pid"), "2147483647")?;
        fs::write(directory.0.join("klicky.sock"), "stale")?;
        let failed_start = || -> Result<()> {
            let _owner = Owner::acquire(&directory.0)?.unwrap();
            assert!(!directory.0.join("klicky.sock").exists());
            bail!("simulated audio initialization failure")
        };
        assert!(failed_start().is_err());
        assert!(!directory.0.join("klicky.pid").exists());
        assert!(Owner::acquire(&directory.0)?.is_some());
        Ok(())
    }

    #[test]
    fn live_unlocked_pid_is_not_overwritten() -> Result<()> {
        let directory = Directory::new();
        let pid = std::process::id().to_string();
        fs::write(directory.0.join("klicky.pid"), &pid)?;
        assert!(Owner::acquire(&directory.0).is_err());
        assert_eq!(fs::read_to_string(directory.0.join("klicky.pid"))?, pid);
        Ok(())
    }

    #[test]
    fn releasing_lock_does_not_wait_for_inherited_descriptors() -> Result<()> {
        let directory = Directory::new();
        let lock = try_lock(&directory.0)?.unwrap();
        // dup and fork share the same open file description and flock ownership.
        let _inherited = lock.0.try_clone()?;
        drop(lock);
        assert!(try_lock(&directory.0)?.is_some());
        Ok(())
    }

    #[test]
    fn stale_pid_while_startup_holds_lock_is_not_stopped() -> Result<()> {
        let directory = Directory::new();
        fs::write(directory.0.join("klicky.pid"), "2147483647")?;
        let _starting = try_lock(&directory.0)?.unwrap();
        assert!(stop(&directory.0).is_err());
        assert!(directory.0.join("klicky.pid").exists());
        Ok(())
    }

    #[test]
    fn exit_wait_never_confuses_acknowledgment_with_termination() {
        assert!(wait_for_exit(std::process::id() as i32, Instant::now()).is_err());
        assert!(wait_for_exit(i32::MAX, Instant::now()).is_ok());
        assert!(!process_exists(0));
        assert!(!process_exists(-1));
    }

    #[test]
    fn readiness_requires_a_matching_live_pid() -> Result<()> {
        for matching in [true, false] {
            let directory = Directory::new();
            let _owner = Owner::acquire(&directory.0)?.unwrap();
            let listener = UnixListener::bind(directory.0.join("klicky.sock"))?;
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                assert_eq!(line.trim(), "\"Status\"");
                let pid = if matching {
                    std::process::id()
                } else {
                    i32::MAX as u32
                };
                writeln!(stream, "{{\"pid\":{pid}}}").unwrap();
            });
            assert_eq!(wait_ready(&directory.0).is_ok(), matching);
            server.join().unwrap();
        }
        Ok(())
    }

    // A real process owns the lock and socket, but shutdown is gated by stdin.
    // The parent controls exactly when cleanup and termination may happen.
    #[test]
    #[ignore = "subprocess fixture, invoked by stop_waits_for_exit_before_restart"]
    fn daemon_fixture() -> Result<()> {
        let Some(directory) = std::env::var_os("KLICKY_TEST_RUNTIME") else {
            return Ok(());
        };
        let directory = PathBuf::from(directory);
        let _owner = Owner::acquire(&directory)?.unwrap();
        let listener = UnixListener::bind(directory.join("klicky.sock"))?;
        println!("READY");
        io::stdout().flush()?;
        let (mut stream, _) = listener.accept()?;
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line)?;
        assert_eq!(line.trim(), "\"Stop\"");
        writeln!(stream, "{{\"pid\":{}}}", std::process::id())?;
        println!("STOP_ACCEPTED");
        io::stdout().flush()?;
        io::stdin().read_exact(&mut [0])?;
        Ok(())
    }

    struct Fixture(Child);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn stop_waits_for_exit_before_restart() -> Result<()> {
        let directory = Directory::new();
        let mut child = Fixture(
            ProcessCommand::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    "lifecycle::tests::daemon_fixture",
                    "--ignored",
                    "--nocapture",
                ])
                .env("KLICKY_TEST_RUNTIME", &directory.0)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?,
        );
        let stdout = child.0.stdout.take().unwrap();
        let (event_tx, event_rx) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if event_tx.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        let await_event = |expected: &str| {
            let deadline = Instant::now() + TIMEOUT;
            loop {
                let line = event_rx
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .unwrap();
                if line.contains(expected) {
                    break;
                }
            }
        };
        await_event("READY");
        let path = directory.0.clone();
        let (result_tx, result_rx) = mpsc::channel();
        let stopping = thread::spawn(move || result_tx.send(stop(&path)).unwrap());
        await_event("STOP_ACCEPTED");
        assert!(matches!(
            result_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        assert!(Owner::acquire(&directory.0)?.is_none());
        assert!(directory.0.join("klicky.pid").exists());
        assert!(directory.0.join("klicky.sock").exists());
        child.0.stdin.take().unwrap().write_all(b"x")?;
        assert!(child.0.wait()?.success());
        assert!(result_rx.recv_timeout(TIMEOUT)??);
        stopping.join().unwrap();
        reader.join().unwrap();
        assert!(!directory.0.join("klicky.pid").exists());
        let _replacement = Owner::acquire(&directory.0)?.unwrap();
        assert!(directory.0.join("klicky.pid").exists());
        Ok(())
    }
}
