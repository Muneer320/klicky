use anyhow::{bail, Context, Result};
use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::ServiceAction;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_POLL: Duration = Duration::from_millis(10);

/// On macOS a direct stop also cancels any launchd retry queued after a crash.
pub fn stop() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        crate::cmd_stop()
    }
    #[cfg(target_os = "macos")]
    {
        macos(ServiceAction::Stop)
    }
}

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

fn command_output(program: &str, args: &[&str]) -> Result<Output> {
    command_output_with_timeout(program, args, COMMAND_TIMEOUT)
}

fn command_output_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Output> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {program} {}", args.join(" ")))?;
    let stdout = child.stdout.take().context("service command stdout unavailable")?;
    let stderr = child.stderr.take().context("service command stderr unavailable")?;
    let stdout_reader = thread::spawn(move || read_output(stdout));
    let stderr_reader = thread::spawn(move || read_output(stderr));
    let deadline = Instant::now() + timeout;

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(COMMAND_POLL),
            Ok(None) => {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                bail!(
                    "{program} {} timed out after {} seconds",
                    args.join(" "),
                    timeout.as_secs_f32()
                );
            }
            Err(error) => {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(error).with_context(|| {
                    format!("failed while waiting for {program} {}", args.join(" "))
                });
            }
        }
    };

    let stdout = stdout_reader
        .join()
        .map_err(|_| anyhow::anyhow!("{program} stdout reader panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| anyhow::anyhow!("{program} stderr reader panicked"))??;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_output(mut stream: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn command_output_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout,
        (true, false) => stderr,
        (false, false) => format!("stdout: {stdout}; stderr: {stderr}"),
    }
}

fn command(program: &str, args: &[&str]) -> Result<()> {
    let output = command_output(program, args)?;
    if !output.status.success() {
        bail!(
            "{program} {} failed ({}): {}",
            args.join(" "),
            output.status,
            command_output_text(&output)
        );
    }
    Ok(())
}

fn classify_launchctl_print(output: &Output, service: &str) -> Result<bool> {
    if output.status.success() {
        return Ok(true);
    }
    let details = command_output_text(output);
    if output.status.code() == Some(113)
        && details.contains("Could not find service")
        && details.contains(service)
    {
        return Ok(false);
    }
    bail!(
        "launchctl print {service} failed ({}): {}",
        output.status,
        details
    );
}

fn launchctl_loaded_with(
    service: &str,
    run: impl FnOnce(&str, &[&str]) -> Result<Output>,
) -> Result<bool> {
    classify_launchctl_print(&run("launchctl", &["print", service])?, service)
}

#[cfg(target_os = "macos")]
fn launchctl_loaded(service: &str) -> Result<bool> {
    launchctl_loaded_with(service, command_output)
}

#[cfg(target_os = "linux")]
fn linux(action: ServiceAction) -> Result<()> {
    let unit = "klicky.service";
    match action {
        ServiceAction::Enable => {
            command("systemctl", &["--user", "enable", "--now", unit])?;
            crate::lifecycle::wait_ready(&crate::config::runtime_dir())?;
        }
        ServiceAction::Disable => command("systemctl", &["--user", "disable", "--now", unit])?,
        ServiceAction::Start => {
            command("systemctl", &["--user", "start", unit])?;
            crate::lifecycle::wait_ready(&crate::config::runtime_dir())?;
        }
        ServiceAction::Stop => command("systemctl", &["--user", "stop", unit])?,
        ServiceAction::Status => {
            let output = command_output("systemctl", &["--user", "is-enabled", "--quiet", unit])?;
            let enabled = if output.status.success() {
                true
            } else if output.status.code() == Some(1) {
                false
            } else {
                bail!(
                    "systemctl --user is-enabled {unit} failed ({}): {}",
                    output.status,
                    command_output_text(&output)
                );
            };
            println!(
                "autostart: {}",
                if enabled { "enabled" } else { "disabled" }
            );
            print_status()?;
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
    match action {
        ServiceAction::Enable => {
            let binary = installed_binary(&home)?;
            if launchctl_loaded(&service)? {
                crate::cmd_stop()?;
                command("launchctl", &["bootout", &service])?;
            }
            write_plist(&plist, LABEL, &binary)?;
            let path = plist.to_str().context("LaunchAgent path is not UTF-8")?;
            command("launchctl", &["bootstrap", &domain, path])?;
            crate::lifecycle::wait_ready(&crate::config::runtime_dir())?;
            println!("Klicky will start at login");
        }
        ServiceAction::Disable => {
            stop_launch_agent(
                || crate::lifecycle::stop(&crate::config::runtime_dir()),
                || {
                    if launchctl_loaded(&service)? {
                        command("launchctl", &["bootout", &service])
                    } else {
                        Ok(())
                    }
                },
            )?;
            if plist.exists() {
                fs::remove_file(&plist)?;
            }
            if session_plist.exists() {
                fs::remove_file(&session_plist)?;
            }
            println!("Klicky automatic startup disabled");
        }
        ServiceAction::Start => {
            let binary = installed_binary(&home)?;
            let active_plist = if plist.exists() {
                &plist
            } else {
                &session_plist
            };
            let changed = plist_needs_update(active_plist, &plist_contents(LABEL, &binary)?)?;
            let loaded = launchctl_loaded(&service)?;
            if loaded && changed {
                // Migrate old paths/recovery policy before starting the job.
                crate::cmd_stop()?;
                command("launchctl", &["bootout", &service])?;
            }
            // Do not mark migration complete on disk until the old job unloaded.
            write_plist(active_plist, LABEL, &binary)?;
            if !loaded {
                let path = active_plist
                    .to_str()
                    .context("LaunchAgent path is not UTF-8")?;
                command("launchctl", &["bootstrap", &domain, path])?;
            } else {
                // Let launchd decide whether its job is running. A reused PID
                // file must not suppress startup; do not force-kill a live job.
                command("launchctl", &["kickstart", &service])?;
            }
            crate::lifecycle::wait_ready(&crate::config::runtime_dir())?;
            println!("Klicky service started");
        }
        ServiceAction::Stop => {
            let stopped = stop_launch_agent(
                || crate::lifecycle::stop(&crate::config::runtime_dir()),
                || {
                    if launchctl_loaded(&service)? {
                        command("launchctl", &["bootout", &service])
                    } else {
                        Ok(())
                    }
                },
            )?;
            println!(
                "{}",
                if stopped {
                    "klicky stopped"
                } else {
                    "klicky is not running"
                }
            );
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
            print_status()?;
        }
    }
    Ok(())
}

fn print_status() -> Result<()> {
    let status = crate::lifecycle::status(&crate::config::runtime_dir())?;
    println!("running: {}", if status.responsive { "yes" } else { "no" });
    println!(
        "process exists: {}",
        if status.process_exists { "yes" } else { "no" }
    );
    println!(
        "responsive: {}",
        if status.responsive { "yes" } else { "no" }
    );
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn stop_launch_agent(
    stop: impl FnOnce() -> Result<bool>,
    unload: impl FnOnce() -> Result<()>,
) -> Result<bool> {
    let stopped = stop()?;
    // Even an already exited process may have a pending launchd restart.
    unload()?;
    Ok(stopped)
}

#[cfg(any(target_os = "macos", test))]
fn installed_binary(home: &std::path::Path) -> Result<std::path::PathBuf> {
    let binary = std::fs::canonicalize(home)?.join(".local/bin/klicky");
    validate_installed_binary(&binary)?;
    Ok(binary)
}

#[cfg(any(target_os = "macos", test))]
fn validate_installed_binary(binary: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let validate = || -> Result<()> {
        if !binary.is_absolute() || !binary.ends_with(".local/bin/klicky") {
            bail!("expected the installed ~/.local/bin/klicky executable");
        }
        let metadata = std::fs::symlink_metadata(binary)?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            bail!("installed path must be a regular executable file, not a symlink");
        }
        if std::fs::canonicalize(binary)? != binary {
            bail!("installed path must not redirect through symlinks or parent traversal");
        }
        Ok(())
    };
    validate().with_context(|| format!("Invalid installed Klicky binary at {}. Run ./scripts/install-macos.sh from the repository to install a regular executable at ~/.local/bin/klicky", binary.display()))
}

#[cfg(any(target_os = "macos", test))]
fn write_plist(path: &std::path::Path, label: &str, binary: &std::path::Path) -> Result<bool> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let xml = plist_contents(label, binary)?;
    let changed = plist_needs_update(path, &xml)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if changed {
        fs::write(path, xml)?;
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(changed)
}

#[cfg(any(target_os = "macos", test))]
fn plist_contents(label: &str, binary: &std::path::Path) -> Result<String> {
    validate_installed_binary(binary)?;
    let binary = binary.to_str().context("binary path is not UTF-8")?;
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>{label}</string>\n<key>ProgramArguments</key><array><string>{}</string><string>start</string></array>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>\n<key>ThrottleInterval</key><integer>30</integer>\n</dict></plist>\n",
        xml_escape(binary)
    ))
}

#[cfg(any(target_os = "macos", test))]
fn plist_needs_update(path: &std::path::Path, xml: &str) -> Result<bool> {
    match std::fs::read_to_string(path) {
        Ok(previous) => Ok(previous != xml),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
    }
}

#[cfg(any(target_os = "macos", test))]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::process::ExitStatusExt;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Home(PathBuf);
    impl Home {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "klicky-agent-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(fs::canonicalize(path).unwrap())
        }
        fn executable(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "test executable").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        }
    }
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn temporary_build_path_cannot_be_written_to_launch_agent() {
        let home = Home::new();
        let binary = home.executable("target/debug/klicky");
        let plist = home.0.join("agent.plist");
        assert!(write_plist(&plist, "dev.klicky.daemon", &binary).is_err());
        assert!(!plist.exists());
    }

    #[test]
    fn launch_agent_restarts_failures_with_throttling() -> Result<()> {
        let home = Home::new();
        let binary = home.executable(".local/bin/klicky");
        let plist = home.0.join("agent.plist");
        write_plist(&plist, "dev.klicky.daemon", &binary)?;
        let xml = fs::read_to_string(plist)?;
        assert!(xml.contains("<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>"));
        assert!(xml.contains("<key>ThrottleInterval</key><integer>30</integer>"));
        assert!(xml.contains(&format!(
            "<array><string>{}</string><string>start</string></array>",
            xml_escape(binary.to_str().unwrap())
        )));
        Ok(())
    }

    #[test]
    fn installed_binary_requires_a_regular_executable_at_the_fixed_path() -> Result<()> {
        let home = Home::new();
        assert!(installed_binary(&home.0).is_err());
        let binary = home.executable(".local/bin/klicky");
        assert_eq!(installed_binary(&home.0)?, binary);
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o600))?;
        assert!(installed_binary(&home.0).is_err());
        fs::remove_file(&binary)?;
        fs::create_dir(&binary)?;
        assert!(installed_binary(&home.0).is_err());
        Ok(())
    }

    #[test]
    fn installed_symlinks_cannot_redirect_to_a_temporary_build() -> Result<()> {
        let home = Home::new();
        let temporary = home.executable("target/release/klicky");
        let installed = home.0.join(".local/bin/klicky");
        fs::create_dir_all(installed.parent().unwrap())?;
        std::os::unix::fs::symlink(&temporary, &installed)?;
        assert!(installed_binary(&home.0).is_err());
        fs::remove_file(&installed)?;
        fs::remove_dir(installed.parent().unwrap())?;
        std::os::unix::fs::symlink(temporary.parent().unwrap(), installed.parent().unwrap())?;
        assert!(installed_binary(&home.0).is_err());
        Ok(())
    }

    #[test]
    fn plist_replacement_repairs_old_policy_and_escapes_paths() -> Result<()> {
        let home = Home::new();
        let binary = home.executable("space & <xml>/.local/bin/klicky");
        let plist = home.0.join("agent.plist");
        fs::write(&plist, "old temporary path and KeepAlive=false")?;
        assert!(plist_needs_update(
            &plist,
            &plist_contents("dev.klicky.daemon", &binary)?
        )?);
        assert_eq!(
            fs::read_to_string(&plist)?,
            "old temporary path and KeepAlive=false"
        );
        assert!(write_plist(&plist, "dev.klicky.daemon", &binary)?);
        assert!(!write_plist(&plist, "dev.klicky.daemon", &binary)?);
        assert!(fs::read_to_string(&plist)?.contains("space &amp; &lt;xml&gt;"));
        assert_eq!(fs::metadata(plist)?.permissions().mode() & 0o777, 0o600);
        Ok(())
    }

    #[test]
    fn stop_cancels_pending_retries_even_without_a_live_process() -> Result<()> {
        use std::cell::RefCell;
        for running in [false, true] {
            let events = RefCell::new(Vec::new());
            assert_eq!(
                stop_launch_agent(
                    || {
                        events.borrow_mut().push("stop");
                        Ok(running)
                    },
                    || {
                        events.borrow_mut().push("unload");
                        Ok(())
                    },
                )?,
                running
            );
            assert_eq!(*events.borrow(), ["stop", "unload"]);
        }
        assert!(stop_launch_agent(|| Ok(false), || bail!("bootout failed")).is_err());
        assert!(stop_launch_agent(
            || bail!("shutdown timed out"),
            || panic!("must not report success")
        )
        .is_err());
        Ok(())
    }

    fn mock_output(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn launchctl_print_distinguishes_absence_from_operational_errors() {
        let service = "gui/501/dev.klicky.daemon";
        let loaded = launchctl_loaded_with(service, |program, args| {
            assert_eq!(program, "launchctl");
            assert_eq!(args, ["print", service]);
            Ok(mock_output(0, "loaded", ""))
        });
        assert!(loaded.unwrap());

        let absent = launchctl_loaded_with(service, |_, _| {
            Ok(mock_output(
                113,
                "",
                "Could not find service \"gui/501/dev.klicky.daemon\" in domain for user gui: 501",
            ))
        });
        assert!(!absent.unwrap());

        for failure in [
            mock_output(1, "", "Operation not permitted"),
            mock_output(113, "", "Could not find service malformed target"),
        ] {
            let error = launchctl_loaded_with(service, |_, _| Ok(failure)).unwrap_err();
            assert!(error.to_string().contains("launchctl print"));
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    fn service_commands_have_a_deadline_and_capture_diagnostics() {
        let error = command_output_with_timeout(
            "sh",
            &["-c", "exec tail -f /dev/null"],
            Duration::from_millis(50),
        )
        .unwrap_err();
        assert!(error.to_string().contains("timed out"));

        let error = command("sh", &["-c", "printf diagnostic >&2; exit 2"]).unwrap_err();
        assert!(error.to_string().contains("diagnostic"));
    }
}
