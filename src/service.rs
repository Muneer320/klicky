use anyhow::{bail, Context, Result};
use std::process::Command;

use crate::{is_running, ServiceAction};

pub fn run(action: ServiceAction) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux(action)
    }
    #[cfg(target_os = "macos")]
    {
        macos(action)
    }
}

fn command(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to run {program}"))?;
    if !status.success() {
        bail!("{program} {} failed ({status})", args.join(" "));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux(action: ServiceAction) -> Result<()> {
    let unit = "klicky.service";
    match action {
        ServiceAction::Enable => command("systemctl", &["--user", "enable", "--now", unit])?,
        ServiceAction::Disable => command("systemctl", &["--user", "disable", "--now", unit])?,
        ServiceAction::Start => command("systemctl", &["--user", "start", unit])?,
        ServiceAction::Stop => command("systemctl", &["--user", "stop", unit])?,
        ServiceAction::Status => {
            let enabled = Command::new("systemctl")
                .args(["--user", "is-enabled", "--quiet", unit])
                .status()?
                .success();
            println!(
                "autostart: {}",
                if enabled { "enabled" } else { "disabled" }
            );
            println!("running: {}", if is_running() { "yes" } else { "no" });
            return Ok(());
        }
    }
    println!("Klicky service updated");
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos(action: ServiceAction) -> Result<()> {
    use std::fs;

    const LABEL: &str = "dev.klicky.daemon";
    let home = dirs::home_dir().context("home directory is unavailable")?;
    let plist = home
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"));
    let session_plist = home.join("Library/Application Support/klicky/session.plist");
    let domain = format!("gui/{}", unsafe { libc::getuid() });
    let service = format!("{domain}/{LABEL}");
    let loaded = || -> bool {
        Command::new("launchctl")
            .args(["print", &service])
            .output()
            .is_ok_and(|output| output.status.success())
    };

    match action {
        ServiceAction::Enable => {
            let binary = std::env::current_exe()?;
            if loaded() {
                command("launchctl", &["bootout", &service])?;
            }
            write_plist(&plist, LABEL, &binary)?;
            let path = plist.to_str().context("LaunchAgent path is not UTF-8")?;
            command("launchctl", &["bootstrap", &domain, path])?;
            println!("Klicky will start at login");
        }
        ServiceAction::Disable => {
            if loaded() {
                command("launchctl", &["bootout", &service])?;
            }
            if is_running() {
                crate::cmd_stop()?;
            }
            if plist.exists() {
                fs::remove_file(&plist)?;
            }
            if session_plist.exists() {
                fs::remove_file(&session_plist)?;
            }
            println!("Klicky automatic startup disabled");
        }
        ServiceAction::Start => {
            if !loaded() {
                let active_plist = if plist.exists() {
                    &plist
                } else {
                    write_plist(&session_plist, LABEL, &std::env::current_exe()?)?;
                    &session_plist
                };
                let path = active_plist
                    .to_str()
                    .context("LaunchAgent path is not UTF-8")?;
                command("launchctl", &["bootstrap", &domain, path])?;
            } else if !is_running() {
                command("launchctl", &["kickstart", "-k", &service])?;
            }
            println!("Klicky service started");
        }
        ServiceAction::Stop => {
            if is_running() {
                crate::cmd_stop()?;
            } else {
                println!("Klicky is not running");
            }
        }
        ServiceAction::Status => {
            println!(
                "autostart: {}",
                if plist.exists() {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            println!("running: {}", if is_running() { "yes" } else { "no" });
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn write_plist(path: &std::path::Path, label: &str, binary: &std::path::Path) -> Result<()> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let binary = binary.to_str().context("binary path is not UTF-8")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>{label}</string>\n<key>ProgramArguments</key><array><string>{}</string><string>start</string></array>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><false/>\n</dict></plist>\n",
        xml_escape(binary)
    );
    fs::write(path, xml)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
