// tsmod-amxx-rs: AMX Mod X's TS modules for The Specialists 3.0 (Rust port).
// Based on the AMX Mod X SDK (public/sdk/amxxmodule.h, amxmodx/amx.h),
// Copyright (C) The AMX Mod X Development Team, and on metamod-hl1 (meta_api.h, plinfo.h,
// mutil.h), Copyright (C) 2001-2003 Will Day. GPL-3.0-or-later with the AMX Mod X exceptions.

//! Hand-written `repr(C)` definitions of the AMX Mod X module ABI and the metamod plugin ABI.
//!
//! Only what this workspace uses is declared; the layouts match the headers named above on the
//! 32-bit targets AMX Mod X ships for (i686 Linux and Windows). Entries of a C table this crate
//! never calls are declared as `*const c_void`, which has the size of a function pointer.

use core::ffi::{c_char, c_int, c_void};

/// A Pawn cell (`cell` in amx.h): 32 bits in every AMX Mod X build.
pub type Cell = i32;

/// `AMXX_INTERFACE_VERSION`: 4 added the library/libclass fields.
pub const AMXX_INTERFACE_VERSION: c_int = 4;
/// Return codes of the `AMXX_*` exports.
pub const AMXX_OK: c_int = 0;
pub const AMXX_IFVERS: c_int = 1;
pub const AMXX_PARAM: c_int = 2;
pub const AMXX_FUNC_NOT_PRESENT: c_int = 3;
pub const AMXX_GAME_OK: c_int = 0;
pub const AMXX_GAME_BAD: c_int = 1;

/// `AMX_ERR_NONE`, `AMX_ERR_NATIVE` (amx.h).
pub const AMX_ERR_NONE: c_int = 0;
pub const AMX_ERR_NATIVE: c_int = 10;

/// `ForwardExecType` / `ForwardParam` (amxxmodule.h).
pub const ET_IGNORE: c_int = 0;
pub const ET_STOP: c_int = 1;
pub const ET_STOP2: c_int = 2;
pub const ET_CONTINUE: c_int = 3;
pub const FP_DONE: c_int = -1;
pub const FP_CELL: c_int = 0;
pub const FP_FLOAT: c_int = 1;
pub const FP_STRING: c_int = 2;
pub const FP_ARRAY: c_int = 4;
pub const FP_CELL_BYREF: c_int = 5;
pub const FP_FLOAT_BYREF: c_int = 6;

/// `amxx_module_info_s`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ModuleInfo {
    pub name: *const c_char,
    pub author: *const c_char,
    pub version: *const c_char,
    /// Reload on map change when non-zero (pure AMX Mod X modules only).
    pub reload: c_int,
    pub logtag: *const c_char,
    pub library: *const c_char,
    pub libclass: *const c_char,
}

/// `tagAMX` from amx.h. GCC builds pack it (`AMX_NO_ALIGN` + `PACKED`) and MSVC builds use
/// `#pragma pack(1)`; on i686 every field is 4 bytes, so the packed and natural layouts agree.
/// The module only allocates these for the score script and passes them back to the core.
#[repr(C, packed)]
pub struct Amx {
    pub base: *mut u8,
    pub data: *mut u8,
    pub callback: *const c_void,
    pub debug: *const c_void,
    pub cip: Cell,
    pub frm: Cell,
    pub hea: Cell,
    pub hlw: Cell,
    pub stk: Cell,
    pub stp: Cell,
    pub flags: c_int,
    pub usertags: [*mut c_void; 4],
    pub userdata: [*mut c_void; 4],
    pub error: c_int,
    pub paramcount: c_int,
    pub pri: Cell,
    pub alt: Cell,
    pub reset_stk: Cell,
    pub reset_hea: Cell,
    pub sysreq_d: Cell,
    pub reloc_size: c_int,
    /// `long`, 4 bytes on both 32-bit ABIs.
    pub code_size: i32,
}

/// `AMX_NATIVE`: natives are plain cdecl on every platform (`AMX_NATIVE_CALL` is empty unless
/// `PAWN_DLL`, which AMX Mod X doesn't define).
pub type AmxNative = unsafe extern "C" fn(amx: *mut Amx, params: *const Cell) -> Cell;

/// `AMX_NATIVE_INFO`: the core keeps the pointer to the array, so arrays must be `'static` and
/// end with a `{NULL, NULL}` entry.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AmxNativeInfo {
    pub name: *const c_char,
    pub func: Option<AmxNative>,
}

// SAFETY: the table holds pointers to `'static` C strings and functions only; nothing writes it.
unsafe impl Sync for AmxNativeInfo {}

/// `PFN_REQ_FNPTR`: the core's function lookup passed to `AMXX_Attach`.
pub type ReqFnptr = unsafe extern "C" fn(name: *const c_char) -> *mut c_void;

// Core functions this workspace requests, with the SDK's `PFN_*` signatures.
pub type PfnAddNatives = unsafe extern "C" fn(list: *const AmxNativeInfo) -> c_int;
pub type PfnBuildPathname = unsafe extern "C" fn(fmt: *const c_char, ...) -> *mut c_char;
pub type PfnGetAmxAddr = unsafe extern "C" fn(amx: *mut Amx, offset: Cell) -> *mut Cell;
pub type PfnPrintSrvConsole = unsafe extern "C" fn(fmt: *const c_char, ...);
pub type PfnSetAmxString = unsafe extern "C" fn(amx: *mut Amx, addr: Cell, src: *const c_char, max: c_int) -> c_int;
pub type PfnGetAmxString = unsafe extern "C" fn(amx: *mut Amx, addr: Cell, buffer: c_int, len: *mut c_int) -> *mut c_char;
pub type PfnLog = unsafe extern "C" fn(fmt: *const c_char, ...);
pub type PfnLogError = unsafe extern "C" fn(amx: *mut Amx, err: c_int, fmt: *const c_char, ...);
pub type PfnRegisterForward = unsafe extern "C" fn(name: *const c_char, exec: c_int, ...) -> c_int;
pub type PfnExecuteForward = unsafe extern "C" fn(id: c_int, ...) -> c_int;
pub type PfnRegisterSpForwardByName = unsafe extern "C" fn(amx: *mut Amx, name: *const c_char, ...) -> c_int;
pub type PfnUnregisterSpForward = unsafe extern "C" fn(id: c_int);
pub type PfnPrepareCellArrayA = unsafe extern "C" fn(ptr: *mut Cell, size: core::ffi::c_uint, copyback: bool) -> Cell;
pub type PfnIsPlayerIngame = unsafe extern "C" fn(id: c_int) -> c_int;
pub type PfnAmxExec = unsafe extern "C" fn(amx: *mut Amx, ret: *mut Cell, index: c_int) -> c_int;
pub type PfnAmxAllot = unsafe extern "C" fn(amx: *mut Amx, len: c_int, amx_addr: *mut Cell, phys: *mut *mut Cell) -> c_int;
pub type PfnAmxFindPublic = unsafe extern "C" fn(amx: *mut Amx, name: *const c_char, index: *mut c_int) -> c_int;
pub type PfnAmxPush = unsafe extern "C" fn(amx: *mut Amx, value: Cell) -> c_int;
/// `LoadAmxScript`: the error buffer is `char[64]`.
pub type PfnLoadAmxScript =
    unsafe extern "C" fn(amx: *mut Amx, code: *mut *mut c_void, path: *const c_char, error: *mut c_char, debug: c_int) -> c_int;
/// `LoadAmxScriptEx` (1.8.3+): the error buffer has an explicit length.
pub type PfnLoadAmxScriptEx = unsafe extern "C" fn(
    amx: *mut Amx,
    code: *mut *mut c_void,
    path: *const c_char,
    error: *mut c_char,
    maxlen: usize,
    debug: c_int,
) -> c_int;
pub type PfnUnloadAmxScript = unsafe extern "C" fn(amx: *mut Amx, code: *mut *mut c_void) -> c_int;

// ---- metamod (meta_api.h, plinfo.h, mutil.h) ----

/// `META_INTERFACE_VERSION` of metamod 1.19+ (metamod-p and metamod-r report the same).
pub const META_INTERFACE_VERSION: &core::ffi::CStr = c"5:13";

/// `PLUG_LOADTIME`.
pub const PT_ANYTIME: c_int = 3;
/// `PL_UNLOAD_REASON::PNL_CMD_FORCED`.
pub const PNL_CMD_FORCED: c_int = 4;

/// `META_RES`.
pub const MRES_UNSET: c_int = 0;
pub const MRES_IGNORED: c_int = 1;
pub const MRES_HANDLED: c_int = 2;
pub const MRES_OVERRIDE: c_int = 3;
pub const MRES_SUPERCEDE: c_int = 4;

/// `ginfo_t::GINFO_REALDLL_FULLPATH`: the game library metamod loaded.
pub const GINFO_REALDLL_FULLPATH: c_int = 5;

/// `plugin_info_t`.
#[repr(C)]
pub struct PluginInfo {
    pub ifvers: *const c_char,
    pub name: *const c_char,
    pub version: *const c_char,
    pub date: *const c_char,
    pub author: *const c_char,
    pub url: *const c_char,
    pub logtag: *const c_char,
    pub loadable: c_int,
    pub unloadable: c_int,
}

// SAFETY: only `'static` C strings, written once; metamod only reads it.
unsafe impl Sync for PluginInfo {}
// SAFETY: as above; it holds no thread-bound data.
unsafe impl Send for PluginInfo {}

/// `meta_globals_t`.
#[repr(C)]
pub struct MetaGlobals {
    pub mres: c_int,
    pub prev_mres: c_int,
    pub status: c_int,
    pub orig_ret: *mut c_void,
    pub override_ret: *mut c_void,
}

pub type GetEntityApi2Fn = unsafe extern "C" fn(table: *mut goldsrc_sys::DLL_FUNCTIONS, version: *mut c_int) -> c_int;
pub type GetEngineFunctionsFn = unsafe extern "C" fn(table: *mut goldsrc_sys::enginefuncs_t, version: *mut c_int) -> c_int;

/// `META_FUNCTIONS`: the GetEntityAPI (v1) and GetNewDLLFunctions entries stay NULL.
#[repr(C)]
pub struct MetaFunctions {
    pub get_entity_api: *const c_void,
    pub get_entity_api_post: *const c_void,
    pub get_entity_api2: Option<GetEntityApi2Fn>,
    pub get_entity_api2_post: Option<GetEntityApi2Fn>,
    pub get_new_dll_functions: *const c_void,
    pub get_new_dll_functions_post: *const c_void,
    pub get_engine_functions: Option<GetEngineFunctionsFn>,
    pub get_engine_functions_post: Option<GetEngineFunctionsFn>,
}

/// `gamedll_funcs_t`: the game library's own tables, for calling it directly (`MDLL_*`).
#[repr(C)]
pub struct GamedllFuncs {
    pub dllapi_table: *mut goldsrc_sys::DLL_FUNCTIONS,
    pub newapi_table: *mut c_void,
}

/// `plid_t`: metamod identifies a plugin by its `plugin_info_t` pointer.
pub type Plid = *const PluginInfo;

/// `mutil_funcs_t`, up to the last entry used (`pfnGetGameInfo`).
#[repr(C)]
pub struct MutilFuncs {
    pub log_console: *const c_void,
    pub log_message: Option<unsafe extern "C" fn(plid: Plid, fmt: *const c_char, ...)>,
    pub log_error: Option<unsafe extern "C" fn(plid: Plid, fmt: *const c_char, ...)>,
    pub log_developer: *const c_void,
    pub center_say: *const c_void,
    pub center_say_parms: *const c_void,
    pub center_say_varargs: *const c_void,
    pub call_game_entity: *const c_void,
    pub get_user_msg_id: Option<unsafe extern "C" fn(plid: Plid, name: *const c_char, size: *mut c_int) -> c_int>,
    pub get_user_msg_name: *const c_void,
    pub get_plugin_path: *const c_void,
    pub get_game_info: Option<unsafe extern "C" fn(plid: Plid, tag: c_int) -> *const c_char>,
}

#[cfg(all(test, target_pointer_width = "32"))]
mod layout {
    use super::*;

    #[test]
    fn sizes_match_the_c_headers() {
        // amx.h: 4 pointers, 6 cells, flags, 2x4 user words, error, paramcount, 5 cells, 2 ints.
        assert_eq!(size_of::<Amx>(), 112);
        assert_eq!(size_of::<ModuleInfo>(), 28);
        assert_eq!(size_of::<PluginInfo>(), 36);
        assert_eq!(size_of::<MetaFunctions>(), 32);
        assert_eq!(core::mem::offset_of!(MutilFuncs, get_game_info), 44);
    }
}
