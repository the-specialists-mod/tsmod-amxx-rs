// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Finding the Rust TS server's plugin API (`ts-plugin-api`): the game library exports tier1's
//! `CreateInterface`, which returns the table for `TSRS_PluginApi001`.

use core::ffi::{c_char, c_int, c_void};
use ts_plugin_api::PluginApi;

pub use ts_plugin_api as api;

type CreateInterfaceFn = unsafe extern "C" fn(name: *const c_char, rc: *mut c_int) -> *mut c_void;

/// Bytes of the table up to and including the last version-1 field: what a module that only
/// uses version 1 needs.
pub const V1_SIZE: usize = core::mem::offset_of!(PluginApi, ham_set_hook);
/// The whole table this crate was built against (version 2: entity hooks).
pub const V2_SIZE: usize = size_of::<PluginApi>();

/// Looks the table up in the loaded game library (metamod's `GINFO_REALDLL_FULLPATH`, then the
/// already loaded library's `CreateInterface`) and checks that it has at least `min_size` bytes
/// and `min_version`. `Err` says why, for the log.
pub fn find(min_size: usize, min_version: u32) -> Result<&'static PluginApi, String> {
    let path = crate::meta::game_dll_path().ok_or("metamod didn't name the game library")?;
    let sym = crate::dl::loaded_symbol(&path, c"CreateInterface").ok_or_else(|| format!("{path} exports no CreateInterface"))?;
    // SAFETY: tier1's `CreateInterface(const char*, int*)`, the signature the plugin API's
    // documentation fixes.
    let create = unsafe { core::mem::transmute::<*mut c_void, CreateInterfaceFn>(sym) };
    let mut rc = 1;
    // SAFETY: a C string and an out-pointer for the return code.
    let p = unsafe { create(ts_plugin_api::INTERFACE_NAME.as_ptr(), &mut rc) } as *const PluginApi;
    if p.is_null() {
        return Err(format!("{path} has no {}", ts_plugin_api::INTERFACE_NAME.to_string_lossy()));
    }
    // SAFETY: a non-NULL table from the server; `size` and `version` are its first fields in
    // every version.
    let (size, version) = unsafe { ((*p).size, (*p).version) };
    if (size as usize) < min_size || version < min_version {
        return Err(format!("the server's plugin API is too old (version {version}, {size} bytes; needs version {min_version})"));
    }
    // SAFETY: the server's static table, at least `min_size` bytes (checked), living as long as
    // the game library. Fields past `size` must not be read: callers only use the part they
    // asked for.
    Ok(unsafe { &*p })
}
