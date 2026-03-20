# HearthDB

A TurtleWoW DLL plugin that gives addon and mod authors access to SQLite
databases from Lua. It exposes a small set of `HDB_*` globals for opening,
querying, and closing databases, and follows the same loader convention as
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

### Functions

| Function | Signature | Description |
|---|---|---|
| `HDB_Open` | `HDB_Open(filename) -> handle` | Opens or creates `CustomData/<filename>`. Returns an integer handle on success, raises a Lua error on failure. |
| `HDB_Close` | `HDB_Close(handle)` | Closes the database and frees the handle slot. Always close handles you no longer need. |
| `HDB_Execute` | `HDB_Execute(handle, sql)` | Executes one or more SQL statements that produce no rows (INSERT, UPDATE, DELETE, CREATE TABLE, etc.). Multiple statements may be separated by semicolons. Raises a Lua error on failure. |
| `HDB_Query` | `HDB_Query(handle, sql) -> rows` | Executes a SELECT and returns an array of row tables keyed by column name: `{ {col=val, ...}, ... }`. |
| `HDB_QueryRaw` | `HDB_QueryRaw(handle, sql) -> cols, rows` | Executes a SELECT and returns two values: a column-name array `{"col1", "col2", ...}` and a positional row array `{ {v1, v2, ...}, ... }`. Slightly more efficient than `HDB_Query` for large result sets. |
| `HDB_OpenAddon` | `HDB_OpenAddon(addon_name, path) -> handle` | Opens `Interface/AddOns/<addon_name>/<path>` **read-only**. `path` may be a plain filename (`data.db`) or a relative path (`subdir/data.db`). The handle is compatible with `HDB_Query`, `HDB_QueryRaw`, and `HDB_Close`. `HDB_Execute` on a read-only handle raises a Lua error. |
| `HDB_GetVersion` | `HDB_GetVersion() -> major, minor, patch` | Returns the HearthDB version as three integers. |

### Notes

**Values are always strings.** All column values — including integers and
floating-point numbers — are returned as Lua strings. `NULL` becomes `nil`.
Blob columns return the literal string `"<blob>"`.

**Handle lifecycle.** Every successful `HDB_Open` / `HDB_OpenAddon` call
allocates a handle slot. You must call `HDB_Close` when you are done. Failing
to close handles will eventually exhaust the 32-slot limit.

**Lua 5.0 compatibility.** WoW 1.12.1 uses Lua 5.0, which does not have the
`#` length operator. Use `table.getn(t)` to get the number of rows returned
by `HDB_Query` and `HDB_QueryRaw`.

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
-- e.g. 0, 1, 0
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
local frame = CreateFrame("Frame")
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
