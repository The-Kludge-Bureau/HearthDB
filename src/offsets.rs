//! Hard-coded offsets for the TurtleWoW 1.12.1 (build 5875) client.
//! All values are absolute virtual addresses in the 32-bit process space.

#![allow(dead_code)]

// ---------------------------------------------------------------------------
// WoW engine hooks
// ---------------------------------------------------------------------------

pub const PLAYER_LOAD_SCRIPT_FUNCTIONS: usize   = 0x0049_0250;
pub const GLUE_LOAD_SCRIPT_FUNCTIONS: usize     = 0x0046_ABB0;
pub const FRAME_SCRIPT_REGISTER_FUNCTION: usize = 0x0070_4120;

/// WoW-specific function that compiles and executes a Lua string.
/// NOT the standard Lua C API lua_call.
/// Signature: void __fastcall(const char *code, const char *source)
pub const LUA_CALL: usize = 0x0070_4CD0;

// ---------------------------------------------------------------------------
// Lua state
// ---------------------------------------------------------------------------

/// Returns the current lua_State* via __fastcall with no arguments.
pub const LUA_STATE_PTR: usize = 0x0070_40D0;

// ---------------------------------------------------------------------------
// Lua stack manipulation
// ---------------------------------------------------------------------------

pub const LUA_GETTOP:    usize = 0x006F_3070;
pub const LUA_SETTOP:    usize = 0x006F_3080;
pub const LUA_PUSHVALUE: usize = 0x006F_3350;
pub const LUA_REMOVE:    usize = 0x006F_30D0;
pub const LUA_INSERT:    usize = 0x006F_31A0;

// ---------------------------------------------------------------------------
// Lua type checks
// ---------------------------------------------------------------------------

pub const LUA_TYPE:      usize = 0x006F_3400;
pub const LUA_TYPENAME:  usize = 0x006F_3480;
pub const LUA_ISNUMBER:  usize = 0x006F_34D0;
pub const LUA_ISSTRING:  usize = 0x006F_3510;

// ---------------------------------------------------------------------------
// Lua value access (stack -> Rust)
// ---------------------------------------------------------------------------

pub const LUA_TONUMBER:   usize = 0x006F_3620;
pub const LUA_TOBOOLEAN:  usize = 0x006F_3660;
pub const LUA_TOSTRING:   usize = 0x006F_3690;
pub const LUA_TOPOINTER:  usize = 0x006F_3790;

// ---------------------------------------------------------------------------
// Lua value push (Rust -> stack)
// ---------------------------------------------------------------------------

pub const LUA_PUSHNIL:     usize = 0x006F_37F0;
pub const LUA_PUSHNUMBER:  usize = 0x006F_3810;
pub const LUA_PUSHSTRING:  usize = 0x006F_3890;
pub const LUA_PUSHBOOLEAN: usize = 0x006F_39F0;

// ---------------------------------------------------------------------------
// Lua table operations
// ---------------------------------------------------------------------------

pub const LUA_GETTABLE: usize = 0x006F_3A40;
pub const LUA_RAWGETI:  usize = 0x006F_3BC0;
pub const LUA_NEWTABLE: usize = 0x006F_3C90;
pub const LUA_GETFENV:  usize = 0x006F_3D50;
pub const LUA_SETTABLE: usize = 0x006F_3E20;
pub const LUA_RAWSETI:  usize = 0x006F_3F60;
pub const LUA_SETFENV:  usize = 0x006F_40D0;
pub const LUA_NEXT:     usize = 0x006F_4450;

// ---------------------------------------------------------------------------
// Lua function calls and errors
// ---------------------------------------------------------------------------

pub const LUA_PCALL: usize = 0x006F_41A0;
pub const LUA_ERROR: usize = 0x006F_4940;

// ---------------------------------------------------------------------------
// Lua upvalue and GC
// ---------------------------------------------------------------------------

pub const LUA_GETUPVALUE: usize = 0x006F_4660;
pub const LUA_SETUPVALUE: usize = 0x006F_47B0;
pub const LUA_ENABLEGC:   usize = 0x006F_43C0;

// ---------------------------------------------------------------------------
// Lua auxiliary library (luaL_*)
// ---------------------------------------------------------------------------

pub const LUAL_CHECKNUMBER: usize = 0x006F_4C80;
pub const LUAL_OPENLIB:     usize = 0x006F_4DC0;
pub const LUAL_REF:         usize = 0x006F_5310;
pub const LUAL_UNREF:       usize = 0x006F_5400;

// ---------------------------------------------------------------------------
// Lua debug API
// ---------------------------------------------------------------------------

pub const LUA_SETHOOK:   usize = 0x006F_BA40;
pub const LUA_GETSTACK:  usize = 0x006F_BAA0;
pub const LUA_GETLOCAL:  usize = 0x006F_BB20;
pub const LUA_SETLOCAL:  usize = 0x006F_BBB0;
pub const LUA_GETINFO:   usize = 0x006F_BC70;
