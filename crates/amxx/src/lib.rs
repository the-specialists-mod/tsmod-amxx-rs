// tsmod-amxx-rs: AMX Mod X's TS modules (tsx, tsfun) for the Rust port of The Specialists 3.0.
// Copyright (C) The AMX Mod X Development Team (the module SDK this crate ports).
// GPL-3.0-or-later with the AMX Mod X linking exceptions; see LICENSE and GPL-EXCEPTION.txt.

//! An AMX Mod X module SDK in Rust: the FFI layer of this workspace.
//!
//! All of the workspace's `unsafe` code that talks to AMX Mod X, metamod or the engine is here,
//! behind safe wrappers. Module crates use them and keep their own `unsafe` to the export glue.
//!
//! * [`module`] / [`export_module!`]: the `AMXX_*` exports and module info.
//! * [`meta`] / [`export_metamod!`]: metamod plugin glue (hook tables, `RETURN_META`, calling the
//!   game library).
//! * [`core`](mod@core): the AMX Mod X core functions (logging, forwards, paths).
//! * [`native`] / [`natives!`]: native tables and their arguments.
//! * [`engine`]: engine functions and entity fields.
//! * [`script`]: the stats modules' score plugin.
//! * [`dl`]: finding the game library's exports; [`tsapi`]: the Rust TS server's plugin API.

pub mod core;
pub mod dl;
pub mod engine;
pub mod ffi;
pub mod meta;
pub mod module;
pub mod native;
pub mod script;
pub mod tsapi;

pub use ffi::Cell;
/// The engine bindings, generated from the FWGS headers by the port's `goldsrc-sys`.
pub use goldsrc_sys as sys;
