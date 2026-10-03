use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config;

#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    Stop,
    Switch { pack: String },
    Volume { level: f32 },
    Status,
}

pub fn start_server(sender: Sender<Command>) -> Result<()> {
    config::ensure_runtime_dir()?;
    let sock_path = config::socket_path();
    if sock_path.exists() {
        std::fs::remove_file(&sock_path)?;
    }

    let listener = bind_listener(&sock_path)?;

    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let reader = BufReader::new(&stream);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(cmd) = serde_json::from_str::<Command>(&line) {
                    let _ = sender.send(cmd);
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

#[cfg(test)]
mod tests {
    use super::*;

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
