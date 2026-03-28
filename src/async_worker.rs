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
