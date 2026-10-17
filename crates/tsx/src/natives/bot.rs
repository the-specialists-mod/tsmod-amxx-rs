// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Port addition: tsbot.inc's bot natives. A Pawn plugin registers itself as a bot backend
//! (`crate::bots`), makes its bots and drives them each `ts_frame` through the server's plugin
//! API (versions 3, 6 and 7: `create_bot`, `run_cmd`, `bot_command`, the loadout functions).
//!
//! They return the API's status (`TSBOT_OK` 0, `TSBOT_DEFERRED` 1, negative for failures: the
//! API's codes, and tsx's `TSBOT_NO_API`, `TSBOT_NO_BACKEND`, `TSBOT_ALREADY`, `TSBOT_NO_SLOT`)
//! and raise no native errors, so a bot's think can check results without flooding the log.
//! Strings the game takes as bytes (names, models, command lines) are passed on as the plugin's
//! bytes, not converted to UTF-8.

use super::world::{fc, out, out_array};
use crate::bots::{self, status};
use crate::game::{BotCmd, BotCmdResult, Game, TsApi};
use crate::state::with;
use amxx::Cell;
use amxx::native::Args;
use ts_plugin_api::cmd;

/// `TSBOT_NO_API`, for natives on a server without the plugin API.
pub(super) fn no_api() -> Cell {
    status::NO_API
}

/// Runs `f` with the API, or returns `TSBOT_NO_API`.
fn with_api(g: &dyn Game, f: impl FnOnce(&dyn TsApi) -> Cell) -> Cell {
    g.api().map_or(status::NO_API, f)
}

/// `BotCmdResult` as `TSRES_*` cells.
pub fn result_cells(r: &BotCmdResult) -> Vec<Cell> {
    let mut v: Vec<Cell> = [r.v_angle, r.origin, r.velocity].iter().flat_map(|x| x.map(fc)).collect();
    v.extend([r.events, r.msec]);
    v
}

/// `ts_bot_register_backend(const name[])`: makes the calling plugin the bot backend `name` (the
/// identity's data directory is `<gamedir>/tsmod/<name>/`); `TSBOT_OK` (also when it is already
/// this plugin's, and on later maps, where the same name binds the existing backend again) or a
/// status: `TSBOT_INVALID` (not 1 to 32 letters, digits, `_` or `-`), `TSBOT_EXISTS` (a server
/// plugin or another AMX Mod X plugin has the name), `TSBOT_ALREADY`, `TSBOT_NO_SLOT`,
/// `TSBOT_NO_API`. When `ts_bot_backend` names it, the backend takes over at once;
/// `ts_bot_activate` then comes at the next `ts_frame`.
pub fn register_backend(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let name = a.string(1);
    bots::register(g, a.amx(), &name)
}

/// `ts_bot_unregister_backend()`: gives the plugin's backend up for good (the server kicks its
/// bots if it was the active one, and native takes over); `TSBOT_OK` or a status.
pub fn unregister_backend(g: &dyn Game, a: &mut dyn Args) -> Cell {
    bots::unregister(g, a.amx())
}

/// The calling plugin's backend identity, or the status saying why there is none.
fn identity(g: &dyn Game, a: &dyn Args) -> Result<i32, Cell> {
    if g.api().is_none() {
        return Err(status::NO_API);
    }
    bots::plugin_of(a.amx()).ok_or(status::NO_BACKEND)
}

/// `ts_bot_data_dir(out[], len)`: the backend's data directory (`<gamedir>/tsmod/<name>/`, with
/// the trailing slash, created on first use); its length, or a status (`out` empty).
pub fn data_dir(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let len = a.cell(2);
    let dir = identity(g, a).and_then(|id| g.api().map_or(Err(status::NO_API), |api| api.data_dir(id)));
    match dir {
        Ok(d) => {
            a.set_string(1, &d, len);
            d.len() as Cell
        }
        Err(rc) => {
            a.set_string(1, "", len);
            rc
        }
    }
}

/// `ts_bot_log(level, const msg[])`: a server console line, `[<backend name>] msg` (`[plugin]`
/// before the plugin registered a backend); `TSLOG_DEBUG` lines only with `developer` on.
pub fn log(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let (level, msg) = (a.cell(1), a.string(2));
    let id = bots::plugin_of(a.amx()).unwrap_or(ts_plugin_api::NO_PLUGIN);
    with_api(g, |api| {
        api.api_log(id, level, &msg);
        ts_plugin_api::OK
    })
}

/// `ts_bot_create(const name[], const model[] = "", Float:skill = 0.0)`: a bot of the calling
/// plugin's backend, joined (in teamplay on the team `model` names) and observing until its
/// `respawn` command, as a joining TS player: its index, or
/// a status (`TSBOT_FULL`, `TSBOT_INVALID`: an empty name or the backend isn't the active one,
/// `TSBOT_NO_BACKEND`). `skill` is only reported. Call it from `ts_bot_add` to answer the
/// server's quota and `addbot`, or any time the backend is active.
pub fn create(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let id = match identity(g, a) {
        Ok(id) => id,
        Err(rc) => return rc,
    };
    let name = a.string_bytes(1);
    let model = if a.count() >= 2 { a.string_bytes(2) } else { Vec::new() };
    let skill = if a.count() >= 3 { a.float(3) } else { 0.0 };
    with_api(g, |api| api.create_bot(id, &name, &model, skill))
}

/// `ts_bot_kick(id)`: kicks a bot (queued as `kick #<userid>`).
pub fn kick(g: &dyn Game, a: &mut dyn Args) -> Cell {
    with_api(g, |api| api.kick_bot(a.cell(1)))
}

/// `ts_bot_runcmd(id, const Float:angles[3], Float:fwd, Float:side, Float:up, buttons, impulse =
/// 0, flags = TSBOT_SETVIEW | TSBOT_AUTOMSEC, msec = 0, Float:out_vangle[3] = {...}, &events =
/// 0)`: runs one usercmd for the bot now (its think, movement and post-think), once per
/// `ts_frame`. `TSBOT_OK` (then `out_vangle` holds the view the bot has, the game's after a
/// `TSBOT_EV_FIXANGLE`, and `events` the `TSBOT_EV_*` bits; `ts_bot_get_result` has the rest), or
/// a status: `TSBOT_BUSY` (called inside one of the bot's own callbacks: run bots from
/// `ts_frame`), `TSBOT_INVALID` (a human, or a bad `msec`/impulse/angle), `TSBOT_NOT_PLAYER`.
pub fn runcmd(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let Some(api) = g.api() else { return status::NO_API };
    let id = a.cell(1);
    let ang = a.read_array(2, 3);
    let opt = |a: &dyn Args, i: usize, def: Cell| if a.count() >= i { a.cell(i) } else { def };
    let c = BotCmd {
        angles: [0, 1, 2].map(|k| f32::from_bits(ang[k] as u32)),
        forward: a.float(3),
        side: a.float(4),
        up: a.float(5),
        buttons: a.cell(6),
        impulse: opt(a, 7, 0),
        flags: opt(a, 8, cmd::SET_VIEW | cmd::AUTO_MSEC),
        msec: opt(a, 9, 0),
    };
    // The bot's think runs inside (plugins may hear it): nothing is held.
    match api.run_cmd(id, &c) {
        Ok(r) => {
            if let Ok(i) = usize::try_from(id) {
                with(|s| {
                    if let Some(slot) = s.bots.results.get_mut(i) {
                        *slot = Some(r);
                    }
                });
            }
            out_array(a, 10, &r.v_angle.map(fc), 3);
            out(a, 11, r.events);
            ts_plugin_api::OK
        }
        Err(rc) => rc,
    }
}

/// `ts_bot_get_result(id, data[TSRES_SIZE])`: what the bot's last successful `ts_bot_runcmd` did
/// (view angles, origin, velocity, events, msec); 1, or 0 when it has run none since it joined.
pub fn get_result(_g: &dyn Game, a: &mut dyn Args) -> Cell {
    let r = usize::try_from(a.cell(1)).ok().and_then(|i| with(|s| s.bots.results.get(i).copied().flatten()).flatten());
    match r {
        Some(r) => {
            out_array(a, 2, &result_cells(&r), super::world::layout::size(super::world::layout::TSRES));
            1
        }
        None => 0,
    }
}

/// `ts_bot_command(id, const line[])`: runs a client command as if the player typed it (`use`,
/// `usepowerup`, `respawn`, `say hi`); `TSBOT_OK` (also for one the game doesn't know),
/// `TSBOT_DEFERRED` or a status.
pub fn command(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let line = a.string_bytes(2);
    with_api(g, |api| api.bot_command(a.cell(1), &line))
}

/// `ts_bot_select_weapon(id, wid)`: switches to the server's weapon id `wid` (0 kung fu) as
/// `weapon_<id>` does (TS ignores a usercmd's `weaponselect`).
pub fn select_weapon(g: &dyn Game, a: &mut dyn Args) -> Cell {
    with_api(g, |api| api.select_weapon(a.cell(1), a.cell(2)))
}

/// `ts_bot_buy(id, weapon, attachments = 0, akimbo = 0, ammo = 0)`: adds a weapon to the
/// loadout bought at the next spawn (`TSA_*` attachments, a second copy for akimbo, `ammo`
/// purchases of its ammo). Death clears the loadout (TS's `Killed`), so bots buy while dead.
pub fn buy(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let opt = |i: usize| if a.count() >= i { a.cell(i) } else { 0 };
    let (att, akimbo, ammo) = (opt(3), opt(4), opt(5));
    with_api(g, |api| api.buy(a.cell(1), a.cell(2), att, akimbo, ammo))
}

/// `ts_bot_buy_item(id, item)`: adds `TSLOAD_KEVLAR` or `TSLOAD_STEALTHSHOES` to the loadout.
pub fn buy_item(g: &dyn Game, a: &mut dyn Args) -> Cell {
    with_api(g, |api| api.buy_item(a.cell(1), a.cell(2)))
}

/// `ts_bot_clear_loadout(id)`: empties the loadout.
pub fn clear_loadout(g: &dyn Game, a: &mut dyn Args) -> Cell {
    with_api(g, |api| api.clear_loadout(a.cell(1)))
}

/// `ts_drop_weapon(id, wid = -1)`: drops weapon `wid`, or the one in hand for -1.
pub fn drop_weapon(g: &dyn Game, a: &mut dyn Args) -> Cell {
    let w = if a.count() >= 2 { a.cell(2) } else { -1 };
    with_api(g, |api| api.drop_weapon(a.cell(1), w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeGame;
    use amxx::native::FakeArgs;

    const PLUGIN: usize = 0x2000;

    fn registered() -> FakeGame {
        let g = FakeGame::with_api();
        let mut a = FakeArgs::new(&[]);
        a.amx = PLUGIN;
        let n = a.alloc_str("rcbot_amxx");
        a.params = vec![n];
        assert_eq!(register_backend(&g, &mut a), 0);
        g
    }

    fn args(params: &[Cell]) -> FakeArgs {
        let mut a = FakeArgs::new(params);
        a.amx = PLUGIN;
        a
    }

    #[test]
    fn backend_natives_need_a_registration() {
        let g = FakeGame::with_api();
        let mut a = args(&[]);
        let buf = a.alloc(vec![7; 64]);
        a.params = vec![buf, 63];
        assert_eq!(data_dir(&g, &mut a), status::NO_BACKEND);
        assert_eq!(a.read_str(buf), "");
        let n = a.alloc_str("Neo");
        a.params = vec![n];
        assert_eq!(create(&g, &mut a), status::NO_BACKEND);
        assert_eq!(unregister_backend(&g, &mut args(&[])), status::NO_BACKEND);
        assert_eq!(create(&FakeGame::new(), &mut a), status::NO_API);
        assert_eq!(register_backend(&FakeGame::new(), &mut a), status::NO_API);
        // Without a backend, logs go out as NO_PLUGIN's.
        let m = a.alloc_str("hello");
        a.params = vec![1, m];
        assert_eq!(log(&g, &mut a), 0);
        assert_eq!(g.api.as_ref().unwrap().calls.borrow().last().unwrap(), "log 0 1 hello");
    }

    #[test]
    fn create_data_dir_and_log_use_the_identity() {
        let g = registered();
        let api = g.api.as_ref().unwrap();
        let mut a = args(&[]);
        let buf = a.alloc(vec![0; 64]);
        a.params = vec![buf, 63];
        assert_eq!(data_dir(&g, &mut a), "ts/tsmod/rcbot_amxx/".len() as Cell);
        assert_eq!(a.read_str(buf), "ts/tsmod/rcbot_amxx/");
        // Names go as the plugin's bytes (a CP1252 é), not UTF-8.
        let name = a.alloc(vec![b'N' as Cell, 0xe9, 0]);
        let model = a.alloc_str("seal");
        api.create_result.set(4);
        a.params = vec![name, model, fc(0.75)];
        assert_eq!(create(&g, &mut a), 4);
        assert_eq!(api.calls.borrow().last().unwrap(), "create_bot 101 [78, 233] [115, 101, 97, 108] 0.75");
        a.params = vec![name];
        create(&g, &mut a);
        assert_eq!(api.calls.borrow().last().unwrap(), "create_bot 101 [78, 233] [] 0", "model and skill default");
        api.create_result.set(ts_plugin_api::FULL);
        assert_eq!(create(&g, &mut a), ts_plugin_api::FULL);
        let m = a.alloc_str("thinking");
        a.params = vec![3, m];
        log(&g, &mut a);
        assert_eq!(api.calls.borrow().last().unwrap(), "log 101 3 thinking");
        assert_eq!(unregister_backend(&g, &mut args(&[])), 0);
        assert_eq!(api.calls.borrow().last().unwrap(), "detach 101");
    }

    #[test]
    fn runcmd_passes_the_command_and_keeps_the_result() {
        let g = registered();
        let api = g.api.as_ref().unwrap();
        let result =
            BotCmdResult { v_angle: [1.0, 90.0, 0.0], origin: [10.0, 20.0, 30.0], velocity: [250.0, 0.0, 0.0], events: 0x2, msec: 10 };
        api.cmd_result.set(result);
        let mut a = args(&[]);
        let ang = a.alloc_floats(&[5.0, 45.0, 0.0]);
        let vang = a.alloc(vec![0; 3]);
        let ev = a.alloc(vec![0]);
        a.params = vec![3, ang, fc(400.0), fc(-100.0), fc(0.0), 2 | 1, 0, 1 | 2, 0, vang, ev];
        assert_eq!(runcmd(&g, &mut a), 0);
        let (p, c) = api.last_cmd.get().unwrap();
        assert_eq!(p, 3);
        assert_eq!(
            c,
            BotCmd {
                angles: [5.0, 45.0, 0.0],
                forward: 400.0,
                side: -100.0,
                up: 0.0,
                buttons: 3,
                impulse: 0,
                msec: 0,
                flags: cmd::SET_VIEW | cmd::AUTO_MSEC
            }
        );
        assert_eq!(a.read_floats(vang, 3), [1.0, 90.0, 0.0]);
        assert_eq!(a.memory[ev as usize][0], 2);
        // The defaults when the plugin passes only the movement.
        a.params = vec![3, ang, fc(1.0), fc(2.0), fc(3.0), 0];
        runcmd(&g, &mut a);
        assert_eq!(api.last_cmd.get().unwrap().1.flags, cmd::SET_VIEW | cmd::AUTO_MSEC);
        // The kept result.
        let res = a.alloc(vec![0; 11]);
        a.params = vec![3, res];
        assert_eq!(get_result(&g, &mut a), 1);
        assert_eq!(a.read_floats(res, 9), [1.0, 90.0, 0.0, 10.0, 20.0, 30.0, 250.0, 0.0, 0.0]);
        assert_eq!(a.memory[res as usize][9..], [2, 10]);
        a.params = vec![4, res];
        assert_eq!(get_result(&g, &mut a), 0);
        // A disconnect forgets it.
        with(|s| s.client_disconnect(&g, 3));
        a.params = vec![3, res];
        assert_eq!(get_result(&g, &mut a), 0);
        // Failures pass the status through and write nothing.
        api.run_result.set(ts_plugin_api::BUSY);
        a.memory[ev as usize][0] = 9;
        a.params = vec![3, ang, fc(0.0), fc(0.0), fc(0.0), 0, 0, 3, 0, vang, ev];
        assert_eq!(runcmd(&g, &mut a), ts_plugin_api::BUSY);
        assert_eq!(a.memory[ev as usize][0], 9);
        assert_eq!(runcmd(&FakeGame::new(), &mut a), status::NO_API);
    }

    #[test]
    fn actions_go_through_the_api() {
        let g = registered();
        let api = g.api.as_ref().unwrap();
        let mut a = args(&[]);
        let line = a.alloc_str("usepowerup");
        a.params = vec![3, line];
        assert_eq!(command(&g, &mut a), 0);
        assert_eq!(select_weapon(&g, &mut args(&[3, 13])), 0);
        assert_eq!(buy(&g, &mut args(&[3, 13, 2, 1, 4])), 0);
        assert_eq!(buy(&g, &mut args(&[3, 1])), 0);
        assert_eq!(buy_item(&g, &mut args(&[3, 0x10])), 0);
        assert_eq!(clear_loadout(&g, &mut args(&[3])), 0);
        assert_eq!(drop_weapon(&g, &mut args(&[3])), 0);
        assert_eq!(drop_weapon(&g, &mut args(&[3, 13])), 0);
        assert_eq!(kick(&g, &mut args(&[3])), 0);
        let calls = api.calls.borrow().clone();
        let n = calls.len();
        assert_eq!(
            calls[n - 9..],
            [
                format!("bot_command 3 {:?}", b"usepowerup"),
                "select_weapon 3 13".into(),
                "buy 3 13 2 1 4".into(),
                "buy 3 1 0 0 0".into(),
                "buy_item 3 16".into(),
                "clear_loadout 3".into(),
                "drop_weapon 3 -1".into(),
                "drop_weapon 3 13".into(),
                "kick_bot 3".into(),
            ]
        );
        api.result.set(ts_plugin_api::INVALID);
        assert_eq!(select_weapon(&g, &mut args(&[3, 40])), ts_plugin_api::INVALID);
        assert_eq!(select_weapon(&FakeGame::new(), &mut args(&[3, 1])), status::NO_API);
    }
}
