#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Home(PathBuf);
impl Home {
    fn new() -> Self {
        let root = PathBuf::from("/tmp").join(format!(
            "km-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("bin")).unwrap();
        let systemctl = root.join("bin/systemctl");
        fs::write(&systemctl, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(systemctl, fs::Permissions::from_mode(0o700)).unwrap();
        Self(root)
    }
    fn config(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.0.join("Library/Application Support/klicky")
        } else {
            self.0.join("config/klicky")
        }
    }
    fn runtime(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.config().join("run")
        } else {
            self.0.join("runtime/klicky")
        }
    }
    fn command(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_klicky"))
            .args(args)
            .env("HOME", &self.0)
            .env("XDG_CONFIG_HOME", self.0.join("config"))
            .env("XDG_RUNTIME_DIR", self.0.join("runtime"))
            .env("PATH", self.0.join("bin"))
            .env("KLICKY_SYSTEM_SOUNDS_DIR", self.0.join("sounds"))
            .output()
            .unwrap()
    }
    fn json_status(&self) -> serde_json::Value {
        let output = self.command(&["status", "--json"]);
        assert!(output.status.success(), "{output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn missing_daemon_has_structured_offline_preferences_and_pack_list() {
    let home = Home::new();
    fs::create_dir_all(home.0.join("sounds/a pack with spaces")).unwrap();
    let status = home.json_status();
    assert_eq!(status["schema_version"], 1);
    assert_eq!(status["running"], false);
    assert_eq!(status["process_exists"], false);
    assert_eq!(status["paused"], false);
    assert_eq!(status["autostart"], false);
    assert_eq!(status["packs"][0], "a pack with spaces");
}

#[test]
fn live_status_uses_confirmed_playback_instead_of_saved_config() {
    let home = Home::new();
    fs::create_dir_all(home.runtime()).unwrap();
    fs::create_dir_all(home.config()).unwrap();
    fs::write(
        home.config().join("config.toml"),
        "sound_pack = 'saved'\nvolume = 0.1\n",
    )
    .unwrap();
    fs::write(
        home.runtime().join("klicky.pid"),
        std::process::id().to_string(),
    )
    .unwrap();
    let listener = UnixListener::bind(home.runtime().join("klicky.sock")).unwrap();
    let server = thread::spawn(move || {
        for expected in ["Status", "PlaybackStatus"] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            assert_eq!(line.trim(), format!("\"{expected}\""));
            let playback = if expected == "PlaybackStatus" {
                ",\"playback\":{\"sound_pack\":\"active\",\"volume\":0.73,\"paused\":true}"
            } else {
                ""
            };
            writeln!(stream, "{{\"pid\":{}{playback}}}", std::process::id()).unwrap();
        }
    });
    let status = home.json_status();
    server.join().unwrap();
    assert_eq!(status["running"], true);
    assert_eq!(status["paused"], true);
    assert_eq!(status["sound_pack"], "active");
    assert!((status["volume"].as_f64().unwrap() - 0.73).abs() < 0.00001);
}

#[test]
fn pause_does_not_start_a_missing_daemon() {
    let home = Home::new();
    assert!(!home.command(&["pause"]).status.success());
    assert!(!home.runtime().join("klicky.pid").exists());
}

#[cfg(target_os = "macos")]
#[test]
fn login_startup_changes_do_not_start_or_stop_the_daemon() {
    let home = Home::new();
    let binary = home.0.join(".local/bin/klicky");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, "#!/bin/sh\nexit 99\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(home
        .command(&["service", "autostart", "enable"])
        .status
        .success());
    assert_eq!(home.json_status()["autostart"], true);
    assert!(!home.runtime().join("klicky.pid").exists());
    assert!(home
        .command(&["service", "autostart", "disable"])
        .status
        .success());
    assert_eq!(home.json_status()["autostart"], false);
    assert!(home.config().join("session.plist").exists());
}
