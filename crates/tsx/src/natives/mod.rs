// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! The natives, as `fn(&dyn Game, &mut dyn Args) -> Cell` so tests can run them against fakes.
//! `crate::ffi` binds them to their upstream names.

pub mod base;
pub mod bot;
pub mod stats;
pub mod world;

use crate::game::{Game, PlayerInfo, TsApi};
use crate::state::with;
use amxx::Cell;
use amxx::native::Args;

/// A player index argument in `1..=maxClients`, else the native error `msg` (with `{}` for the
/// index) and `None`.
fn player_at(g: &dyn Game, a: &mut dyn Args, i: usize, msg: &str) -> Option<usize> {
    let id = a.cell(i);
    if id < 1 || id > g.max_clients() || id > 32 {
        a.error(&msg.replacen("{}", &id.to_string(), 1));
        return None;
    }
    Some(id as usize)
}

/// [`player_at`] for the first argument.
fn player(g: &dyn Game, a: &mut dyn Args, msg: &str) -> Option<usize> {
    player_at(g, a, 1, msg)
}

/// Upstream's `BEGIN_USER_FUNC`: a valid index, with an error (but no stop) when the player
/// isn't in game.
fn user(g: &dyn Game, a: &mut dyn Args) -> Option<usize> {
    let id = player(g, a, "Player {} is not valid")?;
    if !with(|s| s.players[id].ingame).unwrap_or(false) {
        a.error(&format!("Player {id} is not ingame"));
    }
    Some(id)
}

/// The API's view of a player, if the server has the API and the slot holds a player.
fn info(g: &dyn Game, id: usize) -> Option<PlayerInfo> {
    g.api()?.player_info(id as i32).ok()
}

/// Logs `msg` the first time `key` comes up (per server run), so a plugin calling a native every
/// frame doesn't flood the log.
pub(crate) fn log_once(g: &dyn Game, key: &str, msg: impl FnOnce() -> String) {
    let first = with(|s| {
        let first = !s.api_warned.iter().any(|k| k == key);
        if first {
            s.api_warned.push(key.to_owned());
        }
        first
    });
    if first.unwrap_or(false) {
        g.log(&msg());
    }
}

/// Calls an API setter: 1 when it was applied (or queued), 0 otherwise. Upstream raised no
/// native errors here (it wrote whatever it got into `CBasePlayer`), so failures only log, once
/// per native: no API on this server, or a value the server rejects (`INVALID`).
fn api_call(g: &dyn Game, native: &str, f: impl FnOnce(&dyn TsApi) -> Result<(), i32>) -> Cell {
    let Some(api) = g.api() else {
        log_once(g, native, || {
            format!("{native}: this server has no TS plugin API (needs a Rust TS server with ts-plugin-api), so it does nothing")
        });
        return 0;
    };
    match f(api) {
        Ok(()) => 1,
        Err(ts_plugin_api::INVALID) => {
            log_once(g, &format!("{native}:invalid"), || format!("{native}: the server rejected a value as out of range (logged once)"));
            0
        }
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests;
