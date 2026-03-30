//! Thin unsafe wrappers for the Lua 5.0 API functions embedded in the
//! WoW 1.12.1 client. All functions are called through raw function
//! pointers resolved from the offsets in offsets.rs.
//!
//! The Lua state is represented as *mut usize to match the client's
//! non-standard embedding, consistent with how nampower handles it.

#![allow(dead_code)]

use crate::offsets;
use std::ffi::{CStr, CString};

pub type LuaState = *mut usize;

/// Returns the active Lua state from the game process.
pub unsafe fn get_lua_state() -> LuaState {
    let f: unsafe extern "fastcall" fn() -> LuaState =
        std::mem::transmute(offsets::LUA_STATE_PTR);
    f()
}

pub unsafe fn lua_error(l: LuaState, msg: &str) {
    let s = CString::new(msg).unwrap_or_default();
    let f: unsafe extern "cdecl" fn(LuaState, *const i8) =
        std::mem::transmute(offsets::LUA_ERROR);
    f(l, s.as_ptr());
}

pub unsafe fn lua_gettop(l: LuaState) -> i32 {
    let f: unsafe extern "fastcall" fn(LuaState) -> i32 =
        std::mem::transmute(offsets::LUA_GETTOP);
    f(l)
}

pub unsafe fn lua_isstring(l: LuaState, idx: i32) -> bool {
    let f: unsafe extern "fastcall" fn(LuaState, i32) -> bool =
        std::mem::transmute(offsets::LUA_ISSTRING);
    f(l, idx)
}

pub unsafe fn lua_isnumber(l: LuaState, idx: i32) -> bool {
    let f: unsafe extern "fastcall" fn(LuaState, i32) -> bool =
        std::mem::transmute(offsets::LUA_ISNUMBER);
    f(l, idx)
}

pub unsafe fn lua_tostring(l: LuaState, idx: i32) -> Option<String> {
    let f: unsafe extern "fastcall" fn(LuaState, i32) -> *const i8 =
        std::mem::transmute(offsets::LUA_TOSTRING);
    let ptr = f(l, idx);
    if ptr.is_null() {
        None
    } else {
        Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}

pub unsafe fn lua_tonumber(l: LuaState, idx: i32) -> f64 {
    let f: unsafe extern "fastcall" fn(LuaState, i32) -> f64 =
        std::mem::transmute(offsets::LUA_TONUMBER);
    f(l, idx)
}

pub unsafe fn lua_pushnumber(l: LuaState, n: f64) {
    let f: unsafe extern "fastcall" fn(LuaState, f64) =
        std::mem::transmute(offsets::LUA_PUSHNUMBER);
    f(l, n);
}

pub unsafe fn lua_pushstring(l: LuaState, s: &str) {
    let cs = CString::new(s).unwrap_or_default();
    let f: unsafe extern "fastcall" fn(LuaState, *const i8) =
        std::mem::transmute(offsets::LUA_PUSHSTRING);
    f(l, cs.as_ptr());
}

pub unsafe fn lua_pushnil(l: LuaState) {
    let f: unsafe extern "fastcall" fn(LuaState) =
        std::mem::transmute(offsets::LUA_PUSHNIL);
    f(l);
}

pub unsafe fn lua_newtable(l: LuaState) {
    let f: unsafe extern "fastcall" fn(LuaState) =
        std::mem::transmute(offsets::LUA_NEWTABLE);
    f(l);
}

pub unsafe fn lua_settable(l: LuaState, idx: i32) {
    let f: unsafe extern "fastcall" fn(LuaState, i32) =
        std::mem::transmute(offsets::LUA_SETTABLE);
    f(l, idx);
}

pub unsafe fn lua_rawseti(l: LuaState, t: i32, n: i32) {
    let f: unsafe extern "fastcall" fn(LuaState, i32, i32) =
        std::mem::transmute(offsets::LUA_RAWSETI);
    f(l, t, n);
}

pub const LUA_REGISTRYINDEX: i32 = -10000;
pub const LUA_TFUNCTION: i32 = 6;
pub const LUA_REFNIL: i32 = -2;

pub unsafe fn lua_type(l: LuaState, idx: i32) -> i32 {
    let f: unsafe extern "fastcall" fn(LuaState, i32) -> i32 =
        std::mem::transmute(offsets::LUA_TYPE);
    f(l, idx)
}

pub unsafe fn lua_isfunction(l: LuaState, idx: i32) -> bool {
    lua_type(l, idx) == LUA_TFUNCTION
}

pub unsafe fn lua_pcall(l: LuaState, nargs: i32, nresults: i32, errfunc: i32) -> i32 {
    let f: unsafe extern "fastcall" fn(LuaState, i32, i32, i32) -> i32 =
        std::mem::transmute(offsets::LUA_PCALL);
    f(l, nargs, nresults, errfunc)
}

pub unsafe fn lua_rawgeti(l: LuaState, t: i32, n: i32) {
    let f: unsafe extern "fastcall" fn(LuaState, i32, i32) =
        std::mem::transmute(offsets::LUA_RAWGETI);
    f(l, t, n);
}

pub unsafe fn lual_ref(l: LuaState, t: i32) -> i32 {
    let f: unsafe extern "fastcall" fn(LuaState, i32) -> i32 =
        std::mem::transmute(offsets::LUAL_REF);
    f(l, t)
}

pub unsafe fn lual_unref(l: LuaState, t: i32, r: i32) {
    let f: unsafe extern "fastcall" fn(LuaState, i32, i32) =
        std::mem::transmute(offsets::LUAL_UNREF);
    f(l, t, r);
}

pub unsafe fn lua_settop(l: LuaState, idx: i32) {
    let f: unsafe extern "fastcall" fn(LuaState, i32) =
        std::mem::transmute(offsets::LUA_SETTOP);
    f(l, idx);
}

/// Compiles and executes a Lua string. This is a WoW-specific function,
/// NOT the standard Lua C API lua_call.
pub unsafe fn lua_call_string(code: &str, source: &str) {
    let c_code = CString::new(code).unwrap_or_default();
    let c_source = CString::new(source).unwrap_or_default();
    let f: unsafe extern "fastcall" fn(*const i8, *const i8) =
        std::mem::transmute(offsets::LUA_CALL);
    f(c_code.as_ptr(), c_source.as_ptr());
}

pub unsafe fn register_lua_function(name: &str, func: *mut usize) {
    let cs = CString::new(name).unwrap_or_default();
    let f: unsafe extern "fastcall" fn(*const i8, *mut usize) =
        std::mem::transmute(offsets::FRAME_SCRIPT_REGISTER_FUNCTION);
    f(cs.as_ptr(), func);
}
