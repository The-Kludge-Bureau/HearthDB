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

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

static NEXT_TICKET: AtomicU64 = AtomicU64::new(1);
static SENDER: OnceLock<mpsc::Sender<WorkItem>> = OnceLock::new();
static RESULTS: LazyLock<Mutex<HashMap<u64, AsyncResult>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static POISONED: LazyLock<Mutex<HashSet<usize>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

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
