# HearthDB

A TurtleWoW DLL plugin that gives addon and mod authors access to SQLite
databases from Lua. It exposes a set of `HDB_*` globals for opening,
querying, and closing databases, with both synchronous and asynchronous
variants. It follows the same loader convention as
[nampower](https://gitea.com/avitasia/nampower): the DLL exports a `Load()`
function called by the TurtleWoW plugin loader on startup.

Two storage locations are supported:

- **`CustomData/`** — a writable directory next to the WoW executable, suitable
  for addon-generated data, player notes, saved state, etc.
- **`Interface/AddOns/<addon>/`** — read-only access to SQLite files an addon
  ships alongside its Lua code, suitable for static data tables (items, quests,
  spells, NPC dialogue, etc.).

Up to 32 database handles may be open at the same time.

## Lua API

### Synchronous functions

| Function | Signature | Description |
|---|---|---|
| `HDB_Open` | `HDB_Open(filename) -> handle` | Opens or creates `CustomData/<filename>`. Returns an integer handle on success, raises a Lua error on failure. |
| `HDB_OpenAddon` | `HDB_OpenAddon(addon_name, path) -> handle` | Opens `Interface/AddOns/<addon_name>/<path>` **read-only**. `path` may be a plain filename (`data.db`) or a relative path (`subdir/data.db`). The handle is compatible with all query and close functions. `HDB_Execute` on a read-only handle raises a Lua error. |
| `HDB_Close` | `HDB_Close(handle)` | Closes the database, cancels any pending async work for the handle, and frees the handle slot. Always close handles you no longer need. |
| `HDB_Execute` | `HDB_Execute(handle, sql)` | Executes one or more SQL statements that produce no rows (INSERT, UPDATE, DELETE, CREATE TABLE, etc.). Multiple statements may be separated by semicolons. Raises a Lua error on failure. |
| `HDB_Query` | `HDB_Query(handle, sql) -> rows` | Executes a SELECT and returns an array of row tables keyed by column name: `{ {col=val, ...}, ... }`. |
| `HDB_QueryRaw` | `HDB_QueryRaw(handle, sql) -> cols, rows` | Executes a SELECT and returns two values: a column-name array `{"col1", "col2", ...}` and a positional row array `{ {v1, v2, ...}, ... }`. Slightly more efficient than `HDB_Query` for large result sets. |
| `HDB_GetVersion` | `HDB_GetVersion() -> major, minor, patch` | Returns the HearthDB version as three integers. |

### Asynchronous functions

These functions offload SQL execution to a background worker thread, allowing
the game to continue rendering while the database works. Submit an operation,
receive a ticket number, and poll for the result across frames.

| Function | Signature | Description |
|---|---|---|
| `HDB_ExecuteAsync` | `HDB_ExecuteAsync(handle, sql [, callback]) -> ticket` | Submits one or more non-SELECT statements for background execution. Returns a ticket number. If `callback` is provided, it is called with `(rows_affected, err)` when the operation completes. Raises a Lua error if the handle is invalid or poisoned. |
| `HDB_QueryAsync` | `HDB_QueryAsync(handle, sql [, callback]) -> ticket` | Submits a SELECT for background execution. If `callback` is provided, it is called with `(rows, err)` matching the `HDB_Query` result structure. |
| `HDB_QueryRawAsync` | `HDB_QueryRawAsync(handle, sql [, callback]) -> ticket` | Submits a SELECT for background execution. If `callback` is provided, it is called with `(cols, rows, err)` matching the `HDB_QueryRaw` result structure. |
| `HDB_GetResult` | `HDB_GetResult(ticket) -> ...` | Polls for a completed async result. Returns `nil` if still pending. For execute results, returns `rows_affected`. For query results, returns the same structure as the sync equivalent. For errors, returns `nil, error_message`. Results are one-shot: a second call with the same ticket returns `nil`. |
| `HDB_ClearPoison` | `HDB_ClearPoison(handle)` | Clears the poison state on a handle after an async error has been retrieved. Without this, the handle cannot be used for further async operations. |

### Notes

**Values are always strings.** All column values — including integers and
floating-point numbers — are returned as Lua strings. `NULL` becomes `nil`.
Blob columns return the literal string `"<blob>"`.

**Handle lifecycle.** Every successful `HDB_Open` / `HDB_OpenAddon` call
allocates a handle slot. You must call `HDB_Close` when you are done. Failing
to close handles will eventually exhaust the 32-slot limit. On a UI reload,
all open handles are automatically closed and async state is reset, so addons
do not need to worry about stale handles leaking across `/reload`.

**Lua 5.0 compatibility.** WoW 1.12.1 uses Lua 5.0, which does not have the
`#` length operator. Use `table.getn(t)` to get the number of rows returned
by `HDB_Query` and `HDB_QueryRaw`.

**Async workflow.** Call one of the `*Async` functions to submit work. Each
returns a ticket number immediately. On subsequent frames, call
`HDB_GetResult(ticket)` until it returns a non-nil value. Results are
one-shot: once retrieved, the ticket is consumed. You may have multiple
tickets in flight at the same time; they are processed in submission order by
a single background worker thread.

**Poison.** If an async operation fails (bad SQL, read-only violation, etc.),
the handle enters a *poisoned* state. While poisoned, all subsequent async
submissions on that handle are rejected and any already-queued work for it is
cancelled. To recover, retrieve the error result with `HDB_GetResult`, then
call `HDB_ClearPoison(handle)`. Synchronous functions (`HDB_Execute`,
`HDB_Query`, `HDB_QueryRaw`) are not affected by poison.

**Callbacks.** Pass a function as the optional third argument to any
`*Async` function to receive results automatically instead of polling.
The callback fires on the next frame after the operation completes.
Callback signatures mirror the sync return values with an added `err`
parameter: `function(rows_affected, err)` for execute,
`function(rows, err)` for query, and `function(cols, rows, err)` for
query raw. On success `err` is `nil`; on failure the result is `nil` and
`err` is a string. The ticket is still returned when a callback is
provided, but `HDB_GetResult` will return `nil` for that ticket since
the callback consumes the result. Errors inside callbacks are caught and
logged to `Logs/hdb_pump_errors.log` without affecting other callbacks.

**Atomicity and transactions.** Each SQL statement passed to `HDB_Execute`
that is not inside an explicit `BEGIN`/`COMMIT` block runs in its own
implicit transaction. This means a multi-statement call is **not** atomic by
default: if the game crashes between two statements, the first change is
saved and the second is not, leaving your data in a partially-updated state.
WAL mode keeps the database file itself uncorrupted, but it cannot protect
logical consistency across statements that were never grouped into a single
transaction.

Wrap any group of writes that must succeed or fail together in an explicit
transaction:

```lua
-- Without a transaction: two separate commits.
-- A crash between them leaves items updated but inventory untouched.
HDB_Execute(db, [[
    UPDATE items    SET count = count - 1 WHERE id = 42;
    UPDATE inventory SET gold  = gold  - 100;
]])

-- With a transaction: one atomic commit.
-- A crash at any point leaves both tables unchanged.
HDB_Execute(db, [[
    BEGIN;
    UPDATE items    SET count = count - 1 WHERE id = 42;
    UPDATE inventory SET gold  = gold  - 100;
    COMMIT;
]])
```

Single-statement writes are always atomic and do not need a transaction.

**SQLite configuration.** HearthDB applies the following PRAGMAs automatically
when a database is opened so you do not need to set them yourself:

| PRAGMA | Value | Applied to | Reason |
|---|---|---|---|
| `journal_mode` | `WAL` | `HDB_Open` | Write-Ahead Logging keeps the main database file intact on a hard crash. Writes go to a sidecar `.db-wal` file and are checkpointed later, so an abrupt game exit can never corrupt the database. |
| `synchronous` | `NORMAL` | `HDB_Open` | With WAL mode, `NORMAL` fsyncs at checkpoints rather than after every write. This is crash-safe against application crashes and significantly faster than the default `FULL`. |
| `foreign_keys` | `ON` | `HDB_Open` | Enforces `REFERENCES` constraints in your schema. Off by default in SQLite for legacy reasons; turning it on means referential integrity errors are caught rather than silently ignored. |
| `temp_store` | `MEMORY` | both | Sorts and temporary tables are kept in memory instead of written to temp files. Faster queries with no meaningful downside for addon-sized databases. |
| `busy_timeout` | `5000` | both | If two handles try to write the same database simultaneously, SQLite retries for up to 5 seconds before raising an error, preventing spurious `SQLITE_BUSY` failures. |

The `.db-wal` and `.db-shm` sidecar files that appear alongside WAL-mode
databases in `CustomData/` are normal. Do not delete them while the game
is running.

## Examples

### Detecting HearthDB

Check for the presence of `HDB_GetVersion` before calling any `HDB_*`
function. This lets your addon degrade gracefully when HearthDB is not
installed.

```lua
if not HDB_GetVersion then
    DEFAULT_CHAT_FRAME:AddMessage("MyAddon requires HearthDB.")
    return
end

local major, minor, patch = HDB_GetVersion()
-- e.g. 0, 4, 0
```

### Persistent addon storage (read/write)

Use `HDB_Open` to store data that should survive across sessions — player
notes, configuration, statistics, etc.

```lua
local db

local function MyAddon_InitDB()
    local ok, result = pcall(HDB_Open, "MyAddon.db")
    if not ok then
        DEFAULT_CHAT_FRAME:AddMessage("MyAddon: " .. result)
        return
    end
    db = result
    HDB_Execute(db, [[
        CREATE TABLE IF NOT EXISTS notes (
            target TEXT PRIMARY KEY,
            body   TEXT NOT NULL
        )
    ]])
end

local function MyAddon_GetNote(target)
    local rows = HDB_Query(db, "SELECT body FROM notes WHERE target='" .. target .. "'")
    if table.getn(rows) > 0 then
        return rows[1].body
    end
    return nil
end

local function MyAddon_SetNote(target, body)
    HDB_Execute(db,
        "INSERT OR REPLACE INTO notes VALUES ('" .. target .. "', '" .. body .. "')")
end

local function MyAddon_DeleteNote(target)
    HDB_Execute(db, "DELETE FROM notes WHERE target='" .. target .. "'")
end

-- Initialise on login
local frame = CreateFrame("Frame", "MyAddonFrame")
frame:RegisterEvent("PLAYER_LOGIN")
frame:SetScript("OnEvent", MyAddon_InitDB)
```

### Bundled static data (read-only)

Ship a pre-built SQLite file with your addon and open it read-only at
runtime. This is ideal for large look-up tables that you populate offline —
no need to seed anything from Lua.

```lua
-- Interface/AddOns/MyAddon/data/quests.db contains a `quests` table.

local questDb

local function MyAddon_GetQuestDb()
    if questDb then return questDb end
    local ok, result = pcall(HDB_OpenAddon, "MyAddon", "data/quests.db")
    if not ok then
        DEFAULT_CHAT_FRAME:AddMessage("MyAddon: " .. result)
        return nil
    end
    questDb = result
    return questDb
end

local function MyAddon_GetQuest(questId)
    local h = MyAddon_GetQuestDb()
    if not h then return nil end
    local rows = HDB_Query(h, "SELECT * FROM quests WHERE id=" .. questId)
    if table.getn(rows) > 0 then
        return rows[1]  -- { id="42", title="...", zone="...", ... }
    end
    return nil
end
```

### HDB_QueryRaw for large result sets

`HDB_QueryRaw` returns column names and positional row arrays separately.
Access values by index (`row[1]`, `row[2]`, …) instead of by name, which
avoids building a keyed table for every row.

```lua
local h = HDB_OpenAddon("MyAddon", "data/items.db")

local cols, rows = HDB_QueryRaw(h, "SELECT id, name, quality FROM items ORDER BY name")
-- cols = { "id", "name", "quality" }

for i = 1, table.getn(rows) do
    local id, name, quality = rows[i][1], rows[i][2], rows[i][3]
    DEFAULT_CHAT_FRAME:AddMessage(name .. " (quality " .. quality .. ")")
end

HDB_Close(h)
```

### Async writes with callback

Use `HDB_ExecuteAsync` with a callback when you need to write data
without freezing the game and without polling.

```lua
local db = HDB_Open("MyAddon.db")
HDB_Execute(db, "CREATE TABLE IF NOT EXISTS log (ts TEXT, msg TEXT)")

local function MyAddon_LogAsync(message)
    HDB_ExecuteAsync(db,
        "INSERT INTO log VALUES (datetime('now'), '" .. message .. "')",
        function(rows_affected, err)
            if err then
                DEFAULT_CHAT_FRAME:AddMessage("Write failed: " .. err)
                HDB_ClearPoison(db)
                return
            end
            DEFAULT_CHAT_FRAME:AddMessage("Logged (" .. rows_affected .. " row)")
        end)
end
```

### Async queries with callback

```lua
local questDb = HDB_OpenAddon("MyAddon", "data/quests.db")

local function MyAddon_SearchQuestsAsync(zone)
    HDB_QueryAsync(questDb,
        "SELECT id, title FROM quests WHERE zone='" .. zone .. "'",
        function(rows, err)
            if err then
                DEFAULT_CHAT_FRAME:AddMessage("Query failed: " .. err)
                HDB_ClearPoison(questDb)
                return
            end
            for i = 1, table.getn(rows) do
                DEFAULT_CHAT_FRAME:AddMessage(rows[i].id .. ": " .. rows[i].title)
            end
        end)
end
```

### Async polling (without callbacks)

If you prefer manual control, omit the callback and poll with
`HDB_GetResult` from an OnUpdate handler.

```lua
local db = HDB_Open("MyAddon.db")
HDB_Execute(db, "CREATE TABLE IF NOT EXISTS log (ts TEXT, msg TEXT)")

local pendingTicket

local function MyAddon_LogAsync(message)
    pendingTicket = HDB_ExecuteAsync(db,
        "INSERT INTO log VALUES (datetime('now'), '" .. message .. "')")
end

local pollFrame = CreateFrame("Frame", "MyAddonPollFrame")
pollFrame:SetScript("OnUpdate", function()
    if not pendingTicket then return end
    local result, err = HDB_GetResult(pendingTicket)
    if result == nil and err == nil then return end  -- still pending
    if err then
        DEFAULT_CHAT_FRAME:AddMessage("Write failed: " .. err)
        HDB_ClearPoison(db)
    end
    pendingTicket = nil
end)
```

## Installation

1. Build `HearthDB.dll` (see below) or obtain a pre-built binary.
2. Copy `HearthDB.dll` to your TurtleWoW plugin directory alongside
   `WoW.exe`.
3. The plugin loader calls `Load()` on startup, which activates the Lua
   function hooks.

## Building on Linux (cross-compile)

### Prerequisites

1. Install the Rust toolchain:
   ```
   https://rustup.rs
   ```

2. Add the 32-bit Windows target:
   ```bash
   rustup target add i686-pc-windows-msvc
   ```

3. Install `clang-cl` (C compiler) and `lld-link` (linker):
   ```bash
   # Arch / CachyOS
   sudo pacman -S clang lld

   # Debian / Ubuntu
   sudo apt install clang lld
   ```

4. Install `xwin` to obtain the MSVC sysroot:
   ```bash
   cargo install xwin
   ```

5. Populate the sysroot with the 32-bit (x86) libraries. Run from the
   repository root:
   ```bash
   xwin --accept-license --arch x86 splat --include-debug-libs --output xwinSDK
   ```
   If `xwin` is not in your `PATH`:
   ```bash
   ~/.local/share/cargo/bin/xwin --accept-license --arch x86 splat --include-debug-libs --output xwinSDK
   ```
   The `--arch x86` flag is required; the default download is x86_64 only.

### Build

Use `make` rather than invoking `cargo` directly. The Makefile sets the
required `CC` and `CFLAGS` environment variables with absolute paths so
that the C compiler (used by `rusqlite`) can locate the xwinSDK headers
regardless of its working directory.

```bash
make          # release build (default)
make debug    # debug build
```

Output: `target/i686-pc-windows-msvc/release/HearthDB.dll`

## Building on Windows

Install the Rust toolchain and add the 32-bit Windows target:

```cmd
rustup target add i686-pc-windows-msvc
```

With a 32-bit MSVC toolchain available (Visual Studio or the standalone
Build Tools), no further configuration is needed:

```cmd
cargo build --release --target i686-pc-windows-msvc
```
