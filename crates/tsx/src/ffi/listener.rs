// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Port addition: tsx as a listener of the server's plugin API (version 5, `add_listener`). The
//! server calls these back when a player starts a stunt, a power-up takes effect, a kung-fu move
//! is about to land and a death is scored; they fire tsfun.inc's forwards (`crate::forwards`) and
//! finish the death half of `TSHealth` (`State::kill_scored`).

use super::game::RealGame;
use crate::forwards::{self, KillReport};
use crate::game::Game;
use crate::state::with;
use core::ffi::{CStr, c_int};
use core::sync::atomic::{AtomicI32, Ordering};
use ts_plugin_api::{KillInfo, Plugin, PluginApi};

/// The listener's handle, 0 for none.
static HANDLE: AtomicI32 = AtomicI32::new(0);

/// Registers tsx's callbacks with the server (once).
pub fn register(api: &'static PluginApi) {
    if HANDLE.load(Ordering::Relaxed) > 0 {
        return;
    }
    let desc = Plugin {
        name: c"TSX".as_ptr(),
        player_stunt: Some(player_stunt),
        player_powerup: Some(player_powerup),
        melee_attack: Some(melee_attack),
        kill_scored: Some(kill_scored),
        ..Plugin::EMPTY
    };
    // SAFETY: the server copies the descriptor during the call; its callbacks are this
    // library's, which stays loaded until `unregister` (at `AMXX_Detach`).
    let h = unsafe { (api.add_listener)(ts_plugin_api::NO_PLUGIN, &desc) };
    if h > 0 {
        HANDLE.store(h, Ordering::Relaxed);
        with(|s| s.kill_reports = true);
    } else {
        amxx::core::log(&format!(
            "WARNING: the server refused tsx's plugin API listener ({h}); client_stunt, client_powerup and Melee_Attack won't fire"
        ));
    }
}

/// Removes the callbacks (the library may be unloaded next).
pub fn unregister(api: Option<&'static PluginApi>) {
    let h = HANDLE.swap(0, Ordering::Relaxed);
    if let Some(api) = api
        && h > 0
    {
        (api.remove_listener)(h);
    }
    with(|s| s.kill_reports = false);
}

extern "C" fn player_stunt(player: c_int, stunt: c_int) {
    forwards::stunt(&RealGame::new(), player, stunt);
}

extern "C" fn player_powerup(player: c_int, powerup: c_int) {
    forwards::powerup(&RealGame::new(), player, powerup);
}

/// Guard: NULL pointers leave the move alone and only block or not.
unsafe extern "C" fn melee_attack(player: c_int, time: *mut f32, damage: *mut f32) -> c_int {
    if time.is_null() || damage.is_null() {
        return 0;
    }
    // SAFETY: the server passes two floats valid for the call (the API's contract).
    let (t, d) = unsafe { (time.read(), damage.read()) };
    let (t, d, handled) = forwards::melee_attack(&RealGame::new(), player, t, d);
    // SAFETY: as above.
    unsafe {
        time.write(t);
        damage.write(d);
    }
    handled as c_int
}

/// Guard: NULL is ignored.
unsafe extern "C" fn kill_scored(kill: *const KillInfo) {
    if kill.is_null() {
        return;
    }
    // SAFETY: the server passes a `KillInfo` valid for the call (the API's contract).
    let k = unsafe { kill.read_unaligned() };
    let name = CStr::from_bytes_until_nul(name_bytes(&k.weapon_name)).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let report =
        KillReport { killer: k.killer, victim: k.victim, points: k.points, tags: k.tags, killer_spree: k.killer_spree, weapon_name: name };
    let g = RealGame::new();
    if let Some(Some(death)) = with(|s| s.kill_scored(&g, report)) {
        g.fire(death);
    }
}

/// The name's bytes (`c_char` is `i8` or `u8` by target).
fn name_bytes(name: &[core::ffi::c_char]) -> &[u8] {
    // SAFETY: `c_char` and `u8` have the same size and alignment, and every bit pattern is valid
    // for both.
    unsafe { core::slice::from_raw_parts(name.as_ptr().cast::<u8>(), name.len()) }
}
