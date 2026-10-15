// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports the `MF_*` helpers of
// the AMX Mod X module SDK (public/sdk/amxxmodule.cpp), Copyright (C) The AMX Mod X Development
// Team.

//! The AMX Mod X core functions a module requests in `AMXX_Attach`, and safe wrappers.
//!
//! The C++ SDK requests every function it knows and refuses to load if one is missing. This
//! module requests only what the workspace calls, so it also loads on older cores (1.8.2 lacks
//! `LoadAmxScriptEx`, which is optional here).

use crate::ffi::*;
use core::ffi::{CStr, c_char, c_int};
use std::ffi::CString;
use std::sync::OnceLock;

/// The requested core functions.
pub(crate) struct CoreFns {
    pub add_natives: PfnAddNatives,
    pub build_pathname: PfnBuildPathname,
    pub get_amx_addr: PfnGetAmxAddr,
    pub print_srv_console: PfnPrintSrvConsole,
    pub set_amx_string: PfnSetAmxString,
    pub get_amx_string: PfnGetAmxString,
    pub log: PfnLog,
    pub log_error: PfnLogError,
    pub register_forward: PfnRegisterForward,
    pub execute_forward: PfnExecuteForward,
    pub register_sp_forward_by_name: PfnRegisterSpForwardByName,
    pub unregister_sp_forward: PfnUnregisterSpForward,
    pub prepare_cell_array_a: PfnPrepareCellArrayA,
    pub is_player_ingame: PfnIsPlayerIngame,
    pub amx_exec: PfnAmxExec,
    pub amx_allot: PfnAmxAllot,
    pub amx_find_public: PfnAmxFindPublic,
    pub amx_push: PfnAmxPush,
    pub load_amx_script: PfnLoadAmxScript,
    pub load_amx_script_ex: Option<PfnLoadAmxScriptEx>,
    pub unload_amx_script: PfnUnloadAmxScript,
}

static CORE: OnceLock<CoreFns> = OnceLock::new();
static LOGTAG: OnceLock<&'static CStr> = OnceLock::new();

pub(crate) fn core() -> Option<&'static CoreFns> {
    CORE.get()
}

/// Requests the functions from the core. `Err` carries the name of the first missing one (the
/// core reports it to the server operator after `AMXX_FUNC_NOT_PRESENT`).
///
/// # Safety
/// `req` must be the function the core passed to `AMXX_Attach`.
pub(crate) unsafe fn request(req: ReqFnptr, logtag: &'static CStr) -> Result<(), &'static CStr> {
    macro_rules! get {
        ($name:literal, $t:ty) => {{
            // SAFETY: `req` is the core's lookup (caller contract); it takes a C string.
            let p = unsafe { req($name.as_ptr()) };
            if p.is_null() {
                return Err($name);
            }
            // SAFETY: the core returns the function the name stands for, whose signature the
            // SDK declares and the field type repeats.
            unsafe { core::mem::transmute::<*mut core::ffi::c_void, $t>(p) }
        }};
    }
    // SAFETY: as in `get!`; NULL maps to `None`.
    let load_ex = unsafe { req(c"LoadAmxScriptEx".as_ptr()) };
    let fns = CoreFns {
        add_natives: get!(c"AddNatives", PfnAddNatives),
        build_pathname: get!(c"BuildPathname", PfnBuildPathname),
        get_amx_addr: get!(c"GetAmxAddr", PfnGetAmxAddr),
        print_srv_console: get!(c"PrintSrvConsole", PfnPrintSrvConsole),
        set_amx_string: get!(c"SetAmxString", PfnSetAmxString),
        get_amx_string: get!(c"GetAmxString", PfnGetAmxString),
        log: get!(c"Log", PfnLog),
        log_error: get!(c"LogError", PfnLogError),
        register_forward: get!(c"RegisterForward", PfnRegisterForward),
        execute_forward: get!(c"ExecuteForward", PfnExecuteForward),
        register_sp_forward_by_name: get!(c"RegisterSPForwardByName", PfnRegisterSpForwardByName),
        unregister_sp_forward: get!(c"UnregisterSPForward", PfnUnregisterSpForward),
        prepare_cell_array_a: get!(c"PrepareCellArrayA", PfnPrepareCellArrayA),
        is_player_ingame: get!(c"IsPlayerInGame", PfnIsPlayerIngame),
        amx_exec: get!(c"amx_Exec", PfnAmxExec),
        amx_allot: get!(c"amx_Allot", PfnAmxAllot),
        amx_find_public: get!(c"amx_FindPublic", PfnAmxFindPublic),
        amx_push: get!(c"amx_Push", PfnAmxPush),
        load_amx_script: get!(c"LoadAmxScript", PfnLoadAmxScript),
        // SAFETY: as in `get!`.
        load_amx_script_ex: (!load_ex.is_null())
            .then(|| unsafe { core::mem::transmute::<*mut core::ffi::c_void, PfnLoadAmxScriptEx>(load_ex) }),
        unload_amx_script: get!(c"UnloadAmxScript", PfnUnloadAmxScript),
    };
    let _ = CORE.set(fns);
    let _ = LOGTAG.set(logtag);
    Ok(())
}

/// Turns Rust text into a C string, dropping interior NULs rather than failing.
pub(crate) fn c_string(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

fn tag() -> &'static CStr {
    LOGTAG.get().copied().unwrap_or(c"AMXX")
}

/// `MF_Log`: a line in the AMX Mod X log, prefixed with the module's log tag. Before the core
/// attached it goes to stderr, which is what tests see.
pub fn log(msg: &str) {
    let m = c_string(msg);
    match core() {
        // SAFETY: the core's variadic `Log` with a "%s" format and two C strings.
        Some(c) => unsafe { (c.log)(c"[%s] %s".as_ptr(), tag().as_ptr(), m.as_ptr()) },
        None => eprintln!("[{}] {msg}", tag().to_string_lossy()),
    }
}

/// `MF_PrintSrvConsole`: a line on the server console.
pub fn print_srv_console(msg: &str) {
    let m = c_string(msg);
    if let Some(c) = core() {
        // SAFETY: the core's variadic printf-style function with "%s" and one C string.
        unsafe { (c.print_srv_console)(c"%s\n".as_ptr(), m.as_ptr()) }
    }
}

/// `MF_BuildPathname("%s", path)`: `path` below the mod directory, with the platform's separators.
pub fn build_pathname(path: &str) -> String {
    let p = c_string(path);
    let Some(c) = core() else { return path.to_owned() };
    // SAFETY: "%s" with one C string; the core returns a pointer to its static buffer, copied out
    // before anything else can call it.
    unsafe {
        let r = (c.build_pathname)(c"%s".as_ptr(), p.as_ptr());
        if r.is_null() { path.to_owned() } else { CStr::from_ptr(r).to_string_lossy().into_owned() }
    }
}

/// `MF_IsPlayerIngame`.
pub fn is_player_ingame(index: i32) -> bool {
    // SAFETY: a plain core function taking an int.
    core().is_some_and(|c| unsafe { (c.is_player_ingame)(index) } != 0)
}

/// A forward handle from [`register_forward`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Forward(pub i32);

/// `MF_RegisterForward(name, ET_IGNORE, FP_CELL x cells, FP_DONE)`: a multi-plugin forward with
/// `cells` cell parameters (floats as cells holding their bits), the only kind TS's modules
/// register. `None` before the core attached.
pub fn register_forward(name: &CStr, cells: usize) -> Option<Forward> {
    register_forward_exec(name, ET_IGNORE, cells)
}

/// [`register_forward`] with `ET_STOP`: [`execute_forward`] stops at the first plugin returning
/// more than 0 (`PLUGIN_HANDLED`) and returns that, else 0.
pub fn register_forward_stop(name: &CStr, cells: usize) -> Option<Forward> {
    register_forward_exec(name, ET_STOP, cells)
}

fn register_forward_exec(name: &CStr, exec: c_int, cells: usize) -> Option<Forward> {
    let c = core()?;
    let (f, n) = (c.register_forward, name.as_ptr());
    const C: c_int = FP_CELL;
    const D: c_int = FP_DONE;
    // SAFETY: the variadic RegisterForward with the parameter types terminated by FP_DONE, as
    // the SDK documents.
    let id = unsafe {
        match cells {
            0 => f(n, exec, D),
            1 => f(n, exec, C, D),
            2 => f(n, exec, C, C, D),
            3 => f(n, exec, C, C, C, D),
            4 => f(n, exec, C, C, C, C, D),
            5 => f(n, exec, C, C, C, C, C, D),
            6 => f(n, exec, C, C, C, C, C, C, D),
            _ => return None,
        }
    };
    Some(Forward(id))
}

/// `MF_ExecuteForward(id, cells...)`. The plugins run inside this call, so callers must not hold
/// module state they might reach again through a native.
pub fn execute_forward(fwd: Forward, cells: &[Cell]) -> i32 {
    let args: Vec<isize> = cells.iter().map(|&c| c as isize).collect();
    execute_raw(fwd.0, &args)
}

/// `ExecuteForward` with up to 10 pointer-sized values: cells, string pointers (`FP_STRING`)
/// and prepared array ids (`FP_ARRAY`). Floats must be passed as cells holding their bits (and
/// the forward registered with `FP_CELL`): `ExecuteForward` reads `FP_FLOAT` as a double.
fn execute_raw(id: c_int, a: &[isize]) -> i32 {
    let Some(c) = core() else { return 0 };
    let f = c.execute_forward;
    // SAFETY: the variadic ExecuteForward with as many pointer-sized values as the forward has
    // parameters (the caller's contract); on i686 a cell, a pointer and isize are all 4 bytes.
    unsafe {
        match *a {
            [] => f(id),
            [a0] => f(id, a0),
            [a0, a1] => f(id, a0, a1),
            [a0, a1, a2] => f(id, a0, a1, a2),
            [a0, a1, a2, a3] => f(id, a0, a1, a2, a3),
            [a0, a1, a2, a3, a4] => f(id, a0, a1, a2, a3, a4),
            [a0, a1, a2, a3, a4, a5] => f(id, a0, a1, a2, a3, a4, a5),
            [a0, a1, a2, a3, a4, a5, a6] => f(id, a0, a1, a2, a3, a4, a5, a6),
            [a0, a1, a2, a3, a4, a5, a6, a7] => f(id, a0, a1, a2, a3, a4, a5, a6, a7),
            [a0, a1, a2, a3, a4, a5, a6, a7, a8] => f(id, a0, a1, a2, a3, a4, a5, a6, a7, a8),
            [a0, a1, a2, a3, a4, a5, a6, a7, a8, a9] => f(id, a0, a1, a2, a3, a4, a5, a6, a7, a8, a9),
            _ => 0,
        }
    }
}

/// A value for [`execute_sp_forward`].
pub enum FwdValue<'a> {
    Cell(Cell),
    Str(&'a CStr),
    /// Copied into the plugin for the call (`MF_PrepareCellArrayA(.., false)`).
    Array(&'a mut [Cell]),
}

/// `MF_ExecuteForward` of a single-plugin forward with mixed values; returns what the public
/// returned.
pub fn execute_sp_forward(id: i32, values: &mut [FwdValue]) -> i32 {
    let Some(c) = core() else { return 0 };
    let mut args = Vec::with_capacity(values.len());
    for v in values.iter_mut() {
        args.push(match v {
            FwdValue::Cell(x) => *x as isize,
            FwdValue::Str(s) => s.as_ptr() as isize,
            FwdValue::Array(arr) => {
                // SAFETY: the array outlives the ExecuteForward below, where the core copies it.
                let id = unsafe { (c.prepare_cell_array_a)(arr.as_mut_ptr(), arr.len() as core::ffi::c_uint, false) };
                id as isize
            }
        });
    }
    execute_raw(id, &args)
}

/// `MF_RegisterSPForwardByName(amx, name, types..., FP_DONE)`: a forward to one plugin's public,
/// -1 if it has none by that name.
///
/// # Safety
/// `amx` is a plugin's AMX the core handed to a native (`Args::amx`).
pub unsafe fn register_sp_forward(amx: usize, name: &str, types: &[core::ffi::c_int]) -> i32 {
    let Some(c) = core() else { return -1 };
    let n = c_string(name);
    let (f, amx, n) = (c.register_sp_forward_by_name, amx as *mut Amx, n.as_ptr());
    const D: core::ffi::c_int = FP_DONE;
    // SAFETY: the variadic RegisterSPForwardByName with the parameter types terminated by
    // FP_DONE; `amx` per the caller's contract.
    unsafe {
        match *types {
            [] => f(amx, n, D),
            [a] => f(amx, n, a, D),
            [a, b] => f(amx, n, a, b, D),
            [a, b, c2] => f(amx, n, a, b, c2, D),
            [a, b, c2, d] => f(amx, n, a, b, c2, d, D),
            [a, b, c2, d, e] => f(amx, n, a, b, c2, d, e, D),
            [a, b, c2, d, e, g] => f(amx, n, a, b, c2, d, e, g, D),
            [a, b, c2, d, e, g, h] => f(amx, n, a, b, c2, d, e, g, h, D),
            [a, b, c2, d, e, g, h, i] => f(amx, n, a, b, c2, d, e, g, h, i, D),
            [a, b, c2, d, e, g, h, i, j] => f(amx, n, a, b, c2, d, e, g, h, i, j, D),
            [a, b, c2, d, e, g, h, i, j, k] => f(amx, n, a, b, c2, d, e, g, h, i, j, k, D),
            _ => -1,
        }
    }
}

/// `MF_UnregisterSPForward(id)`.
pub fn unregister_sp_forward(id: i32) {
    if let Some(c) = core() {
        // SAFETY: a plain core function taking a forward id.
        unsafe { (c.unregister_sp_forward)(id) }
    }
}

/// `MF_AddNatives`. The core keeps the pointer, hence `'static`; the array must end with a NULL
/// entry (the [`natives!`](crate::natives) macro adds it).
pub fn add_natives(list: &'static [AmxNativeInfo]) {
    debug_assert!(list.last().is_some_and(|n| n.name.is_null()));
    if let Some(c) = core() {
        // SAFETY: a NULL-terminated `'static` table.
        unsafe { (c.add_natives)(list.as_ptr()) };
    }
}

/// `MF_LogError(amx, AMX_ERR_NATIVE, ...)`: a native error with the plugin's backtrace.
///
/// # Safety
/// `amx` must be the AMX the core passed to the running native.
pub(crate) unsafe fn log_error(amx: *mut Amx, msg: &str) {
    let m = c_string(msg);
    match core() {
        // SAFETY: the caller passes the native's AMX; "%s"-style format with C strings.
        Some(c) => unsafe { (c.log_error)(amx, AMX_ERR_NATIVE, c"[%s] %s".as_ptr(), tag().as_ptr(), m.as_ptr()) },
        None => eprintln!("[{}] native error: {msg}", tag().to_string_lossy()),
    }
}

/// Reads a C string the engine or core handed over.
///
/// # Safety
/// `p` is NULL or points to a NUL-terminated string that stays valid while it is read.
pub(crate) unsafe fn cstr_lossy(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: the caller's contract.
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}
