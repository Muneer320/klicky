use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
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

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub sound_pack: String,
    pub volume: f32,
    pub running: bool,
}

pub fn start_server(sender: Sender<Command>) -> Result<()> {
    let sock_path = config::socket_path();
    if sock_path.exists() {
        std::fs::remove_file(&sock_path)?;
    }
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&sock_path)?;

    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let reader = BufReader::new(&stream);
            for line in reader.lines().flatten() {
                if let Ok(cmd) = serde_json::from_str::<Command>(&line) {
                    let _ = sender.send(cmd);
                }
            }
        }
    });

    Ok(())
}

pub fn send_command(cmd: &Command) -> Result<()> {
    let sock_path = config::socket_path();
    let mut stream = UnixStream::connect(&sock_path)?;
    let msg = serde_json::to_string(cmd)?;
    writeln!(stream, "{}", msg)?;
    Ok(())
}

pub fn send_command_with_response(cmd: &Command) -> Result<String> {
    let sock_path = config::socket_path();
    let mut stream = UnixStream::connect(&sock_path)?;
    let msg = serde_json::to_string(cmd)?;
    writeln!(stream, "{}", msg)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut response = String::new();
    BufReader::new(&stream).read_line(&mut response)?;
    Ok(response)
}
