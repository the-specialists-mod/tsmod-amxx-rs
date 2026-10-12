// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports the metamod half of the
// AMX Mod X module SDK (public/sdk/amxxmodule.cpp: Meta_Query, Meta_Attach, Meta_Detach,
// GiveFnptrsToDll, GetEntityAPI2, GetEngineFunctions), Copyright (C) The AMX Mod X Development
// Team.

//! Metamod plugin glue. AMX Mod X loads a module that exports `Meta_Attach` as a metamod plugin
//! first (`CModule::attachMetamod`), so these run before `AMXX_Attach`.
//!
//! Unlike the C++ SDK, which copies whole tables, only the hooks a module sets are written into
//! metamod's tables: FWGS's `enginefuncs_t` (the bindings' layout) is longer than metamod's.

use crate::engine::{self, Edict, common_engine_entries};
use crate::ffi::*;
use core::ffi::{CStr, c_char, c_int};
use core::sync::atomic::{AtomicPtr, Ordering};
use goldsrc_sys::{DLL_FUNCTIONS, KeyValueData, enginefuncs_t, globalvars_t};
use std::sync::OnceLock;

/// What a module tells metamod about itself, and the hooks it wants.
pub struct MetaDef {
    pub name: &'static CStr,
    pub version: &'static CStr,
    pub date: &'static CStr,
    pub author: &'static CStr,
    pub url: &'static CStr,
    pub logtag: &'static CStr,
    /// Runs at the end of `Meta_Attach` (cvars go here, as in tsx's `OnMetaAttach`).
    pub attach: fn(),
    /// Sets the `DLL_FUNCTIONS` hooks called before / after the game's.
    pub dll: fn(&mut DLL_FUNCTIONS),
    pub dll_post: fn(&mut DLL_FUNCTIONS),
    /// Sets the engine hooks called before / after the engine's.
    pub engine: fn(&mut enginefuncs_t),
    pub engine_post: fn(&mut enginefuncs_t),
}

static DEF: OnceLock<&'static MetaDef> = OnceLock::new();
static INFO: OnceLock<PluginInfo> = OnceLock::new();
static GLOBALS: AtomicPtr<MetaGlobals> = AtomicPtr::new(core::ptr::null_mut());
static GAMEDLL: AtomicPtr<GamedllFuncs> = AtomicPtr::new(core::ptr::null_mut());
static MUTIL: AtomicPtr<MutilFuncs> = AtomicPtr::new(core::ptr::null_mut());

fn plid() -> Plid {
    INFO.get().map_or(core::ptr::null(), |i| i as *const PluginInfo)
}

fn mutil() -> Option<&'static MutilFuncs> {
    // SAFETY: metamod's utility table lives as long as metamod (longer than this plugin).
    unsafe { MUTIL.load(Ordering::Relaxed).as_ref() }
}

/// Parses "major:minor".
fn version(v: &str) -> (i32, i32) {
    let mut it = v.split(':').map(|n| n.trim().parse().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}

/// Whether metamod's interface version `theirs` can run a plugin built for `ours`: same major,
/// and metamod's minor not older (the SDK's rule in `Meta_Query`).
pub fn compatible(theirs: &str, ours: &str) -> bool {
    let ((mm, mn), (pm, pn)) = (version(theirs), version(ours));
    mm == pm && mn >= pn
}

#[doc(hidden)]
pub fn query(def: &'static MetaDef, ifvers: *const c_char, info: *mut *const PluginInfo, mutil: *mut MutilFuncs) -> c_int {
    let _ = DEF.set(def);
    let pi = INFO.get_or_init(|| PluginInfo {
        ifvers: META_INTERFACE_VERSION.as_ptr(),
        name: def.name.as_ptr(),
        version: def.version.as_ptr(),
        date: def.date.as_ptr(),
        author: def.author.as_ptr(),
        url: def.url.as_ptr(),
        logtag: def.logtag.as_ptr(),
        loadable: PT_ANYTIME,
        unloadable: PT_ANYTIME,
    });
    if mutil.is_null() || info.is_null() {
        return 0;
    }
    MUTIL.store(mutil, Ordering::Relaxed);
    // SAFETY: metamod passes a valid out-pointer; the info is a `'static` in a OnceLock.
    unsafe { *info = pi };
    // SAFETY: NULL or metamod's version string.
    let theirs = unsafe { crate::core::cstr_lossy(ifvers) };
    if !compatible(&theirs, &META_INTERFACE_VERSION.to_string_lossy()) {
        log_error(&format!("metamod interface {theirs} can't run this plugin (built for {})", META_INTERFACE_VERSION.to_string_lossy()));
        return 0;
    }
    1
}

#[doc(hidden)]
pub fn attach(table: *mut MetaFunctions, globals: *mut MetaGlobals, gamedll: *mut GamedllFuncs) -> c_int {
    if table.is_null() || globals.is_null() {
        return 0;
    }
    GLOBALS.store(globals, Ordering::Relaxed);
    GAMEDLL.store(gamedll, Ordering::Relaxed);
    // SAFETY: metamod passes its table for this plugin, valid for writes.
    unsafe {
        *table = MetaFunctions {
            get_entity_api: core::ptr::null(),
            get_entity_api_post: core::ptr::null(),
            get_entity_api2: Some(get_entity_api2),
            get_entity_api2_post: Some(get_entity_api2_post),
            get_new_dll_functions: core::ptr::null(),
            get_new_dll_functions_post: core::ptr::null(),
            get_engine_functions: Some(get_engine_functions),
            get_engine_functions_post: Some(get_engine_functions_post),
        }
    };
    if let Some(d) = DEF.get() {
        (d.attach)();
    }
    1
}

#[doc(hidden)]
pub fn give_fnptrs(funcs: *const enginefuncs_t, globals: *mut globalvars_t) {
    if !funcs.is_null() && !globals.is_null() {
        // SAFETY: metamod (or the engine) passes its full engine table and the globals.
        unsafe { engine::set(funcs, globals) };
    }
}

/// Copies the non-NULL entries of `src` into the first `n` pointer slots of `dst`, leaving the
/// others as metamod set them.
///
/// # Safety
/// `dst` holds at least `n` pointer-sized slots; `T` is a table of function pointers.
unsafe fn copy_set<T>(src: &T, dst: *mut T, n: usize) {
    let s = src as *const T as *const usize;
    let d = dst as *mut usize;
    for i in 0..n.min(size_of::<T>() / size_of::<usize>()) {
        // SAFETY: `i` is inside both tables (caller's contract for `dst`).
        unsafe {
            if *s.add(i) != 0 {
                *d.add(i) = *s.add(i);
            }
        }
    }
}

unsafe extern "C" fn dll_table(table: *mut DLL_FUNCTIONS, version: *mut c_int, post: bool) -> c_int {
    if table.is_null() || version.is_null() {
        return 0;
    }
    // SAFETY: metamod passes a valid version pointer.
    if unsafe { *version } != goldsrc_sys::INTERFACE_VERSION as c_int {
        // SAFETY: as above; tells metamod which version this plugin has.
        unsafe { *version = goldsrc_sys::INTERFACE_VERSION as c_int };
        return 0;
    }
    let mut t = DLL_FUNCTIONS::default();
    if let Some(d) = DEF.get() {
        if post { (d.dll_post)(&mut t) } else { (d.dll)(&mut t) }
    }
    // SAFETY: DLL_FUNCTIONS has the same size in every SDK; metamod's table is a full one.
    unsafe { copy_set(&t, table, usize::MAX) };
    1
}

unsafe extern "C" fn get_entity_api2(table: *mut DLL_FUNCTIONS, version: *mut c_int) -> c_int {
    // SAFETY: forwarded metamod arguments.
    unsafe { dll_table(table, version, false) }
}

unsafe extern "C" fn get_entity_api2_post(table: *mut DLL_FUNCTIONS, version: *mut c_int) -> c_int {
    // SAFETY: forwarded metamod arguments.
    unsafe { dll_table(table, version, true) }
}

unsafe extern "C" fn engine_table(table: *mut enginefuncs_t, version: *mut c_int, post: bool) -> c_int {
    if table.is_null() {
        return 0;
    }
    let mut t = enginefuncs_t::default();
    if let Some(d) = DEF.get() {
        if post { (d.engine_post)(&mut t) } else { (d.engine)(&mut t) }
    }
    let _ = version;
    // SAFETY: metamod's engine table has at least the entries every engine has.
    unsafe { copy_set(&t, table, common_engine_entries()) };
    1
}

unsafe extern "C" fn get_engine_functions(table: *mut enginefuncs_t, version: *mut c_int) -> c_int {
    // SAFETY: forwarded metamod arguments.
    unsafe { engine_table(table, version, false) }
}

unsafe extern "C" fn get_engine_functions_post(table: *mut enginefuncs_t, version: *mut c_int) -> c_int {
    // SAFETY: forwarded metamod arguments.
    unsafe { engine_table(table, version, true) }
}

fn globals() -> Option<&'static mut MetaGlobals> {
    // SAFETY: metamod's globals live as long as metamod; hooks run on the main thread, one at a
    // time, and the reference doesn't outlive the call that takes it.
    unsafe { GLOBALS.load(Ordering::Relaxed).as_mut() }
}

/// `SET_META_RESULT(MRES_IGNORED)`: every hook must set a result or metamod complains.
pub fn ignored() {
    if let Some(g) = globals() {
        g.mres = MRES_IGNORED;
    }
}

/// `META_RESULT_ORIG_RET(int)` in a post hook of an int-returning function.
pub fn orig_ret_int() -> Option<i32> {
    let g = globals()?;
    // SAFETY: in a post hook of a function returning int, metamod points `orig_ret` at it.
    (!g.orig_ret.is_null()).then(|| unsafe { *(g.orig_ret as *const c_int) })
}

/// `META_RESULT_ORIG_RET(unsigned short)` (`pfnPrecacheEvent`).
pub fn orig_ret_u16() -> Option<u16> {
    let g = globals()?;
    // SAFETY: as in `orig_ret_int`, for a function returning unsigned short.
    (!g.orig_ret.is_null()).then(|| unsafe { *(g.orig_ret as *const u16) })
}

/// `LOG_ERROR(PLID, ...)` (metamod's log), for problems before AMX Mod X attached.
pub fn log_error(msg: &str) {
    let m = crate::core::c_string(msg);
    if let Some(f) = mutil().and_then(|m| m.log_error) {
        // SAFETY: metamod's variadic logger with "%s".
        unsafe { f(plid(), c"%s".as_ptr(), m.as_ptr()) }
    }
}

/// `GET_GAME_INFO(PLID, GINFO_REALDLL_FULLPATH)`: the path of the game library metamod loaded.
pub fn game_dll_path() -> Option<String> {
    let f = mutil()?.get_game_info?;
    // SAFETY: metamod returns NULL or a string in a static buffer, copied at once.
    let s = unsafe { crate::core::cstr_lossy(f(plid(), GINFO_REALDLL_FULLPATH)) };
    (!s.is_empty()).then_some(s)
}

/// `GET_USER_MSG_ID(PLID, name, &size)`: the id the game registered `name` with.
pub fn user_msg_id(name: &CStr) -> Option<i32> {
    let f = mutil()?.get_user_msg_id?;
    // SAFETY: metamod's lookup; the size out-pointer may be NULL.
    let id = unsafe { f(plid(), name.as_ptr(), core::ptr::null_mut()) };
    (id > 0).then_some(id)
}

fn dllapi() -> Option<&'static DLL_FUNCTIONS> {
    let g = GAMEDLL.load(Ordering::Relaxed);
    // SAFETY: metamod's copy of the game's tables lives as long as the game library.
    unsafe { g.as_ref()?.dllapi_table.as_ref() }
}

/// Calls straight into the game library, as the SDK's `MDLL_*` macros do. The game runs inside
/// these calls (and may send messages, so the module's hooks run too).
pub mod gamedll {
    use super::*;

    /// `MDLL_KeyValue(e, {classname, key, value})`; whether the game handled it.
    pub fn key_value(e: Edict, key: &CStr, value: &str) -> bool {
        let Some(f) = dllapi().and_then(|t| t.pfnKeyValue) else { return false };
        if e.is_null() {
            return false;
        }
        let class = crate::core::c_string(&e.classname());
        let v = crate::core::c_string(value);
        let mut kvd = KeyValueData {
            szClassName: class.as_ptr() as *mut c_char,
            szKeyName: key.as_ptr() as *mut c_char,
            szValue: v.as_ptr() as *mut c_char,
            fHandled: 0,
        };
        // SAFETY: the game's KeyValue with a live edict and a key/value block that outlives
        // the call.
        unsafe { f(e.0, &mut kvd) };
        kvd.fHandled != 0
    }

    /// `MDLL_Spawn(e)`.
    pub fn spawn(e: Edict) {
        if let Some(f) = dllapi().and_then(|t| t.pfnSpawn) {
            // SAFETY: the game's Spawn with a live edict.
            unsafe { f(e.0) };
        }
    }

    /// `MDLL_Use(e, other)`.
    pub fn use_(e: Edict, other: Edict) {
        if let Some(f) = dllapi().and_then(|t| t.pfnUse) {
            // SAFETY: the game's Use with live edicts.
            unsafe { f(e.0, other.0) };
        }
    }

    /// `MDLL_Touch(e, other)`.
    pub fn touch(e: Edict, other: Edict) {
        if let Some(f) = dllapi().and_then(|t| t.pfnTouch) {
            // SAFETY: the game's Touch with live edicts.
            unsafe { f(e.0, other.0) };
        }
    }
}

/// Defines the metamod exports (`Meta_Query`, `Meta_Attach`, `Meta_Detach`, `GiveFnptrsToDll`)
/// for a `static` [`MetaDef`]. Use it in the cdylib crate only: AMX Mod X treats any module
/// exporting `Meta_Attach` as a metamod plugin.
#[macro_export]
macro_rules! export_metamod {
    ($def:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn Meta_Query(
            ifvers: *const ::core::ffi::c_char,
            info: *mut *const $crate::ffi::PluginInfo,
            mutil: *mut $crate::ffi::MutilFuncs,
        ) -> ::core::ffi::c_int {
            $crate::meta::query(&$def, ifvers, info, mutil)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn Meta_Attach(
            _now: ::core::ffi::c_int,
            table: *mut $crate::ffi::MetaFunctions,
            globals: *mut $crate::ffi::MetaGlobals,
            gamedll: *mut $crate::ffi::GamedllFuncs,
        ) -> ::core::ffi::c_int {
            $crate::meta::attach(table, globals, gamedll)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn Meta_Detach(_now: ::core::ffi::c_int, _reason: ::core::ffi::c_int) -> ::core::ffi::c_int {
            1
        }

        /// stdcall on Windows (`WINAPI`); the build script maps the undecorated export name.
        #[unsafe(no_mangle)]
        pub extern "system" fn GiveFnptrsToDll(funcs: *const $crate::sys::enginefuncs_t, globals: *mut $crate::sys::globalvars_t) {
            $crate::meta::give_fnptrs(funcs, globals)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::compatible;

    #[test]
    fn interface_versions() {
        assert!(compatible("5:13", "5:13"));
        assert!(compatible("5:16", "5:13"), "newer metamod minor is backwards compatible");
        assert!(!compatible("5:12", "5:13"), "metamod too old");
        assert!(!compatible("6:0", "5:13"));
    }
}
