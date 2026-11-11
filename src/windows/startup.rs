//! Per-user Startup shortcut ownership; never rewrite OS startup approval state.
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ::windows::core::{w, Interface, GUID, HRESULT, PCWSTR, PWSTR};
use ::windows::Win32::Foundation::HANDLE;
use ::windows::Win32::Storage::FileSystem::{
    FileDispositionInfo, MoveFileExW, SetFileInformationByHandle, DELETE, FILE_DISPOSITION_INFO,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_DELETE, FILE_SHARE_READ,
    MOVEFILE_WRITE_THROUGH,
};
use ::windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile, IPersistStream,
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use ::windows::Win32::UI::Shell::{
    FOLDERID_Startup, IShellLinkDataList, IShellLinkW, SHCreateMemStream, ShellLink,
    KF_FLAG_DONT_VERIFY, SLDF_ALLOW_LINK_TO_LINK, SLDF_HAS_DARWINID, SLDF_HAS_EXP_ICON_SZ,
    SLDF_HAS_EXP_SZ, SLDF_PREFER_ENVIRONMENT_PATH, SLDF_RUNAS_USER, SLDF_RUN_IN_SEPARATE,
    SLDF_RUN_WITH_SHIMLAYER, SLGP_RAWPATH,
};
use ::windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use anyhow::{bail, Context, Result};

use super::{bundle, files, Paths};

pub(super) fn open_settings() -> Result<()> {
    let _com = Com::new()?;
    use ::windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
    };
    let mut launch = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        lpVerb: w!("open"),
        lpFile: w!("ms-settings:startupapps"),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut launch) }
        .context("Windows Startup settings could not be opened")?;
    Ok(())
}

const DESCRIPTION: &str = "Klicky per-user login startup; owner klicky.windows.v1";
const MAX_LINK_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Absent,
    Registered,
    Foreign,
    DisabledByUser,
    DisabledByPolicy,
    EnabledByPolicy,
}

#[derive(Debug)]
pub(super) struct ForeignShortcut;
impl std::fmt::Display for ForeignShortcut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("An unrecognized Klicky startup shortcut exists; it was preserved")
    }
}
impl std::error::Error for ForeignShortcut {}

pub(super) struct Startup {
    paths: Paths,
    folder: PathBuf,
}
impl Startup {
    pub fn current(paths: Paths) -> Result<Self> {
        let folder = startup_folder()?;
        if !folder.is_absolute() {
            bail!("Startup known folder must be absolute");
        }
        Ok(Self { paths, folder })
    }
    pub fn status(&self) -> Result<Status> {
        if super::package::current()? {
            return super::package::startup(None);
        }
        self.paths.ensure_data_dir()?;
        let _writer = files::lock(&self.paths.data_dir().join("startup.lock"), deadline())?;
        self.inspect(false).map(|entry| {
            entry.map_or(Status::Absent, |(_, owned)| {
                if owned {
                    Status::Registered
                } else {
                    Status::Foreign
                }
            })
        })
    }
    pub fn set(&self, enabled: bool) -> Result<Status> {
        if super::package::current()? {
            return super::package::startup(Some(enabled));
        }
        self.paths.ensure_data_dir()?;
        let _writer = files::lock(&self.paths.data_dir().join("startup.lock"), deadline())?;
        match self.inspect(!enabled)? {
            Some((file, false)) => {
                drop(file);
                Err(ForeignShortcut.into())
            }
            Some((file, true)) if enabled => {
                // Do not recreate a shortcut that Windows may have disabled.
                drop(file);
                Ok(Status::Registered)
            }
            Some((file, true)) => {
                remove(file)?;
                Ok(Status::Absent)
            }
            None if !enabled => Ok(Status::Absent),
            None => self.create(),
        }
    }
    fn inspect(&self, delete: bool) -> Result<Option<(File, bool)>> {
        let path = self.folder.join("Klicky.lnk");
        let access = FILE_GENERIC_READ.0 | if delete { DELETE.0 } else { 0 };
        let mut file = match OpenOptions::new()
            .access_mode(access)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_DELETE.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("open login startup shortcut"),
        };
        let metadata = file.metadata()?;
        files::check_metadata(&metadata, &path, false)?;
        if metadata.len() > MAX_LINK_BYTES {
            bail!("Startup shortcut exceeds 64 KiB; preserved");
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_LINK_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_LINK_BYTES {
            bail!("Startup shortcut exceeds 64 KiB; preserved");
        }
        // Parse the held object's bytes, without Shell resolving/repairing a path,
        // network search, or opening a potentially replaced link filename.
        let owned = self.owns_bytes(&bytes).unwrap_or(false);
        Ok(Some((file, owned)))
    }
    fn owns_bytes(&self, bytes: &[u8]) -> Result<bool> {
        let _com = Com::new()?;
        let link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
        let persist: IPersistStream = link.cast()?;
        let stream =
            unsafe { SHCreateMemStream(Some(bytes)) }.context("create bounded shortcut stream")?;
        unsafe { persist.Load(&stream)? };
        let mut description = [0u16; 512];
        let mut arguments = vec![0u16; 32768];
        let mut target = vec![0u16; 32768];
        let mut directory = vec![0u16; 32768];
        let mut icon = vec![0u16; 32768];
        let mut icon_index = 0;
        let data: IShellLinkDataList = link.cast()?;
        let flags = unsafe { data.GetFlags()? };
        let custom_flags = SLDF_RUNAS_USER.0
            | SLDF_RUN_IN_SEPARATE.0
            | SLDF_RUN_WITH_SHIMLAYER.0
            | SLDF_HAS_DARWINID.0
            | SLDF_HAS_EXP_SZ.0
            | SLDF_HAS_EXP_ICON_SZ.0
            | SLDF_ALLOW_LINK_TO_LINK.0
            | SLDF_PREFER_ENVIRONMENT_PATH.0;
        unsafe {
            link.GetDescription(&mut description)?;
            link.GetArguments(&mut arguments)?;
            link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)?;
            link.GetWorkingDirectory(&mut directory)?;
            link.GetIconLocation(&mut icon, &mut icon_index)?;
        }
        if units(&description)? != DESCRIPTION.encode_utf16().collect::<Vec<_>>()
            || !units(&arguments)?.is_empty()
            || !units(&icon)?.is_empty()
            || icon_index != 0
            || unsafe { link.GetHotkey()? } != 0
            || unsafe { link.GetShowCmd()? } != SW_SHOWNORMAL
            || flags & custom_flags as u32 != 0
        {
            return Ok(false);
        }
        let directory = PathBuf::from(OsString::from_wide(units(&directory)?));
        let target = PathBuf::from(OsString::from_wide(units(&target)?));
        // Match the locations we wrote, without following a modified working
        // directory/target onto a junction or unavailable network share.
        Ok(
            shell_path(&directory) == shell_path(self.paths.bundle_dir())
                && shell_path(&target)
                    == shell_path(&self.paths.bundle_dir().join(bundle::DESKTOP_EXE)),
        )
    }
    fn create(&self) -> Result<Status> {
        let target = bundle::desktop_path(&self.paths)?;
        fs::create_dir_all(&self.folder).context("create per-user Startup folder")?;
        files::check_metadata(&fs::symlink_metadata(&self.folder)?, &self.folder, true)?;
        let _com = Com::new()?;
        let link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
        let target = shell_path(&target);
        let directory = shell_path(self.paths.bundle_dir());
        unsafe {
            link.SetPath(PCWSTR(target.as_ptr()))?;
            link.SetWorkingDirectory(PCWSTR(directory.as_ptr()))?;
            link.SetArguments(w!(""))?;
            link.SetHotkey(0)?;
            link.SetShowCmd(SW_SHOWNORMAL)?;
            link.SetIconLocation(w!(""), 0)?;
            link.SetDescription(PCWSTR(
                DESCRIPTION
                    .encode_utf16()
                    .chain(Some(0))
                    .collect::<Vec<_>>()
                    .as_ptr(),
            ))?;
        }
        let nonce = super::process::current_identity()?.nonce;
        let nonce = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let temporary = self.folder.join(format!(".klicky-startup-{nonce}.tmp"));
        let reserved = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        drop(reserved);
        struct Temporary(PathBuf);
        impl Drop for Temporary {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let temporary = Temporary(temporary);
        let path = shell_path(&temporary.0);
        let persist: IPersistFile = link.cast()?;
        let saved = unsafe { persist.Save(PCWSTR(path.as_ptr()), true) };
        drop(persist);
        drop(link);
        saved?; // release COM's file references before temporary cleanup on failure
        OpenOptions::new()
            .write(true)
            .open(&temporary.0)?
            .sync_all()?;
        // No replace flag: a concurrently created foreign shortcut is preserved.
        let final_path = shell_path(&self.folder.join("Klicky.lnk"));
        unsafe {
            MoveFileExW(
                PCWSTR(path.as_ptr()),
                PCWSTR(final_path.as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )?
        };
        Ok(Status::Registered)
    }
}

fn startup_folder() -> Result<PathBuf> {
    // This API requires freeing its out-pointer on failure too. Its generated
    // Result<PWSTR> wrapper hides that pointer when HRESULT is an error.
    #[link(name = "shell32")]
    extern "system" {
        #[link_name = "SHGetKnownFolderPath"]
        fn get_path(id: *const GUID, flags: u32, token: HANDLE, path: *mut PWSTR) -> HRESULT;
    }
    struct Buffer(PWSTR);
    impl Drop for Buffer {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(Some(self.0.as_ptr().cast())) };
        }
    }
    let mut raw = PWSTR::null();
    let result = unsafe {
        get_path(
            &FOLDERID_Startup,
            KF_FLAG_DONT_VERIFY.0 as u32,
            HANDLE::default(),
            &mut raw,
        )
    };
    let buffer = Buffer(raw);
    result.ok()?;
    if buffer.0.is_null() {
        bail!("Windows returned no Startup known folder");
    }
    Ok(unsafe { PathBuf::from(OsString::from_wide(buffer.0.as_wide())) })
}

struct Com;

fn remove(file: File) -> Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )?;
    }
    drop(file); // delete the inspected object, never a replacement at its old name
    Ok(())
}
impl Com {
    fn new() -> Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
        Ok(Self)
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(3)
}
fn units(buffer: &[u16]) -> Result<&[u16]> {
    Ok(&buffer[..buffer
        .iter()
        .position(|unit| *unit == 0)
        .context("unterminated shortcut field")?])
}
fn shell_path(path: &Path) -> Vec<u16> {
    let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
    // Shell APIs use normal drive paths, whereas Rust canonicalize returns \\?\.
    if units.starts_with(&[92, 92, 63, 92]) && units.get(5) == Some(&58) {
        units.drain(..4);
    }
    units.push(0);
    units
}
