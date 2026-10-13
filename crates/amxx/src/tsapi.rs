// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Finding the Rust TS server's plugin API (`ts-plugin-api`): the game library exports tier1's
//! `CreateInterface`, which returns the table for `ts_plugin_api::INTERFACE_NAME`
//! (`TSRS_PluginApi003`). The name carries the version, and versions aren't compatible: a server
//! with another version answers NULL, and the module says so.

use core::ffi::{c_char, c_int, c_void};
use ts_plugin_api::PluginApi;

pub use ts_plugin_api as api;

type CreateInterfaceFn = unsafe extern "C" fn(name: *const c_char, rc: *mut c_int) -> *mut c_void;

/// The table this crate was built against (`ts_plugin_api::VERSION`).
pub const SIZE: usize = size_of::<PluginApi>();

/// Looks the table up in the loaded game library (metamod's `GINFO_REALDLL_FULLPATH`, then the
/// already loaded library's `CreateInterface`) and checks that it is exactly the version this
/// crate was built against, at least [`SIZE`] bytes. `Err` says why, for the log.
pub fn find() -> Result<&'static PluginApi, String> {
    let path = crate::meta::game_dll_path().ok_or("metamod didn't name the game library")?;
    let sym = crate::dl::loaded_symbol(&path, c"CreateInterface").ok_or_else(|| format!("{path} exports no CreateInterface"))?;
    // SAFETY: tier1's `CreateInterface(const char*, int*)`, the signature the plugin API's
    // documentation fixes.
    let create = unsafe { core::mem::transmute::<*mut c_void, CreateInterfaceFn>(sym) };
    let mut rc = 1;
    // SAFETY: a C string and an out-pointer for the return code.
    let p = unsafe { create(ts_plugin_api::INTERFACE_NAME.as_ptr(), &mut rc) } as *const PluginApi;
    if p.is_null() {
        return Err(format!(
            "{path} has no {} (a server without the plugin API, or with another version of it)",
            ts_plugin_api::INTERFACE_NAME.to_string_lossy()
        ));
    }
    // SAFETY: a non-NULL table from the server; `size` and `version` are its first fields in
    // every version.
    let (size, version) = unsafe { ((*p).size, (*p).version) };
    if (size as usize) < SIZE || version != ts_plugin_api::VERSION {
        return Err(format!(
            "the server's plugin API is version {version} ({size} bytes); this module needs version {} ({SIZE} bytes)",
            ts_plugin_api::VERSION
        ));
    }
    // SAFETY: the server's static table of this version, at least `SIZE` bytes (checked), living
    // as long as the game library.
    Ok(unsafe { &*p })
}
