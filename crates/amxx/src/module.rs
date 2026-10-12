// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports the AMX Mod X half of
// the module SDK (public/sdk/amxxmodule.cpp: AMXX_Query ... AMXX_PluginsUnloading), Copyright
// (C) The AMX Mod X Development Team.

//! The `AMXX_*` exports every AMX Mod X module has.

use crate::ffi::*;
use core::ffi::{CStr, c_char, c_int};

/// A module's identity and its callbacks.
pub struct ModuleDef {
    pub name: &'static CStr,
    pub author: &'static CStr,
    pub version: &'static CStr,
    pub logtag: &'static CStr,
    /// `#pragma reqlib` name.
    pub library: &'static CStr,
    /// `#pragma reqclass` name, or empty.
    pub libclass: &'static CStr,
    /// Reload on map change (pure AMX Mod X modules only; tsfun sets it, as upstream).
    pub reload: bool,
    /// `AMXX_CheckGame`: whether the module runs on this mod directory. `None` accepts any.
    pub check_game: Option<fn(&str) -> bool>,
    /// After the core functions were requested: add natives here.
    pub attach: fn(),
    pub detach: fn(),
    /// After all plugins loaded: register forwards here.
    pub plugins_loaded: fn(),
    pub plugins_unloading: fn(),
    pub plugins_unloaded: fn(),
}

/// A no-op callback for [`ModuleDef`] fields.
pub fn nothing() {}

/// `AMXX_Query`, for [`export_module!`].
///
/// # Safety
/// NULL or valid pointers, as the core passes them.
#[doc(hidden)]
pub unsafe fn query(def: &ModuleDef, iface: *mut c_int, info: *mut ModuleInfo) -> c_int {
    if iface.is_null() || info.is_null() {
        return AMXX_PARAM;
    }
    // SAFETY: the core passes valid pointers (checked for NULL above).
    unsafe {
        if *iface != AMXX_INTERFACE_VERSION {
            *iface = AMXX_INTERFACE_VERSION;
            return AMXX_IFVERS;
        }
        *info = ModuleInfo {
            name: def.name.as_ptr(),
            author: def.author.as_ptr(),
            version: def.version.as_ptr(),
            reload: def.reload as c_int,
            logtag: def.logtag.as_ptr(),
            library: def.library.as_ptr(),
            libclass: def.libclass.as_ptr(),
        };
    }
    AMXX_OK
}

/// `AMXX_CheckGame`, for [`export_module!`].
///
/// # Safety
/// `game` is NULL or a C string.
#[doc(hidden)]
pub unsafe fn check_game(def: &ModuleDef, game: *const c_char) -> c_int {
    // SAFETY: NULL or the core's mod name.
    let game = unsafe { crate::core::cstr_lossy(game) };
    match def.check_game {
        Some(f) if !f(&game) => AMXX_GAME_BAD,
        _ => AMXX_GAME_OK,
    }
}

/// `AMXX_Attach`, for [`export_module!`].
///
/// # Safety
/// `req` is the core's function lookup.
#[doc(hidden)]
pub unsafe fn attach(def: &ModuleDef, req: Option<ReqFnptr>) -> c_int {
    let Some(req) = req else { return AMXX_PARAM };
    // SAFETY: `req` is the lookup the core passed to AMXX_Attach.
    if unsafe { crate::core::request(req, def.logtag) }.is_err() {
        return AMXX_FUNC_NOT_PRESENT;
    }
    (def.attach)();
    AMXX_OK
}

/// Defines the `AMXX_*` exports for a `static` [`ModuleDef`].
#[macro_export]
macro_rules! export_module {
    ($def:path) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn AMXX_Query(iface: *mut ::core::ffi::c_int, info: *mut $crate::ffi::ModuleInfo) -> ::core::ffi::c_int {
            // SAFETY: the core's arguments, passed through.
            unsafe { $crate::module::query(&$def, iface, info) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn AMXX_CheckGame(game: *const ::core::ffi::c_char) -> ::core::ffi::c_int {
            // SAFETY: the core's argument, passed through.
            unsafe { $crate::module::check_game(&$def, game) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn AMXX_Attach(req: Option<$crate::ffi::ReqFnptr>) -> ::core::ffi::c_int {
            // SAFETY: the core's argument, passed through.
            unsafe { $crate::module::attach(&$def, req) }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn AMXX_Detach() -> ::core::ffi::c_int {
            ($def.detach)();
            $crate::ffi::AMXX_OK
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn AMXX_PluginsLoaded() -> ::core::ffi::c_int {
            ($def.plugins_loaded)();
            $crate::ffi::AMXX_OK
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn AMXX_PluginsUnloading() {
            ($def.plugins_unloading)()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn AMXX_PluginsUnloaded() {
            ($def.plugins_unloaded)()
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    static DEF: ModuleDef = ModuleDef {
        name: c"Test",
        author: c"a",
        version: c"1",
        logtag: c"T",
        library: c"test",
        libclass: c"",
        reload: true,
        check_game: Some(|g| g.eq_ignore_ascii_case("ts")),
        attach: nothing,
        detach: nothing,
        plugins_loaded: nothing,
        plugins_unloading: nothing,
        plugins_unloaded: nothing,
    };

    #[test]
    fn query_checks_the_interface_version() {
        let mut info = ModuleInfo {
            name: core::ptr::null(),
            author: core::ptr::null(),
            version: core::ptr::null(),
            reload: 0,
            logtag: core::ptr::null(),
            library: core::ptr::null(),
            libclass: core::ptr::null(),
        };
        let mut v = 3;
        // SAFETY: valid pointers (or NULL, which `query` checks).
        unsafe {
            assert_eq!(query(&DEF, &mut v, &mut info), AMXX_IFVERS);
            assert_eq!(v, AMXX_INTERFACE_VERSION);
            assert_eq!(query(&DEF, &mut v, &mut info), AMXX_OK);
            assert_eq!(info.reload, 1);
            assert_eq!(query(&DEF, core::ptr::null_mut(), &mut info), AMXX_PARAM);
        }
    }

    #[test]
    fn check_game_is_case_insensitive() {
        // SAFETY: C string literals.
        unsafe {
            assert_eq!(check_game(&DEF, c"TS".as_ptr()), AMXX_GAME_OK);
            assert_eq!(check_game(&DEF, c"cstrike".as_ptr()), AMXX_GAME_BAD);
        }
    }
}
