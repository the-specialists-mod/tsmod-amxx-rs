// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. The forwards and
// `ts_set_fuattack` are declared in AMX Mod X's plugins/include/tsfun.inc, Copyright (C) The
// AMX Mod X Development Team.

//! Port addition: `client_stunt`, `client_powerup` and `Melee_Attack`, which tsfun.inc declares
//! but marks "UNAVAILABLE IN 1.70" (no AMX Mod X module has fired them since). The Rust server's
//! plugin API (version 5) calls tsx back when a stunt starts, a power-up takes effect and a
//! kung-fu move is about to land (`ts_plugin_api::Plugin::player_stunt`, `player_powerup`,
//! `melee_attack`; `ffi::listener` registers for them), and these fire the forwards:
//!
//! * `client_stunt(id, stunttype)`: "when someone stunts, after they do it". `stunttype` is
//!   tsconst.inc's `STUNT_ROLL` 2, `STUNT_DIVE` 3, `STUNT_GETUP` 4 or `STUNT_FLIP` 5, or the
//!   port's 6 (wall jump) and 7 (wall climb), TS 3.0 stunts 2.x lacked (`ts_plugin_api::stunt`).
//! * `client_powerup(id, powerup)`: "when powerups are ran", a `TSPWUP_*` value.
//! * `Melee_Attack(id, Float:time, Float:damage)`: "just before a kung foo attack is done, damage
//!   and time length may be altered with natives. Return PLUGIN_HANDLED to stop attack." The
//!   native is `ts_set_fuattack(id, Float:time, Float:damage)` (commented out in tsfun.inc, so a
//!   plugin declares it itself), valid during the forward.

use crate::game::{Event, Game};
use crate::state::with;

/// A death as the server's `PlayerKilled` scored it (`ts_plugin_api::KillInfo`, plugin API
/// version 5): what the death half of `TSHealth` uses instead of predicting it
/// (`State::kill_scored`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KillReport {
    /// The killing player; the victim for a suicide; 0 for no player.
    pub killer: i32,
    pub victim: i32,
    /// The frags the killer got (-1 a team kill, 0 none).
    pub points: i32,
    /// `ts_plugin_api::kill` bits.
    pub tags: i32,
    /// The killer's spree with this kill.
    pub killer_spree: i32,
    /// The `DeathMsg` weapon name.
    pub weapon_name: String,
}

/// The kung-fu move a `Melee_Attack` forward is about, as `ts_set_fuattack` leaves it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeleeAttack {
    pub player: i32,
    pub time: f32,
    pub damage: f32,
}

/// `client_stunt`.
pub fn stunt(g: &dyn Game, player: i32, stunt: i32) {
    g.fire(Event::Stunt { player, stunt });
}

/// `client_powerup`.
pub fn powerup(g: &dyn Game, player: i32, powerup: i32) {
    g.fire(Event::Powerup { player, powerup });
}

/// `Melee_Attack`: the move's time and damage after the plugins (`ts_set_fuattack`), and whether
/// one returned `PLUGIN_HANDLED`. The state isn't held across the forward; a forward nested in it
/// (a plugin's own attack) keeps its own move.
pub fn melee_attack(g: &dyn Game, player: i32, time: f32, damage: f32) -> (f32, f32, bool) {
    let outer = with(|s| s.melee.replace(MeleeAttack { player, time, damage })).flatten();
    let handled = g.fire(Event::MeleeAttack { player, time, damage }) > 0;
    let m = with(|s| std::mem::replace(&mut s.melee, outer)).flatten();
    let (t, d) = m.map_or((time, damage), |m| (m.time, m.damage));
    (t, d, handled)
}

/// `ts_set_fuattack(id, time, damage)`: changes the move of the `Melee_Attack` forward running
/// for `player`. False when none is.
pub fn set_fuattack(player: i32, time: f32, damage: f32) -> bool {
    with(|s| match &mut s.melee {
        Some(m) if m.player == player => {
            (m.time, m.damage) = (time, damage);
            true
        }
        _ => false,
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeGame;

    #[test]
    fn stunts_and_powerups_fire_their_forwards() {
        let g = FakeGame::new();
        stunt(&g, 3, 2);
        powerup(&g, 4, 256);
        assert_eq!(*g.fired.borrow(), [Event::Stunt { player: 3, stunt: 2 }, Event::Powerup { player: 4, powerup: 256 }]);
    }

    #[test]
    fn melee_attack_takes_the_plugins_changes_and_block() {
        let g = FakeGame::new();
        assert_eq!(melee_attack(&g, 1, 0.35, 20.0), (0.35, 20.0, false), "no plugin");
        // A plugin sets the attack of its player only, and blocks it.
        *g.on_fire.borrow_mut() = Some(Box::new(|_| {
            assert!(!set_fuattack(2, 9.0, 9.0), "another player's");
            assert!(set_fuattack(1, 1.0, 50.0));
        }));
        g.fire_result.set(1);
        assert_eq!(melee_attack(&g, 1, 0.35, 20.0), (1.0, 50.0, true));
        assert_eq!(g.fired.borrow().last(), Some(&Event::MeleeAttack { player: 1, time: 0.35, damage: 20.0 }));
        // Outside the forward the native does nothing.
        assert!(!set_fuattack(1, 1.0, 1.0));
        assert!(with(|s| s.melee.is_none()).unwrap());
    }
}
