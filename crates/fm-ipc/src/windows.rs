//! Narrow Windows security adapter. Named pipes reject remote clients and use
//! an explicit DACL for the process user; default pipe ACLs are not sufficient.
use std::{
    ffi::c_void,
    fs::File,
    io,
    os::windows::{ffi::OsStrExt, io::FromRawHandle},
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, LocalFree, ERROR_ALREADY_EXISTS, GENERIC_WRITE, INVALID_HANDLE_VALUE,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        GetTokenInformation, SetFileSecurityW, TokenUser, DACL_SECURITY_INFORMATION,
        OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SECURITY_ATTRIBUTES,
        TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::{
        CreateDirectoryW, CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ,
    },
    System::{
        Pipes::GetNamedPipeServerProcessId,
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

unsafe fn sid_string(sid: *mut c_void) -> io::Result<String> {
    let mut string = ptr::null_mut();
    if ConvertSidToStringSidW(sid, &mut string) == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut len = 0;
    while *string.add(len) != 0 {
        len += 1;
    }
    let out = String::from_utf16_lossy(std::slice::from_raw_parts(string, len));
    LocalFree(string as _);
    Ok(out)
}

fn user_sid() -> io::Result<String> {
    process_sid(unsafe { GetCurrentProcess() })
}

fn process_sid(process: *mut c_void) -> io::Result<String> {
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut size = 0;
        GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut size);
        let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        if GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        ) == 0
        {
            let err = io::Error::last_os_error();
            CloseHandle(token);
            return Err(err);
        }
        let user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let result = sid_string(user.User.Sid);
        CloseHandle(token);
        result
    }
}

pub fn verify_server(pipe: *mut c_void, expected_pid: u32) -> io::Result<()> {
    unsafe {
        let mut pid = 0;
        if GetNamedPipeServerProcessId(pipe, &mut pid) == 0 {
            return Err(io::Error::last_os_error());
        }
        if pid != expected_pid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unexpected engine process",
            ));
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let sid = process_sid(process);
        CloseHandle(process);
        if sid? != user_sid()? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "engine belongs to another Windows user",
            ));
        }
    }
    Ok(())
}

pub struct Security {
    descriptor: *mut c_void,
}
impl Security {
    pub fn user_only(inherit: bool) -> io::Result<Self> {
        let sid = user_sid()?;
        let ace_flags = if inherit { "OICI" } else { "" };
        let sddl = format!("O:{sid}D:P(A;{ace_flags};GA;;;{sid})");
        let text = wide(std::ffi::OsStr::new(&sddl));
        let mut descriptor = ptr::null_mut();
        unsafe {
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Self { descriptor })
    }
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor,
            bInheritHandle: 0,
        }
    }
}
impl Drop for Security {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.descriptor);
        }
    }
}

/// Creation sets the owner as well as the DACL. An elevated Windows process can
/// otherwise inherit a private DACL while assigning Administrators as owner.
pub fn create_private_file(path: &Path) -> io::Result<File> {
    let security = Security::user_only(false)?;
    let attributes = security.attributes();
    let name = wide(path.as_os_str());
    unsafe {
        let handle = CreateFileW(
            name.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(File::from_raw_handle(handle))
    }
}

pub fn private_profile(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let security = Security::user_only(true)?;
    let attributes = security.attributes();
    let name = wide(path.as_os_str());
    unsafe {
        if CreateDirectoryW(name.as_ptr(), &attributes) == 0
            && io::Error::last_os_error().raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32)
        {
            return Err(io::Error::last_os_error());
        }
        let md = std::fs::symlink_metadata(path)?;
        use std::os::windows::fs::MetadataExt;
        if !md.is_dir() || md.file_attributes() & 0x400 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "profile must be a directory, not a reparse point",
            ));
        }
        let mut owner = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        let status = GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        );
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let owner_sid = sid_string(owner);
        LocalFree(descriptor);
        if owner_sid? != user_sid()? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "profile belongs to another Windows user",
            ));
        }
        if SetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            security.descriptor,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_file_has_explicit_process_user_owner() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("profile");
        private_profile(&profile).unwrap();
        let path = profile.join("discovery.json");
        drop(create_private_file(&path).unwrap());
        let name = wide(path.as_os_str());
        unsafe {
            let mut owner = ptr::null_mut();
            let mut descriptor = ptr::null_mut();
            assert_eq!(
                GetNamedSecurityInfoW(
                    name.as_ptr(),
                    SE_FILE_OBJECT,
                    OWNER_SECURITY_INFORMATION,
                    &mut owner,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    &mut descriptor
                ),
                0
            );
            let owner_sid = sid_string(owner);
            LocalFree(descriptor);
            assert_eq!(owner_sid.unwrap(), user_sid().unwrap());
        }
        assert!(
            create_private_file(&path).is_err(),
            "existing content must not be replaced"
        );
    }
}
