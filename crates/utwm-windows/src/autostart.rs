//! Start at login, per user: a value under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, which is what Task
//! Manager's Startup apps page lists and toggles. Only the tray menu turns this
//! on; nothing installs it behind the user's back.

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegCloseKey,
    RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};

use crate::wide;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Task Manager records "disabled" here without touching the Run value.
const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
pub const VALUE_NAME: &str = "universal-tiling-window-manager";

/// Whether the app starts at login: listed under Run and not disabled in Task
/// Manager.
pub fn is_enabled() -> bool {
    let listed = get_value(RUN_KEY, VALUE_NAME, RRF_RT_REG_SZ).is_some();
    // The first byte of the StartupApproved entry is odd when the user disabled it.
    let disabled = get_value(APPROVED_KEY, VALUE_NAME, RRF_RT_REG_BINARY)
        .and_then(|bytes| bytes.first().copied())
        .is_some_and(|flag| flag & 1 == 1);
    listed && !disabled
}

/// Turns start at login on for `exe`, or off.
pub fn set_enabled(enabled: bool, exe: &std::path::Path) -> std::io::Result<()> {
    if enabled {
        let command = format!("\"{}\"", exe.display());
        let data: Vec<u8> = wide(&command)
            .iter()
            .flat_map(|unit| unit.to_le_bytes())
            .collect();
        with_key(RUN_KEY, KEY_SET_VALUE, |key| {
            let name = wide(VALUE_NAME);
            let len = u32::try_from(data.len()).unwrap_or(0);
            // SAFETY: `key` is open for writing; name and data outlive the call.
            unsafe { RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, data.as_ptr(), len) }
        })?;
        // Clear a "disabled" left by Task Manager so the entry really runs.
        let _ = with_key(APPROVED_KEY, KEY_SET_VALUE, |key| {
            let name = wide(VALUE_NAME);
            // SAFETY: `key` is open for writing; the name outlives the call.
            unsafe { RegDeleteValueW(key, name.as_ptr()) }
        });
        Ok(())
    } else {
        with_key(RUN_KEY, KEY_SET_VALUE, |key| {
            let name = wide(VALUE_NAME);
            // SAFETY: `key` is open for writing; the name outlives the call.
            let status = unsafe { RegDeleteValueW(key, name.as_ptr()) };
            if status == windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND {
                ERROR_SUCCESS
            } else {
                status
            }
        })
    }
}

fn with_key(path: &str, access: u32, f: impl FnOnce(HKEY) -> u32) -> std::io::Result<()> {
    let path = wide(path);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `path` outlives the call and `key` is a valid out-pointer.
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) };
    if status != ERROR_SUCCESS {
        return Err(std::io::Error::from_raw_os_error(
            i32::try_from(status).unwrap_or(-1),
        ));
    }
    let result = f(key);
    // SAFETY: `key` was opened above.
    unsafe { RegCloseKey(key) };
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(
            i32::try_from(result).unwrap_or(-1),
        ))
    }
}

fn get_value(path: &str, name: &str, kind: u32) -> Option<Vec<u8>> {
    let path = wide(path);
    let name = wide(name);
    let mut len = 0_u32;
    // SAFETY: a size query: null data pointer, `len` receives the size.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            name.as_ptr(),
            kind,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut len,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let mut data = vec![0_u8; usize::try_from(len).ok()?];
    // SAFETY: `data` has room for `len` bytes and `len` is updated in place.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            name.as_ptr(),
            kind,
            std::ptr::null_mut(),
            data.as_mut_ptr().cast(),
            &mut len,
        )
    };
    (status == ERROR_SUCCESS).then(|| {
        data.truncate(usize::try_from(len).unwrap_or(0));
        data
    })
}
