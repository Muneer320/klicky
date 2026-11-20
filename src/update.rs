//! Explicit update checks and handoff to the installation's existing owner.
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

const RELEASE_API: &str = "https://api.github.com/repos/Muneer320/klicky/releases/latest";
pub const GUIDE: &str = "https://klicky.muneer320.tech/guide.html#installation";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Store,
    Standalone,
    Scoop,
    Homebrew,
    Apt,
    Pacman,
    Manual,
}

impl Route {
    pub fn description(self) -> &'static str {
        match self {
            Self::Store => "Microsoft Store (Store manages the offered version)",
            Self::Standalone => "Windows standalone installer",
            Self::Scoop => "Scoop (per-user)",
            Self::Homebrew => "Homebrew",
            Self::Apt => "APT",
            Self::Pacman => "pacman",
            Self::Manual => "source, portable or unrecognized installation",
        }
    }

    pub fn action(self) -> &'static str {
        match self {
            Self::Store => "Open Microsoft Store's Downloads and updates page.",
            Self::Standalone => "Open a terminal and run the official standalone installer. Its wizard handles replacement and startup restoration.",
            Self::Scoop => "Open a terminal, stop Klicky, run scoop update and scoop update klicky, then restore playback.",
            Self::Homebrew => "Stop Klicky, run brew update and brew upgrade muneer320/tap/klicky, then restore playback.",
            Self::Apt => "Stop Klicky, run sudo apt-get update and sudo apt-get install --only-upgrade klicky, then restore playback.",
            Self::Pacman => "Stop Klicky, run sudo pacman -Syu (a full system upgrade), then restore playback.",
            Self::Manual => "Use your original source checkout or portable download. Automatic replacement is disabled for this installation.",
        }
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

pub struct Check {
    pub route: Route,
    pub latest: String,
    pub newer: bool,
}

impl Check {
    pub fn summary(&self) -> String {
        let state = if self.newer {
            "A newer stable release is available."
        } else {
            "This build is at or ahead of the latest stable release."
        };
        format!(
            "Installed: {CURRENT}\nLatest stable: {}\nInstallation: {}\n{state}\n\n{}\nPackage-manager and Store versions may arrive later than GitHub releases.",
            self.latest, self.route.description(), self.route.action()
        )
    }
}

fn version(value: &str) -> Result<[u64; 3]> {
    let numbers: Vec<_> = value.split('.').collect();
    if numbers.len() != 3
        || numbers
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()))
    {
        bail!("expected a stable three-part version");
    }
    Ok([
        numbers[0].parse()?,
        numbers[1].parse()?,
        numbers[2].parse()?,
    ])
}

fn parse_release(bytes: &[u8]) -> Result<(String, bool)> {
    let release: Release = serde_json::from_slice(bytes).context("read GitHub release response")?;
    if release.draft || release.prerelease {
        bail!("GitHub did not return a published stable release");
    }
    let latest = release
        .tag_name
        .strip_prefix('v')
        .context("release tag is missing v")?;
    let newer = version(latest)? > version(CURRENT)?;
    Ok((latest.to_owned(), newer))
}

pub fn check() -> Result<Check> {
    let route = detect(&std::env::current_exe()?.canonicalize()?)?;
    // Windows/macOS supply curl; Linux installations already require it for the
    // download helper. No shell, credentials, configuration or input data is sent.
    let mut request = Command::new(curl());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        request.creation_flags(0x08000000); // CREATE_NO_WINDOW for tray checks
    }
    let output = request
        .args([
            "--disable",
            "--fail",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--connect-timeout",
            "5",
            "--max-time",
            "15",
            "--max-filesize",
            "1048576",
            "--header",
            "Accept: application/vnd.github+json",
            "--header",
            "X-GitHub-Api-Version: 2022-11-28",
            "--user-agent",
            "Klicky-update-check",
            RELEASE_API,
        ])
        .output()
        .context("could not run curl for the update check; install curl or use your package manager directly")?;
    if !output.status.success() {
        bail!(
            "Update check failed; nothing was changed. {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let (latest, newer) = parse_release(&output.stdout)?;
    Ok(Check {
        route,
        latest,
        newer,
    })
}

#[cfg(target_os = "windows")]
fn curl() -> std::path::PathBuf {
    system_tool("curl.exe")
}
#[cfg(not(target_os = "windows"))]
fn curl() -> std::path::PathBuf {
    "curl".into()
}

#[cfg(target_os = "windows")]
fn system_tool(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
        .join("System32")
        .join(name)
}

pub fn run(check_only: bool, yes: bool) -> Result<()> {
    let result = check()?;
    println!("{}", result.summary());
    if check_only || !result.newer {
        return Ok(());
    }
    if result.route == Route::Manual {
        bail!("Update manually using {GUIDE}; no files were changed");
    }
    if !yes {
        if !io::stdin().is_terminal() {
            bail!("Use --check, or --yes to accept the displayed handoff");
        }
        print!("Continue with this update route? [y/N] ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Cancelled; nothing was changed.");
            return Ok(());
        }
    }
    handoff(result.route, true)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn detect(exe: &Path) -> Result<Route> {
    let scoop = std::env::var_os("SCOOP")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|p| p.join("scoop")));
    windows_route(
        exe,
        dirs::data_local_dir().as_deref(),
        scoop.as_deref(),
        crate::windows::package::current()?,
    )
}

#[cfg(target_os = "windows")]
fn windows_route(
    exe: &Path,
    local: Option<&Path>,
    scoop: Option<&Path>,
    packaged: bool,
) -> Result<Route> {
    if packaged {
        return Ok(Route::Store);
    }
    let parent = exe.parent().context("executable has no directory")?;
    if let Some(local) = local {
        let expected = local.join("Programs/Klicky");
        if expected.canonicalize().is_ok_and(|path| path == parent)
            && parent.join("INSTALLER-IDENTITY.ini").is_file()
        {
            return Ok(Route::Standalone);
        }
    }
    // Resolve the real keg directory, not the shim on PATH. Scoop retains both
    // manifests beside the installed binary; a portable ZIP has neither.
    // Global/custom unknown roots need explicit manager guidance, not a local
    // scoop update that could target a different installation.
    if scoop
        .and_then(|p| p.canonicalize().ok())
        .is_some_and(|root| parent.ancestors().nth(3) == Some(root.as_path()))
        && parent.parent().is_some_and(|p| {
            p.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("klicky"))
        })
        && parent.parent().and_then(Path::parent).is_some_and(|p| {
            p.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("apps"))
        })
        && parent.join("manifest.json").is_file()
        && parent.join("install.json").is_file()
    {
        return Ok(Route::Scoop);
    }
    Ok(Route::Manual)
}

#[cfg(not(target_os = "windows"))]
fn detect(exe: &Path) -> Result<Route> {
    #[cfg(target_os = "macos")]
    if let Some(launcher) = option_env!("KLICKY_MACOS_INSTALL_BINARY") {
        // The tap's stable opt/bin launcher is a shell wrapper. The actual Rust
        // executable is installed under the keg's libexec directory.
        if let Some(keg) = Path::new(launcher)
            .parent()
            .and_then(Path::parent)
            .and_then(|p| p.canonicalize().ok())
        {
            if keg.join("INSTALL_RECEIPT.json").is_file()
                && keg
                    .join("libexec/bin/klicky")
                    .canonicalize()
                    .is_ok_and(|p| p == exe)
            {
                return Ok(Route::Homebrew);
            }
        }
    }
    #[cfg(target_os = "linux")]
    if exe == Path::new("/usr/bin/klicky") {
        if Command::new("dpkg-query")
            .args(["-S", "/usr/bin/klicky"])
            .output()
            .is_ok_and(|o| {
                o.status.success()
                    && String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .any(|s| s == "klicky: /usr/bin/klicky")
            })
        {
            return Ok(Route::Apt);
        }
        if Command::new("pacman")
            .args(["-Qoq", "/usr/bin/klicky"])
            .output()
            .is_ok_and(|o| o.status.success() && o.stdout == b"klicky\n")
        {
            return Ok(Route::Pacman);
        }
    }
    let _ = exe;
    Ok(Route::Manual)
}

#[cfg(target_os = "windows")]
pub fn handoff(route: Route, wait_for_cli: bool) -> Result<()> {
    use std::os::windows::process::CommandExt;
    if route == Route::Store {
        return open_store();
    }
    let channel = match route {
        Route::Standalone => "Standalone",
        Route::Scoop => "Scoop",
        _ => bail!("Automatic replacement is disabled for this installation; use {GUIDE}"),
    };
    let exe = std::env::current_exe()?.canonicalize()?;
    let directory = exe.parent().context("no bundle directory")?;
    let cli = if route == Route::Scoop {
        directory
            .parent()
            .context("no Scoop application directory")?
            .join("current/klicky.exe")
    } else {
        directory.join("klicky.exe")
    };
    let cli = cli
        .to_str()
        .context("CLI path is not Unicode")?
        .replace('\'', "''");
    let caller = if wait_for_cli { std::process::id() } else { 0 };
    // Run outside the bundle so the CLI can exit before replacement. A separate
    // console keeps installer errors visible; the manager owns the actual update.
    let script = format!(
        "& {{ {} }} -Channel {channel} -Cli '{cli}' -CallerPid {caller}",
        include_str!("update/windows.ps1")
    );
    Command::new(system_tool("WindowsPowerShell/v1.0/powershell.exe"))
        .args(["-NoProfile", "-Command", &script])
        .creation_flags(0x00000010) // CREATE_NEW_CONSOLE: interactive update handoff
        .spawn()
        .context("open the update terminal")?;
    println!(
        "Update handed to a separate terminal. This is not confirmation of a completed update."
    );
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_store() -> Result<()> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("ms-windows-store://downloadsandupdates"),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    unsafe {
        CoUninitialize();
    }
    if result.0 as isize <= 32 {
        bail!("Microsoft Store could not open; use Store > Library > Get updates");
    }
    println!("Opened Microsoft Store; it manages this installation's updates.");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn handoff(route: Route, _wait_for_cli: bool) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let manager = match route {
        Route::Homebrew => "homebrew",
        Route::Apt => "apt",
        Route::Pacman => "pacman",
        _ => bail!("Update manually using {GUIDE}; no files were changed"),
    };
    let playback = match crate::ipc::playback_status() {
        Ok(state) => Some(state),
        Err(error) if crate::ipc::is_daemon_unavailable(&error) => None,
        Err(error) => return Err(error).context("cannot preserve current playback state"),
    };
    let launcher = option_env!("KLICKY_MACOS_INSTALL_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_exe()?);
    // exec preserves the foreground terminal while releasing the old executable.
    // Debian/Arch guards must not mistake the updater CLI for a running engine.
    let error = Command::new("/bin/sh")
        .args([
            "-c",
            include_str!("update/unix.sh"),
            "klicky-update",
            manager,
        ])
        .arg(launcher)
        .arg(if playback.is_some() { "1" } else { "0" })
        .arg(if playback.is_some_and(|state| state.paused) {
            "1"
        } else {
            "0"
        })
        .exec();
    Err(error).context("hand off to the installation owner; no update started")
}
