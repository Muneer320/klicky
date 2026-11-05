//! Local current-logon endpoints. Binding is not engine readiness.
pub(super) mod io;
use std::mem::size_of;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};

use ::windows::core::{BOOL, PCWSTR, PWSTR};
use ::windows::Win32::Foundation::{LocalFree, ERROR_INSUFFICIENT_BUFFER, HANDLE, HLOCAL};
use ::windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SetSecurityInfo, SE_KERNEL_OBJECT,
};
use ::windows::Win32::Security::{
    EqualSid, GetAce, GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetTokenInformation,
    TokenLogonSid, ACCESS_ALLOWED_ACE, ACL, DACL_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
    SE_DACL_PROTECTED, TOKEN_GROUPS, TOKEN_QUERY,
};
use ::windows::Win32::Storage::FileSystem::{
    FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
    PIPE_ACCESS_DUPLEX, WRITE_DAC,
};
use ::windows::Win32::System::Pipes::{CreateNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS};
use ::windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use ::windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcessToken,
};
use anyhow::{bail, Context, Result};

use super::protocol::{InstanceIdentity, PROTOCOL_MAJOR};
use super::InstanceOwner;

pub const PIPE_INSTANCES: usize = 8;
/// Explicit client rights; exclude FILE_APPEND_DATA / FILE_CREATE_PIPE_INSTANCE.
pub const CLIENT_ACCESS: u32 = 0x0012_019b;

#[derive(Clone)]
pub struct PipeAddress {
    name: Vec<u16>,
    logon_sid: String,
}

impl PipeAddress {
    pub fn current() -> Result<Self> {
        let logon_sid = current_logon_sid()?;
        let mut session = 0;
        unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session)? };
        let name = format!(r"\\.\pipe\Klicky.{logon_sid}.{session}.v{PROTOCOL_MAJOR}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        Ok(Self { name, logon_sid })
    }

    /// NUL-terminated local Win32 path, resolved from this process's logon token.
    pub fn name(&self) -> &[u16] {
        &self.name
    }
}

/// Fixed, hardened server endpoints. Borrowed handles must not be duplicated or
/// recreated independently of this owner. All endpoints close before the lease.
pub struct PipePool {
    handles: Vec<OwnedHandle>,
    owner: InstanceOwner,
    address: PipeAddress,
}

impl PipePool {
    pub fn bind(owner: InstanceOwner) -> Result<Self> {
        Self::bind_at(owner, PipeAddress::current()?)
    }

    fn bind_at(owner: InstanceOwner, address: PipeAddress) -> Result<Self> {
        let bootstrap = Descriptor::new(&format!("D:P(A;;GA;;;{})", address.logon_sid))?;
        let strict = Descriptor::new(&format!(
            "D:P(A;;0x{CLIENT_ACCESS:08x};;;{})",
            address.logon_sid
        ))?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: bootstrap.0 .0,
            bInheritHandle: false.into(),
        };
        let mut handles = Vec::with_capacity(PIPE_INSTANCES);
        for index in 0..PIPE_INSTANCES {
            let mut flags =
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAGS_AND_ATTRIBUTES(WRITE_DAC.0);
            if index == 0 {
                flags |= FILE_FLAG_FIRST_PIPE_INSTANCE;
            }
            let handle = unsafe {
                CreateNamedPipeW(
                    PCWSTR(address.name.as_ptr()),
                    flags,
                    PIPE_REJECT_REMOTE_CLIENTS,
                    PIPE_INSTANCES as u32,
                    4096,
                    4096,
                    0,
                    Some(&attributes),
                )
            };
            if handle.is_invalid() {
                return Err(std::io::Error::last_os_error()).context("create local control pipe");
            }
            handles.push(unsafe { OwnedHandle::from_raw_handle(handle.0) });
        }
        // Creating more instances requires rights that clients must not have.
        // Precreate the fixed pool, harden every handle, then reuse those handles.
        let dacl = strict.dacl()?;
        for handle in &handles {
            let handle = HANDLE(handle.as_raw_handle());
            unsafe {
                SetSecurityInfo(
                    handle,
                    SE_KERNEL_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    None,
                    None,
                    Some(dacl),
                    None,
                )
                .ok()?;
            }
            strict.verify_applied(handle)?;
        }
        Ok(Self {
            handles,
            owner,
            address,
        })
    }

    pub fn address(&self) -> &PipeAddress {
        &self.address
    }

    pub fn identity(&self) -> &InstanceIdentity {
        self.owner.identity()
    }

    pub fn handles(&self) -> impl ExactSizeIterator<Item = BorrowedHandle<'_>> {
        self.handles.iter().map(|handle| handle.as_handle())
    }
}

struct Allocation(*mut std::ffi::c_void);
impl Drop for Allocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(Some(HLOCAL(self.0))) };
        }
    }
}

struct Descriptor(PSECURITY_DESCRIPTOR);
impl Drop for Descriptor {
    fn drop(&mut self) {
        if !self.0 .0.is_null() {
            unsafe { LocalFree(Some(HLOCAL(self.0 .0))) };
        }
    }
}

impl Descriptor {
    fn new(sddl: &str) -> Result<Self> {
        let wide: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut descriptor = Self(PSECURITY_DESCRIPTOR::default());
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(wide.as_ptr()),
                1,
                &mut descriptor.0,
                None,
            )?;
        }
        Ok(descriptor)
    }

    fn dacl(&self) -> Result<*mut ACL> {
        let mut present = BOOL::default();
        let mut defaulted = BOOL::default();
        let mut dacl = std::ptr::null_mut();
        unsafe { GetSecurityDescriptorDacl(self.0, &mut present, &mut dacl, &mut defaulted)? };
        if !present.as_bool() || dacl.is_null() {
            bail!("control pipe must have an explicit non-null DACL");
        }
        Ok(dacl)
    }

    fn verify_applied(&self, handle: HANDLE) -> Result<()> {
        let mut applied = Self(PSECURITY_DESCRIPTOR::default());
        unsafe {
            GetSecurityInfo(
                handle,
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                None,
                None,
                Some(&mut applied.0),
            )
            .ok()?;
            let mut control = 0;
            let mut revision = 0;
            GetSecurityDescriptorControl(applied.0, &mut control, &mut revision)?;
            if control & SE_DACL_PROTECTED.0 == 0 {
                bail!("control pipe DACL is not protected");
            }
            let actual = applied.dacl()?;
            let expected = self.dacl()?;
            if (*actual).AceCount != 1 || (*expected).AceCount != 1 {
                bail!("control pipe must have exactly one logon allow ACE");
            }
            let mut actual_ace = std::ptr::null_mut();
            let mut expected_ace = std::ptr::null_mut();
            GetAce(actual, 0, &mut actual_ace)?;
            GetAce(expected, 0, &mut expected_ace)?;
            // Both ACLs were validated/allocated by Windows and remain owned here.
            let actual = &*actual_ace.cast::<ACCESS_ALLOWED_ACE>();
            let expected = &*expected_ace.cast::<ACCESS_ALLOWED_ACE>();
            if actual.Header.AceType != 0
                || actual.Header.AceFlags != 0
                || actual.Mask != CLIENT_ACCESS
            {
                bail!("control pipe has unexpected ACE rights");
            }
            EqualSid(
                PSID(std::ptr::addr_of!(actual.SidStart).cast_mut().cast()),
                PSID(std::ptr::addr_of!(expected.SidStart).cast_mut().cast()),
            )?;
        }
        Ok(())
    }
}

fn current_logon_sid() -> Result<String> {
    process_logon_sid(unsafe { GetCurrentProcess() })
}

pub(super) fn process_logon_sid(process: HANDLE) -> Result<String> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token)? };
    let token = unsafe { OwnedHandle::from_raw_handle(token.0) };
    let mut needed = 0;
    match unsafe {
        GetTokenInformation(
            HANDLE(token.as_raw_handle()),
            TokenLogonSid,
            None,
            0,
            &mut needed,
        )
    } {
        Err(error) if error.code() == ERROR_INSUFFICIENT_BUFFER.to_hresult() => (),
        Err(error) => return Err(error.into()),
        Ok(()) => bail!("unexpected empty logon-SID query"),
    }
    if needed < size_of::<TOKEN_GROUPS>() as u32 || needed > 64 * 1024 {
        bail!("invalid logon-SID token size");
    }
    let mut storage = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    let capacity = (storage.len() * size_of::<usize>()) as u32;
    unsafe {
        GetTokenInformation(
            HANDLE(token.as_raw_handle()),
            TokenLogonSid,
            Some(storage.as_mut_ptr().cast()),
            capacity,
            &mut needed,
        )?;
        if needed > capacity || needed < size_of::<TOKEN_GROUPS>() as u32 {
            bail!("invalid returned logon-SID size");
        }
        let groups = &*storage.as_ptr().cast::<TOKEN_GROUPS>();
        if groups.GroupCount != 1 {
            bail!("interactive control requires one logon SID");
        }
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(groups.Groups[0].Sid, &mut text)?;
        let _allocation = Allocation(text.0.cast());
        Ok(text.to_string()?)
    }
}
