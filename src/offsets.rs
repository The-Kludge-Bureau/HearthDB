//! Hard-coded offsets for the TurtleWoW 1.12.1 (build 5875) client.
//! All values are absolute virtual addresses in the 32-bit process space.

pub const PLAYER_LOAD_SCRIPT_FUNCTIONS: usize   = 0x0049_0250;
pub const GLUE_LOAD_SCRIPT_FUNCTIONS: usize     = 0x0046_ABB0;
pub const FRAME_SCRIPT_REGISTER_FUNCTION: usize = 0x0070_4120;

/// Returns the current lua_State* via __fastcall with no arguments.
pub const LUA_STATE_PTR: usize = 0x0070_40D0;

pub const LUA_ERROR:       usize = 0x006F_4940;
pub const LUA_GETTOP:      usize = 0x006F_3070;
pub const LUA_SETTOP:      usize = 0x006F_3080;
pub const LUA_ISSTRING:    usize = 0x006F_3510;
pub const LUA_ISNUMBER:    usize = 0x006F_34D0;
pub const LUA_TOSTRING:    usize = 0x006F_3690;
pub const LUA_TONUMBER:    usize = 0x006F_3620;
pub const LUA_PUSHNUMBER:  usize = 0x006F_3810;
pub const LUA_PUSHSTRING:  usize = 0x006F_3890;
pub const LUA_PUSHNIL:     usize = 0x006F_37F0;
pub const LUA_PUSHBOOLEAN: usize = 0x006F_39F0;
pub const LUA_NEWTABLE:    usize = 0x006F_3C90;
pub const LUA_SETTABLE:    usize = 0x006F_3E20;
pub const LUA_RAWSETI:     usize = 0x006F_3F60;
pub const LUA_TYPE:        usize = 0x006F_3400;
pub const LUA_PCALL:       usize = 0x006F_41A0;
pub const LUA_RAWGETI:     usize = 0x006F_3BC0;
pub const LUAL_REF:        usize = 0x006F_5310;
pub const LUAL_UNREF:      usize = 0x006F_5400;

/// WoW-specific function that compiles and executes a Lua string.
/// NOT the standard Lua C API lua_call.
/// Signature: void __fastcall(const char *code, const char *source)
pub const LUA_CALL:        usize = 0x0070_4CD0;
