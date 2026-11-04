#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
fn main() {
    if let Err(error) = klicky::windows::desktop::run_entry() {
        klicky::windows::desktop::show_error(&format!("{error:#}"));
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("klicky-desktop is available on Windows only; use klicky on this platform.");
    std::process::exit(1);
}
