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

// ---------------------------------------------------------------------------
// Lua script functions
// ---------------------------------------------------------------------------

pub unsafe extern "fastcall" fn script_hdb_open(l: LuaState) -> u32 {
    let l = lua::get_lua_state();

    if lua::lua_gettop(l) != 1 || !lua::lua_isstring(l, 1) {
        lua::lua_error(l, "Usage: HDB_Open(filename)");
        return 0;
    }

    let filename = match lua::lua_tostring(l, 1) {
        Some(s) => s,
        None => {
            lua::lua_error(l, "HDB_Open: filename is nil");
            return 0;
        }
    };

    if !is_valid_filename(&filename) {
        lua::lua_error(l, "HDB_Open: invalid filename (must not contain < > : \" / \\ | ? *)");
        return 0;
    }

    if !ensure_custom_data_dir() {
        lua::lua_error(l, "HDB_Open: could not create CustomData directory");
        return 0;
    }

    let full_path = match resolve_db_path(&filename) {
        Some(p) => p,
        None => {
            lua::lua_error(l, "HDB_Open: path must remain inside CustomData");
            return 0;
        }
    };

    let db = match Connection::open(&full_path) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("HDB_Open: {}", e);
            lua::lua_error(l, &msg);
            return 0;
        }
    };

    match alloc_handle(db) {
        Some(h) => {
            lua::lua_pushnumber(l, h as f64);
            1
        }
        None => {
            lua::lua_error(l, "HDB_Open: too many open databases (max 32)");
            0
        }
    }
}

pub unsafe extern "fastcall" fn script_hdb_close(l: LuaState) -> u32 {
    let l = lua::get_lua_state();

    if lua::lua_gettop(l) != 1 || !lua::lua_isnumber(l, 1) {
        lua::lua_error(l, "Usage: HDB_Close(handle)");
        return 0;
    }

    let h = lua::lua_tonumber(l, 1) as usize;
    let valid = {
        let handles = HANDLES.lock().unwrap();
        handles.get(h.wrapping_sub(1)).map_or(false, |s| s.is_some())
    };
    if !valid {
        lua::lua_error(l, "HDB_Close: invalid or already-closed handle");
        return 0;
    }

    free_handle(h);
    0
}

pub unsafe extern "fastcall" fn script_hdb_execute(l: LuaState) -> u32 {
    let l = lua::get_lua_state();

    if lua::lua_gettop(l) != 2 || !lua::lua_isnumber(l, 1) || !lua::lua_isstring(l, 2) {
        lua::lua_error(l, "Usage: HDB_Execute(handle, sql)");
        return 0;
    }

    let h = lua::lua_tonumber(l, 1) as usize;
    let sql = match lua::lua_tostring(l, 2) {
        Some(s) => s,
        None => {
            lua::lua_error(l, "HDB_Execute: sql is nil");
            return 0;
        }
    };

    let result = with_handle(h, |db| db.execute_batch(&sql));
    match result {
        None => {
            lua::lua_error(l, "HDB_Execute: invalid handle");
        }
        Some(Err(e)) => {
            let msg = format!("HDB_Execute: {}", e);
            lua::lua_error(l, &msg);
        }
        Some(Ok(())) => {}
    }
    0
}

pub unsafe extern "fastcall" fn script_hdb_query(l: LuaState) -> u32 {
    let l = lua::get_lua_state();

    if lua::lua_gettop(l) != 2 || !lua::lua_isnumber(l, 1) || !lua::lua_isstring(l, 2) {
        lua::lua_error(l, "Usage: HDB_Query(handle, sql)");
        return 0;
    }

    let h = lua::lua_tonumber(l, 1) as usize;
    let sql = match lua::lua_tostring(l, 2) {
        Some(s) => s,
        None => {
            lua::lua_error(l, "HDB_Query: sql is nil");
            return 0;
        }
    };

    let rows_result: Option<Result<Vec<Vec<(String, Option<String>)>>, rusqlite::Error>> =
        with_handle(h, |db| {
            let mut stmt = db.prepare(&sql)?;
            let col_names: Vec<String> = stmt
                .column_names()
                .iter()
                .map(|s| s.to_string())
                .collect();
            let col_count = col_names.len();
            let mut rows: Vec<Vec<(String, Option<String>)>> = Vec::new();
            let mut query = stmt.query([])?;
            while let Some(row) = query.next()? {
                let mut r = Vec::with_capacity(col_count);
                for (i, name) in col_names.iter().enumerate() {
                    let val: Option<String> = row.get(i).ok();
                    r.push((name.clone(), val));
                }
                rows.push(r);
            }
            Ok(rows)
        });

    match rows_result {
        None => {
            lua::lua_error(l, "HDB_Query: invalid handle");
            return 0;
        }
        Some(Err(e)) => {
            let msg = format!("HDB_Query: {}", e);
            lua::lua_error(l, &msg);
            return 0;
        }
        Some(Ok(rows)) => {
            lua::lua_newtable(l);
            for (row_idx, row) in rows.iter().enumerate() {
                lua::lua_newtable(l);
                for (col_name, val) in row {
                    lua::lua_pushstring(l, col_name);
                    match val {
                        Some(s) => lua::lua_pushstring(l, s),
                        None    => lua::lua_pushnil(l),
                    }
                    lua::lua_settable(l, -3);
                }
                lua::lua_rawseti(l, -2, (row_idx + 1) as i32);
            }
        }
    }
    1
}

pub unsafe extern "fastcall" fn script_hdb_query_raw(l: LuaState) -> u32 {
    let l = lua::get_lua_state();

    if lua::lua_gettop(l) != 2 || !lua::lua_isnumber(l, 1) || !lua::lua_isstring(l, 2) {
        lua::lua_error(l, "Usage: HDB_QueryRaw(handle, sql)");
        return 0;
    }

    let h = lua::lua_tonumber(l, 1) as usize;
    let sql = match lua::lua_tostring(l, 2) {
        Some(s) => s,
        None => {
            lua::lua_error(l, "HDB_QueryRaw: sql is nil");
            return 0;
        }
    };

    let result: Option<Result<(Vec<String>, Vec<Vec<Option<String>>>), rusqlite::Error>> =
        with_handle(h, |db| {
            let mut stmt = db.prepare(&sql)?;
            let col_names: Vec<String> = stmt
                .column_names()
                .iter()
                .map(|s| s.to_string())
                .collect();
            let col_count = col_names.len();
            let mut rows: Vec<Vec<Option<String>>> = Vec::new();
            let mut query = stmt.query([])?;
            while let Some(row) = query.next()? {
                let mut r = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    r.push(row.get(i).ok());
                }
                rows.push(r);
            }
            Ok((col_names, rows))
        });

    match result {
        None => {
            lua::lua_error(l, "HDB_QueryRaw: invalid handle");
            return 0;
        }
        Some(Err(e)) => {
            let msg = format!("HDB_QueryRaw: {}", e);
            lua::lua_error(l, &msg);
            return 0;
        }
        Some(Ok((col_names, rows))) => {
            lua::lua_newtable(l); // cols array
            for (i, name) in col_names.iter().enumerate() {
                lua::lua_pushstring(l, name);
                lua::lua_rawseti(l, -2, (i + 1) as i32);
            }

            lua::lua_newtable(l); // rows array
            for (row_idx, row) in rows.iter().enumerate() {
                lua::lua_newtable(l); // positional row
                for (col_idx, val) in row.iter().enumerate() {
                    match val {
                        Some(s) => lua::lua_pushstring(l, s),
                        None    => lua::lua_pushnil(l),
                    }
                    lua::lua_rawseti(l, -2, (col_idx + 1) as i32);
                }
                lua::lua_rawseti(l, -2, (row_idx + 1) as i32);
            }
        }
    }
    2
}
