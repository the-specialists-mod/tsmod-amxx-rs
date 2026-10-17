// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Port addition: tsbot.inc's world natives, the server's per-frame view of players, items,
//! objectives, rules and entities (plugin API versions 4 to 7) for a Pawn bot.
//!
//! The structures go to Pawn as cell arrays in the API's field order (every field is 4 bytes:
//! ints as cells, floats as their bits), indexed by tsbot.inc's `TSPS_*`, `TSINV_*`, `TSITM_*`,
//! `TSOBJ_*`, `TSGS_*`, `TSFI_*`, `TSEI_*` constants; [`layout`] has the same numbers and a test
//! checks them against the API's structs and the include. Weapon ids are the server's (TS 3.0's
//! 1..37, 0 kung fu), not tsx's (`tsx_legacy_weaponids`, kung fu 38). Natives return the API's
//! negative status where something went wrong (no API: `TSBOT_NO_API`); only a bad field index
//! raises a native error.

use super::bot::no_api;
use crate::game::{EntityInfo, FrameInfo, Game, GameState, ItemInfo, ObjectiveInfo, PlayerSnapshot, WeaponEntry};
use amxx::Cell;
use amxx::native::Args;

/// The cell layouts, as tsbot.inc numbers them.
pub mod layout {
    /// `ts_get_player_snapshot`: `PlayerSnapshot`.
    pub const TSPS: &[(&str, usize)] = &[
        ("TSPS_USERID", 0),
        ("TSPS_FLAGS", 1),
        ("TSPS_TEAM", 2),
        ("TSPS_ORIGIN", 3),
        ("TSPS_VELOCITY", 6),
        ("TSPS_VANGLE", 9),
        ("TSPS_VIEWOFS", 12),
        ("TSPS_ABSMIN", 15),
        ("TSPS_ABSMAX", 18),
        ("TSPS_HEALTH", 21),
        ("TSPS_ARMOR", 22),
        ("TSPS_BUTTONS", 23),
        ("TSPS_WEAPON", 24),
        ("TSPS_CLIP", 25),
        ("TSPS_AMMO", 26),
        ("TSPS_FIREMODE", 27),
        ("TSPS_ATTACHMENTS", 28),
        ("TSPS_STANCE", 29),
        ("TSPS_STUNTBITS", 30),
        ("TSPS_ACTIVE_PWUP", 31),
        ("TSPS_STORED_PWUP", 32),
        ("TSPS_PWUP_UNTIL", 33),
        ("TSPS_TIMESCALE", 34),
        ("TSPS_FRAGS", 35),
        ("TSPS_DEATHS", 36),
        ("TSPS_FREESLOTS", 37),
        ("TSPS_INVSERIAL", 38),
        ("TSPS_COMBAT", 39),
        ("TSPS_SIZE", 49),
    ];
    /// `ts_get_player_combat` / `TSPS_COMBAT + n`: `CombatState`.
    pub const TSCOMBAT: &[(&str, usize)] = &[
        ("TSCOMBAT_MELEE_POWER", 0),
        ("TSCOMBAT_NEXT_ATTACK", 1),
        ("TSCOMBAT_NEXT_STUNT", 2),
        ("TSCOMBAT_RECOVER_UNTIL", 3),
        ("TSCOMBAT_PRONE_LOCK", 4),
        ("TSCOMBAT_DODGE_UNTIL", 5),
        ("TSCOMBAT_BLOCK_KUNGFU", 6),
        ("TSCOMBAT_BLOCK_KNIFE", 7),
        ("TSCOMBAT_BLOCK_KATANA", 8),
        ("TSCOMBAT_MELEE_WINDOW", 9),
        ("TSCOMBAT_SIZE", 10),
    ];
    /// `ts_get_inventory_ex`: `WeaponEntry`.
    pub const TSINV: &[(&str, usize)] =
        &[("TSINV_WEAPON", 0), ("TSINV_CLIP", 1), ("TSINV_AMMO", 2), ("TSINV_FIREMODE", 3), ("TSINV_ATTACHMENTS", 4), ("TSINV_SIZE", 5)];
    /// `ts_get_items`: `ItemInfo`.
    pub const TSITM: &[(&str, usize)] = &[
        ("TSITM_ENT", 0),
        ("TSITM_SERIAL", 1),
        ("TSITM_KIND", 2),
        ("TSITM_WEAPON", 3),
        ("TSITM_POWERUP", 4),
        ("TSITM_ORIGIN", 5),
        ("TSITM_AVAILABLE", 8),
        ("TSITM_RESPAWN_AT", 9),
        ("TSITM_SIZE", 10),
    ];
    /// `ts_get_objectives`: `ObjectiveInfo` without its name.
    pub const TSOBJ: &[(&str, usize)] =
        &[("TSOBJ_ID", 0), ("TSOBJ_ENT", 1), ("TSOBJ_ORIGIN", 2), ("TSOBJ_ACTIVE", 5), ("TSOBJ_SIZE", 6), ("TSOBJ_NAME_LEN", 64)];
    /// `ts_get_gamestate`: `GameState`.
    pub const TSGS: &[(&str, usize)] = &[
        ("TSGS_MODE", 0),
        ("TSGS_TEAMPLAY", 1),
        ("TSGS_LMS", 2),
        ("TSGS_THEONE", 3),
        ("TSGS_CTB", 4),
        ("TSGS_TEAMS", 5),
        ("TSGS_ROUND", 6),
        ("TSGS_ROUND_ENDS", 7),
        ("TSGS_ROUND_RESTART", 8),
        ("TSGS_GAMEOVER", 9),
        ("TSGS_TIME", 10),
        ("TSGS_MAXCLIENTS", 11),
        ("TSGS_SCORES", 12),
        ("TSGS_SIZE", 16),
    ];
    /// `ts_get_frameinfo`: `FrameInfo`.
    pub const TSFI: &[(&str, usize)] = &[
        ("TSFI_TIME", 0),
        ("TSFI_FRAMETIME", 1),
        ("TSFI_HIGHEST_EDICT", 2),
        ("TSFI_TEAMPLAY", 3),
        ("TSFI_CTB", 4),
        ("TSFI_MODE", 5),
        ("TSFI_ROUND", 6),
        ("TSFI_SCORES", 7),
        ("TSFI_SIZE", 11),
    ];
    /// `ts_get_entity_info`: `EntityInfo`.
    pub const TSEI: &[(&str, usize)] = &[
        ("TSEI_KIND", 0),
        ("TSEI_WEAPON", 1),
        ("TSEI_POWERUP", 2),
        ("TSEI_OWNER", 3),
        ("TSEI_TEAM", 4),
        ("TSEI_AVAILABLE", 5),
        ("TSEI_SIZE", 6),
    ];
    /// `ts_bot_get_result`: `BotCmdResult`.
    pub const TSRES: &[(&str, usize)] =
        &[("TSRES_VANGLE", 0), ("TSRES_ORIGIN", 3), ("TSRES_VELOCITY", 6), ("TSRES_EVENTS", 9), ("TSRES_MSEC", 10), ("TSRES_SIZE", 11)];

    /// The size entry of a layout.
    pub const fn size(l: &[(&str, usize)]) -> usize {
        let mut i = 0;
        while i < l.len() {
            let b = l[i].0.as_bytes();
            if b.len() > 5 && b[b.len() - 4] == b'S' && b[b.len() - 3] == b'I' && b[b.len() - 2] == b'Z' && b[b.len() - 1] == b'E' {
                return l[i].1;
            }
            i += 1;
        }
        0
    }
}

pub const TSPS_SIZE: usize = layout::size(layout::TSPS);
pub const TSCOMBAT_SIZE: usize = layout::size(layout::TSCOMBAT);
pub const TSINV_SIZE: usize = layout::size(layout::TSINV);
pub const TSITM_SIZE: usize = layout::size(layout::TSITM);
pub const TSOBJ_SIZE: usize = layout::size(layout::TSOBJ);
pub const TSGS_SIZE: usize = layout::size(layout::TSGS);
pub const TSFI_SIZE: usize = layout::size(layout::TSFI);
pub const TSEI_SIZE: usize = layout::size(layout::TSEI);

/// A float as Pawn's `Float:` cell.
pub fn fc(f: f32) -> Cell {
    f.to_bits() as Cell
}

fn v3(v: [f32; 3]) -> [Cell; 3] {
    v.map(fc)
}

/// `PlayerSnapshot` as `TSPS_*` cells.
pub fn snapshot_cells(s: &PlayerSnapshot) -> Vec<Cell> {
    let c = &s.combat;
    let mut v = vec![s.user_id, s.flags, s.team];
    for x in [s.origin, s.velocity, s.v_angle, s.view_ofs, s.absmin, s.absmax] {
        v.extend(v3(x));
    }
    v.extend([fc(s.health), fc(s.armor), s.buttons, s.weapon, s.clip, s.ammo, s.fire_mode, s.attachments, s.stance, s.stunt_bits]);
    v.extend([
        s.active_powerup,
        s.stored_powerup,
        fc(s.powerup_until),
        fc(s.time_scale),
        s.frags,
        s.deaths,
        s.free_slots,
        s.inventory_serial,
    ]);
    v.extend(
        [
            c.melee_power,
            c.next_attack_at,
            c.next_stunt_at,
            c.recover_until,
            c.prone_lock,
            c.dodge_until,
            c.block_kungfu,
            c.block_knife,
            c.block_katana,
            c.melee_window,
        ]
        .map(fc),
    );
    v
}

pub fn weapon_cells(w: &WeaponEntry) -> [Cell; TSINV_SIZE] {
    [w.weapon, w.clip, w.ammo, w.fire_mode, w.attachments]
}

pub fn item_cells(i: &ItemInfo) -> Vec<Cell> {
    let mut v = vec![i.index, i.serial, i.kind, i.weapon, i.powerup];
    v.extend(v3(i.origin));
    v.extend([i.available, fc(i.respawn_at)]);
    v
}

pub fn objective_cells(o: &ObjectiveInfo) -> Vec<Cell> {
    let mut v = vec![o.id, o.entity];
    v.extend(v3(o.origin));
    v.push(o.active);
    v
}

pub fn gamestate_cells(g: &GameState) -> Vec<Cell> {
    let mut v =
        vec![g.mode, g.teamplay, g.last_man_standing, g.the_one, g.ctb, g.teams, g.round, fc(g.round_ends_at), fc(g.round_restart_at)];
    v.extend([g.game_over, fc(g.time), g.max_clients]);
    v.extend(g.team_scores);
    v
}

pub fn frameinfo_cells(f: &FrameInfo) -> Vec<Cell> {
    let mut v = vec![fc(f.time), fc(f.frametime), f.highest_edict, f.teamplay, f.ctb, f.mode, f.round];
    v.extend(f.team_scores);
    v
}

pub fn entity_cells(e: &EntityInfo) -> [Cell; TSEI_SIZE] {
    [e.kind, e.weapon, e.powerup, e.owner, e.team, e.available]
}

/// The objective's name (NUL-terminated `c_char`s) as text.
fn objective_name(o: &ObjectiveInfo) -> String {
    let bytes: Vec<u8> = o.name.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Writes `v` to the by-reference argument `i` if the plugin passed it.
pub(super) fn out(a: &mut dyn Args, i: usize, v: Cell) {
    if a.count() >= i {
        a.set_ref(i, v);
    }
}

/// Writes up to `len` cells of `v` to the array argument `i` (when passed).
pub(super) fn out_array(a: &mut dyn Args, i: usize, v: &[Cell], len: usize) {
    if a.count() >= i {
        a.write_array(i, 0, &v[..v.len().min(len)]);
    }
}

/// A length argument (`max`, `len`): `def` when not passed, never below 0.
fn len_arg(a: &dyn Args, i: usize, def: usize) -> usize {
    if a.count() >= i { a.cell(i).max(0) as usize } else { def }
}

fn snapshot(g: &dyn Game, a: &dyn Args) -> Option<PlayerSnapshot> {
    g.api()?.snapshot(a.cell(1))
}

/// `ts_get_player_snapshot(id, data[], len = TSPS_SIZE)`: the frame's snapshot of player `id`
/// (all zeroes for an empty slot); returns its user id, 0 for an empty slot or no API.
pub fn get_player_snapshot(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let s = snapshot(g, a).unwrap_or_default();
    let len = len_arg(a, 3, TSPS_SIZE).min(TSPS_SIZE);
    out_array(a, 2, &snapshot_cells(&s), len);
    s.user_id
}

/// `any:ts_get_player_field(id, field)`: one `TSPS_*` cell (floats as `Float:`); 0 for an empty
/// slot. A field outside `0..TSPS_SIZE` is a native error.
pub fn get_player_field(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let f = a.cell(2);
    if !(0..TSPS_SIZE as Cell).contains(&f) {
        a.error(&format!("Invalid snapshot field {f}"));
        return 0;
    }
    snapshot(g, a).map_or(0, |s| snapshot_cells(&s)[f as usize])
}

/// `ts_get_player_flags(id)`: `TSPF_*` bits, 0 for an empty slot.
pub fn get_player_flags(g: &dyn Game, a: &mut dyn Args) -> Cell {
    snapshot(g, a).map_or(0, |s| s.flags)
}

/// `ts_get_player_team(id)`: the rules team (1.. in teamplay), 0 for none.
pub fn get_player_team(g: &dyn Game, a: &mut dyn Args) -> Cell {
    snapshot(g, a).map_or(0, |s| s.team)
}

/// `Float:ts_get_player_health(id, &Float:armor = 0.0)`.
pub fn get_player_health(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let s = snapshot(g, a).unwrap_or_default();
    out(a, 2, fc(s.armor));
    fc(s.health)
}

/// `ts_get_player_stance(id, &stunt_bits = 0)`: `TSState` (0 stand, 1 crouch, 2 prone or
/// diving), -1 for an empty slot.
pub fn get_player_stance(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(s) = snapshot(g, a) else {
        out(a, 2, 0);
        return -1;
    };
    out(a, 2, s.stunt_bits);
    s.stance
}

/// `ts_get_player_weapon(id, &clip = 0, &ammo = 0, &fire_mode = 0, &attachments = 0)`: the TS id
/// in hand (0 kung fu, whose "clip" is the kung-fu bar), -1 for an empty slot.
pub fn get_player_weapon(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let s = snapshot(g, a);
    let v = s.map_or([0; 4], |s| [s.clip, s.ammo, s.fire_mode, s.attachments]);
    for (k, x) in v.into_iter().enumerate() {
        out(a, 2 + k, x);
    }
    s.map_or(-1, |s| s.weapon)
}

/// `ts_get_player_powerup(id, &stored = 0, &Float:until = 0.0)`: the running power-up's bits.
pub fn get_player_powerup(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let s = snapshot(g, a).unwrap_or_default();
    out(a, 2, s.stored_powerup);
    out(a, 3, fc(s.powerup_until));
    s.active_powerup
}

/// `Float:ts_get_player_timescale(id)`: 1.0 normal, below slowed; 0.0 for an empty slot.
pub fn get_player_timescale(g: &dyn Game, a: &mut dyn Args) -> Cell {
    fc(snapshot(g, a).map_or(0.0, |s| s.time_scale))
}

/// `ts_get_player_combat(id, Float:combat[TSCOMBAT_SIZE])`: 1, or 0 (zeroes) for an empty slot.
pub fn get_player_combat(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let s = snapshot(g, a);
    let cells = snapshot_cells(&s.unwrap_or_default());
    out_array(a, 2, &cells[TSPS_SIZE - TSCOMBAT_SIZE..], TSCOMBAT_SIZE);
    s.is_some() as Cell
}

/// `ts_get_inventory(id, weapons[], clips[], ammo[], max)`: how many weapons the player owns (may
/// be more than `max`; kung fu isn't one), or a status.
pub fn get_inventory(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(api) = g.api() else { return no_api() };
    let list = match api.inventory(a.cell(1)) {
        Ok(l) => l,
        Err(rc) => return rc,
    };
    let max = len_arg(a, 5, 0).min(list.len());
    for (k, pick) in [|w: &WeaponEntry| w.weapon, |w: &WeaponEntry| w.clip, |w: &WeaponEntry| w.ammo].into_iter().enumerate() {
        let v: Vec<Cell> = list[..max].iter().map(pick).collect();
        out_array(a, 2 + k, &v, max);
    }
    list.len() as Cell
}

/// `ts_get_inventory_ex(id, data[], max)`: the weapons as `TSINV_SIZE` cells each (`max`
/// entries); how many the player owns, or a status.
pub fn get_inventory_ex(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(api) = g.api() else { return no_api() };
    let list = match api.inventory(a.cell(1)) {
        Ok(l) => l,
        Err(rc) => return rc,
    };
    let max = len_arg(a, 3, 0).min(list.len());
    let v: Vec<Cell> = list[..max].iter().flat_map(weapon_cells).collect();
    out_array(a, 2, &v, v.len());
    list.len() as Cell
}

/// `ts_get_items(data[], max)`: the map's items as `TSITM_SIZE` cells each; how many there are.
pub fn get_items(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(api) = g.api() else { return no_api() };
    let list = api.items();
    let max = len_arg(a, 2, 0).min(list.len());
    let v: Vec<Cell> = list[..max].iter().flat_map(item_cells).collect();
    out_array(a, 1, &v, v.len());
    list.len() as Cell
}

fn objectives(g: &dyn Game, player: Cell) -> Result<Vec<ObjectiveInfo>, Cell> {
    g.api().ok_or_else(no_api)?.objectives(player)
}

/// `ts_get_objectives(id, data[], max)`: the HUD's objectives as `TSOBJ_SIZE` cells each (names
/// with [`get_objective_name`]); how many there are, or a status. `id` 0 asks for no team.
pub fn get_objectives(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let list = match objectives(g, a.cell(1)) {
        Ok(l) => l,
        Err(rc) => return rc,
    };
    let max = len_arg(a, 3, 0).min(list.len());
    let v: Vec<Cell> = list[..max].iter().flat_map(objective_cells).collect();
    out_array(a, 2, &v, v.len());
    list.len() as Cell
}

/// `ts_get_objective_name(id, index, name[], len)`: the text `id`'s team reads for objective
/// `index` of the list; its length, or a status (`TSBOT_INVALID` for an index past the list).
pub fn get_objective_name(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let list = match objectives(g, a.cell(1)) {
        Ok(l) => l,
        Err(rc) => return rc,
    };
    let Some(o) = usize::try_from(a.cell(2)).ok().and_then(|i| list.get(i)) else { return ts_plugin_api::INVALID };
    let name = objective_name(o);
    let len = a.cell(4);
    a.set_string(3, &name, len);
    name.len() as Cell
}

/// `ts_get_gamestate(data[TSGS_SIZE])`: 1, or a status.
pub fn get_gamestate(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(api) = g.api() else { return no_api() };
    out_array(a, 1, &gamestate_cells(&api.game_state()), TSGS_SIZE);
    1
}

/// `ts_get_frameinfo(data[TSFI_SIZE])`: 1, or a status.
pub fn get_frameinfo(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(api) = g.api() else { return no_api() };
    out_array(a, 1, &frameinfo_cells(&api.frame_info()), TSFI_SIZE);
    1
}

/// `ts_entity_key(ent, const key[], out[], len)`: a map key the server keeps outside `entvars_t`
/// (a door's `master`); its length, or a status (`TSBOT_UNSUPPORTED`: the entity has no such key;
/// read entvars fields with fakemeta).
pub fn entity_key(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(api) = g.api() else { return no_api() };
    let key = a.string(2);
    let len = a.cell(4);
    match api.entity_key(a.cell(1), &key) {
        Ok(v) => {
            a.set_string(3, &v, len);
            v.len() as Cell
        }
        Err(rc) => {
            a.set_string(3, "", len);
            rc
        }
    }
}

/// `ts_get_entity_info(ent, data[TSEI_SIZE])`: `TSBOT_OK` or a status (`TSBOT_NOT_PLAYER`: not a
/// game entity).
pub fn get_entity_info(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(api) = g.api() else { return no_api() };
    match api.entity_info(a.cell(1)) {
        Ok(e) => {
            out_array(a, 2, &entity_cells(&e), TSEI_SIZE);
            ts_plugin_api::OK
        }
        Err(rc) => rc,
    }
}

/// `ts_master_triggered(const master[], activator = 0)`: 1 if the `master` entities let
/// `activator` through (an empty name is 1), 0 if not, or a status.
pub fn master_triggered(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(api) = g.api() else { return no_api() };
    let m = a.string(1);
    let activator = if a.count() >= 2 { a.cell(2) } else { 0 };
    api.master_triggered(&m, activator) as Cell
}

/// `ts_projectile_owner(ent)`: the player responsible for a thrown knife, grenade or placed
/// charge; 0 for none, or a status.
pub fn projectile_owner(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(api) = g.api() else { return no_api() };
    api.projectile_owner(a.cell(1)).unwrap_or_else(|rc| rc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeGame;
    use amxx::native::FakeArgs;
    use core::mem::offset_of;
    use ts_plugin_api::{BotCmdResult, CombatState};

    /// The include's `#define NAME value` lines (decimal or hex).
    fn include_defines() -> std::collections::HashMap<String, i64> {
        let text = include_str!("../../../../include/tsbot.inc");
        let mut m = std::collections::HashMap::new();
        for l in text.lines() {
            let mut it = l.split_whitespace();
            if it.next() != Some("#define") {
                continue;
            }
            let (Some(name), Some(v)) = (it.next(), it.next()) else { continue };
            let v = v.trim_start_matches('(').trim_end_matches(')');
            let parsed = if let Some(h) = v.strip_prefix("0x") { i64::from_str_radix(h, 16).ok() } else { v.parse().ok() };
            if let Some(p) = parsed {
                m.insert(name.to_owned(), p);
            }
        }
        m
    }

    #[test]
    fn layouts_match_the_structs_and_the_include() {
        let inc = include_defines();
        let check = |l: &[(&str, usize)], offsets: &[usize], size: usize| {
            let mut fields: Vec<usize> = l.iter().filter(|(n, _)| !n.ends_with("_SIZE") && !n.ends_with("_LEN")).map(|e| e.1).collect();
            fields.sort();
            let mut o: Vec<usize> = offsets.iter().map(|b| b / 4).collect();
            o.dedup();
            assert!(fields.iter().all(|f| o.contains(f)), "{l:?} vs {o:?}");
            assert_eq!(layout::size(l) * 4, size, "{l:?}");
            for (n, v) in l {
                assert_eq!(inc.get(*n).copied(), Some(*v as i64), "tsbot.inc's {n}");
            }
        };
        use ts_plugin_api::{FrameInfo as F, GameState as G, ItemInfo as I, PlayerSnapshot as P};
        check(
            layout::TSPS,
            &[
                offset_of!(P, user_id),
                offset_of!(P, flags),
                offset_of!(P, team),
                offset_of!(P, origin),
                offset_of!(P, velocity),
                offset_of!(P, v_angle),
                offset_of!(P, view_ofs),
                offset_of!(P, absmin),
                offset_of!(P, absmax),
                offset_of!(P, health),
                offset_of!(P, armor),
                offset_of!(P, buttons),
                offset_of!(P, weapon),
                offset_of!(P, clip),
                offset_of!(P, ammo),
                offset_of!(P, fire_mode),
                offset_of!(P, attachments),
                offset_of!(P, stance),
                offset_of!(P, stunt_bits),
                offset_of!(P, active_powerup),
                offset_of!(P, stored_powerup),
                offset_of!(P, powerup_until),
                offset_of!(P, time_scale),
                offset_of!(P, frags),
                offset_of!(P, deaths),
                offset_of!(P, free_slots),
                offset_of!(P, inventory_serial),
                offset_of!(P, combat),
            ],
            size_of::<P>(),
        );
        check(layout::TSCOMBAT, &(0..10).map(|i| i * 4).collect::<Vec<_>>(), size_of::<CombatState>());
        check(layout::TSINV, &(0..5).map(|i| i * 4).collect::<Vec<_>>(), size_of::<WeaponEntry>());
        check(
            layout::TSITM,
            &[
                offset_of!(I, index),
                offset_of!(I, serial),
                offset_of!(I, kind),
                offset_of!(I, weapon),
                offset_of!(I, powerup),
                offset_of!(I, origin),
                offset_of!(I, available),
                offset_of!(I, respawn_at),
            ],
            size_of::<I>(),
        );
        check(
            layout::TSGS,
            &[
                offset_of!(G, mode),
                offset_of!(G, teamplay),
                offset_of!(G, last_man_standing),
                offset_of!(G, the_one),
                offset_of!(G, ctb),
                offset_of!(G, teams),
                offset_of!(G, round),
                offset_of!(G, round_ends_at),
                offset_of!(G, round_restart_at),
                offset_of!(G, game_over),
                offset_of!(G, time),
                offset_of!(G, max_clients),
                offset_of!(G, team_scores),
            ],
            size_of::<G>(),
        );
        check(
            layout::TSFI,
            &[
                offset_of!(F, time),
                offset_of!(F, frametime),
                offset_of!(F, highest_edict),
                offset_of!(F, teamplay),
                offset_of!(F, ctb),
                offset_of!(F, mode),
                offset_of!(F, round),
                offset_of!(F, team_scores),
            ],
            size_of::<F>(),
        );
        check(layout::TSEI, &(0..6).map(|i| i * 4).collect::<Vec<_>>(), size_of::<EntityInfo>());
        check(layout::TSRES, &[0, 12, 24, 36, 40], size_of::<BotCmdResult>());
        // The objective entry is the struct up to its name.
        assert_eq!(TSOBJ_SIZE * 4, offset_of!(ObjectiveInfo, name));
        assert_eq!(ts_plugin_api::OBJECTIVE_NAME_LEN, 64);
    }

    #[test]
    fn constants_match_the_api() {
        use crate::bots::status;
        use ts_plugin_api::{cmd, cmd_event, entity, item, log, mode, player_flag as pf, round, stunt_bits as sb};
        let inc = include_defines();
        let want: &[(&str, i32)] = &[
            ("TSBOT_OK", ts_plugin_api::OK),
            ("TSBOT_DEFERRED", ts_plugin_api::DEFERRED),
            ("TSBOT_NOT_PLAYER", ts_plugin_api::NOT_PLAYER),
            ("TSBOT_INVALID", ts_plugin_api::INVALID),
            ("TSBOT_UNSUPPORTED", ts_plugin_api::UNSUPPORTED),
            ("TSBOT_BUSY", ts_plugin_api::BUSY),
            ("TSBOT_FULL", ts_plugin_api::FULL),
            ("TSBOT_EXISTS", ts_plugin_api::EXISTS),
            ("TSBOT_NO_API", status::NO_API),
            ("TSBOT_NO_BACKEND", status::NO_BACKEND),
            ("TSBOT_ALREADY", status::ALREADY),
            ("TSBOT_NO_SLOT", status::NO_SLOT),
            ("TSBOT_SETVIEW", cmd::SET_VIEW),
            ("TSBOT_AUTOMSEC", cmd::AUTO_MSEC),
            ("TSBOT_EV_TELEPORTED", cmd_event::TELEPORTED),
            ("TSBOT_EV_FIXANGLE", cmd_event::FIXANGLE),
            ("TSBOT_EV_LANDED", cmd_event::LANDED),
            ("TSPF_JOINED", pf::JOINED),
            ("TSPF_ALIVE", pf::ALIVE),
            ("TSPF_BOT", pf::BOT),
            ("TSPF_ON_GROUND", pf::ON_GROUND),
            ("TSPF_DUCKING", pf::DUCKING),
            ("TSPF_ON_LADDER", pf::ON_LADDER),
            ("TSPF_IN_WATER", pf::IN_WATER),
            ("TSPF_HAS_FLAG", pf::HAS_FLAG),
            ("TSPF_SPECTATOR", pf::SPECTATOR),
            ("TSPF_PRONE", pf::PRONE),
            ("TSPF_STUNTING", pf::STUNTING),
            ("TSPF_KUNG_FU", pf::KUNG_FU),
            ("TSPF_SUPERJUMP", pf::SUPERJUMP),
            ("TSSB_RECOVER", sb::RECOVER),
            ("TSSB_DEAD_LOCK", sb::DEAD_LOCK),
            ("TSSB_GO_DUCK", sb::GO_DUCK),
            ("TSSB_STAND_UP", sb::STAND_UP),
            ("TSSB_DIVING", sb::DIVING),
            ("TSSB_PRONE", sb::PRONE),
            ("TSSB_STEALTH_SHOES", sb::STEALTH_SHOES),
            ("TSSB_FLIP_HIGH", sb::FLIP_HIGH),
            ("TSSB_FLIP_LOW", sb::FLIP_LOW),
            ("TSSB_WALLJUMPING", sb::WALLJUMPING),
            ("TSSB_WALLCLIMBING", sb::WALLCLIMBING),
            ("TSSB_ROLLING", sb::ROLLING),
            ("TSSB_SUPERJUMP_GLIDE", sb::SUPERJUMP_GLIDE),
            ("TSSB_WALLDIVE", sb::WALLDIVE),
            ("TSSB_STUNTING", sb::STUNTING),
            ("TSENT_OTHER", entity::OTHER),
            ("TSENT_PLAYER", entity::PLAYER),
            ("TSENT_GROUND_WEAPON", entity::GROUND_WEAPON),
            ("TSENT_POWERUP", entity::POWERUP),
            ("TSENT_PROJECTILE", entity::PROJECTILE),
            ("TSENT_GUN", entity::GUN),
            ("TSENT_CTB_FLAG", entity::CTB_FLAG),
            ("TSENT_CTB_BASE", entity::CTB_BASE),
            ("TSENT_OBJECTIVE", entity::OBJECTIVE),
            ("TSMODE_DEATHMATCH", mode::DEATHMATCH),
            ("TSMODE_TEAMDM", mode::TEAM_DEATHMATCH),
            ("TSMODE_ROUND_TEAMDM", mode::ROUND_TEAM_DEATHMATCH),
            ("TSMODE_OBJECTIVE", mode::OBJECTIVE),
            ("TSROUND_NONE", round::NONE),
            ("TSROUND_RUNNING", round::RUNNING),
            ("TSROUND_OVER", round::OVER),
            ("TSLOAD_KEVLAR", item::KEVLAR),
            ("TSLOAD_STEALTHSHOES", item::STEALTH_SHOES),
            ("TSLOG_INFO", log::INFO),
            ("TSLOG_WARNING", log::WARNING),
            ("TSLOG_ERROR", log::ERROR),
            ("TSLOG_DEBUG", log::DEBUG),
            ("TSBOT_KUNGFU", 0),
        ];
        for (n, v) in want {
            assert_eq!(inc.get(*n).copied(), Some(*v as i64), "tsbot.inc's {n}");
        }
        // The buttons (IN_ALT1, TS's stunt key) come from AMX Mod X's hlsdk_const.inc.
        assert!(include_str!("../../../../include/tsbot.inc").contains("#include <hlsdk_const>"));
    }

    fn sample_snapshot() -> PlayerSnapshot {
        PlayerSnapshot {
            user_id: 7,
            flags: ts_plugin_api::player_flag::ALIVE | ts_plugin_api::player_flag::KUNG_FU,
            team: 2,
            origin: [1.0, 2.0, 3.0],
            velocity: [4.0, 5.0, 6.0],
            v_angle: [7.0, 8.0, 9.0],
            view_ofs: [0.0, 0.0, 17.0],
            absmin: [-16.0, -16.0, -36.0],
            absmax: [16.0, 16.0, 36.0],
            health: 80.0,
            armor: 25.0,
            buttons: 1,
            weapon: 13,
            clip: 30,
            ammo: 90,
            fire_mode: 1,
            attachments: 2,
            stance: 1,
            stunt_bits: ts_plugin_api::stunt_bits::DIVING,
            active_powerup: 1,
            stored_powerup: 256,
            powerup_until: 99.5,
            time_scale: 0.5,
            frags: 3,
            deaths: 4,
            free_slots: 40,
            inventory_serial: 12,
            combat: CombatState {
                melee_power: 60.0,
                next_attack_at: 10.0,
                next_stunt_at: 11.0,
                recover_until: 12.0,
                prone_lock: 13.0,
                dodge_until: 14.0,
                block_kungfu: 15.0,
                block_knife: 16.0,
                block_katana: 17.0,
                melee_window: 18.0,
            },
        }
    }

    fn game() -> FakeGame {
        let g = FakeGame::with_api();
        g.api.as_ref().unwrap().snapshots.borrow_mut().insert(3, sample_snapshot());
        g
    }

    fn idx(l: &[(&str, usize)], n: &str) -> usize {
        l.iter().find(|e| e.0 == n).unwrap().1
    }

    #[test]
    fn snapshots_and_their_getters() {
        let g = game();
        let mut a = FakeArgs::new(&[]);
        let buf = a.alloc(vec![-1; TSPS_SIZE]);
        a.params = vec![3, buf, TSPS_SIZE as Cell];
        assert_eq!(get_player_snapshot(&g, &mut a), 7);
        let m = &a.memory[buf as usize];
        let at = |n| m[idx(layout::TSPS, n)];
        assert_eq!((at("TSPS_FLAGS"), at("TSPS_TEAM"), at("TSPS_WEAPON"), at("TSPS_INVSERIAL")), (0x802, 2, 13, 12));
        assert_eq!(f32::from_bits(at("TSPS_HEALTH") as u32), 80.0);
        assert_eq!(f32::from_bits(m[idx(layout::TSPS, "TSPS_ORIGIN") + 2] as u32), 3.0);
        assert_eq!(f32::from_bits(m[idx(layout::TSPS, "TSPS_ABSMAX") + 2] as u32), 36.0);
        assert_eq!(f32::from_bits(at("TSPS_TIMESCALE") as u32), 0.5);
        assert_eq!(f32::from_bits(m[TSPS_SIZE - 1] as u32), 18.0, "the last combat timer");
        // A short buffer gets only `len` cells; an empty slot zeroes and returns 0.
        let small = a.alloc(vec![-1; 4]);
        a.params = vec![3, small, 2];
        get_player_snapshot(&g, &mut a);
        assert_eq!(a.memory[small as usize], [7, 0x802, -1, -1]);
        a.params = vec![5, buf];
        assert_eq!(get_player_snapshot(&g, &mut a), 0);
        assert!(a.memory[buf as usize].iter().all(|&c| c == 0));

        let mut f = FakeArgs::new(&[3, idx(layout::TSPS, "TSPS_STUNTBITS") as Cell]);
        assert_eq!(get_player_field(&g, &mut f), 0x10);
        f.params = vec![3, TSPS_SIZE as Cell];
        assert_eq!(get_player_field(&g, &mut f), 0);
        assert_eq!(f.errors, ["Invalid snapshot field 49"]);

        let mut c = FakeArgs::new(&[3]);
        assert_eq!((get_player_flags(&g, &mut c), get_player_team(&g, &mut c)), (0x802, 2));
        assert_eq!(f32::from_bits(get_player_timescale(&g, &mut c) as u32), 0.5);
        let refs: Vec<Cell> = (0..4).map(|_| c.alloc(vec![0])).collect();
        c.params = vec![3, refs[0], refs[1], refs[2], refs[3]];
        assert_eq!(get_player_weapon(&g, &mut c), 13);
        assert_eq!(refs.iter().map(|&r| c.memory[r as usize][0]).collect::<Vec<_>>(), [30, 90, 1, 2]);
        assert_eq!(get_player_stance(&g, &mut c), 1);
        assert_eq!(c.memory[refs[0] as usize][0], 0x10);
        assert_eq!(get_player_powerup(&g, &mut c), 1);
        assert_eq!((c.memory[refs[0] as usize][0], f32::from_bits(c.memory[refs[1] as usize][0] as u32)), (256, 99.5));
        assert_eq!(f32::from_bits(get_player_health(&g, &mut c) as u32), 80.0);
        assert_eq!(f32::from_bits(c.memory[refs[0] as usize][0] as u32), 25.0);
        let combat = c.alloc(vec![0; TSCOMBAT_SIZE]);
        c.params = vec![3, combat];
        assert_eq!(get_player_combat(&g, &mut c), 1);
        assert_eq!(c.read_floats(combat, 10), [60.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0]);
        // Empty slots: -1 where 0 is a value (kung fu, standing), else 0.
        let mut e = FakeArgs::new(&[9]);
        assert_eq!((get_player_weapon(&g, &mut e), get_player_stance(&g, &mut e), get_player_flags(&g, &mut e)), (-1, -1, 0));
        e.params = vec![9, combat];
        assert_eq!(get_player_combat(&g, &mut e), 0);
        // No API.
        let n = FakeGame::new();
        assert_eq!(get_player_snapshot(&n, &mut FakeArgs::new(&[3])), 0);
        assert_eq!(get_items(&n, &mut FakeArgs::new(&[])), crate::bots::status::NO_API);
    }

    #[test]
    fn inventory_items_and_objectives_are_lists() {
        let g = game();
        let api = g.api.as_ref().unwrap();
        api.inventories.borrow_mut().insert(
            3,
            vec![
                WeaponEntry { weapon: 1, clip: 17, ammo: 51, fire_mode: 0, attachments: 0 },
                WeaponEntry { weapon: 13, clip: 30, ammo: 90, fire_mode: 1, attachments: 6 },
            ],
        );
        let mut a = FakeArgs::new(&[]);
        let (w, c, m) = (a.alloc(vec![0; 1]), a.alloc(vec![0; 1]), a.alloc(vec![0; 1]));
        a.params = vec![3, w, c, m, 1];
        assert_eq!(get_inventory(&g, &mut a), 2, "the total, past max");
        assert_eq!((a.memory[w as usize][0], a.memory[c as usize][0], a.memory[m as usize][0]), (1, 17, 51));
        let flat = a.alloc(vec![0; 10]);
        a.params = vec![3, flat, 2];
        assert_eq!(get_inventory_ex(&g, &mut a), 2);
        assert_eq!(a.memory[flat as usize], [1, 17, 51, 0, 0, 13, 30, 90, 1, 6]);
        a.params = vec![4, flat, 2];
        assert_eq!(get_inventory_ex(&g, &mut a), ts_plugin_api::NOT_PLAYER);

        api.items.borrow_mut().push(ItemInfo {
            index: 40,
            serial: 2,
            kind: ts_plugin_api::entity::POWERUP,
            weapon: 0,
            powerup: 64,
            origin: [1.0, 2.0, 3.0],
            available: 0,
            respawn_at: 30.0,
        });
        let items = a.alloc(vec![0; TSITM_SIZE]);
        a.params = vec![items, 1];
        assert_eq!(get_items(&g, &mut a), 1);
        assert_eq!(a.memory[items as usize][..5], [40, 2, 3, 0, 64]);
        assert_eq!(f32::from_bits(a.memory[items as usize][9] as u32), 30.0);

        let mut o = ObjectiveInfo { id: 2, entity: 50, origin: [5.0, 6.0, 7.0], active: 1, ..Default::default() };
        for (d, s) in o.name.iter_mut().zip(b"Hack the computer") {
            *d = *s as core::ffi::c_char;
        }
        api.objectives.borrow_mut().push(o);
        let objs = a.alloc(vec![0; TSOBJ_SIZE]);
        a.params = vec![3, objs, 1];
        assert_eq!(get_objectives(&g, &mut a), 1);
        assert_eq!(a.memory[objs as usize][..2], [2, 50]);
        assert_eq!(a.memory[objs as usize][5], 1);
        let name = a.alloc(vec![0; 8]);
        a.params = vec![3, 0, name, 7];
        assert_eq!(get_objective_name(&g, &mut a), 17);
        assert_eq!(a.read_str(name), "Hack th");
        a.params = vec![3, 1, name, 7];
        assert_eq!(get_objective_name(&g, &mut a), ts_plugin_api::INVALID);
        a.params = vec![8, objs, 1];
        assert_eq!(get_objectives(&g, &mut a), ts_plugin_api::NOT_PLAYER);
    }

    #[test]
    fn rules_frame_and_entities() {
        let g = game();
        let api = g.api.as_ref().unwrap();
        api.game_state.set(GameState {
            mode: 3,
            teamplay: 1,
            ctb: 1,
            teams: 2,
            round: 1,
            time: 50.0,
            max_clients: 16,
            team_scores: [3, 1, 0, 0],
            ..Default::default()
        });
        api.frame.set(FrameInfo {
            time: 50.0,
            frametime: 0.01,
            highest_edict: 120,
            teamplay: 1,
            ctb: 1,
            mode: 3,
            round: 1,
            team_scores: [3, 1, 0, 0],
        });
        let mut a = FakeArgs::new(&[]);
        let gs = a.alloc(vec![0; TSGS_SIZE]);
        a.params = vec![gs];
        assert_eq!(get_gamestate(&g, &mut a), 1);
        assert_eq!(a.memory[gs as usize][..7], [3, 1, 0, 0, 1, 2, 1]);
        assert_eq!(a.memory[gs as usize][11..], [16, 3, 1, 0, 0]);
        let fi = a.alloc(vec![0; TSFI_SIZE]);
        a.params = vec![fi];
        assert_eq!(get_frameinfo(&g, &mut a), 1);
        assert_eq!(a.read_floats(fi, 2), [50.0, 0.01]);
        assert_eq!(a.memory[fi as usize][2..], [120, 1, 1, 3, 1, 3, 1, 0, 0]);

        api.entities
            .borrow_mut()
            .insert(60, EntityInfo { kind: ts_plugin_api::entity::GROUND_WEAPON, weapon: 13, available: 1, ..Default::default() });
        api.keys.borrow_mut().insert((60, "master".into()), "door_ms".into());
        let ei = a.alloc(vec![0; TSEI_SIZE]);
        a.params = vec![60, ei];
        assert_eq!(get_entity_info(&g, &mut a), ts_plugin_api::OK);
        assert_eq!(a.memory[ei as usize], [2, 13, 0, 0, 0, 1]);
        a.params = vec![61, ei];
        assert_eq!(get_entity_info(&g, &mut a), ts_plugin_api::NOT_PLAYER);
        let key = a.alloc_str("master");
        let out = a.alloc(vec![0; 32]);
        a.params = vec![60, key, out, 31];
        assert_eq!(entity_key(&g, &mut a), 7);
        assert_eq!(a.read_str(out), "door_ms");
        let other = a.alloc_str("wait");
        a.params = vec![60, other, out, 31];
        assert_eq!(entity_key(&g, &mut a), ts_plugin_api::UNSUPPORTED);
        assert_eq!(a.read_str(out), "");

        let m = a.alloc_str("open");
        a.params = vec![m, 3];
        assert_eq!(master_triggered(&g, &mut a), 1);
        let shut = a.alloc_str("shut");
        a.params = vec![shut];
        assert_eq!(master_triggered(&g, &mut a), 0);
        assert_eq!(api.calls.borrow().last().unwrap(), "master_triggered shut 0");
        api.knife_owner.set(4);
        assert_eq!(projectile_owner(&g, &mut FakeArgs::new(&[70])), 4);
    }
}
