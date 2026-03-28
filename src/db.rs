//! HDB_* Lua function implementations and supporting infrastructure.

use crate::lua::{self, LuaState};
use rusqlite::types::Value;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

const VERSION_MAJOR: u32 = 0;
const VERSION_MINOR: u32 = 2;
const VERSION_PATCH: u32 = 0;

// ---------------------------------------------------------------------------
// Handle table
// ---------------------------------------------------------------------------

const MAX_DBS: usize = 32;

/// Wrapper to allow sending rusqlite::Connection to the worker thread.
/// Safety: bundled SQLite compiles in serialized mode, so Connection is safe
/// to use from any thread as long as access is externally synchronized.
pub struct SendConnection(pub Connection);
unsafe impl Send for SendConnection {}

pub type Handle = Arc<Mutex<SendConnection>>;

static HANDLES: Mutex<[Option<Handle>; MAX_DBS]> = {
    const NONE: Option<Handle> = None;
    Mutex::new([NONE; MAX_DBS])
};

fn alloc_handle(db: Connection) -> Option<usize> {
    let handle = Arc::new(Mutex::new(SendConnection(db)));
    let mut handles = HANDLES.lock().unwrap();
    for (i, slot) in handles.iter_mut().enumerate() {
        if slot.is_none() {
            *slot = Some(handle);
            return Some(i + 1); // 1-based
        }
    }
    None
}

/// Clone the Arc for the given handle so callers can use the connection
/// without holding the global handle-table lock.
pub fn clone_handle(h: usize) -> Option<Handle> {
    let handles = HANDLES.lock().unwrap();
    handles.get(h.wrapping_sub(1))?.as_ref().map(Arc::clone)
}

fn with_handle<F, R>(h: usize, f: F) -> Option<R>
where
    F: FnOnce(&Connection) -> R,
{
    let conn = clone_handle(h)?;
    let guard = conn.lock().unwrap();
    Some(f(&guard.0))
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
fn ensure_custom_data_dir() -> bool {
    match std::fs::create_dir("CustomData") {
        Ok(()) => true,
        Err(e) => e.kind() == std::io::ErrorKind::AlreadyExists,
    }
}

/// Builds the path for `filename` inside CustomData.
/// Safety is guaranteed by is_valid_filename, which rejects any character
/// that could introduce a path separator or traversal component.
fn resolve_db_path(filename: &str) -> String {
    format!("CustomData\\{}", filename)
}

/// Returns true if every component of a slash- or backslash-delimited path is
/// a valid filename component (non-empty, no separator chars, not `.` or `..`).
fn is_valid_path(path: &str) -> bool {
    !path.is_empty()
        && path
            .split(|c| c == '/' || c == '\\')
            .all(|part| part != "." && part != ".." && is_valid_filename(part))
}

/// Builds the path for `path` inside `Interface\AddOns\<addon_name>`.
/// Safety is guaranteed by is_valid_filename (addon_name) and is_valid_path
/// (path), which together reject any separator or traversal component.
fn resolve_addon_db_path(addon_name: &str, path: &str) -> String {
    format!("Interface\\AddOns\\{}\\{}", addon_name, path.replace('/', "\\"))
}

/// Converts a rusqlite Value to an Option<String>.
/// NULL becomes None; all other types are coerced to their string
/// representation so Lua always receives a string or nil.
fn value_to_string(v: Value) -> Option<String> {
    match v {
        Value::Null        => None,
        Value::Integer(n)  => Some(n.to_string()),
        Value::Real(f)     => Some(f.to_string()),
        Value::Text(s)     => Some(s),
        Value::Blob(_)     => Some("<blob>".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Lua script functions
// ---------------------------------------------------------------------------

pub unsafe extern "fastcall" fn script_hdb_open(_l: LuaState) -> u32 {
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

    let full_path = resolve_db_path(&filename);

    let db = match Connection::open(&full_path) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("HDB_Open: {}", e);
            lua::lua_error(l, &msg);
            return 0;
        }
    };

    if let Err(e) = db.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA temp_store=MEMORY;
         PRAGMA busy_timeout=5000;",
    ) {
        let msg = format!("HDB_Open: could not configure database: {}", e);
        lua::lua_error(l, &msg);
        return 0;
    }

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

pub unsafe extern "fastcall" fn script_hdb_close(_l: LuaState) -> u32 {
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

pub unsafe extern "fastcall" fn script_hdb_execute(_l: LuaState) -> u32 {
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

pub unsafe extern "fastcall" fn script_hdb_query(_l: LuaState) -> u32 {
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
                    let val = value_to_string(row.get::<_, Value>(i).unwrap_or(Value::Null));
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

pub unsafe extern "fastcall" fn script_hdb_query_raw(_l: LuaState) -> u32 {
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
                    r.push(value_to_string(row.get::<_, Value>(i).unwrap_or(Value::Null)));
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

pub unsafe extern "fastcall" fn script_hdb_open_addon(_l: LuaState) -> u32 {
    let l = lua::get_lua_state();

    if lua::lua_gettop(l) != 2 || !lua::lua_isstring(l, 1) || !lua::lua_isstring(l, 2) {
        lua::lua_error(l, "Usage: HDB_OpenAddon(addon_name, path)");
        return 0;
    }

    let addon_name = match lua::lua_tostring(l, 1) {
        Some(s) => s,
        None => {
            lua::lua_error(l, "HDB_OpenAddon: addon_name is nil");
            return 0;
        }
    };

    let path = match lua::lua_tostring(l, 2) {
        Some(s) => s,
        None => {
            lua::lua_error(l, "HDB_OpenAddon: path is nil");
            return 0;
        }
    };

    if !is_valid_filename(&addon_name) {
        lua::lua_error(
            l,
            "HDB_OpenAddon: invalid addon_name (must not contain < > : \" / \\ | ? *)",
        );
        return 0;
    }

    if !is_valid_path(&path) {
        lua::lua_error(
            l,
            "HDB_OpenAddon: invalid path (components must not be empty, ., or .., \
             or contain < > : \" / \\ | ? *)",
        );
        return 0;
    }

    let full_path = resolve_addon_db_path(&addon_name, &path);

    let db = match Connection::open_with_flags(
        &full_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("HDB_OpenAddon: {}", e);
            lua::lua_error(l, &msg);
            return 0;
        }
    };

    if let Err(e) = db.execute_batch(
        "PRAGMA temp_store=MEMORY;
         PRAGMA busy_timeout=5000;",
    ) {
        let msg = format!("HDB_OpenAddon: could not configure database: {}", e);
        lua::lua_error(l, &msg);
        return 0;
    }

    match alloc_handle(db) {
        Some(h) => {
            lua::lua_pushnumber(l, h as f64);
            1
        }
        None => {
            lua::lua_error(l, "HDB_OpenAddon: too many open databases (max 32)");
            0
        }
    }
}

pub unsafe extern "fastcall" fn script_hdb_get_version(_l: LuaState) -> u32 {
    let l = lua::get_lua_state();
    lua::lua_pushnumber(l, VERSION_MAJOR as f64);
    lua::lua_pushnumber(l, VERSION_MINOR as f64);
    lua::lua_pushnumber(l, VERSION_PATCH as f64);
    3
}

// ---------------------------------------------------------------------------
// Async Lua script functions
// ---------------------------------------------------------------------------

pub unsafe extern "fastcall" fn script_hdb_execute_async(_l: LuaState) -> u32 {
    use crate::async_worker;

    let l = lua::get_lua_state();

    if lua::lua_gettop(l) != 2 || !lua::lua_isnumber(l, 1) || !lua::lua_isstring(l, 2) {
        lua::lua_error(l, "Usage: HDB_ExecuteAsync(handle, sql)");
        return 0;
    }

    let h = lua::lua_tonumber(l, 1) as usize;
    let sql = match lua::lua_tostring(l, 2) {
        Some(s) => s,
        None => {
            lua::lua_error(l, "HDB_ExecuteAsync: sql is nil");
            return 0;
        }
    };

    let conn = match clone_handle(h) {
        Some(c) => c,
        None => {
            lua::lua_error(l, "HDB_ExecuteAsync: invalid handle");
            return 0;
        }
    };

    match async_worker::submit(h, conn, async_worker::AsyncOp::Execute(sql)) {
        Ok(ticket) => {
            lua::lua_pushnumber(l, ticket as f64);
            1
        }
        Err(msg) => {
            lua::lua_error(l, &format!("HDB_ExecuteAsync: {}", msg));
            0
        }
    }
}

pub unsafe extern "fastcall" fn script_hdb_query_async(_l: LuaState) -> u32 {
    use crate::async_worker;

    let l = lua::get_lua_state();

    if lua::lua_gettop(l) != 2 || !lua::lua_isnumber(l, 1) || !lua::lua_isstring(l, 2) {
        lua::lua_error(l, "Usage: HDB_QueryAsync(handle, sql)");
        return 0;
    }

    let h = lua::lua_tonumber(l, 1) as usize;
    let sql = match lua::lua_tostring(l, 2) {
        Some(s) => s,
        None => {
            lua::lua_error(l, "HDB_QueryAsync: sql is nil");
            return 0;
        }
    };

    let conn = match clone_handle(h) {
        Some(c) => c,
        None => {
            lua::lua_error(l, "HDB_QueryAsync: invalid handle");
            return 0;
        }
    };

    match async_worker::submit(h, conn, async_worker::AsyncOp::Query(sql)) {
        Ok(ticket) => {
            lua::lua_pushnumber(l, ticket as f64);
            1
        }
        Err(msg) => {
            lua::lua_error(l, &format!("HDB_QueryAsync: {}", msg));
            0
        }
    }
}

pub unsafe extern "fastcall" fn script_hdb_query_raw_async(_l: LuaState) -> u32 {
    use crate::async_worker;

    let l = lua::get_lua_state();

    if lua::lua_gettop(l) != 2 || !lua::lua_isnumber(l, 1) || !lua::lua_isstring(l, 2) {
        lua::lua_error(l, "Usage: HDB_QueryRawAsync(handle, sql)");
        return 0;
    }

    let h = lua::lua_tonumber(l, 1) as usize;
    let sql = match lua::lua_tostring(l, 2) {
        Some(s) => s,
        None => {
            lua::lua_error(l, "HDB_QueryRawAsync: sql is nil");
            return 0;
        }
    };

    let conn = match clone_handle(h) {
        Some(c) => c,
        None => {
            lua::lua_error(l, "HDB_QueryRawAsync: invalid handle");
            return 0;
        }
    };

    match async_worker::submit(h, conn, async_worker::AsyncOp::QueryRaw(sql)) {
        Ok(ticket) => {
            lua::lua_pushnumber(l, ticket as f64);
            1
        }
        Err(msg) => {
            lua::lua_error(l, &format!("HDB_QueryRawAsync: {}", msg));
            0
        }
    }
}

pub unsafe extern "fastcall" fn script_hdb_get_result(_l: LuaState) -> u32 {
    use crate::async_worker::{self, AsyncResult};

    let l = lua::get_lua_state();

    if lua::lua_gettop(l) != 1 || !lua::lua_isnumber(l, 1) {
        lua::lua_error(l, "Usage: HDB_GetResult(ticket)");
        return 0;
    }

    let ticket = lua::lua_tonumber(l, 1) as u64;

    match async_worker::get_result(ticket) {
        None => {
            // Still pending: return nil (1 value)
            lua::lua_pushnil(l);
            1
        }
        Some(AsyncResult::Execute { rows_affected }) => {
            lua::lua_pushnumber(l, rows_affected as f64);
            1
        }
        Some(AsyncResult::Query { rows }) => {
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
            1
        }
        Some(AsyncResult::QueryRaw { cols, rows }) => {
            lua::lua_newtable(l);
            for (i, name) in cols.iter().enumerate() {
                lua::lua_pushstring(l, name);
                lua::lua_rawseti(l, -2, (i + 1) as i32);
            }

            lua::lua_newtable(l);
            for (row_idx, row) in rows.iter().enumerate() {
                lua::lua_newtable(l);
                for (col_idx, val) in row.iter().enumerate() {
                    match val {
                        Some(s) => lua::lua_pushstring(l, s),
                        None    => lua::lua_pushnil(l),
                    }
                    lua::lua_rawseti(l, -2, (col_idx + 1) as i32);
                }
                lua::lua_rawseti(l, -2, (row_idx + 1) as i32);
            }
            2
        }
        Some(AsyncResult::Error { message }) => {
            // Error: return nil, error_message (2 values)
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &message);
            2
        }
    }
}
