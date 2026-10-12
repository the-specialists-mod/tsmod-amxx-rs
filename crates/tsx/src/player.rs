// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports `CPlayer`
// (modules/ts/tsx/CMisc.cpp, CMisc.h), Copyright (C) The AMX Mod X Development Team, Copyright
// (C) 2004 Lukasz Wlasinski.

//! Per-player state: what tsx learns from the game's messages, and the stats it keeps.
//!
//! Upstream's `CPlayer` also wrote TS's `CBasePlayer` at fixed offsets (money, slots, power-ups),
//! offsets that belonged to TS 2.x and don't exist in the Rust server at all; those natives go
//! through the server's plugin API instead (`natives::base`).

use crate::rank::RankKey;
use crate::stats::Stats;
use crate::weapons::MAX_WEAPONS;

/// `TSITEM_*` bits of `ts_getuseritems`.
pub const ITEM_KUNGFU: i32 = 1;
pub const ITEM_SUPERJUMP: i32 = 2;
/// `TSKF_*` bits of `ts_getuserkillflags`.
pub const KF_STUNTKILL: i32 = 1;
pub const KF_SLIDINGKILL: i32 = 2;
pub const KF_DOUBLEKILL: i32 = 4;
pub const KF_ISSPEC: i32 = 8;
pub const KF_KILLEDSPEC: i32 = 16;

/// `CPlayer::PlayerWeapon`: stats with a weapon (or against a player), plus the weapon's ammo
/// state for the player's own weapons.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerWeapon {
    pub stats: Stats,
    /// Weapon id whose name `get_user_[av]stats` report (upstream keeps a name pointer).
    pub name: Option<usize>,
    pub ammo: i32,
    pub clip: i32,
    /// Fire mode (`TSA_*` for attachments below).
    pub mode: i32,
    pub attach: i32,
}

/// `CPlayer`.
#[derive(Clone, Debug)]
pub struct Player {
    pub index: usize,
    /// Address without the port (`Connect`).
    pub ip: String,
    /// From `ScoreInfo` (the port sends team + 1; 0 outside teamplay).
    pub team_id: i32,
    /// Hit group of the last player this player's traces hit.
    pub aiming: i32,
    /// tsx weapon id in hand (kung fu as 36).
    pub current: usize,
    /// `TSSpace`, `TSCash`, `PwUp` (type and value), as last sent.
    pub space: i32,
    pub money: i32,
    pub pwup: i32,
    pub pwup_value: i32,
    /// `TSITEM_*` from the permanent `PwUp` flags.
    pub items: i32,
    /// The `TSPAward` byte last broadcast for this player (port addition: upstream read it from
    /// `CBasePlayer`); `None` until one was seen.
    pub award: Option<i32>,
    pub killing_spree: i32,
    pub is_specialist: bool,
    pub kill_flags: i32,
    pub last_frag: i32,
    pub last_kill: f32,
    /// tsx's own frag count, compared with `pev->frags` to check its kill bookkeeping.
    pub frags: i32,
    pub ingame: bool,
    /// When to commit life stats to the rank (just after a spawn).
    pub clear_stats: f32,
    pub checkstate: bool,
    pub state: i32,
    pub oldstate: i32,
    pub weapons: Vec<PlayerWeapon>,
    pub attackers: Vec<PlayerWeapon>,
    pub victims: Vec<PlayerWeapon>,
    /// Per-weapon stats of the round (`get_user_wrstats`).
    pub weapons_rnd: Vec<Stats>,
    /// Stats since the last spawn.
    pub life: Stats,
    pub rank: Option<RankKey>,
}

impl Player {
    pub fn new(index: usize) -> Self {
        Player {
            index,
            ip: String::new(),
            team_id: 0,
            aiming: 0,
            current: 0,
            space: 0,
            money: 0,
            pwup: 0,
            pwup_value: 0,
            items: 0,
            award: None,
            killing_spree: 0,
            is_specialist: false,
            kill_flags: 0,
            last_frag: 0,
            last_kill: 0.0,
            frags: 0,
            ingame: false,
            clear_stats: 0.0,
            checkstate: false,
            state: 0,
            oldstate: 0,
            weapons: vec![PlayerWeapon::default(); MAX_WEAPONS],
            attackers: vec![PlayerWeapon::default(); 33],
            victims: vec![PlayerWeapon::default(); 33],
            weapons_rnd: vec![Stats::default(); MAX_WEAPONS],
            life: Stats::default(),
            rank: None,
        }
    }

    /// `CPlayer::Init` (`ServerActivate`).
    pub fn init(&mut self) {
        self.current = 0;
        self.clear_stats = 0.0;
        self.ingame = false;
    }

    /// `CPlayer::Connect`: keeps the address without its port.
    pub fn connect(&mut self, address: &str) {
        self.ip = address.split(':').next().unwrap_or("").chars().take(31).collect();
    }

    /// `CPlayer::restartStats`.
    pub fn restart_stats(&mut self, all: bool) {
        if all {
            self.weapons.fill(PlayerWeapon::default());
        }
        self.weapons_rnd.fill(Stats::default());
        self.attackers.fill(PlayerWeapon::default());
        self.victims.fill(PlayerWeapon::default());
        self.life = Stats::default();
    }

    /// `CPlayer::saveShot`.
    pub fn save_shot(&mut self, weapon: usize) {
        if weapon >= MAX_WEAPONS {
            return;
        }
        self.victims[0].stats.shots += 1;
        self.weapons[weapon].stats.shots += 1;
        self.weapons[0].stats.shots += 1;
        self.life.shots += 1;
        self.weapons_rnd[weapon].shots += 1;
        self.weapons_rnd[0].shots += 1;
    }
}

/// Both players of a hit or kill. `att == vic` is allowed (self-damage), so the two are
/// borrowed one after the other.
fn pair(players: &mut [Player], att: usize, vic: usize) -> Option<()> {
    (att < players.len() && vic < players.len()).then_some(())
}

/// `CPlayer::saveHit` (`att` hit `vic`).
pub fn save_hit(players: &mut [Player], att: usize, vic: usize, weapon: usize, damage: i32, body: usize) {
    if pair(players, att, vic).is_none() || weapon >= MAX_WEAPONS {
        return;
    }
    let v = &mut players[vic];
    v.attackers[att].stats.hit(damage, body);
    v.attackers[0].stats.hit(damage, body);
    let a = &mut players[att];
    a.victims[vic].stats.hit(damage, body);
    a.victims[0].stats.hit(damage, body);
    a.weapons_rnd[weapon].hit(damage, body);
    a.weapons_rnd[0].hit(damage, body);
    a.weapons[weapon].stats.hit(damage, body);
    a.weapons[0].stats.hit(damage, body);
    a.life.hit(damage, body);
}

/// `CPlayer::saveKill` (`att` killed `vic`).
pub fn save_kill(players: &mut [Player], att: usize, vic: usize, weapon: usize, hs: i32, tk: i32) {
    if pair(players, att, vic).is_none() || weapon >= MAX_WEAPONS {
        return;
    }
    let v = &mut players[vic];
    v.attackers[att].name = Some(weapon);
    v.attackers[att].stats.kill(hs, tk);
    v.attackers[0].stats.kill(hs, tk);
    let cur = v.current.min(MAX_WEAPONS - 1);
    v.weapons[cur].stats.deaths += 1;
    v.weapons[0].stats.deaths += 1;
    v.life.deaths += 1;
    v.weapons_rnd[cur].deaths += 1;
    v.weapons_rnd[0].deaths += 1;
    let a = &mut players[att];
    a.victims[vic].name = Some(weapon);
    a.victims[vic].stats.victim(hs, tk);
    a.victims[0].stats.victim(hs, tk);
    a.weapons_rnd[weapon].kill(hs, tk);
    a.weapons_rnd[0].kill(hs, tk);
    a.weapons[weapon].stats.kill(hs, tk);
    a.weapons[0].stats.kill(hs, tk);
    a.life.kill(hs, tk);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn players() -> Vec<Player> {
        (0..33).map(Player::new).collect()
    }

    #[test]
    fn connect_strips_the_port() {
        let mut p = Player::new(1);
        p.connect("10.0.0.5:27005");
        assert_eq!(p.ip, "10.0.0.5");
        p.connect("loopback");
        assert_eq!(p.ip, "loopback");
    }

    #[test]
    fn hits_and_kills_land_in_every_table() {
        let mut ps = players();
        ps[2].current = 5;
        save_hit(&mut ps, 1, 2, 13, 40, 1);
        save_kill(&mut ps, 1, 2, 13, 1, 0);
        let (a, v) = (&ps[1], &ps[2]);
        assert_eq!((a.weapons[13].stats.hits, a.weapons[13].stats.damage, a.weapons[13].stats.kills), (1, 40, 1));
        assert_eq!((a.weapons[0].stats.kills, a.weapons_rnd[13].kills, a.life.kills, a.life.hs), (1, 1, 1, 1));
        assert_eq!((a.victims[2].stats.hits, a.victims[2].stats.deaths, a.victims[2].name), (1, 1, Some(13)));
        assert_eq!((v.attackers[1].stats.hits, v.attackers[1].stats.kills, v.attackers[1].name), (1, 1, Some(13)));
        assert_eq!((v.weapons[5].stats.deaths, v.life.deaths, v.attackers[0].stats.body_hits[1]), (1, 1, 1));
    }

    #[test]
    fn out_of_range_body_parts_and_weapons_are_ignored() {
        let mut ps = players();
        save_hit(&mut ps, 1, 2, 13, 10, 10); // HITGROUP_SHIELD (10) has no slot
        assert_eq!(ps[1].life.hits, 1);
        save_hit(&mut ps, 1, 2, 99, 10, 1);
        ps[1].save_shot(99);
        assert_eq!(ps[1].life.hits, 1);
    }

    #[test]
    fn restart_keeps_weapons_unless_asked() {
        let mut p = Player::new(1);
        p.save_shot(13);
        p.restart_stats(false);
        assert_eq!((p.weapons[13].stats.shots, p.weapons_rnd[13].shots, p.life.shots), (1, 0, 0));
        p.restart_stats(true);
        assert_eq!(p.weapons[13].stats.shots, 0);
    }
}
