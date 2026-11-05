use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use ::windows::Win32::Globalization::{CompareStringOrdinal, CSTR_EQUAL};
use ::windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use ::windows::Win32::System::Threading::GetCurrentProcessId;
use anyhow::{bail, Context, Result};

use super::files;

#[derive(Clone, Debug)]
pub struct Paths {
    data: PathBuf,
    bundle: PathBuf,
    session: u32,
}

impl Paths {
    /// Resolve known per-user data and the real executable directory; no CWD fallback.
    pub fn discover() -> Result<Self> {
        let local = dirs::data_local_dir().context("LocalAppData is unavailable")?;
        if !local.is_absolute() {
            bail!("LocalAppData must be an absolute path");
        }
        let local = local.canonicalize().context("resolve LocalAppData")?;
        let executable = std::env::current_exe()?.canonicalize()?;
        let bundle = executable.parent().context("executable has no directory")?;
        let mut session = 0;
        // Read this process's session, not whichever session is at the console.
        unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session)? };
        Ok(Self {
            data: local.join("Klicky"),
            bundle: bundle.to_path_buf(),
            session,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data
    }

    pub fn config_path(&self) -> PathBuf {
        self.data.join("config.toml")
    }

    pub fn user_sounds_dir(&self) -> PathBuf {
        self.data.join("sounds")
    }

    pub fn runtime_dir(&self) -> PathBuf {
        self.data
            .join("run")
            .join(format!("session-{}", self.session))
    }

    pub fn session_id(&self) -> u32 {
        self.session
    }

    pub fn sound_roots(&self) -> Result<Vec<PathBuf>> {
        let bundled = match std::env::var_os("KLICKY_SYSTEM_SOUNDS_DIR") {
            Some(path) => {
                let path = PathBuf::from(path);
                if !path.is_absolute() {
                    bail!("KLICKY_SYSTEM_SOUNDS_DIR must be absolute on Windows");
                }
                path
            }
            None => self.bundle.join("sounds"),
        };
        Ok(vec![self.user_sounds_dir(), bundled])
    }

    pub fn resolve_sound_pack(&self, name: &str) -> Result<Option<PathBuf>> {
        validate_pack_name(name)?;
        for root in self.sound_roots()? {
            let path = root.join(name);
            match fs::metadata(&path) {
                Ok(metadata) if metadata.is_dir() => return Ok(Some(path)),
                Ok(_) => bail!("sound pack is not a directory: {}", path.display()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => {
                    return Err(error).with_context(|| format!("inspect {}", path.display()))
                }
            }
        }
        Ok(None)
    }

    pub fn available_sound_packs(&self) -> Result<Vec<String>> {
        let mut names: Vec<String> = Vec::new();
        for root in self.sound_roots()? {
            let entries = match fs::read_dir(&root) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            let mut root_names: Vec<String> = Vec::new();
            for entry in entries {
                let entry = entry?;
                if fs::metadata(entry.path())?.is_dir() {
                    let name = entry
                        .file_name()
                        .into_string()
                        .map_err(|_| anyhow::anyhow!("sound-pack name is not valid Unicode"))?;
                    validate_pack_name(&name)?;
                    for previous in &root_names {
                        if same_name(previous, &name)? {
                            bail!(
                                "ambiguous Windows pack names {previous:?} and {name:?} in {}",
                                root.display()
                            );
                        }
                    }
                    root_names.push(name);
                }
            }
            root_names.sort();
            for name in root_names {
                let mut overridden = false;
                for previous in &names {
                    if same_name(previous, &name)? {
                        overridden = true;
                        break;
                    }
                }
                if !overridden {
                    names.push(name);
                }
            }
        }
        names.sort();
        Ok(names)
    }

    pub(super) fn ensure_data_dir(&self) -> Result<()> {
        files::ensure_directory(&self.data)
    }

    pub(super) fn ensure_runtime_dir(&self) -> Result<PathBuf> {
        self.ensure_data_dir()?;
        files::ensure_directory(&self.data.join("run"))?;
        let directory = self.runtime_dir();
        files::ensure_directory(&directory)?;
        Ok(directory)
    }

    pub(super) fn writer_lock(&self) -> PathBuf {
        self.data.join("config.lock")
    }
}

pub(super) fn validate_pack_name(name: &str) -> Result<()> {
    let mut components = Path::new(name).components();
    let single = matches!(components.next(), Some(Component::Normal(part)) if part == name)
        && components.next().is_none();
    let base = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_uppercase();
    let reserved = matches!(
        base.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        base.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    });
    if !single
        || reserved
        || name.encode_utf16().count() > 255
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|ch| ch <= '\u{1f}' || "<>:\"/\\|?*".contains(ch))
    {
        bail!("invalid Windows sound-pack name: {name:?}");
    }
    Ok(())
}

fn same_name(left: &str, right: &str) -> Result<bool> {
    let left: Vec<u16> = left.encode_utf16().collect();
    let right: Vec<u16> = right.encode_utf16().collect();
    // Validated single components bound both slices and keep the OS comparison ordinal.
    let result = unsafe { CompareStringOrdinal(&left, &right, true) };
    if result.0 == 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(result == CSTR_EQUAL)
}
