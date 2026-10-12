// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Natives against a fake AMX ([`FakeArgs`]) and a fake game. Each test runs on its own thread,
//! so each has its own module state.

use super::{base, stats};
use crate::game::{Event, PlayerInfo};
use crate::state::with;
use crate::testing::FakeGame;
use amxx::Cell;
use amxx::native::FakeArgs;

fn joined(g: &FakeGame, ids: &[i32]) {
    for &i in ids {
        g.player(i);
        with(|s| s.put_in_server(g, i));
    }
}

#[test]
fn weapon_names_and_lognames() {
    let g = FakeGame::new();
    let mut a = FakeArgs::new(&[]);
    let buf = a.alloc(vec![0; 32]);
    a.params = vec![13, buf, 31];
    assert_eq!(base::get_weapon_name(&g, &mut a), 4);
    assert_eq!(a.read_str(buf), "AK47");
    a.params = vec![4, buf, 31];
    base::get_weapon_logname(&g, &mut a);
    assert_eq!(a.read_str(buf), "benelli_m3");
    a.params = vec![44, buf, 31];
    assert_eq!(base::get_weapon_name(&g, &mut a), 0);
    assert_eq!(a.errors, ["Weapon 44 is not valid"]);

    let log = a.alloc_str("desert_eagle");
    a.params = vec![log];
    assert_eq!(base::wpnlog_to_id(&g, &mut a), 12);
    a.params = vec![log, buf, 5];
    assert_eq!(base::wpnlog_to_name(&g, &mut a), 5);
    assert_eq!(a.read_str(buf), "Deser", "cut to len like MF_SetAmxString");

    let mut m = FakeArgs::new(&[0]);
    assert_eq!(base::is_melee(&g, &mut m), 0);
    assert_eq!(m.errors.len(), 1, "0 isn't a valid id for xmod_is_melee_wpn");
    m.params = vec![34];
    assert_eq!(base::is_melee(&g, &mut m), 1);
    m.params = vec![39];
    assert_eq!((base::is_custom(&g, &mut m), base::get_maxweapons(&g, &mut m), base::get_stats_size(&g, &mut m)), (1, 44, 8));
}

#[test]
fn getuserwpn_from_messages_without_the_api() {
    let g = FakeGame::new();
    joined(&g, &[1]);
    with(|s| {
        s.players[1].current = 5;
        let w = &mut s.players[1].weapons[5];
        (w.clip, w.ammo, w.mode, w.attach) = (30, 60, 2, 3);
    });
    let mut a = FakeArgs::new(&[]);
    let refs: Vec<Cell> = (0..4).map(|_| a.alloc(vec![0])).collect();
    a.params = [&[1], &refs[..]].concat();
    assert_eq!(base::ts_get_user_weapon(&g, &mut a), 5);
    assert_eq!(refs.iter().map(|&r| a.memory[r as usize][0]).collect::<Vec<_>>(), [30, 60, 2, 3]);
    // Only the passed references are written.
    let mut b = FakeArgs::new(&[1]);
    assert_eq!(base::ts_get_user_weapon(&g, &mut b), 5);
    // Invalid and absent players.
    let mut c = FakeArgs::new(&[33]);
    assert_eq!(base::ts_get_user_weapon(&g, &mut c), 0);
    assert_eq!(c.errors, ["Player 33 is not valid"]);
    let mut d = FakeArgs::new(&[7]);
    assert_eq!(base::ts_get_user_weapon(&g, &mut d), 0);
    assert!(d.errors.is_empty());
}

#[test]
fn getters_prefer_the_api() {
    let g = FakeGame::with_api();
    joined(&g, &[1]);
    with(|s| s.players[1].money = 100);
    let info = PlayerInfo {
        joined: 1,
        cash: 7000,
        free_slots: 33,
        award: 0,
        kill_streak: 4,
        stored_powerup: 16,
        stored_powerup_value: 20,
        kungfu: 1,
        superjump: 0,
        time_scale: 0.5,
        powerup_until: 42.5,
        active_powerup: 1,
        weapon: 0,
        clip: 80,
        ..Default::default()
    };
    g.api.as_ref().unwrap().info.borrow_mut().insert(1, info);
    let mut a = FakeArgs::new(&[1]);
    assert_eq!(base::get_user_cash(&g, &mut a), 7000);
    assert_eq!(base::get_user_space(&g, &mut a), 33);
    assert_eq!(base::get_user_slots(&g, &mut a), 33);
    assert_eq!(base::get_killing_streak(&g, &mut a), 4);
    assert_eq!(base::get_user_items(&g, &mut a), 1);
    assert_eq!((base::has_fupowerup(&g, &mut a), base::has_superjump(&g, &mut a)), (1, 0));
    assert_eq!(base::get_user_message(&g, &mut a), 6, "award 0 reads as TSMSG_NORMAL");
    assert_eq!(base::is_running_powerup(&g, &mut a), 1);
    assert_eq!(f32::from_bits(base::is_in_slowmo(&g, &mut a) as u32), 42.5);
    assert_eq!(base::ts_get_user_weapon(&g, &mut a), 36, "API weapon 0 is kung fu");
    let v = a.alloc(vec![0]);
    a.params = vec![1, v];
    assert_eq!(base::get_user_pwup(&g, &mut a), 16);
    assert_eq!(a.memory[v as usize][0], 20);
}

#[test]
fn setters_call_the_api() {
    let g = FakeGame::with_api();
    joined(&g, &[2]);
    let f = |x: f32| x.to_bits() as Cell;
    type Native = fn(&dyn crate::game::Game, &mut dyn amxx::native::Args) -> Cell;
    let calls: [(Native, Vec<Cell>, Cell, &str); 10] = [
        (base::set_user_cash, vec![2, 500], 0, "set_cash 2 500"),
        (base::set_user_slots, vec![2, 40], 1, "set_free_slots 2 40"),
        (base::set_user_message, vec![2, 4], 1, "set_award 2 4"),
        (base::set_user_message, vec![2, 6], 1, "set_award 2 0"),
        (base::set_bullettrail, vec![2, 1], 1, "set_bullet_trail 2 true"),
        (base::set_fake_slowmo, vec![2, f(5.0)], 1, "run_powerup 2 1 5"),
        (base::set_fake_slowpause, vec![2, f(2.5)], 1, "run_powerup 2 8 2.5"),
        (base::force_powerup_run, vec![2, 16], 1, "run_powerup 2 16 -1"),
        (base::set_speed, vec![2, f(0.5), f(200.0), f(10.0)], 1, "set_speed 2 0.5 200 10"),
        (base::set_physics_speed, vec![2, f(-1.0)], 1, "set_physics_speed 2 -1"),
    ];
    for (n, params, ret, call) in calls {
        let mut a = FakeArgs::new(&params);
        assert_eq!(n(&g, &mut a), ret, "{call}");
        assert_eq!(g.api.as_ref().unwrap().calls.borrow().last().map(String::as_str), Some(call));
        assert!(a.errors.is_empty(), "{call}: {:?}", a.errors);
    }
    assert_eq!(with(|s| s.players[2].money), Some(500));
    let mut bad = FakeArgs::new(&[2, 17]);
    assert_eq!(base::set_user_message(&g, &mut bad), 0);
    assert_eq!(bad.errors, ["Invalid message id: 17"]);
    g.api.as_ref().unwrap().result.set(ts_plugin_api::INVALID);
    for _ in 0..2 {
        let mut inv = FakeArgs::new(&[2, -5]);
        assert_eq!(base::set_user_slots(&g, &mut inv), 0);
        assert!(inv.errors.is_empty(), "upstream raised no error here");
    }
    assert_eq!(*g.logs.borrow(), ["ts_setuserslots: the server rejected a value as out of range (logged once)"]);
}

#[test]
fn setters_without_the_api_fail_softly_and_log_once() {
    let g = FakeGame::new();
    joined(&g, &[1]);
    for _ in 0..3 {
        let mut a = FakeArgs::new(&[1, 100]);
        assert_eq!(base::set_user_slots(&g, &mut a), 0);
        assert!(a.errors.is_empty());
    }
    assert_eq!(g.logs.borrow().len(), 1);
    // A user native on a player not in game logs the error but carries on (BEGIN_USER_FUNC).
    let mut a = FakeArgs::new(&[9, 1]);
    assert_eq!(base::set_bullettrail(&g, &mut a), 0);
    assert_eq!(a.errors, ["Player 9 is not ingame"]);
}

#[test]
fn giveweapon_and_powerups() {
    let g = FakeGame::new();
    joined(&g, &[1]);
    let mut a = FakeArgs::new(&[1, 13, 200, 8]);
    assert_eq!(base::give_weapon(&g, &mut a), 1);
    assert_eq!(*g.given.borrow(), [(1, 13, 200, 8)]);
    g.edit(1, |e| e.alive = false);
    assert_eq!(base::give_weapon(&g, &mut a), 0, "the dead get nothing");
    g.edit(1, |e| e.alive = true);

    let mut c = FakeArgs::new(&[16]);
    assert_eq!(base::create_pwup(&g, &mut c), 100, "origin is optional (tsfun.inc declares one argument)");
    let mut p = FakeArgs::new(&[1, 100]);
    assert_eq!(base::give_pwup(&g, &mut p), 1);
    let mut q = FakeArgs::new(&[1, 55]);
    assert_eq!(base::give_pwup(&g, &mut q), 0, "not a ts_powerup");
}

#[test]
fn stats_natives() {
    let g = FakeGame::new();
    joined(&g, &[1, 2]);
    with(|s| {
        crate::player::save_hit(&mut s.players, 1, 2, 13, 39, 1);
        s.players[1].save_shot(13);
        crate::player::save_kill(&mut s.players, 1, 2, 13, 1, 0);
    });
    let mut a = FakeArgs::new(&[]);
    let (st, body, name) = (a.alloc(vec![0; 8]), a.alloc(vec![0; 8]), a.alloc(vec![0; 32]));
    a.params = vec![1, 13, st, body];
    assert_eq!(stats::get_user_wstats(&g, &mut a), 1);
    // kills, deaths, hs, tks, shots, hits, damage
    assert_eq!(a.memory[st as usize][..7], [1, 0, 1, 0, 1, 1, 39]);
    assert_eq!(a.memory[body as usize][1], 1);
    a.params = vec![1, 12, st, body];
    assert_eq!(stats::get_user_wstats(&g, &mut a), 0, "no shots with that weapon");
    a.params = vec![2, 1, st, body, name, 31];
    assert_eq!(stats::get_user_astats(&g, &mut a), 1);
    assert_eq!(a.read_str(name), "AK47");
    a.params = vec![1, 2, st, body, name, 31];
    assert_eq!(stats::get_user_vstats(&g, &mut a), 1);
    a.params = vec![1, 33, st, body, name, 31];
    assert_eq!(stats::get_user_vstats(&g, &mut a), 0);
    assert_eq!(a.errors, ["Invalid victim 33"]);

    // The rank: commit p1's life, then read it back.
    with(|s| s.server_deactivate(&g));
    with(|s| s.put_in_server(&g, 1));
    a.params = vec![1, st, body];
    assert_eq!(stats::get_user_stats(&g, &mut a), 1);
    assert_eq!(a.memory[st as usize], [1, 0, 1, 0, 1, 1, 39, 1]);
    let mut n = FakeArgs::new(&[]);
    assert_eq!(stats::get_statsnum(&g, &mut n), 2);
    let (st2, body2, name2) = (n.alloc(vec![0; 8]), n.alloc(vec![0; 8]), n.alloc(vec![0; 32]));
    n.params = vec![0, st2, body2, name2, 31];
    assert_eq!(stats::get_stats(&g, &mut n), 1, "returns the next index while more follow");
    assert_eq!(n.read_str(name2), "p1");
    n.params = vec![1, st2, body2, name2, 31];
    assert_eq!(stats::get_stats(&g, &mut n), 0, "last entry");
    n.params = vec![5, st2, body2, name2, 31];
    assert_eq!(stats::get_stats(&g, &mut n), 0);

    let mut r = FakeArgs::new(&[1]);
    assert_eq!(stats::reset_user_wstats(&g, &mut r), 1);
    assert_eq!(with(|s| s.players[1].weapons[13].stats.shots), Some(0));
}

#[test]
fn custom_weapons() {
    let g = FakeGame::new();
    joined(&g, &[1, 2]);
    let mut a = FakeArgs::new(&[]);
    let (n, l) = (a.alloc_str("Laser"), a.alloc_str("laser"));
    a.params = vec![n, 0, l];
    assert_eq!(stats::register_cwpn(&g, &mut a), 39);
    let mut d = FakeArgs::new(&[39, 1, 2, 25, 1]);
    assert_eq!(stats::cwpn_dmg(&g, &mut d), 1);
    assert_eq!(*g.fired.borrow(), [Event::Damage { attacker: 1, victim: 2, damage: 25, weapon: 39, aim: 1, ta: 1 }]);
    g.edit(2, |e| e.alive = false);
    stats::cwpn_dmg(&g, &mut d);
    assert_eq!(g.fired.borrow()[2], Event::Death { killer: 1, victim: 2, weapon: 39, aim: 1, tk: 1 });
    let mut bad = FakeArgs::new(&[13, 1, 2, 25, 1]);
    assert_eq!(stats::cwpn_dmg(&g, &mut bad), 0);
    assert_eq!(bad.errors, ["Invalid weapon 13"]);
    let mut shot = FakeArgs::new(&[39, 1]);
    assert_eq!(stats::cwpn_shot(&g, &mut shot), 1);
    assert_eq!(with(|s| s.players[1].weapons[39].stats.shots), Some(1));
}
