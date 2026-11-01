//! Reading and writing registry values: Start at login lives under the
//! current user's key, and the installer records where it put the app for
//! Settings → Apps, under the machine's key since releases install for every
//! user and under the user's own before that.

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegCloseKey, RegDeleteValueW,
    RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};

use crate::wide;

pub use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

/// A string value, or `None` when the key or the value isn't there.
pub fn read_string(root: HKEY, path: &str, name: &str) -> Option<String> {
    let bytes = read(root, path, name, RRF_RT_REG_SZ)?;
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|unit| *unit != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

/// A binary value, or `None` when the key or the value isn't there.
pub fn read_binary(root: HKEY, path: &str, name: &str) -> Option<Vec<u8>> {
    read(root, path, name, RRF_RT_REG_BINARY)
}

/// Sets a string value in a key that already exists.
pub fn write_string(root: HKEY, path: &str, name: &str, value: &str) -> std::io::Result<()> {
    let data: Vec<u8> = wide(value)
        .iter()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    with_key(root, path, KEY_SET_VALUE, |key| {
        let name = wide(name);
        let len = u32::try_from(data.len()).unwrap_or(0);
        // SAFETY: `key` is open for writing; name and data outlive the call.
        unsafe { RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, data.as_ptr(), len) }
    })
}

/// Deletes a value; one that isn't there counts as deleted.
pub fn delete_value(root: HKEY, path: &str, name: &str) -> std::io::Result<()> {
    with_key(root, path, KEY_SET_VALUE, |key| {
        let name = wide(name);
        // SAFETY: `key` is open for writing; the name outlives the call.
        let status = unsafe { RegDeleteValueW(key, name.as_ptr()) };
        if status == ERROR_FILE_NOT_FOUND {
            ERROR_SUCCESS
        } else {
            status
        }
    })
}

fn with_key(
    root: HKEY,
    path: &str,
    access: u32,
    f: impl FnOnce(HKEY) -> u32,
) -> std::io::Result<()> {
    let path = wide(path);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `path` outlives the call and `key` is a valid out-pointer.
    let status = unsafe { RegOpenKeyExW(root, path.as_ptr(), 0, access, &mut key) };
    if status != ERROR_SUCCESS {
        return Err(os_error(status));
    }
    let result = f(key);
    // SAFETY: `key` was opened above.
    unsafe { RegCloseKey(key) };
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(os_error(result))
    }
}

fn read(root: HKEY, path: &str, name: &str, kind: u32) -> Option<Vec<u8>> {
    let path = wide(path);
    let name = wide(name);
    let mut len = 0_u32;
    // SAFETY: a size query: null data pointer, `len` receives the size.
    let status = unsafe {
        RegGetValueW(
            root,
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
            root,
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

fn os_error(status: u32) -> std::io::Error {
    std::io::Error::from_raw_os_error(i32::try_from(status).unwrap_or(-1))
}
