// tsmod-amxx-rs: AMX Mod X's TS modules (tsx, tsfun) for the Rust port of The Specialists 3.0.
// A port of AMX Mod X's modules/ts/tsx, Copyright (C) The AMX Mod X Development Team,
// Copyright (C) 2004 Lukasz Wlasinski. GPL-3.0-or-later with the AMX Mod X linking exceptions;
// see LICENSE and GPL-EXCEPTION.txt.

//! TSX: AMX Mod X's stats and TS natives module (library `tsx`, class `xstats`), for servers
//! running the Rust TS server (`the-specialistsmod-rs`). It is a metamod plugin too: it reads
//! the game's user messages and registers `client_damage` / `client_death`.
//!
//! Layout: [`state`] (hook logic), [`msgs`] (message parsing), [`natives`], [`rank`] and
//! [`stats`] (the persistent rank), [`weapons`], [`player`]; [`game`] is the boundary to the
//! engine, the game and the server's plugin API, which `ffi` implements. Only `ffi` uses
//! `unsafe`.

#![deny(unsafe_code)]

pub mod game;
pub mod msgs;
pub mod natives;
pub mod player;
pub mod rank;
pub mod state;
pub mod stats;
pub mod weapons;

#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod testing;

// The FFI layer: exports (`#[no_mangle]` is an unsafe attribute), metamod hooks and the real
// `Game`. Every `unsafe` block there has a `// SAFETY:` comment.
#[allow(unsafe_code)]
mod ffi;
