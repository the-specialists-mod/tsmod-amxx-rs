// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports
// modules/ts/tsx/NRank.cpp (`stats_Natives`), Copyright (C) The AMX Mod X Development Team,
// Copyright (C) 2004 Lukasz Wlasinski.

//! `stats_Natives` (tsstats.inc, tsx.inc's custom weapon natives).

use super::player_at;
use crate::game::{Event, Game};
use crate::player::{self, PlayerWeapon};
use crate::state::{ignore_bots, with};
use crate::stats::Stats;
use crate::weapons::{FIRST_CUSTOM, MAX_WEAPONS};
use amxx::Cell;
use amxx::native::Args;

/// Writes `stats[8]` (7 counters, plus `extra` at index 7 when given) and `bodyhits[8]`.
fn write_stats(a: &mut dyn Args, stats_arg: usize, s: &Stats, extra: Option<Cell>) {
    a.write_array(stats_arg, 0, &s.cells());
    if let Some(x) = extra {
        a.write_array(stats_arg, 7, &[x]);
    }
    a.write_array(stats_arg + 1, 1, s.body());
}

/// A second index argument in `lo..=maxClients` (`attacker`/`victim` may be 0: the totals).
fn index_at(g: &dyn Game, a: &mut dyn Args, i: usize, lo: Cell, what: &str) -> Option<usize> {
    let v = a.cell(i);
    if v < lo || v > g.max_clients() || v > 32 {
        a.error(&format!("Invalid {what} {v}"));
        return None;
    }
    Some(v as usize)
}

fn weapon_at(a: &mut dyn Args, i: usize) -> Option<usize> {
    let w = a.cell(i);
    if !(0..MAX_WEAPONS as Cell).contains(&w) {
        a.error(&format!("Invalid weapon {w}"));
        return None;
    }
    Some(w as usize)
}

/// `get_user_astats` / `get_user_vstats`: stats against one player (or all, with 0).
fn player_vs(g: &dyn Game, a: &mut dyn Args, what: &str, pick: fn(&crate::player::Player, usize) -> PlayerWeapon) -> Cell {
    let Some(id) = player_at(g, a, 1, "Invalid player {}") else { return 0 };
    let Some(other) = index_at(g, a, 2, 0, what) else { return 0 };
    let Some((pw, name)) = with(|s| {
        let pw = pick(&s.players[id], other);
        (pw, pw.name.and_then(|w| s.weapons.get(w)).map(|w| w.name.clone()))
    }) else {
        return 0;
    };
    if pw.stats.hits == 0 {
        return 0;
    }
    write_stats(a, 3, &pw.stats, None);
    let len = a.cell(6);
    if len != 0
        && other != 0
        && let Some(n) = name
    {
        a.set_string(5, &n, len);
    }
    1
}

/// `get_user_astats(index, wpnindex, stats[8], bodyhits[8], wpnname[] = "", len = 0)`: stats
/// of the attacker (`wpnindex` is the attacker's index, as in upstream).
pub fn get_user_astats(g: &dyn Game, a: &mut dyn Args) -> Cell {
    player_vs(g, a, "attacker", |p, i| p.attackers[i])
}

/// `get_user_vstats(index, victim, stats[8], bodyhits[8], wpnname[] = "", len = 0)`.
pub fn get_user_vstats(g: &dyn Game, a: &mut dyn Args) -> Cell {
    player_vs(g, a, "victim", |p, i| p.victims[i])
}

fn weapon_stats(g: &dyn Game, a: &mut dyn Args, round: bool) -> Cell {
    let Some(id) = player_at(g, a, 1, "Invalid player {}") else { return 0 };
    let Some(w) = weapon_at(a, 2) else { return 0 };
    let Some(s) = with(|s| if round { s.players[id].weapons_rnd[w] } else { s.players[id].weapons[w].stats }) else { return 0 };
    if s.shots == 0 {
        return 0;
    }
    write_stats(a, 3, &s, None);
    1
}

/// `get_user_wrstats(index, wpnindex, stats[8], bodyhits[8])`: this round's stats with a weapon.
pub fn get_user_wrstats(g: &dyn Game, a: &mut dyn Args) -> Cell {
    weapon_stats(g, a, true)
}

/// `get_user_wstats(index, wpnindex, stats[8], bodyhits[8])`: stats with a weapon since
/// connecting (0: all weapons). Like upstream, nothing for a weapon without shots.
pub fn get_user_wstats(g: &dyn Game, a: &mut dyn Args) -> Cell {
    weapon_stats(g, a, false)
}

/// `reset_user_wstats(index)`.
pub fn reset_user_wstats(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = player_at(g, a, 1, "Invalid player {}") else { return 0 };
    with(|s| s.players[id].restart_stats(true));
    1
}

/// `get_user_stats(index, stats[8], bodyhits[8])`: the rank entry; returns the position (also
/// in `stats[7]`).
pub fn get_user_stats(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = player_at(g, a, 1, "Invalid player {}") else { return 0 };
    let Some(Some((st, pos))) = with(|s| {
        let k = s.players[id].rank?;
        Some((s.rank.get(k)?.stats, s.rank.position(k)? as Cell))
    }) else {
        return 0;
    };
    write_stats(a, 2, &st, Some(pos));
    pos
}

/// `get_user_rstats(index, stats[8], bodyhits[8])` (and `get_user_lstats`): stats since the
/// last spawn.
pub fn get_user_rstats(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(id) = player_at(g, a, 1, "Invalid player {}") else { return 0 };
    let Some(Some(life)) = with(|s| s.players[id].rank.map(|_| s.players[id].life)) else { return 0 };
    write_stats(a, 2, &life, None);
    1
}

/// `get_stats(index, stats[8], bodyhits[8], name[], len)`: rank entry `index` (0-based); returns
/// `index + 1` while more entries follow, else 0.
pub fn get_stats(_g: &dyn Game, a: &mut dyn Args) -> Cell {
    let pos = a.cell(1).saturating_add(1);
    let Some(Some((st, name, more))) =
        with(|s| usize::try_from(pos).ok().and_then(|p| s.rank.at(p)).map(|e| (e.stats, e.name.clone(), (pos as usize) < s.rank.len())))
    else {
        return 0;
    };
    write_stats(a, 2, &st, Some(pos));
    let len = a.cell(5);
    a.set_string(4, &name, len);
    if more { pos } else { 0 }
}

/// `get_statsnum()`.
pub fn get_statsnum(_g: &dyn Game, _a: &mut dyn Args) -> Cell {
    with(|s| s.rank.len() as Cell).unwrap_or(0)
}

/// `custom_weapon_add(const wpnname[], melee = 0, const logname[] = "")`: the new id, or 0 when
/// the five slots are taken.
pub fn register_cwpn(_g: &dyn Game, a: &mut dyn Args) -> Cell {
    let name = a.string(1);
    let melee = a.cell(2) != 0;
    let log = if a.count() >= 3 { a.string(3) } else { String::new() };
    with(|s| s.weapons.add_custom(&name, melee, &log)).flatten().map_or(0, |i| i as Cell)
}

/// `custom_weapon_dmg(weapon, att, vic, damage, hitplace = 0)`: records the hit and runs
/// `client_damage`, then, if the victim died, the kill and `client_death`.
pub fn cwpn_dmg(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let weapon = a.cell(1);
    if weapon < FIRST_CUSTOM as Cell || weapon >= MAX_WEAPONS as Cell {
        a.error(&format!("Invalid weapon {weapon}"));
        return 0;
    }
    let Some(att) = index_at(g, a, 2, 1, "attacker") else { return 0 };
    let Some(vic) = index_at(g, a, 3, 1, "victim") else { return 0 };
    let dmg = a.cell(4);
    if dmg < 1 {
        a.error(&format!("Invalid damage {dmg}"));
        return 0;
    }
    let aim = a.cell(5);
    if !(0..=7).contains(&aim) {
        a.error(&format!("Invalid aim {aim}"));
        return 0;
    }
    let w = weapon as usize;
    g.clear_dmg_inflictor(vic as i32);
    with(|s| {
        if !ignore_bots(s, g, att, Some(vic)) {
            player::save_hit(&mut s.players, att, vic, w, dmg, aim as usize);
        }
    });
    let ta = (g.team(vic as i32) == g.team(att as i32) && vic != att) as Cell;
    g.fire(Event::Damage { attacker: att as Cell, victim: vic as Cell, damage: dmg, weapon, aim, ta });
    if g.is_alive(vic as i32) {
        return 1;
    }
    with(|s| {
        if !ignore_bots(s, g, att, Some(vic)) {
            player::save_kill(&mut s.players, att, vic, w, (aim == 1) as Cell, ta);
        }
    });
    g.fire(Event::Death { killer: att as Cell, victim: vic as Cell, weapon, aim, tk: ta });
    1
}

/// `custom_weapon_shot(weapon, index)`.
pub fn cwpn_shot(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let index = a.cell(2);
    if !g.amxx_ingame(index) || !(1..=32).contains(&index) {
        a.error(&format!("Invalid player {index}"));
        return 0;
    }
    let weapon = a.cell(1);
    if weapon < FIRST_CUSTOM as Cell || weapon >= MAX_WEAPONS as Cell {
        a.error(&format!("Invalid weapon {weapon}"));
        return 0;
    }
    with(|s| {
        if !ignore_bots(s, g, index as usize, None) {
            s.players[index as usize].save_shot(weapon as usize);
        }
    });
    1
}
