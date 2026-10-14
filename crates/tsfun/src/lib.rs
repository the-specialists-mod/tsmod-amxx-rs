// tsmod-amxx-rs: AMX Mod X's TS modules (tsx, tsfun) for the Rust port of The Specialists 3.0.
// A port of AMX Mod X's modules/ts/tsfun (moduleconfig.h), Copyright (C) The AMX Mod X
// Development Team. GPL-3.0-or-later with the AMX Mod X linking exceptions; see LICENSE and
// GPL-EXCEPTION.txt.

//! TSFun: an empty module with the library name `tsfun`, so plugins that `#include <tsfun>`
//! (`#pragma reqlib tsfun`) load. Upstream moved every tsfun native into tsx; this is the same
//! wrapper upstream ships.

use amxx::module::{ModuleDef, nothing};

static MODULE: ModuleDef = ModuleDef {
    name: c"TSFun Wrapper",
    author: c"AMX Mod X Dev Team; Rust port: tsmod-amxx-rs",
    version: match core::ffi::CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
        Ok(v) => v,
        Err(_) => c"0",
    },
    logtag: c"TSFUN",
    library: c"tsfun",
    libclass: c"",
    reload: true,
    check_game: None,
    attach: nothing,
    detach: nothing,
    plugins_loaded: nothing,
    plugins_unloading: nothing,
    plugins_unloaded: nothing,
};

// The `AMXX_*` exports (`#[no_mangle]` is an unsafe attribute in Rust 2024).
amxx::export_module!(MODULE);
