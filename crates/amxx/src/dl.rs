// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Finding an export of a library the process has already loaded (the game library), without
//! loading anything new.

use core::ffi::{CStr, c_void};

/// The address of `symbol` in the already loaded library at `path`, or `None` if that library
/// isn't loaded or doesn't export it.
pub fn loaded_symbol(path: &str, symbol: &CStr) -> Option<*mut c_void> {
    imp::loaded_symbol(path, symbol)
}

#[cfg(unix)]
mod imp {
    use super::*;

    pub fn loaded_symbol(path: &str, symbol: &CStr) -> Option<*mut c_void> {
        let p = crate::core::c_string(path);
        // SAFETY: RTLD_NOLOAD only returns a handle to a library that is already loaded (adding a
        // reference), never runs its initializers again.
        let h = unsafe { libc::dlopen(p.as_ptr(), libc::RTLD_NOW | libc::RTLD_NOLOAD) };
        if h.is_null() {
            return None;
        }
        // SAFETY: a valid handle and a C string.
        let s = unsafe { libc::dlsym(h, symbol.as_ptr()) };
        // SAFETY: drops the reference RTLD_NOLOAD added; metamod's own handle keeps it loaded.
        unsafe { libc::dlclose(h) };
        (!s.is_null()).then_some(s)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use core::ffi::c_char;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    pub fn loaded_symbol(path: &str, symbol: &CStr) -> Option<*mut c_void> {
        // GetModuleHandle matches a full path only in the loader's spelling, so fall back to the
        // file name (there is one game library loaded).
        let full = path.replace('/', "\\");
        let name = full.rsplit('\\').next().unwrap_or(&full).to_owned();
        [full, name].iter().find_map(|cand| {
            let c = crate::core::c_string(cand);
            // SAFETY: looks up an already loaded module; doesn't add a reference.
            let h = unsafe { GetModuleHandleA(c.as_ptr()) };
            if h.is_null() {
                return None;
            }
            // SAFETY: a module handle and a C string.
            let s = unsafe { GetProcAddress(h, symbol.as_ptr()) };
            (!s.is_null()).then_some(s)
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    #[test]
    fn finds_nothing_in_a_library_that_isnt_loaded() {
        assert!(super::loaded_symbol("/nonexistent/libnothing.so", c"CreateInterface").is_none());
    }
}
