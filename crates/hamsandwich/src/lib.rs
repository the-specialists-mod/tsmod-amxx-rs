// tsmod-amxx-rs: AMX Mod X modules for the Rust port of The Specialists 3.0.
// A port of AMX Mod X's modules/hamsandwich, Copyright (C) The AMX Mod X Development Team.
// GPL-3.0-or-later with the AMX Mod X linking exceptions; see LICENSE and GPL-EXCEPTION.txt.

//! Ham Sandwich (library `hamsandwich`): hooks and calls entity "virtual functions". Stock patches
//! C++ vtables; the Rust server has none, so this module installs the server's single entity hook
//! (`ts-plugin-api`'s entity hooks, version 3) and dispatches each call to the plugins' forwards per function and
//! class, with stock's semantics.
//!
//! Layout: [`hooklist`] (every `Ham` function and its signature, generated from upstream),
//! [`hooks`] (types and values), [`state`] (registered hooks, calls in progress, the dispatcher),
//! [`natives`], and [`host`], the boundary `ffi` implements. Only `ffi` uses `unsafe`.

#![deny(unsafe_code)]

pub mod hooklist;
pub mod hooks;
pub mod host;
pub mod natives;
pub mod state;

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

// The FFI layer: exports (`#[no_mangle]` is an unsafe attribute), the server hook, AMX Mod X
// forwards and ItemInfo memory. Every `unsafe` block there has a `// SAFETY:` comment.
#[allow(unsafe_code)]
mod ffi;
