// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Message parsing and the damage/death logic, fed by the port's own encoders.

use crate::game::{Event, Game};
use crate::msgs::Kind;
use crate::player::{ITEM_KUNGFU, ITEM_SUPERJUMP, KF_DOUBLEKILL, KF_ISSPEC, KF_KILLEDSPEC, KF_SLIDINGKILL, KF_STUNTKILL};
use crate::state::State;
use crate::testing::{FakeGame, Feed, register_all};
use shared::protocol::hud_wire::{PwUp, TsCash, TsHealth, TsPAward, TsSpace, TsState};
use shared::protocol::rules::{DeathMsg, ScoreInfo};
use shared::protocol::weapons::{WStatus, WeaponInfo};

fn setup() -> (State, FakeGame) {
    let mut s = State::default();
    register_all(&mut s);
    let g = FakeGame::new();
    for i in 1..=3 {
        g.player(i);
        s.put_in_server(&g, i);
    }
    (s, g)
}

#[test]
fn weapon_info_sets_the_weapon_in_hand() {
    let (mut s, g) = setup();
    let m = WeaponInfo { id: 13, clip: 30, reserve: 90, mode: 1, items: 8 };
    Feed::send(&mut s, &g, Kind::WeaponInfo, Some(1), |f| m.write(f));
    let p = &s.players[1];
    assert_eq!(p.current, 13);
    let w = &p.weapons[13];
    assert_eq!((w.clip, w.ammo, w.mode, w.attach), (30, 90, 1, 8));
    // Kung fu: id 0 on the wire, 38 in tsx; 36 is TS 3.0's Contender G2.
    let kf = WeaponInfo { id: 0, clip: 50, reserve: 0, mode: 0, items: 0 };
    Feed::send(&mut s, &g, Kind::WeaponInfo, Some(1), |f| kf.write(f));
    assert_eq!(s.players[1].current, 38);
    let g2 = WeaponInfo { id: 36, clip: 1, reserve: 10, mode: 0, items: 0 };
    Feed::send(&mut s, &g, Kind::WeaponInfo, Some(1), |f| g2.write(f));
    assert_eq!(s.players[1].current, 36);
}

#[test]
fn legacy_weapon_ids_follow_the_cvar() {
    let (mut s, g) = setup();
    let kf = WeaponInfo { id: 0, clip: 50, reserve: 0, mode: 0, items: 0 };
    Feed::send(&mut s, &g, Kind::WeaponInfo, Some(1), |f| kf.write(f));
    assert_eq!(s.players[1].current, 38);
    // amxx.cfg runs after ServerActivate: the next player to enter brings the change, and whoever
    // holds kung fu keeps holding it.
    g.set_cvar("tsx_legacy_weaponids", 1.0);
    s.put_in_server(&g, 2);
    assert!(s.weapons.legacy());
    assert_eq!(s.players[1].current, 36);
    assert_eq!(s.weapons.get(36).unwrap().name, "Kung Fu");
    Feed::send(&mut s, &g, Kind::WeaponInfo, Some(1), |f| WeaponInfo { id: 13, ..kf }.write(f));
    Feed::send(&mut s, &g, Kind::WeaponInfo, Some(1), |f| kf.write(f));
    assert_eq!(s.players[1].current, 36);
    g.set_cvar("tsx_legacy_weaponids", 0.0);
    s.server_activate(&g);
    assert!(!s.weapons.legacy());
}

#[test]
fn wstatus_of_a_lost_weapon_means_kung_fu() {
    let (mut s, g) = setup();
    s.players[2].current = 13;
    Feed::send(&mut s, &g, Kind::WStatus, Some(2), |f| WStatus { id: 5, owned: 1, clip: 30 }.write(f));
    assert_eq!(s.players[2].current, 13);
    Feed::send(&mut s, &g, Kind::WStatus, Some(2), |f| WStatus { id: 13, owned: 0, clip: 0 }.write(f));
    assert_eq!(s.players[2].current, 38);
}

#[test]
fn hud_values() {
    let (mut s, g) = setup();
    Feed::send(&mut s, &g, Kind::TsCash, Some(1), |f| TsCash { cash: 12345 }.write(f));
    Feed::send(&mut s, &g, Kind::TsSpace, Some(1), |f| TsSpace { space: 41 }.write(f));
    Feed::send(&mut s, &g, Kind::TsState, Some(1), |f| TsState { state: 2 }.write(f));
    let p = &s.players[1];
    assert_eq!((p.money, p.space, p.state, p.checkstate), (12345, 41, 2, true));
    // TSCash is a short: TS clamps money to it.
    Feed::send(&mut s, &g, Kind::TsCash, Some(1), |f| TsCash { cash: -2 }.write(f));
    assert_eq!(s.players[1].money, -2);
}

#[test]
fn pwup_keeps_permanent_flags_apart() {
    let (mut s, g) = setup();
    Feed::send(&mut s, &g, Kind::PwUp, Some(1), |f| PwUp { bits: 4, secs: 0 }.write(f));
    Feed::send(&mut s, &g, Kind::PwUp, Some(1), |f| PwUp { bits: 0x100, secs: 0 }.write(f));
    Feed::send(&mut s, &g, Kind::PwUp, Some(1), |f| PwUp { bits: 16, secs: 25 }.write(f));
    let p = &s.players[1];
    assert_eq!(p.items, ITEM_KUNGFU | ITEM_SUPERJUMP);
    assert_eq!((p.pwup, p.pwup_value), (16, 25));
}

#[test]
fn broadcasts_carry_their_player() {
    let (mut s, g) = setup();
    Feed::send(&mut s, &g, Kind::ScoreInfo, None, |f| ScoreInfo { idx: 2, frags: 5, deaths: 1, class: 0, team: 2 }.write(f));
    Feed::send(&mut s, &g, Kind::TsPAward, None, |f| TsPAward { entindex: 3, award: 4 }.write(f));
    assert_eq!((s.players[2].team_id, s.players[3].award), (2, Some(4)));
    // Messages to a player without one ignored, as are unregistered ids.
    s.message_begin(9999, Some(1));
    s.message_int(1);
    assert_eq!(s.message_end(&g), crate::msgs::Followup::None);
}

#[test]
fn reset_hud_on_spawn_and_death() {
    let (mut s, g) = setup();
    g.time.set(10.0);
    Feed::send(&mut s, &g, Kind::ResetHud, Some(1), |f| shared::protocol::hud_wire::Wire::byte(f, 0));
    assert_eq!(s.players[1].clear_stats, 10.25);
    s.players[1].killing_spree = 4;
    g.edit(1, |e| {
        e.alive = false;
        e.frags = 7;
    });
    Feed::send(&mut s, &g, Kind::ResetHud, Some(1), |f| shared::protocol::hud_wire::Wire::byte(f, 0));
    assert_eq!((s.players[1].killing_spree, s.players[1].frags), (0, 7));
}

#[test]
fn hit_fires_client_damage_with_the_attackers_weapon_and_aim() {
    let (mut s, g) = setup();
    s.players[1].current = 13;
    s.trace_line(1, 2, 1, &g);
    g.edit(2, |e| {
        e.inflictor = Some(1);
        e.dmg_take = 39.0;
    });
    Feed::send(&mut s, &g, Kind::TsHealth, Some(2), |f| TsHealth { hp: 61 }.write(f));
    assert_eq!(*g.fired.borrow(), [Event::Damage { attacker: 1, victim: 2, damage: 39, weapon: 13, aim: 1, ta: 0 }]);
    assert_eq!(g.damage(2).1, 0.0, "dmg_take is consumed");
    assert_eq!((s.players[1].weapons[13].stats.hits, s.players[2].attackers[1].stats.damage), (1, 39));
    // No new damage: nothing.
    Feed::send(&mut s, &g, Kind::TsHealth, Some(2), |f| TsHealth { hp: 61 }.write(f));
    assert_eq!(g.fired.borrow().len(), 1);
}

/// [`setup`] with players 1 to `n`, at 5 s.
fn setup_n(n: i32) -> (State, FakeGame) {
    let (mut s, g) = setup();
    // Off 0: a kill within 2 s of the start would be a double kill (`last_kill` starts at 0).
    g.time.set(5.0);
    for i in 4..=n {
        g.player(i);
        s.put_in_server(&g, i);
    }
    (s, g)
}

/// TS's side of `killer` killing `victim` once `PlayerKilled` gave `frags` points: the
/// `DeathMsg` (if `weapon`), then `Killed`'s `TSHealth` 0. Returns the `client_death` weapon.
fn kill(s: &mut State, g: &FakeGame, killer: i32, victim: i32, frags: i32, weapon: Option<&str>) -> i32 {
    g.edit(killer, |e| e.frags += frags);
    g.edit(victim, |e| {
        e.inflictor = Some(killer);
        e.dmg_take = 100.0;
        e.alive = false;
    });
    if let Some(w) = weapon {
        let m = DeathMsg { killer: killer as u8, victim: victim as u8, weapon: w.to_owned() };
        Feed::send(s, g, Kind::DeathMsg, None, |f| m.write(f));
    }
    let before = g.fired.borrow().len();
    Feed::send(s, g, Kind::TsHealth, Some(victim), |f| TsHealth { hp: 0 }.write(f));
    let fired = g.fired.borrow();
    match fired[before..] {
        [Event::Damage { .. }, Event::Death { killer: k, victim: v, weapon, .. }] => {
            assert_eq!((k, v), (killer, victim), "killer and victim");
            weapon
        }
        ref other => panic!("no kill: {other:?}"),
    }
}

#[test]
fn kill_fires_client_death_with_the_stats() {
    let (mut s, g) = setup_n(3);
    s.players[1].current = 13;
    g.time.set(50.0);
    assert_eq!(kill(&mut s, &g, 1, 2, 1, Some("AK47")), 13);
    let k = &s.players[1];
    assert_eq!((k.last_frag, k.kill_flags, k.killing_spree, k.frags), (1, 0, 1, 1));
    assert_eq!((k.life.kills, s.players[2].life.deaths), (1, 1));
}

#[test]
fn katana_kills_are_one_frag_and_stay_katana_kills() {
    for legacy in [false, true] {
        let (mut s, g) = setup_n(3);
        g.set_cvar("tsx_legacy_weaponids", legacy as i32 as f32);
        s.sync_weapon_ids(&g);
        s.players[1].current = 34;
        // TS 3.0: a slash is no kung-fu frag, 1 point (upstream expected 2 and said kung fu).
        assert_eq!(kill(&mut s, &g, 1, 2, 1, Some("Katana")), 34, "legacy {legacy}");
        assert_eq!((s.players[1].last_frag, s.players[1].kill_flags), (1, 0));
        // Without a DeathMsg the prediction agrees as well.
        g.time.set(10.0);
        assert_eq!(kill(&mut s, &g, 1, 3, 1, None), 34, "legacy {legacy}");
    }
}

#[test]
fn death_msg_kung_fu_is_kung_fu_in_both_numberings() {
    for (legacy, kung_fu) in [(false, 38), (true, 36)] {
        let (mut s, g) = setup_n(3);
        g.set_cvar("tsx_legacy_weaponids", legacy as i32 as f32);
        s.sync_weapon_ids(&g);
        // A kick with a gun in hand: "Kung Fu" under glast_closecombat, 2 points.
        s.players[1].current = 13;
        assert_eq!(kill(&mut s, &g, 1, 2, 2, Some("Kung Fu")), kung_fu);
        assert_eq!(s.players[1].last_frag, 2);
        // Kung fu out outside a close-combat attack is "world".
        g.time.set(10.0);
        assert_eq!(kill(&mut s, &g, 1, 3, 2, Some("world")), kung_fu);
        // A named gun is that gun, also when tsx thought kung fu was out.
        s.players[1].current = kung_fu as usize;
        g.time.set(20.0);
        assert_eq!(kill(&mut s, &g, 1, 2, 1, Some("Desert Eagle Stock")), 12);
    }
}

#[test]
fn stunt_kills_score_one_more_and_set_the_flags() {
    let (mut s, g) = setup_n(3);
    s.players[1].current = 13;
    let fast = [200.0, 0.0, 0.0];
    // A dive with a gun: 1 + 1 (upstream: + 2), and still the gun.
    g.edit(1, |e| {
        e.iuser4 = 0x10;
        e.velocity = fast;
    });
    assert_eq!(kill(&mut s, &g, 1, 2, 2, Some("AK47")), 13);
    assert_eq!((s.players[1].last_frag, s.players[1].kill_flags), (2, KF_STUNTKILL));
    // Without a DeathMsg tsx predicts the same and keeps the gun.
    g.time.set(10.0);
    assert_eq!(kill(&mut s, &g, 1, 3, 2, None), 13);
    assert_eq!(s.players[1].kill_flags, KF_STUNTKILL);
    // A kick in a flip: close combat, the tag without the point.
    g.time.set(20.0);
    g.edit(1, |e| e.iuser4 = 0x400);
    assert_eq!(kill(&mut s, &g, 1, 2, 2, Some("Kung Fu")), 38);
    assert_eq!((s.players[1].last_frag, s.players[1].kill_flags), (2, KF_STUNTKILL));
    // A slide (prone, friction under 1).
    g.time.set(30.0);
    g.edit(1, |e| {
        e.iuser4 = 0x20;
        e.friction = 0.5;
    });
    assert_eq!(kill(&mut s, &g, 1, 3, 2, Some("AK47")), 13);
    assert_eq!(s.players[1].kill_flags, KF_SLIDINGKILL);
    // Too slow for either.
    g.time.set(40.0);
    g.edit(1, |e| {
        e.iuser4 = 0x10;
        e.velocity = [50.0, 0.0, 0.0];
    });
    assert_eq!(kill(&mut s, &g, 1, 2, 1, Some("AK47")), 13);
    assert_eq!(s.players[1].kill_flags, 0);
}

#[test]
fn a_grenade_kill_has_no_stunt() {
    let (mut s, g) = setup_n(3);
    g.edit(50, |e| {
        e.class = "grenade".into();
        e.owner = Some(1);
    });
    g.edit(1, |e| {
        e.frags = 1;
        e.iuser4 = 0x10;
        e.velocity = [200.0, 0.0, 0.0];
    });
    g.edit(2, |e| {
        e.inflictor = Some(50);
        e.dmg_take = 100.0;
        e.alive = false;
    });
    let m = DeathMsg { killer: 1, victim: 2, weapon: "M61 Grenade".into() };
    Feed::send(&mut s, &g, Kind::DeathMsg, None, |f| m.write(f));
    Feed::send(&mut s, &g, Kind::TsHealth, Some(2), |f| TsHealth { hp: 0 }.write(f));
    assert_eq!(g.fired.borrow()[1], Event::Death { killer: 1, victim: 2, weapon: 24, aim: 0, tk: 0 });
    assert_eq!((s.players[1].last_frag, s.players[1].kill_flags), (1, 0));
}

#[test]
fn double_kills_count_within_two_seconds() {
    let (mut s, g) = setup_n(3);
    s.players[1].current = 13;
    g.time.set(50.0);
    kill(&mut s, &g, 1, 2, 1, Some("AK47"));
    assert_eq!(s.players[1].kill_flags, 0);
    // 1.5 s later: x2 (upstream's window was 1 s, so it guessed kung fu here).
    g.time.set(51.5);
    assert_eq!(kill(&mut s, &g, 1, 3, 2, Some("AK47")), 13);
    assert_eq!((s.players[1].last_frag, s.players[1].kill_flags), (2, KF_DOUBLEKILL));
    // A double kill stops the clock: a third kill 0.5 s on is no double.
    g.time.set(52.0);
    assert_eq!(kill(&mut s, &g, 1, 2, 1, None), 13);
    assert_eq!((s.players[1].last_frag, s.players[1].kill_flags), (1, 0));
    // 1.9 s: double; then 2.5 s after a kill: not.
    g.time.set(53.9);
    assert_eq!(kill(&mut s, &g, 1, 3, 2, None), 13, "predicted without a DeathMsg");
    assert_eq!(s.players[1].kill_flags, KF_DOUBLEKILL);
    g.time.set(60.0);
    kill(&mut s, &g, 1, 2, 1, None);
    g.time.set(62.5);
    kill(&mut s, &g, 1, 3, 1, None);
    assert_eq!(s.players[1].kill_flags, 0);
}

#[test]
fn sprees_make_specialists_then_double() {
    let (mut s, g) = setup_n(12);
    s.players[1].current = 13;
    for (n, victim) in (2..=9).enumerate() {
        g.time.set(10.0 * (n + 1) as f32);
        assert_eq!(kill(&mut s, &g, 1, victim, 1, None), 13);
    }
    assert!(!s.players[1].is_specialist);
    // The 9th kill: Specialist, no x2.
    g.time.set(100.0);
    assert_eq!(kill(&mut s, &g, 1, 10, 1, None), 13);
    assert_eq!((s.players[1].killing_spree, s.players[1].last_frag, s.players[1].kill_flags), (9, 1, KF_ISSPEC));
    // The 10th: Unstoppable, x2.
    g.time.set(110.0);
    assert_eq!(kill(&mut s, &g, 1, 11, 2, None), 13);
    assert_eq!((s.players[1].last_frag, s.players[1].kill_flags), (2, KF_ISSPEC));
    // Killing the specialist: +5; as a double kill x2 first: (1 x 2) + 5.
    s.players[12].current = 13;
    g.time.set(120.0);
    kill(&mut s, &g, 12, 2, 1, None);
    g.time.set(121.0);
    assert_eq!(kill(&mut s, &g, 12, 1, 7, Some("AK47")), 13);
    assert_eq!((s.players[12].last_frag, s.players[12].kill_flags), (7, KF_DOUBLEKILL | KF_KILLEDSPEC));
}

#[test]
fn every_rule_at_once_in_ts_order() {
    // Kung fu (2) in a dive (+1), double (x2), a specialist victim (+5), unstoppable (x2): 22.
    let (mut s, g) = setup_n(3);
    s.players[1].killing_spree = 9;
    s.players[1].last_kill = 49.0;
    s.players[2].killing_spree = 9;
    g.edit(1, |e| {
        e.iuser4 = 0x10;
        e.velocity = [0.0, 0.0, -100.0];
    });
    g.time.set(50.0);
    assert_eq!(kill(&mut s, &g, 1, 2, 22, Some("world")), 38);
    let flags = KF_STUNTKILL | KF_DOUBLEKILL | KF_ISSPEC | KF_KILLEDSPEC;
    assert_eq!((s.players[1].last_frag, s.players[1].kill_flags), (22, flags));
}

#[test]
fn a_kill_after_a_team_kill_is_still_a_kill() {
    let (mut s, g) = setup_n(3);
    s.players[1].current = 13;
    s.players[1].team_id = 1;
    s.players[2].team_id = 1;
    s.players[3].team_id = 2;
    g.edit(1, |e| e.frags = 5);
    s.players[1].frags = 5;
    // The team kill: -1 frag, TK set.
    g.edit(1, |e| e.frags = 4);
    g.edit(2, |e| {
        e.inflictor = Some(1);
        e.dmg_take = 100.0;
        e.alive = false;
    });
    Feed::send(&mut s, &g, Kind::TsHealth, Some(2), |f| TsHealth { hp: 0 }.write(f));
    assert_eq!(g.fired.borrow()[1], Event::Death { killer: 1, victim: 2, weapon: 13, aim: 0, tk: 1 });
    assert_eq!((s.players[1].frags, s.players[1].killing_spree), (4, 1), "TS counts it in the spree");
    // Upstream kept 5 here, so +1 back to 5 looked like no frag: a self kill.
    g.time.set(10.0);
    assert_eq!(kill(&mut s, &g, 1, 3, 1, Some("AK47")), 13);
    assert_eq!((s.players[1].frags, s.players[1].killing_spree), (5, 2));
}

#[test]
fn without_a_death_msg_frags_only_a_kick_explains_mean_kung_fu() {
    for (legacy, kung_fu) in [(false, 38), (true, 36)] {
        let (mut s, g) = setup_n(3);
        g.set_cvar("tsx_legacy_weaponids", legacy as i32 as f32);
        s.sync_weapon_ids(&g);
        s.players[1].current = 13;
        // 2 frags for what looks like a 1-frag gun kill: a kick with the gun out.
        assert_eq!(kill(&mut s, &g, 1, 2, 2, None), kung_fu);
        // 3 frags nothing explains (a plugin's frags): the weapon stays, the frag count is TS's.
        g.time.set(10.0);
        assert_eq!(kill(&mut s, &g, 1, 3, 3, None), 13);
        assert_eq!((s.players[1].last_frag, s.players[1].frags), (3, 5));
    }
}

#[test]
fn a_death_msg_about_another_killer_is_ignored() {
    let (mut s, g) = setup_n(3);
    s.players[1].current = 13;
    let m = DeathMsg { killer: 3, victim: 2, weapon: "Kung Fu".into() };
    Feed::send(&mut s, &g, Kind::DeathMsg, None, |f| m.write(f));
    assert_eq!(kill(&mut s, &g, 1, 2, 1, None), 13);
    assert_eq!(s.players[2].death_notice, None, "taken by the death");
}

#[test]
fn grenades_and_thrown_knives_find_their_thrower() {
    let (mut s, g) = setup();
    g.edit(50, |e| {
        e.class = "grenade".into();
        e.owner = Some(1);
    });
    g.edit(2, |e| {
        e.inflictor = Some(50);
        e.dmg_take = 80.0;
    });
    Feed::send(&mut s, &g, Kind::TsHealth, Some(2), |f| TsHealth { hp: 20 }.write(f));
    assert_eq!(g.fired.borrow()[0], Event::Damage { attacker: 1, victim: 2, damage: 80, weapon: 24, aim: 0, ta: 0 });

    // Without the API the knife's `pev->owner` names the thrower.
    g.edit(51, |e| {
        e.class = "knife".into();
        e.owner = Some(3);
    });
    g.edit(2, |e| {
        e.inflictor = Some(51);
        e.dmg_take = 30.0;
    });
    Feed::send(&mut s, &g, Kind::TsHealth, Some(2), |f| TsHealth { hp: 1 }.write(f));
    assert_eq!(g.fired.borrow()[1], Event::Damage { attacker: 3, victim: 2, damage: 30, weapon: 29, aim: 0, ta: 0 });
    assert_eq!(s.players[3].weapons[29].stats.shots, 1, "melee hits count as shots");
}

#[test]
fn the_api_names_a_thrown_knifes_owner() {
    let (mut s, _) = setup();
    let g = FakeGame::with_api();
    for i in 1..=3 {
        g.player(i);
    }
    g.api.as_ref().unwrap().knife_owner.set(1);
    g.edit(51, |e| e.class = "knife".into());
    g.edit(2, |e| {
        e.inflictor = Some(51);
        e.dmg_take = 30.0;
    });
    Feed::send(&mut s, &g, Kind::TsHealth, Some(2), |f| TsHealth { hp: 1 }.write(f));
    assert_eq!(g.fired.borrow()[0], Event::Damage { attacker: 1, victim: 2, damage: 30, weapon: 29, aim: 0, ta: 0 });
    // Upstream's numbering: 37.
    g.set_cvar("tsx_legacy_weaponids", 1.0);
    s.server_activate(&g);
    g.edit(2, |e| {
        e.inflictor = Some(51);
        e.dmg_take = 30.0;
    });
    Feed::send(&mut s, &g, Kind::TsHealth, Some(2), |f| TsHealth { hp: 1 }.write(f));
    assert_eq!(g.fired.borrow()[1], Event::Damage { attacker: 1, victim: 2, damage: 30, weapon: 37, aim: 0, ta: 0 });
}

#[test]
fn world_damage_is_self_damage_and_teams_make_ta() {
    let (mut s, g) = setup();
    g.edit(0, |e| e.class = "worldspawn".into());
    g.edit(2, |e| {
        e.inflictor = Some(0);
        e.dmg_take = 12.0;
    });
    Feed::send(&mut s, &g, Kind::TsHealth, Some(2), |f| TsHealth { hp: 88 }.write(f));
    assert_eq!(g.fired.borrow()[0], Event::Damage { attacker: 2, victim: 2, damage: 12, weapon: 0, aim: 0, ta: 0 });

    s.players[1].team_id = 1;
    s.players[2].team_id = 1;
    g.edit(2, |e| {
        e.inflictor = Some(1);
        e.dmg_take = 5.0;
    });
    Feed::send(&mut s, &g, Kind::TsHealth, Some(2), |f| TsHealth { hp: 83 }.write(f));
    assert!(matches!(g.fired.borrow()[1], Event::Damage { ta: 1, .. }));
}

#[test]
fn fire_events_count_shots_of_the_weapon_in_hand() {
    let (mut s, g) = setup();
    s.precache_event("events/fire.sc", 3);
    s.precache_event("events/firelinked.sc", 4);
    s.precache_event("events/reload.sc", 7);
    s.players[1].current = 13;
    for e in [3, 4, 7, 3] {
        s.playback_event(&g, 1, e);
    }
    assert_eq!((s.players[1].weapons[13].stats.shots, s.players[1].life.shots), (3, 3));
}

#[test]
fn bots_are_ignored_without_rankbots() {
    let (mut s, g) = setup();
    g.edit(2, |e| e.bot = true);
    s.rank_bots = false;
    s.players[1].current = 13;
    g.edit(2, |e| {
        e.inflictor = Some(1);
        e.dmg_take = 10.0;
    });
    Feed::send(&mut s, &g, Kind::TsHealth, Some(2), |f| TsHealth { hp: 90 }.write(f));
    assert_eq!(g.fired.borrow().len(), 1, "the forward still runs");
    assert_eq!(s.players[1].life.hits, 0, "but no stats");
}

#[test]
fn rank_follows_lives_and_name_changes() {
    let (mut s, g) = setup();
    s.players[1].life.kills = 2;
    s.server_deactivate(&g);
    assert_eq!(s.rank.at(1).map(|e| (e.name.as_str(), e.score)), Some(("p1", 2)));
    // tsstats_rank 0 keys by name: a rename moves to another entry.
    s.put_in_server(&g, 1);
    s.userinfo_changed(&g, 1, "newname");
    assert_eq!(s.rank.get(s.players[1].rank.unwrap()).unwrap().unique, "newname");
    // Reset on request.
    g.set_cvar("tsstats_reset", 1.0);
    s.server_deactivate(&g);
    assert!(s.rank.is_empty());
    assert_eq!(g.cvar("tsstats_reset"), 0.0);
}

#[test]
fn rank_keys_by_authid_or_ip() {
    let (mut s, g) = setup();
    g.set_cvar("tsstats_rank", 1.0);
    g.edit(1, |e| e.auth = Some("STEAM_0:1:5".into()));
    s.put_in_server(&g, 1);
    assert_eq!(s.rank.get(s.players[1].rank.unwrap()).unwrap().unique, "STEAM_0:1:5");
    g.set_cvar("tsstats_rank", 2.0);
    s.client_connect(2, "192.168.0.7:27005");
    s.put_in_server(&g, 2);
    assert_eq!(s.rank.get(s.players[2].rank.unwrap()).unwrap().unique, "192.168.0.7");
}

#[test]
fn api_refreshes_the_weapon_in_hand_for_bots() {
    let mut s = State::default();
    let g = FakeGame::with_api();
    g.player(1);
    g.api.as_ref().unwrap().info.borrow_mut().insert(1, crate::game::PlayerInfo { weapon: 18, clip: 4, ammo: 20, ..Default::default() });
    s.pre_think(&g, 1);
    assert_eq!((s.players[1].current, s.players[1].weapons[18].clip), (18, 4));
}

#[test]
fn bots_without_the_api_hold_what_their_model_shows() {
    let (mut s, g) = setup();
    g.edit(2, |e| {
        e.bot = true;
        e.weapon_model = "models/p_m4_sil.mdl".into();
    });
    s.pre_think(&g, 2);
    s.pre_think(&g, 1);
    assert_eq!((s.players[2].current, s.players[1].current), (5, 0), "humans keep WeaponInfo's");
}
