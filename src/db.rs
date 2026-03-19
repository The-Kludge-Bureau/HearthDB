//! HDB_* Lua function implementations and supporting infrastructure.

use crate::lua::{self, LuaState};
use rusqlite::Connection;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Handle table
// ---------------------------------------------------------------------------

const MAX_DBS: usize = 32;

static HANDLES: Mutex<[Option<Connection>; MAX_DBS]> = {
    const NONE: Option<Connection> = None;
    Mutex::new([NONE; MAX_DBS])
};

fn alloc_handle(db: Connection) -> Option<usize> {
    let mut handles = HANDLES.lock().unwrap();
    for (i, slot) in handles.iter_mut().enumerate() {
        if slot.is_none() {
            *slot = Some(db);
            return Some(i + 1); // 1-based
        }
    }
    None
}

fn with_handle<F, R>(h: usize, f: F) -> Option<R>
where
    F: FnOnce(&Connection) -> R,
{
    let handles = HANDLES.lock().unwrap();
    handles.get(h.wrapping_sub(1))?.as_ref().map(f)
}

fn free_handle(h: usize) {
    let mut handles = HANDLES.lock().unwrap();
    if h >= 1 && h <= MAX_DBS {
        handles[h - 1] = None;
    }
}

// ---------------------------------------------------------------------------
// Path helpers (Windows-only)
// ---------------------------------------------------------------------------

/// Returns false if the filename contains any character that would allow
/// path traversal or is otherwise unsafe.
fn is_valid_filename(name: &str) -> bool {
    !name.is_empty() && !name.chars().any(|c| "<>:\"/\\|?*".contains(c))
}

/// Creates the CustomData directory if it does not already exist.
/// Returns true on success or if the directory already exists.
unsafe fn ensure_custom_data_dir() -> bool {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS;
    use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;
    let path: Vec<u16> = "CustomData\0".encode_utf16().collect();
    CreateDirectoryW(path.as_ptr(), std::ptr::null()) != 0
        || GetLastError() == ERROR_ALREADY_EXISTS
}

/// Resolves `filename` relative to CustomData, canonicalizes the result,
/// and confirms it is a strict child of CustomData. Returns the absolute
/// UTF-8 path on success.
unsafe fn resolve_db_path(filename: &str) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::GetFullPathNameW;

    let base_input: Vec<u16> = "CustomData\0".encode_utf16().collect();
    let mut base_buf = vec![0u16; 260];
    let base_len = GetFullPathNameW(
        base_input.as_ptr(),
        base_buf.len() as u32,
        base_buf.as_mut_ptr(),
        std::ptr::null_mut(),
    ) as usize;
    if base_len == 0 {
        return None;
    }
    let mut base = OsString::from_wide(&base_buf[..base_len])
        .to_string_lossy()
        .into_owned();
    if !base.ends_with('\\') {
        base.push('\\');
    }

    let candidate = format!("CustomData\\{}", filename);
    let cand_input: Vec<u16> = {
        let mut v: Vec<u16> = candidate.encode_utf16().collect();
        v.push(0);
        v
    };
    let mut cand_buf = vec![0u16; 260];
    let cand_len = GetFullPathNameW(
        cand_input.as_ptr(),
        cand_buf.len() as u32,
        cand_buf.as_mut_ptr(),
        std::ptr::null_mut(),
    ) as usize;
    if cand_len == 0 {
        return None;
    }
    let cand = OsString::from_wide(&cand_buf[..cand_len])
        .to_string_lossy()
        .into_owned();

    if cand.len() <= base.len() {
        return None;
    }
    if !cand.to_ascii_lowercase().starts_with(&base.to_ascii_lowercase()) {
        return None;
    }

    Some(cand)
}
