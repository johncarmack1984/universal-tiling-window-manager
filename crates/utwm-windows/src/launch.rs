//! Starting programs the way the shell does. An executable with UI Access in
//! its manifest, which a release's tray app is, can't be started through
//! CreateProcess, the way a terminal starts programs: only ShellExecute starts
//! it, by way of Windows' Application Information service, which is also what
//! grants the access. The service starts it with the token of whoever asked,
//! so an elevated process would start it elevated; such a process runs the
//! asking step as the desktop's own user instead.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree, WAIT_OBJECT_0};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    DuplicateTokenEx, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, SecurityImpersonation,
    TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_SESSIONID, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE,
    TOKEN_QUERY, TokenPrimary,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, FILE_SHARE_READ, GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessWithTokenW, GetExitCodeProcess, OpenProcess, OpenProcessToken,
    PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetShellWindow, GetWindowThreadProcessId, SW_SHOWNORMAL,
};

use crate::token::Standing;

/// Opens `program` with `parameters` through the shell, as a double-click
/// would, and returns once it has started, or once the administrator prompt it
/// needs has been answered: declining that fails with `ERROR_CANCELLED`.
pub fn shell_open(program: &Path, parameters: &str) -> io::Result<()> {
    let verb = wide(OsStr::new("open"));
    let file = wide(program.as_os_str());
    let parameters = wide(OsStr::new(parameters));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(std::mem::size_of::<SHELLEXECUTEINFOW>()).unwrap_or(0),
        // The caller may exit as soon as this returns, so it returns only once
        // the start is done.
        fMask: SEE_MASK_NOASYNC,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: parameters.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // SAFETY: `info` is initialised and every string it points to outlives the call.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Writes `bytes` to a file named `name` in a new folder, `folder`, and opens
/// it through the shell with `parameters` as [`shell_open`] does, making sure
/// that what opens is `bytes` even when what runs is an installer that asks
/// for an administrator. The folder carries this process's integrity level as
/// its label, so no process below that level can create, change, rename or
/// delete anything in it; the file is then held so that nothing at all can
/// write, rename or delete it, or move a folder it is in, until it has
/// started; and it opens by the path the held file has, with any junction on
/// the way resolved.
pub fn write_and_open(folder: &Path, name: &str, bytes: &[u8], parameters: &str) -> io::Result<()> {
    create_guarded_folder(folder)?;
    let path = folder.join(name);
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?
        .write_all(bytes)?;
    let mut held = hold(&path)?;
    let mut written = Vec::with_capacity(bytes.len());
    held.read_to_end(&mut written)?;
    if written != bytes {
        return Err(io::Error::other(format!(
            "{} changed before it could run",
            path.display()
        )));
    }
    let opened = shell_open(&final_path(&held)?, parameters);
    drop(held);
    opened
}

/// Opens `path` for reading, refusing everyone else write and delete access
/// for as long as the handle is open.
fn hold(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
}

/// Creates `folder`, which must not exist yet, labelled with this process's
/// integrity level and no write-up: a process below that level can't write
/// in it, and what is created in it carries the same label.
fn create_guarded_folder(folder: &Path) -> io::Result<()> {
    let integrity = Standing::current()?.integrity;
    let sddl = wide(OsStr::new(&format!("S:(ML;OICI;NW;;;S-1-16-{integrity})")));
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: parses a constant-shaped SDDL string; `descriptor` is freed below.
    let parsed = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if parsed == 0 {
        return Err(io::Error::last_os_error());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let path = wide(folder.as_os_str());
    // SAFETY: the path and the descriptor outlive the call; the folder gets
    // the label as it is created, so it never exists without it.
    let created = unsafe { CreateDirectoryW(path.as_ptr(), &attributes) };
    let result = if created == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    };
    // SAFETY: frees what the conversion above allocated.
    unsafe { LocalFree(descriptor) };
    result
}

/// Whether the desktop's shell, Explorer, runs elevated, as it does with UAC
/// off or as the built-in Administrator. `None` without a desktop, as over SSH.
pub fn shell_is_elevated() -> Option<bool> {
    let pid = shell_pid()?;
    Standing::of(pid).ok().map(|standing| standing.elevated)
}

/// Runs `program` with `parameters` as the desktop's user, with a copy of the
/// shell's own token: how an elevated process starts one that isn't. Waits up
/// to `wait` for it to finish and returns its exit code, or `None` if it is
/// still running.
pub fn run_as_shell_user(
    program: &Path,
    parameters: &str,
    wait: Duration,
) -> io::Result<Option<u32>> {
    let pid = shell_pid().ok_or_else(|| io::Error::other("no desktop shell is running"))?;
    // SAFETY: opens the shell's process for querying only; closed below.
    let shell = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if shell.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mut shell_token: HANDLE = std::ptr::null_mut();
    // SAFETY: `shell_token` is a valid out-pointer.
    let opened = unsafe { OpenProcessToken(shell, TOKEN_DUPLICATE, &mut shell_token) };
    let opened = if opened == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(shell_token)
    };
    // SAFETY: closes the process handle opened above.
    unsafe { CloseHandle(shell) };
    let shell_token = opened?;

    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: duplicates the shell's token as a primary token for a new
    // process; `token` is a valid out-pointer.
    let duplicated = unsafe {
        DuplicateTokenEx(
            shell_token,
            TOKEN_QUERY
                | TOKEN_DUPLICATE
                | TOKEN_ASSIGN_PRIMARY
                | TOKEN_ADJUST_DEFAULT
                | TOKEN_ADJUST_SESSIONID,
            std::ptr::null(),
            SecurityImpersonation,
            TokenPrimary,
            &mut token,
        )
    };
    // SAFETY: closes the token opened above.
    unsafe { CloseHandle(shell_token) };
    if duplicated == 0 {
        return Err(io::Error::last_os_error());
    }

    let application = wide(program.as_os_str());
    let mut command_line = quoted(program);
    if !parameters.is_empty() {
        command_line.push(' ');
        command_line.push_str(parameters);
    }
    let mut command_line = wide(OsStr::new(&command_line));
    let startup = STARTUPINFOW {
        cb: u32::try_from(std::mem::size_of::<STARTUPINFOW>()).unwrap_or(0),
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: the token was duplicated for exactly this; the strings outlive
    // the call and the command line buffer is writable, as the call requires.
    let created = unsafe {
        CreateProcessWithTokenW(
            token,
            0,
            application.as_ptr(),
            command_line.as_mut_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut process,
        )
    };
    let error = io::Error::last_os_error();
    // SAFETY: closes the duplicated token.
    unsafe { CloseHandle(token) };
    if created == 0 {
        return Err(error);
    }
    let millis = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
    // SAFETY: waits on, reads and closes the handles the call returned.
    unsafe {
        let mut code = None;
        if WaitForSingleObject(process.hProcess, millis) == WAIT_OBJECT_0 {
            let mut exit = 0_u32;
            if GetExitCodeProcess(process.hProcess, &mut exit) != 0 {
                code = Some(exit);
            }
        }
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
        Ok(code)
    }
}

/// The path `file` is open at, with every link and junction on the way
/// resolved: the path to name it by when nobody must be able to put another
/// file there in between.
pub fn final_path(file: &std::fs::File) -> io::Result<PathBuf> {
    let handle = file.as_raw_handle();
    let mut buffer = vec![0_u16; 512];
    loop {
        let capacity = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
        // SAFETY: `buffer` has room for `capacity` characters.
        let length = unsafe {
            GetFinalPathNameByHandleW(handle, buffer.as_mut_ptr(), capacity, VOLUME_NAME_DOS)
        };
        let length = usize::try_from(length).unwrap_or(0);
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if length < buffer.len() {
            let path = String::from_utf16_lossy(&buffer[..length]);
            return Ok(PathBuf::from(without_verbatim_prefix(&path)));
        }
        // Too small: `length` is the size needed, terminator included.
        buffer.resize(length, 0);
    }
}

/// `\\?\C:\x` as `C:\x`, and `\\?\UNC\server\share` as `\\server\share`: the
/// shell doesn't take the verbatim forms.
fn without_verbatim_prefix(path: &str) -> String {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(local) = path.strip_prefix(r"\\?\") {
        local.to_owned()
    } else {
        path.to_owned()
    }
}

/// The process the desktop's shell window belongs to.
fn shell_pid() -> Option<u32> {
    // SAFETY: no arguments; returns null without a shell.
    let window = unsafe { GetShellWindow() };
    if window.is_null() {
        return None;
    }
    let mut pid = 0_u32;
    // SAFETY: `pid` is a valid out-pointer.
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    (pid != 0).then_some(pid)
}

fn quoted(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_paths_lose_their_prefix() {
        assert_eq!(
            without_verbatim_prefix(r"\\?\C:\Users\me\AppData\Local\Temp\x\setup.exe"),
            r"C:\Users\me\AppData\Local\Temp\x\setup.exe"
        );
        assert_eq!(
            without_verbatim_prefix(r"\\?\UNC\server\share\setup.exe"),
            r"\\server\share\setup.exe"
        );
        assert_eq!(without_verbatim_prefix(r"C:\x.exe"), r"C:\x.exe");
    }

    /// [`write_and_open`] relies on this: while the file is held, no folder
    /// above it can be moved, so its path keeps leading to it.
    #[test]
    fn a_held_file_keeps_its_folders_in_place() {
        let root = std::env::temp_dir().join(format!("utwm-held-{}", std::process::id()));
        let inner = root.join("inner");
        std::fs::create_dir_all(&inner).expect("creates the folders");
        let file = inner.join("held.exe");
        std::fs::write(&file, [1, 2, 3]).expect("writes the file");
        let held = hold(&file).expect("holds the file");
        let inner_moved = std::fs::rename(&inner, root.join("inner-moved"));
        let root_moved = std::fs::rename(&root, root.with_extension("moved"));
        let rewritten = std::fs::write(&file, [4, 5, 6]);
        drop(held);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(root.with_extension("moved"));
        assert!(inner_moved.is_err(), "its folder moved");
        assert!(root_moved.is_err(), "the folder above moved");
        assert!(rewritten.is_err(), "it was rewritten");
    }

    #[test]
    fn a_guarded_folder_carries_this_process_label() {
        let folder = std::env::temp_dir().join(format!("utwm-guarded-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let created = create_guarded_folder(&folder);
        let label = label_of(&folder);
        let again = create_guarded_folder(&folder);
        let _ = std::fs::remove_dir_all(&folder);
        created.expect("creates the folder");
        let integrity = Standing::current().expect("reads this process").integrity;
        assert_eq!(label, Some((integrity, true, true)));
        assert!(again.is_err(), "a folder that exists is never reused");
    }

    /// A folder's mandatory label: its integrity level, whether it stops
    /// writes from below, and whether what is created in it inherits it.
    fn label_of(folder: &Path) -> Option<(u32, bool, bool)> {
        use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
        use windows_sys::Win32::Security::{
            ACL, CONTAINER_INHERIT_ACE, GetAce, GetSidSubAuthority, GetSidSubAuthorityCount,
            LABEL_SECURITY_INFORMATION, OBJECT_INHERIT_ACE, SYSTEM_MANDATORY_LABEL_ACE,
        };
        // winnt.h's SYSTEM_MANDATORY_LABEL_ACE_TYPE and _NO_WRITE_UP.
        const LABEL_ACE: u8 = 0x11;
        const NO_WRITE_UP: u32 = 0x1;

        let path = wide(folder.as_os_str());
        let mut sacl: *mut ACL = std::ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: reads the folder's label into memory freed below.
        let status = unsafe {
            GetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                LABEL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut sacl,
                &mut descriptor,
            )
        };
        if status != 0 || sacl.is_null() {
            return None;
        }
        let mut ace: *mut core::ffi::c_void = std::ptr::null_mut();
        // SAFETY: `sacl` points into `descriptor`, still allocated; a label
        // ACE's SID starts at its SidStart field.
        let label = unsafe {
            (GetAce(sacl, 0, &mut ace) != 0)
                .then(|| &*ace.cast::<SYSTEM_MANDATORY_LABEL_ACE>())
                .filter(|label| label.Header.AceType == LABEL_ACE)
                .map(|label| {
                    let sid = std::ptr::from_ref(&label.SidStart).cast_mut().cast();
                    let count = u32::from(*GetSidSubAuthorityCount(sid));
                    let flags = u32::from(label.Header.AceFlags);
                    (
                        *GetSidSubAuthority(sid, count.saturating_sub(1)),
                        label.Mask & NO_WRITE_UP != 0,
                        flags & (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE)
                            == OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
                    )
                })
        };
        // SAFETY: frees what GetNamedSecurityInfoW allocated.
        unsafe { LocalFree(descriptor) };
        label
    }

    #[test]
    fn an_open_file_has_a_final_path() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("utwm-final-path-{}.tmp", std::process::id()));
        let file = std::fs::File::create(&path).expect("creates a temporary file");
        let resolved = final_path(&file).expect("resolves its path");
        drop(file);
        let _ = std::fs::remove_file(&path);
        assert!(
            resolved
                .file_name()
                .is_some_and(|name| name == path.file_name().unwrap_or_default()),
            "{} for {}",
            resolved.display(),
            path.display()
        );
        assert!(!resolved.to_string_lossy().starts_with(r"\\?\"));
    }
}
