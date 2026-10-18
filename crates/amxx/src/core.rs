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
    pub pointer_to_handle: Option<PfnPointerToHandle>,
    pub handle_to_pointer: Option<PfnHandleToPointer>,
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
    // SAFETY: as in `get!`; only goldsrc-net's fork has these (see `pointer_to_handle`).
    let (to_handle, to_pointer) = unsafe { (req(c"PointerToHandle".as_ptr()), req(c"HandleToPointer".as_ptr())) };
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
        // SAFETY: as in `get!`.
        pointer_to_handle: (!to_handle.is_null())
            .then(|| unsafe { core::mem::transmute::<*mut core::ffi::c_void, PfnPointerToHandle>(to_handle) }),
        // SAFETY: as in `get!`.
        handle_to_pointer: (!to_pointer.is_null())
            .then(|| unsafe { core::mem::transmute::<*mut core::ffi::c_void, PfnHandleToPointer>(to_pointer) }),
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

/// `MF_PointerToHandle`: the cell a plugin holds for a native pointer (a `TraceResult *`, an
/// `ItemInfo *`), 0 for NULL. Cells are 32 bits in every build, so on 64-bit servers goldsrc-net's
/// AMX Mod X fork numbers the pointers in a table of its core's, which its fakemeta and
/// hamsandwich share: a handle from one module works in the others. On 32-bit the handle is the
/// pointer, as in stock AMX Mod X. A 64-bit core without the table gets this module's own, which
/// no other module knows.
pub fn pointer_to_handle(p: *const core::ffi::c_void) -> Cell {
    if p.is_null() {
        return 0;
    }
    if cfg!(target_pointer_width = "32") {
        return p as usize as Cell;
    }
    match core().and_then(|c| c.pointer_to_handle) {
        // SAFETY: the core's function takes any pointer and only stores it.
        Some(f) => unsafe { f(p.cast_mut()) },
        None => own_handles::to_handle(p),
    }
}

/// `MF_HandleToPointer`: the pointer [`pointer_to_handle`] gave `h` for, NULL for 0 or a handle
/// it never gave.
pub fn handle_to_pointer(h: Cell) -> *mut core::ffi::c_void {
    if h == 0 {
        return core::ptr::null_mut();
    }
    if cfg!(target_pointer_width = "32") {
        return h as usize as *mut core::ffi::c_void;
    }
    match core().and_then(|c| c.handle_to_pointer) {
        // SAFETY: the core's function looks the cell up in its table.
        Some(f) => unsafe { f(h) },
        None => own_handles::to_pointer(h),
    }
}

/// The handle table of [`pointer_to_handle`] for a core without one: 1-based, an entry per
/// pointer for the module's lifetime, as the fork's.
mod own_handles {
    use super::Cell;
    use core::ffi::c_void;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Table {
        pointers: Vec<usize>,
        handles: HashMap<usize, Cell>,
    }

    thread_local! {
        static TABLE: RefCell<Table> = RefCell::default();
    }

    pub(super) fn to_handle(p: *const c_void) -> Cell {
        TABLE.with(|t| {
            let mut t = t.borrow_mut();
            let t = &mut *t;
            *t.handles.entry(p as usize).or_insert_with(|| {
                t.pointers.push(p as usize);
                Cell::try_from(t.pointers.len()).unwrap_or(0)
            })
        })
    }

    pub(super) fn to_pointer(h: Cell) -> *mut c_void {
        let i = usize::try_from(h).ok().and_then(|h| h.checked_sub(1));
        TABLE.with(|t| i.and_then(|i| t.borrow().pointers.get(i).copied()).unwrap_or(0) as *mut c_void)
    }
}

/// A forward handle from [`register_forward`] or [`register_forward_params`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Forward(pub i32);

/// Most parameters a forward registered or executed through this crate has (AMX Mod X's own limit,
/// `FORWARD_MAX_PARAMS`, is 32).
pub const MAX_FORWARD_PARAMS: usize = 16;

/// How a forward runs its plugins (`ForwardExecType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecType {
    /// Every plugin; returns 0.
    Ignore,
    /// Stops at the first plugin returning more than 0 (`PLUGIN_HANDLED`) and returns that.
    Stop,
    /// Every plugin unless one returns `PLUGIN_HANDLED`; returns the biggest value.
    Stop2,
    /// Every plugin; returns the biggest value.
    Continue,
}

impl ExecType {
    fn raw(self) -> c_int {
        match self {
            ExecType::Ignore => ET_IGNORE,
            ExecType::Stop => ET_STOP,
            ExecType::Stop2 => ET_STOP2,
            ExecType::Continue => ET_CONTINUE,
        }
    }
}

/// A forward parameter's type. Pass the matching [`FwdValue`] when executing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Param {
    /// A cell ([`FwdValue::Cell`]).
    Cell,
    /// A `Float:` ([`FwdValue::Float`]). Registered as `FP_CELL` holding the float's bits:
    /// `ExecuteForward` would read an `FP_FLOAT` vararg as a double.
    Float,
    /// A `const name[]` ([`FwdValue::Str`]).
    String,
    /// An array, `arr[]` / `Float:v[3]` ([`FwdValue::Array`] or, written back after each plugin,
    /// [`FwdValue::ArrayRef`]).
    Array,
}

impl Param {
    fn raw(self) -> c_int {
        match self {
            Param::Cell | Param::Float => FP_CELL,
            Param::String => FP_STRING,
            Param::Array => FP_ARRAY,
        }
    }
}

/// `MF_RegisterForward(name, ET_IGNORE, FP_CELL x cells, FP_DONE)`: a multi-plugin forward with
/// `cells` cell parameters (floats as cells holding their bits), the only kind TS's modules
/// register. `None` before the core attached or for more than [`MAX_FORWARD_PARAMS`] cells.
pub fn register_forward(name: &CStr, cells: usize) -> Option<Forward> {
    register_forward_params(name, ExecType::Ignore, &vec![Param::Cell; cells])
}

/// [`register_forward`] with `ET_STOP`: [`execute_forward`] stops at the first plugin returning
/// more than 0 (`PLUGIN_HANDLED`) and returns that, else 0.
pub fn register_forward_stop(name: &CStr, cells: usize) -> Option<Forward> {
    register_forward_params(name, ExecType::Stop, &vec![Param::Cell; cells])
}

/// The types as the core's varargs take them (`FP_*`), `None` past [`MAX_FORWARD_PARAMS`].
fn raw_types(params: &[Param]) -> Option<Vec<c_int>> {
    (params.len() <= MAX_FORWARD_PARAMS).then(|| params.iter().map(|p| p.raw()).collect())
}

/// Calls a C variadic function with the fixed arguments, then `$v[0..n]` for the slice's length
/// (at most 16), then the optional terminator; evaluates to `None` for a longer slice.
macro_rules! var_call {
    ($f:expr, ($($fixed:expr),*), $v:expr $(, $done:expr)?) => {{
        let v = $v;
        match v.len() {
            0 => Some($f($($fixed,)* $($done)?)),
            1 => Some($f($($fixed,)* v[0] $(, $done)?)),
            2 => Some($f($($fixed,)* v[0], v[1] $(, $done)?)),
            3 => Some($f($($fixed,)* v[0], v[1], v[2] $(, $done)?)),
            4 => Some($f($($fixed,)* v[0], v[1], v[2], v[3] $(, $done)?)),
            5 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4] $(, $done)?)),
            6 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4], v[5] $(, $done)?)),
            7 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4], v[5], v[6] $(, $done)?)),
            8 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7] $(, $done)?)),
            9 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8] $(, $done)?)),
            10 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9] $(, $done)?)),
            11 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10] $(, $done)?)),
            12 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11] $(, $done)?)),
            13 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12] $(, $done)?)),
            14 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12], v[13] $(, $done)?)),
            15 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12], v[13], v[14] $(, $done)?)),
            16 => Some($f($($fixed,)* v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12], v[13], v[14], v[15] $(, $done)?)),
            _ => None,
        }
    }};
}

/// `MF_RegisterForward(name, exec, types..., FP_DONE)`: a multi-plugin forward (every plugin with
/// a public `name`) with typed parameters, up to [`MAX_FORWARD_PARAMS`]. Run it with
/// [`execute_forward_values`]. `None` before the core attached or with too many parameters.
///
/// The core clears its forwards when the plugins unload (`ServerDeactivate`) and the module
/// registers them again at `AMXX_PluginsLoaded`: forget the handle in `AMXX_PluginsUnloading`.
pub fn register_forward_params(name: &CStr, exec: ExecType, params: &[Param]) -> Option<Forward> {
    let c = core()?;
    let types = raw_types(params)?;
    let (f, n, e) = (c.register_forward, name.as_ptr(), exec.raw());
    // SAFETY: the variadic RegisterForward with up to 16 `FP_*` types (ints) terminated by
    // FP_DONE, as the SDK documents.
    let id = unsafe { var_call!(f, (n, e), &types[..], FP_DONE) }?;
    Some(Forward(id))
}

/// `MF_ExecuteForward(id, cells...)`. The plugins run inside this call, so callers must not hold
/// module state they might reach again through a native.
pub fn execute_forward(fwd: Forward, cells: &[Cell]) -> i32 {
    let args: Vec<isize> = cells.iter().map(|&c| c as isize).collect();
    execute_raw(fwd.0, &args)
}

/// `ExecuteForward` with up to 16 pointer-sized values: cells, string pointers (`FP_STRING`)
/// and prepared array ids (`FP_ARRAY`). Floats must be passed as cells holding their bits (and
/// the forward registered with `FP_CELL`): `ExecuteForward` reads `FP_FLOAT` as a double.
fn execute_raw(id: c_int, a: &[isize]) -> i32 {
    let Some(c) = core() else { return 0 };
    let f = c.execute_forward;
    // SAFETY: the variadic ExecuteForward with as many pointer-sized values as the forward has
    // parameters (the caller's contract); on i686 a cell, a pointer and isize are all 4 bytes.
    unsafe { var_call!(f, (id), a) }.unwrap_or(0)
}

/// A value for [`execute_forward_values`] and [`execute_sp_forward`], matching the forward's
/// [`Param`].
pub enum FwdValue<'a> {
    Cell(Cell),
    /// Passed as a cell holding the float's bits ([`Param::Float`]).
    Float(f32),
    Str(&'a CStr),
    /// Copied into the plugin for the call (`MF_PrepareCellArrayA(.., false)`).
    Array(&'a mut [Cell]),
    /// Copied in, and the plugin's changes copied back after each plugin runs
    /// (`MF_PrepareCellArrayA(.., true)`): a by-reference array.
    ArrayRef(&'a mut [Cell]),
}

/// `MF_PrepareCellArrayA(arr, len, copyback)`: the id an `FP_ARRAY` parameter takes.
fn prepare(c: &CoreFns, arr: &mut [Cell], copyback: bool) -> isize {
    // SAFETY: the array outlives the ExecuteForward that follows (the caller's borrow), where the
    // core copies it in and, with `copyback`, back out; it is valid for `len` cells.
    unsafe { (c.prepare_cell_array_a)(arr.as_mut_ptr(), arr.len() as core::ffi::c_uint, copyback) as isize }
}

/// The values as `ExecuteForward`'s varargs: arrays are prepared (the core keeps the pointer
/// until the forward ran, so `values` must outlive that call).
fn raw_values(c: &CoreFns, values: &mut [FwdValue]) -> Vec<isize> {
    let mut args = Vec::with_capacity(values.len());
    for v in values.iter_mut() {
        args.push(match v {
            FwdValue::Cell(x) => *x as isize,
            FwdValue::Float(x) => x.to_bits() as Cell as isize,
            FwdValue::Str(s) => s.as_ptr() as isize,
            FwdValue::Array(arr) => prepare(c, arr, false),
            FwdValue::ArrayRef(arr) => prepare(c, arr, true),
        });
    }
    args
}

/// `MF_ExecuteForward` of a multi-plugin forward from [`register_forward_params`], one value per
/// parameter; returns what the [`ExecType`] makes of the plugins' results (0 before the core
/// attached, -1 for a stale handle). Plugins run inside: hold no module state across the call.
pub fn execute_forward_values(fwd: Forward, values: &mut [FwdValue]) -> i32 {
    let Some(c) = core() else { return 0 };
    let args = raw_values(c, values);
    execute_raw(fwd.0, &args)
}

/// `MF_ExecuteForward` of a single-plugin forward with mixed values; returns what the public
/// returned.
pub fn execute_sp_forward(id: i32, values: &mut [FwdValue]) -> i32 {
    let Some(c) = core() else { return 0 };
    let args = raw_values(c, values);
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
    // SAFETY: the variadic RegisterSPForwardByName with up to 16 parameter types terminated by
    // FP_DONE; `amx` per the caller's contract.
    unsafe { var_call!(f, (amx, n), types, FP_DONE) }.unwrap_or(-1)
}

/// [`register_sp_forward`] with typed parameters (as [`register_forward_params`]): a forward to
/// one plugin's public, run with [`execute_sp_forward`]; -1 if it has none by that name or with
/// more than [`MAX_FORWARD_PARAMS`] parameters. The core drops single-plugin forwards with the
/// plugins (`ServerDeactivate`), so forget the id in `AMXX_PluginsUnloading`.
///
/// # Safety
/// `amx` is a plugin's AMX the core handed to a native (`Args::amx`), during the map it was given.
pub unsafe fn register_sp_forward_params(amx: usize, name: &str, params: &[Param]) -> i32 {
    let Some(types) = raw_types(params) else { return -1 };
    // SAFETY: the caller's contract.
    unsafe { register_sp_forward(amx, name, &types) }
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
