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
/// `cells` cell parameters, the only kind TS's modules register. `None` before the core attached.
pub fn register_forward(name: &CStr, cells: usize) -> Option<Forward> {
    let c = core()?;
    let (f, n) = (c.register_forward, name.as_ptr());
    const C: c_int = FP_CELL;
    const D: c_int = FP_DONE;
    // SAFETY: the variadic RegisterForward with the parameter types terminated by FP_DONE, as
    // the SDK documents.
    let id = unsafe {
        match cells {
            0 => f(n, ET_IGNORE, D),
            1 => f(n, ET_IGNORE, C, D),
            2 => f(n, ET_IGNORE, C, C, D),
            3 => f(n, ET_IGNORE, C, C, C, D),
            4 => f(n, ET_IGNORE, C, C, C, C, D),
            5 => f(n, ET_IGNORE, C, C, C, C, C, D),
            6 => f(n, ET_IGNORE, C, C, C, C, C, C, D),
            _ => return None,
        }
    };
    Some(Forward(id))
}

/// `MF_ExecuteForward(id, cells...)`. The plugins run inside this call, so callers must not hold
/// module state they might reach again through a native.
pub fn execute_forward(fwd: Forward, cells: &[Cell]) -> i32 {
    let Some(c) = core() else { return 0 };
    let (f, id) = (c.execute_forward, fwd.0);
    // SAFETY: the variadic ExecuteForward with as many cells as the forward was registered with
    // (the caller's contract mirrors `register_forward`).
    unsafe {
        match *cells {
            [] => f(id),
            [a] => f(id, a),
            [a, b] => f(id, a, b),
            [a, b, c] => f(id, a, b, c),
            [a, b, c, d] => f(id, a, b, c, d),
            [a, b, c, d, e] => f(id, a, b, c, d, e),
            [a, b, c, d, e, g] => f(id, a, b, c, d, e, g),
            _ => 0,
        }
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
