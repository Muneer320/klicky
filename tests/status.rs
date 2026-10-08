#![cfg(target_os = "linux")]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Runtime(PathBuf);
impl Runtime {
    fn new(pid: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "klicky-status-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("klicky")).unwrap();
        fs::write(root.join("klicky/klicky.pid"), pid).unwrap();
        Self(root)
    }
    fn status(&self) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_klicky"))
            .arg("status")
            .env("XDG_RUNTIME_DIR", &self.0)
            .env("XDG_CONFIG_HOME", self.0.join("config"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap()
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn field<'a>(output: &'a str, name: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix(name))
        .unwrap()
        .trim()
}

#[test]
fn unrelated_live_pid_is_not_a_running_daemon() {
    // Our test process is alive but is not Klicky: deterministic PID-reuse proxy.
    let runtime = Runtime::new(&std::process::id().to_string());
    let output = runtime.status();
    assert_eq!(field(&output, "running:"), "no");
    assert_eq!(field(&output, "responsive:"), "no");
    assert_eq!(field(&output, "process exists:"), "yes");
}

#[test]
fn service_status_does_not_tell_menu_a_reused_pid_is_on() {
    let runtime = Runtime::new(&std::process::id().to_string());
    let bin = runtime.0.join("bin");
    fs::create_dir(&bin).unwrap();
    let systemctl = bin.join("systemctl");
    fs::write(&systemctl, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(systemctl, fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_klicky"))
        .args(["service", "status"])
        .env("XDG_RUNTIME_DIR", &runtime.0)
        .env("PATH", bin)
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();
    assert_eq!(field(&output, "autostart:"), "enabled");
    assert!(!output.contains("running: yes"));
    assert_eq!(field(&output, "process exists:"), "yes");
    assert_eq!(field(&output, "responsive:"), "no");
}

#[test]
fn mismatched_response_is_not_daemon_health() {
    let runtime = Runtime::new(&std::process::id().to_string());
    let listener = UnixListener::bind(runtime.0.join("klicky/klicky.sock")).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "\"Status\"");
        writeln!(stream, "{{\"pid\":2147483647}}").unwrap();
    });
    let output = runtime.status();
    server.join().unwrap();
    assert_eq!(field(&output, "responsive:"), "no");
    assert_eq!(field(&output, "running:"), "no");
}

#[test]
fn stale_or_invalid_pid_reports_no_process_and_no_daemon() {
    for pid in ["2147483647", "0", "-1", "not a pid"] {
        let runtime = Runtime::new(pid);
        let output = runtime.status();
        assert_eq!(field(&output, "running:"), "no");
        assert_eq!(field(&output, "process exists:"), "no");
        assert_eq!(field(&output, "responsive:"), "no");
    }
}

#[test]
fn matching_reply_requires_the_pid_to_still_be_recorded() {
    for remove_pid in [false, true] {
        let runtime = Runtime::new(&std::process::id().to_string());
        let listener = UnixListener::bind(runtime.0.join("klicky/klicky.sock")).unwrap();
        let pid_file = runtime.0.join("klicky/klicky.pid");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            if remove_pid {
                fs::remove_file(pid_file).unwrap();
            }
            writeln!(stream, "{{\"pid\":{}}}", std::process::id()).unwrap();
        });
        let output = runtime.status();
        server.join().unwrap();
        assert_eq!(
            field(&output, "responsive:"),
            if remove_pid { "no" } else { "yes" }
        );
        assert_eq!(
            field(&output, "running:"),
            if remove_pid { "no" } else { "yes" }
        );
    }
}

#[test]
fn connected_but_unresponsive_process_is_reported_separately() {
    let runtime = Runtime::new(&std::process::id().to_string());
    let listener = UnixListener::bind(runtime.0.join("klicky/klicky.sock")).unwrap();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        // Keep the connection open until the CLI finishes its bounded probe.
        release_rx.recv().unwrap();
    });
    let output = runtime.status();
    release_tx.send(()).unwrap();
    server.join().unwrap();
    assert_eq!(field(&output, "running:"), "no");
    assert_eq!(field(&output, "responsive:"), "no");
    assert_eq!(field(&output, "process exists:"), "yes");
}
