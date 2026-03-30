//! Background worker thread for async HDB_* operations.

use crate::db::{Handle, SendConnection};
use rusqlite::types::Value;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, LazyLock, Mutex, OnceLock};
use std::thread;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub struct WorkItem {
    pub ticket: u64,
    pub handle_id: usize,
    pub conn: Handle,
    pub operation: AsyncOp,
}

pub enum AsyncOp {
    Execute(String),
    Query(String),
    QueryRaw(String),
}

pub enum AsyncResult {
    Execute { rows_affected: usize },
    Query { rows: Vec<Vec<(String, Option<String>)>> },
    QueryRaw { cols: Vec<String>, rows: Vec<Vec<Option<String>>> },
    Error { message: String },
}

#[derive(Clone, Copy)]
pub enum CallbackType {
    Execute,
    Query,
    QueryRaw,
}

pub struct PendingCallback {
    pub lua_ref: i32,
    pub cb_type: CallbackType,
    pub handle_id: usize,
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

static NEXT_TICKET: AtomicU64 = AtomicU64::new(1);
static SENDER: OnceLock<mpsc::Sender<WorkItem>> = OnceLock::new();
static RESULTS: LazyLock<Mutex<HashMap<u64, AsyncResult>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static POISONED: LazyLock<Mutex<HashSet<usize>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static CALLBACKS: LazyLock<Mutex<HashMap<u64, PendingCallback>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// ---------------------------------------------------------------------------
// Query execution helpers
// ---------------------------------------------------------------------------

fn value_to_string(v: Value) -> Option<String> {
    match v {
        Value::Null       => None,
        Value::Integer(n) => Some(n.to_string()),
        Value::Real(f)    => Some(f.to_string()),
        Value::Text(s)    => Some(s),
        Value::Blob(_)    => Some("<blob>".to_string()),
    }
}

fn run_operation(conn: &SendConnection, op: &AsyncOp) -> AsyncResult {
    match op {
        AsyncOp::Execute(sql) => {
            match conn.0.execute_batch(sql) {
                Ok(()) => {
                    let changes = conn.0.changes();
                    AsyncResult::Execute { rows_affected: changes as usize }
                }
                Err(e) => AsyncResult::Error { message: e.to_string() },
            }
        }
        AsyncOp::Query(sql) => {
            match execute_query(&conn.0, sql) {
                Ok(rows) => AsyncResult::Query { rows },
                Err(e) => AsyncResult::Error { message: e.to_string() },
            }
        }
        AsyncOp::QueryRaw(sql) => {
            match execute_query_raw(&conn.0, sql) {
                Ok((cols, rows)) => AsyncResult::QueryRaw { cols, rows },
                Err(e) => AsyncResult::Error { message: e.to_string() },
            }
        }
    }
}

fn execute_query(
    db: &rusqlite::Connection,
    sql: &str,
) -> Result<Vec<Vec<(String, Option<String>)>>, rusqlite::Error> {
    let mut stmt = db.prepare(sql)?;
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let col_count = col_names.len();
    let mut rows = Vec::new();
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
}

fn execute_query_raw(
    db: &rusqlite::Connection,
    sql: &str,
) -> Result<(Vec<String>, Vec<Vec<Option<String>>>), rusqlite::Error> {
    let mut stmt = db.prepare(sql)?;
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let col_count = col_names.len();
    let mut rows = Vec::new();
    let mut query = stmt.query([])?;
    while let Some(row) = query.next()? {
        let mut r = Vec::with_capacity(col_count);
        for i in 0..col_count {
            r.push(value_to_string(row.get::<_, Value>(i).unwrap_or(Value::Null)));
        }
        rows.push(r);
    }
    Ok((col_names, rows))
}

// ---------------------------------------------------------------------------
// Worker thread
// ---------------------------------------------------------------------------

fn worker_loop(rx: mpsc::Receiver<WorkItem>) {
    for item in rx {
        // Check if this handle is poisoned
        {
            let poisoned = POISONED.lock().unwrap();
            if poisoned.contains(&item.handle_id) {
                let result = AsyncResult::Error {
                    message: format!(
                        "cancelled: prior operation on handle {} failed",
                        item.handle_id
                    ),
                };
                RESULTS.lock().unwrap().insert(item.ticket, result);
                continue;
            }
        }

        // Lock the per-connection mutex and run the operation
        let conn = item.conn.lock().unwrap();
        let result = run_operation(&conn, &item.operation);
        drop(conn);

        // On failure, poison the handle
        if matches!(result, AsyncResult::Error { .. }) {
            POISONED.lock().unwrap().insert(item.handle_id);
        }

        RESULTS.lock().unwrap().insert(item.ticket, result);
    }
}

fn ensure_worker() -> &'static mpsc::Sender<WorkItem> {
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("hdb-worker".into())
            .spawn(|| worker_loop(rx))
            .expect("failed to spawn HearthDB worker thread");
        tx
    })
}

// ---------------------------------------------------------------------------
// Public API (called from db.rs script functions)
// ---------------------------------------------------------------------------

/// Submit an async operation. Returns the ticket number.
/// Returns Err if the handle is poisoned (caller should raise a Lua error).
pub fn submit(handle_id: usize, conn: Handle, operation: AsyncOp) -> Result<u64, String> {
    {
        let poisoned = POISONED.lock().unwrap();
        if poisoned.contains(&handle_id) {
            return Err(format!(
                "handle {} has a pending failure -- call HDB_GetResult to retrieve it",
                handle_id
            ));
        }
    }

    let ticket = NEXT_TICKET.fetch_add(1, Ordering::Relaxed);
    let item = WorkItem {
        ticket,
        handle_id,
        conn,
        operation,
    };
    ensure_worker().send(item).expect("worker thread died");
    Ok(ticket)
}

/// Retrieve a completed result. Returns None if still pending.
/// One-shot: the result is removed from the map on retrieval.
pub fn get_result(ticket: u64) -> Option<AsyncResult> {
    RESULTS.lock().unwrap().remove(&ticket)
}

/// Clear poison for a specific handle (called after addon retrieves the failure).
pub fn clear_poison(handle_id: usize) {
    POISONED.lock().unwrap().remove(&handle_id);
}

/// Check if a handle is currently poisoned.
pub fn is_poisoned(handle_id: usize) -> bool {
    POISONED.lock().unwrap().contains(&handle_id)
}

/// Cancel all pending results for a handle (called by HDB_Close).
/// Inserts the handle into the poison set so the worker skips any
/// remaining queued work for this handle.
pub fn cancel_handle(handle_id: usize) {
    POISONED.lock().unwrap().insert(handle_id);
}

pub fn register_callback(ticket: u64, lua_ref: i32, cb_type: CallbackType, handle_id: usize) {
    CALLBACKS.lock().unwrap().insert(ticket, PendingCallback { lua_ref, cb_type, handle_id });
}

/// Returns all tickets that have both a completed result and a registered callback.
pub fn ready_callbacks() -> Vec<(u64, AsyncResult, PendingCallback)> {
    let mut results = RESULTS.lock().unwrap();
    let mut callbacks = CALLBACKS.lock().unwrap();
    let tickets: Vec<u64> = callbacks.keys()
        .filter(|t| results.contains_key(t))
        .copied()
        .collect();
    let mut ready = Vec::new();
    for ticket in tickets {
        if let (Some(result), Some(cb)) = (results.remove(&ticket), callbacks.remove(&ticket)) {
            ready.push((ticket, result, cb));
        }
    }
    ready
}

/// Remove all callbacks for a specific handle. Returns the Lua refs to release.
pub fn cancel_handle_callbacks(handle_id: usize) -> Vec<i32> {
    let mut callbacks = CALLBACKS.lock().unwrap();
    let mut refs = Vec::new();
    callbacks.retain(|_, cb| {
        if cb.handle_id == handle_id {
            refs.push(cb.lua_ref);
            false
        } else {
            true
        }
    });
    refs
}

/// Clear all completed results, poison state, and callbacks.
/// Called on UI reload to prevent stale data from leaking across sessions.
/// Returns the Lua refs from any stale callbacks so the caller can release them.
pub fn reset() -> Vec<i32> {
    RESULTS.lock().unwrap().clear();
    POISONED.lock().unwrap().clear();
    let stale_refs: Vec<i32> = CALLBACKS.lock().unwrap()
        .drain()
        .map(|(_, cb)| cb.lua_ref)
        .collect();
    stale_refs
}
