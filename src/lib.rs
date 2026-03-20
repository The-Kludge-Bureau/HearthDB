#![allow(non_snake_case)]

mod db;
mod lua;
mod offsets;

use lua::LuaState;
use minhook::MinHook;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{BOOL, TRUE};
use windows_sys::Win32::System::LibraryLoader::DLL_PROCESS_ATTACH;

type LoadScriptFunctionsT = unsafe extern "stdcall" fn();

static ORIG_PLAYER_LOAD: OnceLock<LoadScriptFunctionsT> = OnceLock::new();
static ORIG_GLUE_LOAD:   OnceLock<LoadScriptFunctionsT> = OnceLock::new();

unsafe fn register_hdb_functions() {
    lua::register_lua_function("HDB_Open",     db::script_hdb_open      as *mut usize);
    lua::register_lua_function("HDB_Close",    db::script_hdb_close     as *mut usize);
    lua::register_lua_function("HDB_Execute",  db::script_hdb_execute   as *mut usize);
    lua::register_lua_function("HDB_Query",    db::script_hdb_query     as *mut usize);
    lua::register_lua_function("HDB_QueryRaw", db::script_hdb_query_raw as *mut usize);
}

unsafe extern "stdcall" fn player_load_hook() {
    if let Some(orig) = ORIG_PLAYER_LOAD.get() {
        orig();
    }
    register_hdb_functions();
}

unsafe extern "stdcall" fn glue_load_hook() {
    if let Some(orig) = ORIG_GLUE_LOAD.get() {
        orig();
    }
    register_hdb_functions();
}

#[no_mangle]
pub unsafe extern "system" fn DllMain(
    _hinstance: *mut std::ffi::c_void,
    reason: u32,
    _reserved: *mut std::ffi::c_void,
) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        if let Ok(orig) = MinHook::create_hook(
            offsets::PLAYER_LOAD_SCRIPT_FUNCTIONS as *mut _,
            player_load_hook as *mut _,
        ) {
            let _ = ORIG_PLAYER_LOAD.set(std::mem::transmute(orig));
        }
        if let Ok(orig) = MinHook::create_hook(
            offsets::GLUE_LOAD_SCRIPT_FUNCTIONS as *mut _,
            glue_load_hook as *mut _,
        ) {
            let _ = ORIG_GLUE_LOAD.set(std::mem::transmute(orig));
        }
    }
    TRUE
}

#[no_mangle]
pub extern "C" fn Load() -> u32 {
    unsafe {
        let _ = MinHook::enable_all_hooks();
    }
    0
}
