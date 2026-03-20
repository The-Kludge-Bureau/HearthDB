# HearthDB

A TurtleWoW DLL plugin that exposes Lua functions for reading from and writing
to SQLite databases stored in the `CustomData` directory. It follows the same
loader convention as nampower: the DLL exports a `Load()` function called by
the TurtleWoW plugin loader on startup.

## Lua API

| Function | Signature | Description |
|---|---|---|
| `HDB_Open` | `HDB_Open(filename) -> handle` | Opens or creates `CustomData/<filename>`. Returns an integer handle on success, raises a Lua error on failure. |
| `HDB_Close` | `HDB_Close(handle)` | Closes the database and frees the handle slot. |
| `HDB_Execute` | `HDB_Execute(handle, sql)` | Executes a SQL statement that produces no rows (INSERT, UPDATE, CREATE, DELETE, etc.). Raises a Lua error on failure. |
| `HDB_Query` | `HDB_Query(handle, sql) -> table` | Executes a query and returns an array of row tables keyed by column name: `{{col=val, ...}, ...}`. |
| `HDB_QueryRaw` | `HDB_QueryRaw(handle, sql) -> cols, rows` | Executes a query and returns two values: a column-name array `{"col1", "col2", ...}` and a positional row array `{{v1, v2, ...}, ...}`. |

Up to 32 databases may be open simultaneously.

## Building on Linux (cross-compile)

### Prerequisites

1. Install the Rust toolchain if you have not already:
   ```
   https://rustup.rs
   ```

2. Add the Windows target:
   ```bash
   rustup target add i686-pc-windows-msvc
   ```

3. Install `xwin` to obtain the MSVC sysroot:
   ```bash
   cargo install xwin
   ```

4. Populate the sysroot (run from the repository root):
   ```bash
   xwin --accept-license splat --include-debug-libs --output xwinSDK
   ```
   If `xwin` is not in your `PATH`, use the full path:
   ```bash
   ~/.local/share/cargo/bin/xwin --accept-license splat --include-debug-libs --output xwinSDK
   ```

5. Ensure `lld-link` is available. It is provided by the `lld` or `llvm`
   package on most distributions:
   ```bash
   # Arch / CachyOS
   sudo pacman -S lld

   # Debian / Ubuntu
   sudo apt install lld
   ```

### Build

```bash
cargo build --release --target i686-pc-windows-msvc
```

Output: `target/i686-pc-windows-msvc/release/HearthDB.dll`

## Building on Windows

With a 32-bit MSVC toolchain installed, no additional configuration is needed:

```cmd
cargo build --release
```

Or explicitly:

```cmd
cargo build --release --target i686-pc-windows-msvc
```

## Installation

Copy `HearthDB.dll` to your TurtleWoW plugin directory alongside `nampower.dll`
and other plugins.
