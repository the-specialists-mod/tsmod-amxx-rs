// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports
// modules/ts/tsx/NBase.cpp (`base_Natives`), Copyright (C) The AMX Mod X Development Team,
// Copyright (C) 2004 Lukasz Wlasinski.

//! `base_Natives`: weapon lookups and the TS player natives (tsx.inc, tsfun.inc).
//!
//! Getters read the server's plugin API where it is authoritative (money, slots, award,
//! power-ups, the weapon in hand) and otherwise the values the game's messages carried, as
//! upstream did. Setters need the API: upstream wrote TS 2.x's `CBasePlayer` at fixed offsets,
//! which the Rust server doesn't have. Without the API they log once and return 0.

use super::{api_call, info, user};
use crate::game::Game;
use crate::state::with;
use crate::weapons::{FIRST_CUSTOM, MAX_WEAPONS};
use amxx::Cell;
use amxx::native::Args;
use ts_plugin_api::powerup;

/// `TSMSG_NORMAL`: the board message of a player without an award. TS 3.0 sends award 0.
const TSMSG_NORMAL: i32 = 6;

fn weapon_text(a: &mut dyn Args, log: bool) -> Cell {
    let id = a.cell(1);
    if !(0..MAX_WEAPONS as Cell).contains(&id) {
        a.error(&format!("Weapon {id} is not valid"));
        return 0;
    }
    let Some(s) = with(|s| s.weapons.get(id as usize).map(|w| if log { w.logname.clone() } else { w.name.clone() })).flatten() else {
        return 0;
    };
    let max = a.cell(3);
    a.set_string(2, &s, max)
}

/// `xmod_get_wpnname(wpnindex, name[], len)`, `get_weaponname`.
pub fn get_weapon_name(_g: &dyn Game, a: &mut dyn Args) -> Cell {
    weapon_text(a, false)
}

/// `xmod_get_wpnlogname(wpnindex, name[], len)`.
pub fn get_weapon_logname(_g: &dyn Game, a: &mut dyn Args) -> Cell {
    weapon_text(a, true)
}

/// `ts_wpnlogtoname(const logname[], name[], len)`.
pub fn wpnlog_to_name(_g: &dyn Game, a: &mut dyn Args) -> Cell {
    let log = a.string(1);
    let Some(name) = with(|s| s.weapons.by_logname(&log).and_then(|i| s.weapons.get(i)).map(|w| w.name.clone())).flatten() else {
        return 0;
    };
    let max = a.cell(3);
    a.set_string(2, &name, max)
}

/// `ts_wpnlogtoid(const logname[])`.
pub fn wpnlog_to_id(_g: &dyn Game, a: &mut dyn Args) -> Cell {
    let log = a.string(1);
    with(|s| s.weapons.by_logname(&log)).flatten().map_or(0, |i| i as Cell)
}

/// `xmod_is_melee_wpn(wpnindex)`.
pub fn is_melee(_g: &dyn Game, a: &mut dyn Args) -> Cell {
    let id = a.cell(1);
    if !(1..MAX_WEAPONS as Cell).contains(&id) {
        a.error(&format!("Weapon {id} is not valid"));
        return 0;
    }
    with(|s| s.weapons.melee(id as usize)).unwrap_or(false) as Cell
}

/// `xmod_get_maxweapons()`.
pub fn get_maxweapons(_g: &dyn Game, _a: &mut dyn Args) -> Cell {
    MAX_WEAPONS as Cell
}

/// `xmod_get_stats_size()`.
pub fn get_stats_size(_g: &dyn Game, _a: &mut dyn Args) -> Cell {
    8
}

/// `xmod_is_custom_wpn(wpnindex)`: any id from the first custom slot on (upstream doesn't check
/// the upper bound either).
pub fn is_custom(_g: &dyn Game, a: &mut dyn Args) -> Cell {
    (a.cell(1) >= FIRST_CUSTOM as Cell) as Cell
}

/// The weapon in hand and its clip, reserve, mode and attachments: the API's when the server
/// has it, else the last `WeaponInfo`.
fn current_weapon(g: &dyn Game, id: usize) -> (Cell, [Cell; 4]) {
    if let Some(i) = info(g, id) {
        let w = with(|s| s.weapons.from_wire(i.weapon)).flatten().map_or(i.weapon, |w| w as Cell);
        return (w, [i.clip, i.ammo, i.fire_mode, i.attachments]);
    }
    with(|s| {
        let p = &s.players[id];
        let w = &p.weapons[p.current];
        (p.current as Cell, [w.clip, w.ammo, w.mode, w.attach])
    })
    .unwrap_or_default()
}

fn ingame(id: usize) -> bool {
    with(|s| s.players[id].ingame).unwrap_or(false)
}

/// Writes the by-reference arguments `first..` that the plugin passed.
fn write_refs(a: &mut dyn Args, first: usize, values: &[Cell]) {
    for (k, &v) in values.iter().enumerate() {
        if a.count() >= first + k {
            a.set_ref(first + k, v);
        }
    }
}

/// `ts_getuserwpn(index, &clip, &ammo, &mode, &extra)`.
pub fn ts_get_user_weapon(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    if !ingame(id) {
        return 0;
    }
    let (w, v) = current_weapon(g, id);
    write_refs(a, 2, &v);
    w
}

/// `get_user_weapon(index, &clip, &ammo)`, the tsx override of the core native.
pub fn get_user_weapon(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    if !ingame(id) {
        return 0;
    }
    let (w, v) = current_weapon(g, id);
    write_refs(a, 2, &v[..2]);
    w
}

/// `ts_getuserslots(index)`: free inventory slots.
pub fn get_user_slots(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    info(g, id).map_or_else(|| with(|s| s.players[id].space).unwrap_or(0), |i| i.free_slots)
}

/// `ts_setuserslots(index, slots)`.
pub fn set_user_slots(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    let v = a.cell(2);
    api_call(g, "ts_setuserslots", |api| api.set_free_slots(id as i32, v))
}

/// `ts_getuserstate(index)`: the stance `TSState` carries.
pub fn get_user_state(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    info(g, id).map_or_else(|| with(|s| s.players[id].state).unwrap_or(0), |i| i.stunt_state)
}

/// `ts_get_message(id)`: the board message (`TSMSG_*`), TS's award byte with "none" as
/// `TSMSG_NORMAL`.
pub fn get_user_message(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    let award = info(g, id).map(|i| i.award).or_else(|| with(|s| s.players[id].award).flatten());
    match award {
        Some(0) => TSMSG_NORMAL,
        Some(v) => v & 15,
        None => 0,
    }
}

/// `ts_set_message(id, message)`.
pub fn set_user_message(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    let m = a.cell(2);
    if !(1..=16).contains(&m) {
        a.error(&format!("Invalid message id: {m}"));
        return 0;
    }
    let award = if m == TSMSG_NORMAL { 0 } else { m };
    api_call(g, "ts_set_message", |api| api.set_award(id as i32, award))
}

/// `ts_set_bullettrail(id, yesorno)`.
pub fn set_bullettrail(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    let on = a.cell(2) != 0;
    api_call(g, "ts_set_bullettrail", |api| api.set_bullet_trail(id as i32, on))
}

/// `ts_set_fakeslowmo(id, Float:time)`: slow motion now, without using up a held power-up.
pub fn set_fake_slowmo(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    let t = a.float(2);
    api_call(g, "ts_set_fakeslowmo", |api| api.run_powerup(id as i32, powerup::SLOWMO, t))
}

/// `ts_set_fakeslowpause(id, Float:time)`.
pub fn set_fake_slowpause(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    let t = a.float(2);
    api_call(g, "ts_set_fakeslowpause", |api| api.run_powerup(id as i32, powerup::SLOWPAUSE, t))
}

/// `ts_is_in_slowmo(id)`: when the player is slowed, the time their power-up ends (a float in
/// the cell, as upstream); 0 otherwise.
pub fn is_in_slowmo(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    match info(g, id) {
        Some(i) if i.time_scale < 1.0 => i.powerup_until.to_bits() as Cell,
        _ => 0,
    }
}

/// `ts_set_speed(id, Float:speed, Float:auradist, Float:time)`.
pub fn set_speed(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    let (speed, aura, time) = (a.float(2), a.float(3), a.float(4));
    api_call(g, "ts_set_speed", |api| api.set_speed(id as i32, speed, aura, time))
}

/// `ts_set_physics_speed(id, Float:speed)`.
pub fn set_physics_speed(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    let speed = a.float(2);
    api_call(g, "ts_set_physics_speed", |api| api.set_physics_speed(id as i32, speed))
}

/// `ts_is_running_powerup(id)`: the running power-up's bits, 0 for none.
pub fn is_running_powerup(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    info(g, id).map_or(0, |i| i.active_powerup)
}

/// The power-ups that run for a time, lowest bit first: what the API's `run_powerup` takes, one
/// at a time (TS 3.0 runs one power-up at a time).
const RUNNABLE: [Cell; 5] = [powerup::SLOWMO, powerup::INFAMMO, powerup::SLOWPAUSE, powerup::DFIRERATE, powerup::SUPERJUMP];

/// Seconds a forced superjump runs. TS 3.0 gives it no running length: a picked-up superjump is
/// a permanent item (`PlayerAddPowerUp` 0x97bc8 sets `mj`), and `SetupPowerUp` (0x98014) has no
/// base time for it, so the API needs one. 3 s is `SetupPowerUp`'s base time (with `pwupmult`
/// 1) of slow motion, infinite clip and double fire rate, the stored timed power-ups.
pub const SUPERJUMP_SECS: f32 = 3.0;

/// `ts_force_run_powerup(id, PWUP_TYPE)`: runs the power-up for its normal time (0 stops it).
/// Upstream wrote any value into TS 2.x's running-power-up field; TS 3.0 runs one power-up at a
/// time, so with several bits set this runs the lowest runnable one and logs once that the
/// others were dropped. Superjump runs for [`SUPERJUMP_SECS`]. A value without a runnable bit
/// does nothing and returns 0 (logged once).
pub fn force_powerup_run(g: &dyn Game, a: &mut dyn Args) -> Cell {
    const NATIVE: &str = "ts_force_run_powerup";
    let Some(id) = user(g, a) else { return 0 };
    let bits = a.cell(2);
    if bits == 0 {
        return api_call(g, NATIVE, |api| api.run_powerup(id as i32, 0, -1.0));
    }
    let Some(run) = RUNNABLE.into_iter().find(|&b| bits & b != 0) else {
        // Nothing TS 3.0 can run (kung fu, grenade, health, armor are instant): the server's
        // `INVALID`, without asking it.
        return api_call(g, NATIVE, |_| Err(ts_plugin_api::INVALID));
    };
    if run != bits && g.api().is_some() {
        super::log_once(g, &format!("{NATIVE}:dropped"), || {
            format!("{NATIVE}: TS 3.0 runs one power-up at a time; ran {run} of {bits} and dropped the other bits (logged once)")
        });
    }
    let secs = if run == powerup::SUPERJUMP { SUPERJUMP_SECS } else { -1.0 };
    api_call(g, NATIVE, |api| api.run_powerup(id as i32, run, secs))
}

/// `ts_has_superjump(id)`.
pub fn has_superjump(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    let has = info(g, id)
        .map_or_else(|| with(|s| s.players[id].items & crate::player::ITEM_SUPERJUMP != 0).unwrap_or(false), |i| i.superjump != 0);
    has as Cell
}

/// `ts_has_fupowerup(id)`: permanent kung fu.
pub fn has_fupowerup(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = user(g, a) else { return 0 };
    let has =
        info(g, id).map_or_else(|| with(|s| s.players[id].items & crate::player::ITEM_KUNGFU != 0).unwrap_or(false), |i| i.kungfu != 0);
    has as Cell
}

/// `ts_setusercash(index, money)`: returns 0 like upstream.
pub fn set_user_cash(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    if ingame(id) {
        let v = a.cell(2);
        if api_call(g, "ts_setusercash", |api| api.set_cash(id as i32, v)) == 1 {
            with(|s| s.players[id].money = v);
        }
    }
    0
}

/// `ts_getusercash(index)`.
pub fn get_user_cash(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    if !ingame(id) {
        return 0;
    }
    info(g, id).map_or_else(|| with(|s| s.players[id].money).unwrap_or(0), |i| i.cash)
}

/// `ts_getuserspace(index)`.
pub fn get_user_space(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    if !ingame(id) {
        return 0;
    }
    info(g, id).map_or_else(|| with(|s| s.players[id].space).unwrap_or(0), |i| i.free_slots)
}

/// `ts_getuserpwup(index, &value)`: the held power-up (tsfun.inc declares only the index; the
/// value is written when a second argument is passed).
pub fn get_user_pwup(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    if !ingame(id) {
        return 0;
    }
    let (t, v) = info(g, id)
        .map(|i| (i.stored_powerup, i.stored_powerup_value))
        .unwrap_or_else(|| with(|s| (s.players[id].pwup, s.players[id].pwup_value)).unwrap_or_default());
    write_refs(a, 2, &[v]);
    t
}

/// `ts_getuseritems(index)`: `TSITEM_KUNGFU | TSITEM_SUPERJUMP`.
pub fn get_user_items(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    if !ingame(id) {
        return 0;
    }
    info(g, id).map_or_else(
        || with(|s| s.players[id].items).unwrap_or(0),
        |i| (i.kungfu != 0) as Cell * crate::player::ITEM_KUNGFU + (i.superjump != 0) as Cell * crate::player::ITEM_SUPERJUMP,
    )
}

/// `ts_getkillingstreak(index)`: TS's own count when the API has it, else tsx's.
pub fn get_killing_streak(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    if !ingame(id) {
        return 0;
    }
    info(g, id).map_or_else(|| with(|s| s.players[id].killing_spree).unwrap_or(0), |i| i.kill_streak)
}

/// `ts_getuserlastfrag(index)`.
pub fn get_last_frag(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    with(|s| if s.players[id].ingame { s.players[id].last_frag } else { 0 }).unwrap_or(0)
}

/// `ts_getuserkillflags(killer)`.
pub fn get_kill_flags(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    with(|s| if s.players[id].ingame { s.players[id].kill_flags } else { 0 }).unwrap_or(0)
}

/// `ts_giveweapon(index, weapon, clips, extra)`.
pub fn give_weapon(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    if !ingame(id) || !g.is_alive(id as i32) {
        return 0;
    }
    g.give_weapon(id as i32, a.cell(2), a.cell(3), a.cell(4)) as Cell
}

/// `ts_createpwup(pwup, Float:origin[3])`: tsfun.inc declares only the type; without an
/// origin the power-up spawns at the world origin.
pub fn create_pwup(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let kind = a.cell(1);
    let origin = if a.count() >= 2 {
        let v = a.read_array(2, 3);
        [f32::from_bits(v[0] as u32), f32::from_bits(v[1] as u32), f32::from_bits(v[2] as u32)]
    } else {
        [0.0; 3]
    };
    g.create_powerup(kind, origin)
}

/// `ts_givepwup(index, pwupent)`: after `ts_createpwup` (and a frame's wait).
pub fn give_pwup(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let ent = a.cell(2);
    if g.is_free(ent) || g.classname(ent) != "ts_powerup" {
        return 0;
    }
    let Some(id) = super::player(g, a, "Invalid player {}") else { return 0 };
    if !ingame(id) || !g.is_alive(id as i32) {
        a.error(&format!("Player {id} is not ingame."));
        return 0;
    }
    g.give_powerup(id as i32, ent);
    1
}

/// `ts_setpddata(knifeoffset)`: kept, unused (the API names a thrown knife's owner).
pub fn ts_setup(_g: &dyn Game, a: &mut dyn Args) -> Cell {
    let v = a.cell(1);
    with(|s| s.knife_offset = v);
    1
}

/// `ts_set_fuattack(id, Float:time, Float:damage)` (tsfun.inc, commented out there: "Alters a fu
/// attack. Use with fu forward"). Port addition: changes the move of the `Melee_Attack` forward
/// running for `id` (`crate::forwards`); 1 if one is, else 0.
pub fn set_fuattack(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    let (time, damage) = (a.float(2), a.float(3));
    crate::forwards::set_fuattack(id as i32, time, damage) as Cell
}

/// `register_statsfwd(ftype)`: forwards are always registered.
pub fn register_forward(_g: &dyn Game, _a: &mut dyn Args) -> Cell {
    1
}

/// `ts_setuserammo(id, weapon, ammo)`, upstream a `tsxaddon.inc` function that wrote TS 2.x's
/// private data (Avalanche, Twilight Suzuka, Harbu). Port addition as a native, with that
/// function's visible behaviour: kung fu (38, or 36 with `tsx_legacy_weaponids`) switches to
/// it and gives 0; other ids outside 0..37 (0..35 with `tsx_legacy_weaponids`: TS 2.x's
/// weapons), 29 (C4, or TS 3.0's thrown knife) and the katana (34) give 0; otherwise it switches
/// to the weapon and sets its reserve, or for the grenade and the knives (24, 25, 35) its clip,
/// through the plugin API.
pub fn set_user_ammo(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = super::player(g, a, "Player {} is not valid") else { return 0 };
    let (weapon, ammo) = (a.cell(2), a.cell(3));
    let (kung_fu, legacy) = with(|s| (s.weapons.kung_fu() as Cell, s.weapons.legacy())).unwrap_or_default();
    if weapon == kung_fu {
        g.client_cmd(id as i32, "weapon_0");
        return 0;
    }
    if !(0..=if legacy { 35 } else { 37 }).contains(&weapon) {
        return 0;
    }
    g.client_cmd(id as i32, &format!("weapon_{weapon}"));
    if weapon == 29 || weapon == 34 {
        return 0;
    }
    let (clip, reserve) = if matches!(weapon, 24 | 25 | 35) { (ammo, -1) } else { (-1, ammo) };
    api_call(g, "ts_setuserammo", |api| api.set_weapon_ammo(id as i32, weapon, clip, reserve))
}
