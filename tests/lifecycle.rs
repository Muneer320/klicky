#![cfg(target_os = "linux")]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Runtime(PathBuf);

impl Runtime {
    fn new(pid: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "klicky-lifecycle-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("klicky")).unwrap();
        fs::write(path.join("klicky/klicky.pid"), pid).unwrap();
        Self(path)
    }

    fn stop(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_klicky"))
            .arg("stop")
            .env("XDG_RUNTIME_DIR", &self.0)
            .output()
            .unwrap()
    }

    fn assert_state_present(&self) {
        assert!(self.0.join("klicky/klicky.pid").exists());
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn live_process_without_socket_is_an_error_and_retains_state() {
    let runtime = Runtime::new(&std::process::id().to_string());
    let output = runtime.stop();
    assert!(!output.status.success(), "{:?}", output);
    runtime.assert_state_present();
}

#[test]
fn delivered_stop_is_not_completed_shutdown() {
    // This process deliberately remains alive after accepting Stop. No sleep or
    // scheduling assumption: the client must time out, never announce success.
    let runtime = Runtime::new(&std::process::id().to_string());
    let socket = runtime.0.join("klicky/klicky.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "\"Stop\"");
        let _ = writeln!(stream, "{{\"pid\":{}}}", std::process::id());
    });
    let output = runtime.stop();
    server.join().unwrap();
    assert!(!output.status.success(), "{:?}", output);
    runtime.assert_state_present();
    assert!(socket.exists());
}

#[test]
fn stale_pid_is_already_stopped_but_not_removed_by_client() {
    let runtime = Runtime::new("2147483647");
    let output = runtime.stop();
    assert!(output.status.success(), "{:?}", output);
    runtime.assert_state_present();
}

#[test]
fn already_stopped_without_runtime_directory_succeeds() {
    let runtime = Runtime::new("2147483647");
    fs::remove_dir_all(runtime.0.join("klicky")).unwrap();
    assert!(runtime.stop().status.success());
    assert!(!runtime.0.join("klicky").exists());
}

#[test]
fn failed_start_cleans_its_pid_and_keeps_the_lock_inode() {
    let runtime = Runtime::new("2147483647");
    let output = Command::new(env!("CARGO_BIN_EXE_klicky"))
        .arg("start")
        .env("XDG_RUNTIME_DIR", &runtime.0)
        .env("XDG_CONFIG_HOME", runtime.0.join("config"))
        .env("KLICKY_SYSTEM_SOUNDS_DIR", runtime.0.join("no-sounds"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("No sound packs found"));
    assert!(!runtime.0.join("klicky/klicky.pid").exists());
    assert!(runtime.0.join("klicky/klicky.lock").exists());
}

#[test]
fn successful_service_manager_command_is_not_daemon_readiness() {
    let runtime = Runtime::new("2147483647");
    let bin = runtime.0.join("bin");
    fs::create_dir(&bin).unwrap();
    let systemctl = bin.join("systemctl");
    fs::write(&systemctl, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_klicky"))
        .args(["service", "start"])
        .env("XDG_RUNTIME_DIR", &runtime.0)
        .env("PATH", &bin)
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("did not become responsive"));
}

#[test]
fn mismatched_acknowledgment_cannot_confirm_shutdown() {
    let runtime = Runtime::new(&std::process::id().to_string());
    let socket = runtime.0.join("klicky/klicky.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        writeln!(stream, "{{\"pid\":2147483647}}").unwrap();
    });
    let output = runtime.stop();
    server.join().unwrap();
    assert!(!output.status.success());
    runtime.assert_state_present();
    assert!(socket.exists());
}

#[test]
fn removed_pid_does_not_mean_process_exit() {
    let runtime = Runtime::new(&std::process::id().to_string());
    let socket = runtime.0.join("klicky/klicky.sock");
    let pid = runtime.0.join("klicky/klicky.pid");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        // Simulate cleanup followed by a stalled process teardown, without ack.
        fs::remove_file(pid).unwrap();
        fs::remove_file(socket).unwrap();
    });
    let output = runtime.stop();
    server.join().unwrap();
    assert!(!output.status.success(), "{output:?}");
}
