//! What a process may reach, as its access token says: its integrity level,
//! whether it has UI Access, and whether it runs elevated. Windows lets a
//! process drive another's windows only at an integrity level at least as
//! high, and a release's tray app has UI Access so that, started by an
//! administrator, it runs at High and reaches the administrator's elevated
//! windows without being elevated itself.

use std::io;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    CheckTokenMembership, CreateWellKnownSid, GetSidSubAuthority, GetSidSubAuthorityCount,
    GetTokenInformation, SECURITY_MAX_SID_SIZE, TOKEN_ELEVATION, TOKEN_INFORMATION_CLASS,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenElevation, TokenIntegrityLevel, TokenUIAccess,
    WinBuiltinAdministratorsSid,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// The parts of a process's token that decide whose windows it can move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Standing {
    /// The integrity level's relative identifier: 0x2000 is Medium, 0x3000
    /// High.
    pub integrity: u32,
    pub ui_access: bool,
    /// Running with an administrator's full token, as Run as administrator
    /// does; a UI Access copy at High integrity isn't.
    pub elevated: bool,
}

impl Standing {
    /// This process's.
    pub fn current() -> io::Result<Self> {
        // SAFETY: the pseudo-handle for this process needs no closing.
        read(unsafe { GetCurrentProcess() })
    }

    /// The process `pid`'s. Querying it needs nothing beyond what any
    /// process of the same user is granted.
    pub fn of(pid: u32) -> io::Result<Self> {
        // SAFETY: opens a process for querying only; the handle is closed below.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let standing = read(process);
        // SAFETY: closes the handle opened above.
        unsafe { CloseHandle(process) };
        standing
    }

    /// The integrity level as Windows names it.
    pub fn integrity_name(&self) -> &'static str {
        integrity_name(self.integrity)
    }
}

/// Whether this process has an administrator's rights: the Administrators
/// group enabled in its token, as an elevated process has it. An
/// administrator's process that isn't elevated has the group only to deny
/// access, and so does one whose token Windows cut down from an elevated one,
/// as `runas /trustlevel` does, though that still reports elevated.
pub fn runs_as_administrator() -> bool {
    let mut sid = [0_u32; SECURITY_MAX_SID_SIZE as usize / 4];
    let mut size = SECURITY_MAX_SID_SIZE;
    // SAFETY: `sid` has room for any SID, `size` says so, and it is u32s so
    // the SID is aligned.
    let made = unsafe {
        CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            std::ptr::null_mut(),
            sid.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if made == 0 {
        return false;
    }
    let mut member = 0;
    // SAFETY: a null token checks this thread's token, or this process's; the
    // SID was made above.
    let checked =
        unsafe { CheckTokenMembership(std::ptr::null_mut(), sid.as_mut_ptr().cast(), &mut member) };
    checked != 0 && member != 0
}

/// The name of an integrity level's relative identifier.
pub fn integrity_name(rid: u32) -> &'static str {
    match rid {
        0x4000.. => "System",
        0x3000.. => "High",
        0x2100.. => "Medium Plus",
        0x2000.. => "Medium",
        0x1000.. => "Low",
        _ => "Untrusted",
    }
}

/// Reads the standing of `process`, a handle open for querying.
fn read(process: HANDLE) -> io::Result<Standing> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: `token` is a valid out-pointer; it is closed below.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let standing = read_token(token);
    // SAFETY: closes the token opened above.
    unsafe { CloseHandle(token) };
    standing
}

fn read_token(token: HANDLE) -> io::Result<Standing> {
    let mut ui_access = 0_u32;
    query(token, TokenUIAccess, &mut ui_access)?;
    let mut elevation = TOKEN_ELEVATION::default();
    query(token, TokenElevation, &mut elevation)?;
    Ok(Standing {
        integrity: integrity(token)?,
        ui_access: ui_access != 0,
        elevated: elevation.TokenIsElevated != 0,
    })
}

/// Reads a fixed-size piece of token information into `value`.
fn query<T>(token: HANDLE, class: TOKEN_INFORMATION_CLASS, value: &mut T) -> io::Result<()> {
    let size = u32::try_from(std::mem::size_of::<T>()).unwrap_or(0);
    let mut returned = 0_u32;
    // SAFETY: `value` has room for `size` bytes, the size of the type this
    // information class fills in.
    let ok = unsafe {
        GetTokenInformation(
            token,
            class,
            std::ptr::from_mut(value).cast(),
            size,
            &mut returned,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The integrity level: the last part of the label SID's identifier.
fn integrity(token: HANDLE) -> io::Result<u32> {
    let mut size = 0_u32;
    // SAFETY: a size query with no buffer; it fails, filling in `size`.
    unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            std::ptr::null_mut(),
            0,
            &mut size,
        );
    }
    if size == 0 {
        return Err(io::Error::last_os_error());
    }
    // u64s, so the label's pointer to its SID is aligned.
    let mut buffer = vec![0_u64; usize::try_from(size).unwrap_or(0).div_ceil(8)];
    // SAFETY: `buffer` has room for `size` bytes.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call filled the buffer with a TOKEN_MANDATORY_LABEL whose
    // SID lies within it, and a label SID has at least one sub-authority.
    unsafe {
        let label = &*buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let sid = label.Label.Sid;
        let count = u32::from(*GetSidSubAuthorityCount(sid));
        Ok(*GetSidSubAuthority(sid, count.saturating_sub(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrity_levels_have_their_windows_names() {
        assert_eq!(integrity_name(0x2000), "Medium");
        assert_eq!(integrity_name(0x2100), "Medium Plus");
        assert_eq!(integrity_name(0x3000), "High");
        assert_eq!(integrity_name(0x4000), "System");
        assert_eq!(integrity_name(0x1000), "Low");
        assert_eq!(integrity_name(0), "Untrusted");
    }

    /// An administrator's rights come only with elevation, though an elevated
    /// token cut down by runas /trustlevel has neither.
    #[test]
    fn an_administrator_runs_elevated() {
        let standing = Standing::current().expect("reads this process");
        if runs_as_administrator() {
            assert!(standing.elevated, "{standing:?}");
        }
    }

    #[test]
    fn this_process_has_a_standing() {
        let standing = Standing::current().expect("this process's token is readable");
        assert!(standing.integrity >= 0x1000, "{standing:?}");
    }
}
