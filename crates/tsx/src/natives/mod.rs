// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! The natives, as `fn(&dyn Game, &mut dyn Args) -> Cell` so tests can run them against fakes.
//! `crate::ffi` binds them to their upstream names.

pub mod base;
pub mod stats;

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

/// Calls an API setter: 1 when it was applied (or queued), 0 otherwise. Without the API the
/// native logs that once per map and fails softly; an out-of-range argument is a native error
/// (upstream had none, as it wrote whatever it got).
fn api_call(g: &dyn Game, a: &mut dyn Args, native: &'static str, f: impl FnOnce(&dyn TsApi) -> Result<(), i32>) -> Cell {
    let Some(api) = g.api() else {
        let first = with(|s| {
            let first = !s.api_warned.contains(&native);
            if first {
                s.api_warned.push(native);
            }
            first
        });
        if first.unwrap_or(false) {
            g.log(&format!("{native}: this server has no TS plugin API (needs a Rust TS server with ts-plugin-api), so it does nothing"));
        }
        return 0;
    };
    match f(api) {
        Ok(()) => 1,
        Err(ts_plugin_api::INVALID) => {
            a.error(&format!("{native}: argument out of range"));
            0
        }
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests;
