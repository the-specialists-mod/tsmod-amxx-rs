// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports the module setup of
// modules/ts/tsx/moduleconfig.cpp (`OnMetaAttach`, `OnAmxxAttach`, `OnPluginsLoaded`,
// `OnAmxxDetach`, `AmxxCheckGame`) and its moduleconfig.h, Copyright (C) The AMX Mod X
// Development Team, Copyright (C) 2004 Lukasz Wlasinski.

//! The FFI layer: exports, hooks, the native table and the real [`Game`](crate::game::Game).
//! The only module of this crate that may use `unsafe`; the rest is `deny(unsafe_code)`.

mod game;
mod hooks;
mod listener;

use crate::natives::{base, bot, stats, world};
use crate::state::with;
use amxx::Cell;
use amxx::engine::engine;
use amxx::meta::MetaDef;
use amxx::module::{ModuleDef, nothing};
use amxx::native::Args;
use core::sync::atomic::Ordering;
use game::{FWD_DAMAGE, FWD_DEATH, FWD_MELEE, FWD_POWERUP, FWD_STUNT, RealGame};

const VERSION: &core::ffi::CStr = match core::ffi::CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
    Ok(v) => v,
    Err(_) => c"0",
};

static MODULE: ModuleDef = ModuleDef {
    name: c"TSX",
    author: c"AMX Mod X Dev Team; Rust port: tsmod-amxx-rs",
    version: VERSION,
    logtag: c"TSX",
    library: c"tsx",
    libclass: c"xstats",
    reload: false,
    check_game: Some(|g| g.eq_ignore_ascii_case("ts")),
    attach: amxx_attach,
    detach: amxx_detach,
    plugins_loaded,
    plugins_unloading,
    plugins_unloaded: nothing,
};

static META: MetaDef = MetaDef {
    name: c"TSX",
    version: VERSION,
    date: c"",
    author: c"AMX Mod X Dev Team; Rust port: tsmod-amxx-rs",
    url: c"https://github.com/the-specialists-mod/tsmod-amxx-rs",
    logtag: c"TSX",
    attach: meta_attach,
    dll: hooks::dll,
    dll_post: hooks::dll_post,
    engine: hooks::engine_pre,
    engine_post: hooks::engine_post,
};

amxx::export_module!(MODULE);
amxx::export_metamod!(META);

/// `OnMetaAttach`: the stats cvars.
fn meta_attach() {
    let Some(e) = engine() else { return };
    e.register_cvar(c"tsstats_maxsize", c"3500", 0);
    e.register_cvar(c"tsstats_reset", c"0", 0);
    e.register_cvar(c"tsstats_rank", c"0", 0);
    e.register_cvar(c"tsstats_rankbots", c"1", 0);
    e.register_cvar(c"tsstats_pause", c"0", 0);
    // Port addition: upstream's weapon ids (kung fu 36, thrown knife 37) for old plugins.
    e.register_cvar(c"tsx_legacy_weaponids", c"0", 0);
}

/// `get_localinfo(name, def)`: the localinfo value, set to `def` first when empty.
fn localinfo(name: &str, def: &str) -> String {
    let Some(e) = engine() else { return def.to_owned() };
    let v = e.localinfo(name);
    if v.is_empty() {
        e.set_localinfo(name, def);
        return def.to_owned();
    }
    v
}

/// `OnAmxxAttach`: natives, the score plugin, and the rank file.
fn amxx_attach() {
    amxx::core::add_natives(&NATIVES);
    // Port addition: Pawn bot backends' server callbacks (`crate::bots`).
    amxx::tsapi::backend::set_dispatch(game::backend_dispatch);
    let calc = localinfo("tsstats_score", "addons/amxmodx/data/tsstats.amxx");
    if !calc.is_empty() {
        let path = amxx::core::build_pathname(&calc);
        match amxx::script::ScoreScript::load(&path) {
            Ok(mut script) => {
                let score: crate::rank::ScoreFn = Box::new(move |s: &crate::stats::Stats| {
                    let body = s.body_hits;
                    match script.score(&s.cells(), &body) {
                        Ok(v) => v,
                        Err(err) => {
                            amxx::core::log(&format!("Run time error {err} on line (plugin \"{path}\")"));
                            0
                        }
                    }
                });
                with(|st| st.rank.set_calc(Some(score)));
            }
            Err(e) => amxx::core::print_srv_console(&format!("[TSX] Couldn't load plugin (file \"{path}\"): {e}")),
        }
    }
    if with(|s| s.rank.is_empty()).unwrap_or(false) {
        let path = amxx::core::build_pathname(&localinfo("tsstats", "addons/amxmodx/data/tsstats.dat"));
        match std::fs::File::open(&path) {
            Ok(f) => {
                let mut r = std::io::BufReader::new(f);
                with(|s| s.rank.load(&mut r));
            }
            Err(_) => amxx::core::log(&format!("Could not load stats file: {path}")),
        }
    }
}

/// `OnAmxxDetach`.
fn amxx_detach() {
    with(|s| {
        s.rank.clear();
        s.rank.set_calc(None);
    });
    // Port addition: the server mustn't call into the library once it is unloaded (its Pawn
    // backends' trampolines, its listener).
    crate::bots::detach_all(&RealGame::new());
    listener::unregister(game::api_table());
}

/// `OnPluginsLoaded`: the two forwards, and (port addition) tsfun.inc's `client_stunt`,
/// `client_powerup` and `Melee_Attack` (`crate::forwards`; floats as cells holding their bits,
/// `Melee_Attack` stopping at the first `PLUGIN_HANDLED`, each with stock tsfun.inc's 4th
/// `UNAVAILABLE` parameter as 0); then the plugin API.
fn plugins_loaded() {
    let forwards = [
        (&FWD_DAMAGE, amxx::core::register_forward(c"client_damage", 6)),
        (&FWD_DEATH, amxx::core::register_forward(c"client_death", 5)),
        (&FWD_STUNT, amxx::core::register_forward(c"client_stunt", 3)),
        (&FWD_POWERUP, amxx::core::register_forward(c"client_powerup", 3)),
        (&FWD_MELEE, amxx::core::register_forward_stop(c"Melee_Attack", 4)),
    ];
    for (id, f) in forwards {
        if let Some(f) = f {
            id.store(f.0, Ordering::Relaxed);
        }
    }
    // Port addition: tsbot.inc's forwards.
    game::register_ts_forwards();
    check_api();
}

/// Port addition: the core is about to drop its plugins and their forwards (`ServerDeactivate`).
/// tsbot.inc's forward ids and the Pawn backends' bindings go; the backends stay registered with
/// the server until the next map's plugin binds them again (`crate::bots`).
fn plugins_unloading() {
    game::forget_ts_forwards();
    with(|s| s.bots.unbind_all());
}

/// Looks the server's plugin API up; logs once when it is missing (the module still works,
/// with the setters failing softly).
fn check_api() {
    use std::sync::atomic::AtomicBool;
    static WARNED: AtomicBool = AtomicBool::new(false);
    match game::find_api() {
        Ok(()) => {}
        Err(why) => {
            if !WARNED.swap(true, Ordering::Relaxed) {
                amxx::core::log(&format!(
                    "WARNING: no TS plugin API ({why}). Needs a Rust TS server with ts-plugin-api; until then the natives that change \
                     players (ts_setusercash, ts_set_message, power-ups, speed, ...) do nothing."
                ));
            }
        }
    }
}

/// Saves the rank (`ServerDeactivate`).
fn save_rank() {
    let path = amxx::core::build_pathname(&localinfo("tsstats", "addons/amxmodx/data/tsstats.dat"));
    let mut buf = Vec::new();
    if with(|s| s.rank.save(&mut buf)).is_some_and(|r| r.is_ok())
        && let Err(e) = std::fs::write(&path, &buf)
    {
        amxx::core::log(&format!("Could not save stats file {path}: {e}"));
    }
}

/// Binds the natives' logic to the real game.
macro_rules! bind {
    ($($name:ident => $f:path),* $(,)?) => {
        $(fn $name(a: &mut dyn Args) -> Cell { $f(&RealGame::new(), a) })*
    };
}

bind! {
    n_get_weapon_name => base::get_weapon_name,
    n_get_weapon_logname => base::get_weapon_logname,
    n_is_melee => base::is_melee,
    n_get_maxweapons => base::get_maxweapons,
    n_get_stats_size => base::get_stats_size,
    n_is_custom => base::is_custom,
    n_wpnlog_to_name => base::wpnlog_to_name,
    n_wpnlog_to_id => base::wpnlog_to_id,
    n_ts_get_user_weapon => base::ts_get_user_weapon,
    n_get_user_cash => base::get_user_cash,
    n_set_user_cash => base::set_user_cash,
    n_get_user_space => base::get_user_space,
    n_get_user_pwup => base::get_user_pwup,
    n_get_user_items => base::get_user_items,
    n_get_killing_streak => base::get_killing_streak,
    n_get_last_frag => base::get_last_frag,
    n_get_kill_flags => base::get_kill_flags,
    n_get_user_slots => base::get_user_slots,
    n_set_user_slots => base::set_user_slots,
    n_get_user_state => base::get_user_state,
    n_give_weapon => base::give_weapon,
    n_create_pwup => base::create_pwup,
    n_give_pwup => base::give_pwup,
    n_has_superjump => base::has_superjump,
    n_has_fupowerup => base::has_fupowerup,
    n_set_user_message => base::set_user_message,
    n_get_user_message => base::get_user_message,
    n_ts_setup => base::ts_setup,
    n_register_forward => base::register_forward,
    n_set_bullettrail => base::set_bullettrail,
    n_set_fake_slowmo => base::set_fake_slowmo,
    n_set_fake_slowpause => base::set_fake_slowpause,
    n_is_in_slowmo => base::is_in_slowmo,
    n_set_speed => base::set_speed,
    n_set_physics_speed => base::set_physics_speed,
    n_is_running_powerup => base::is_running_powerup,
    n_force_powerup_run => base::force_powerup_run,
    n_get_user_weapon => base::get_user_weapon,
    n_set_user_ammo => base::set_user_ammo,
    n_set_fuattack => base::set_fuattack,
    n_ts_get_player_snapshot => world::get_player_snapshot,
    n_ts_get_player_field => world::get_player_field,
    n_ts_get_player_flags => world::get_player_flags,
    n_ts_get_player_team => world::get_player_team,
    n_ts_get_player_health => world::get_player_health,
    n_ts_get_player_stance => world::get_player_stance,
    n_ts_get_player_weapon => world::get_player_weapon,
    n_ts_get_player_powerup => world::get_player_powerup,
    n_ts_get_player_timescale => world::get_player_timescale,
    n_ts_get_player_combat => world::get_player_combat,
    n_ts_get_inventory => world::get_inventory,
    n_ts_get_inventory_ex => world::get_inventory_ex,
    n_ts_get_items => world::get_items,
    n_ts_get_objectives => world::get_objectives,
    n_ts_get_objective_name => world::get_objective_name,
    n_ts_get_gamestate => world::get_gamestate,
    n_ts_get_frameinfo => world::get_frameinfo,
    n_ts_entity_key => world::entity_key,
    n_ts_get_entity_info => world::get_entity_info,
    n_ts_master_triggered => world::master_triggered,
    n_ts_projectile_owner => world::projectile_owner,
    n_ts_bot_register_backend => bot::register_backend,
    n_ts_bot_unregister_backend => bot::unregister_backend,
    n_ts_bot_data_dir => bot::data_dir,
    n_ts_bot_log => bot::log,
    n_ts_bot_create => bot::create,
    n_ts_bot_kick => bot::kick,
    n_ts_bot_runcmd => bot::runcmd,
    n_ts_bot_get_result => bot::get_result,
    n_ts_bot_command => bot::command,
    n_ts_bot_select_weapon => bot::select_weapon,
    n_ts_bot_buy => bot::buy,
    n_ts_bot_buy_item => bot::buy_item,
    n_ts_bot_clear_loadout => bot::clear_loadout,
    n_ts_drop_weapon => bot::drop_weapon,
    n_get_stats => stats::get_stats,
    n_get_statsnum => stats::get_statsnum,
    n_get_user_astats => stats::get_user_astats,
    n_get_user_rstats => stats::get_user_rstats,
    n_get_user_stats => stats::get_user_stats,
    n_get_user_vstats => stats::get_user_vstats,
    n_get_user_wrstats => stats::get_user_wrstats,
    n_get_user_wstats => stats::get_user_wstats,
    n_reset_user_wstats => stats::reset_user_wstats,
    n_register_cwpn => stats::register_cwpn,
    n_cwpn_dmg => stats::cwpn_dmg,
    n_cwpn_shot => stats::cwpn_shot,
}

// `stats_Natives` then `base_Natives`, with upstream's names (NRank.cpp, NBase.cpp).
amxx::natives! {
    pub(crate) static NATIVES = [
        c"get_stats" => n_get_stats,
        c"get_statsnum" => n_get_statsnum,
        c"get_user_astats" => n_get_user_astats,
        c"get_user_rstats" => n_get_user_rstats,
        c"get_user_lstats" => n_get_user_rstats,
        c"get_user_stats" => n_get_user_stats,
        c"get_user_vstats" => n_get_user_vstats,
        c"get_user_wrstats" => n_get_user_wrstats,
        c"get_user_wstats" => n_get_user_wstats,
        c"reset_user_wstats" => n_reset_user_wstats,
        c"custom_weapon_add" => n_register_cwpn,
        c"custom_weapon_dmg" => n_cwpn_dmg,
        c"custom_weapon_shot" => n_cwpn_shot,
        c"xmod_get_wpnname" => n_get_weapon_name,
        c"xmod_get_wpnlogname" => n_get_weapon_logname,
        c"xmod_is_melee_wpn" => n_is_melee,
        c"xmod_get_maxweapons" => n_get_maxweapons,
        c"xmod_get_stats_size" => n_get_stats_size,
        c"xmod_is_custom_wpn" => n_is_custom,
        c"ts_wpnlogtoname" => n_wpnlog_to_name,
        c"ts_wpnlogtoid" => n_wpnlog_to_id,
        c"ts_getuserwpn" => n_ts_get_user_weapon,
        c"ts_getusercash" => n_get_user_cash,
        c"ts_setusercash" => n_set_user_cash,
        c"ts_getuserspace" => n_get_user_space,
        c"ts_getuserpwup" => n_get_user_pwup,
        c"ts_getuseritems" => n_get_user_items,
        c"ts_getkillingstreak" => n_get_killing_streak,
        c"ts_getuserlastfrag" => n_get_last_frag,
        c"ts_getuserkillflags" => n_get_kill_flags,
        c"ts_getuserslots" => n_get_user_slots,
        c"ts_setuserslots" => n_set_user_slots,
        c"ts_getuserstate" => n_get_user_state,
        c"ts_giveweapon" => n_give_weapon,
        c"ts_createpwup" => n_create_pwup,
        c"ts_givepwup" => n_give_pwup,
        c"ts_has_superjump" => n_has_superjump,
        c"ts_has_fupowerup" => n_has_fupowerup,
        c"ts_set_message" => n_set_user_message,
        c"ts_get_message" => n_get_user_message,
        c"ts_setpddata" => n_ts_setup,
        c"register_statsfwd" => n_register_forward,
        c"ts_set_bullettrail" => n_set_bullettrail,
        c"ts_set_fakeslowmo" => n_set_fake_slowmo,
        c"ts_set_fakeslowpause" => n_set_fake_slowpause,
        c"ts_is_in_slowmo" => n_is_in_slowmo,
        c"ts_set_speed" => n_set_speed,
        c"ts_set_physics_speed" => n_set_physics_speed,
        c"ts_is_running_powerup" => n_is_running_powerup,
        c"ts_force_run_powerup" => n_force_powerup_run,
        c"get_weaponname" => n_get_weapon_name,
        c"get_user_weapon" => n_get_user_weapon,
        // Port addition: tsxaddon.inc's function, now a native (include/tsxaddon.inc).
        c"ts_setuserammo" => n_set_user_ammo,
        // Port addition: tsfun.inc's commented-out native for `Melee_Attack` (`crate::forwards`).
        c"ts_set_fuattack" => n_set_fuattack,
        // Port addition: tsbot.inc, the world and bot natives a Pawn bot backend uses
        // (`crate::natives::world`, `crate::natives::bot`).
        c"ts_get_player_snapshot" => n_ts_get_player_snapshot,
        c"ts_get_player_field" => n_ts_get_player_field,
        c"ts_get_player_flags" => n_ts_get_player_flags,
        c"ts_get_player_team" => n_ts_get_player_team,
        c"ts_get_player_health" => n_ts_get_player_health,
        c"ts_get_player_stance" => n_ts_get_player_stance,
        c"ts_get_player_weapon" => n_ts_get_player_weapon,
        c"ts_get_player_powerup" => n_ts_get_player_powerup,
        c"ts_get_player_timescale" => n_ts_get_player_timescale,
        c"ts_get_player_combat" => n_ts_get_player_combat,
        c"ts_get_inventory" => n_ts_get_inventory,
        c"ts_get_inventory_ex" => n_ts_get_inventory_ex,
        c"ts_get_items" => n_ts_get_items,
        c"ts_get_objectives" => n_ts_get_objectives,
        c"ts_get_objective_name" => n_ts_get_objective_name,
        c"ts_get_gamestate" => n_ts_get_gamestate,
        c"ts_get_frameinfo" => n_ts_get_frameinfo,
        c"ts_entity_key" => n_ts_entity_key,
        c"ts_get_entity_info" => n_ts_get_entity_info,
        c"ts_master_triggered" => n_ts_master_triggered,
        c"ts_projectile_owner" => n_ts_projectile_owner,
        c"ts_bot_register_backend" => n_ts_bot_register_backend,
        c"ts_bot_unregister_backend" => n_ts_bot_unregister_backend,
        c"ts_bot_data_dir" => n_ts_bot_data_dir,
        c"ts_bot_log" => n_ts_bot_log,
        c"ts_bot_create" => n_ts_bot_create,
        c"ts_bot_kick" => n_ts_bot_kick,
        c"ts_bot_runcmd" => n_ts_bot_runcmd,
        c"ts_bot_get_result" => n_ts_bot_get_result,
        c"ts_bot_command" => n_ts_bot_command,
        c"ts_bot_select_weapon" => n_ts_bot_select_weapon,
        c"ts_bot_buy" => n_ts_bot_buy,
        c"ts_bot_buy_item" => n_ts_bot_buy_item,
        c"ts_bot_clear_loadout" => n_ts_bot_clear_loadout,
        c"ts_drop_weapon" => n_ts_drop_weapon,
    ];
}

#[cfg(test)]
mod tests {
    use super::NATIVES;

    #[test]
    fn native_table_has_every_upstream_name_once() {
        let names: Vec<String> = NATIVES
            .iter()
            .take_while(|n| !n.name.is_null())
            // SAFETY: the table's names are `'static` C string literals.
            .map(|n| unsafe { core::ffi::CStr::from_ptr(n.name) }.to_string_lossy().into())
            .collect();
        // 13 in stats_Natives and 39 in base_Natives (NRank.cpp, NBase.cpp), plus ts_setuserammo
        // and ts_set_fuattack, plus tsbot.inc's 21 world and 14 bot natives.
        assert_eq!(names.len(), 54 + 21 + 14);
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
        assert!(NATIVES.last().unwrap().name.is_null());
    }

    #[test]
    fn tsbot_inc_declares_every_tsbot_native_and_forward() {
        let inc = include_str!("../../../../include/tsbot.inc");
        let declared = |kind: &str, name: &str| inc.lines().any(|l| l.starts_with(kind) && l.contains(&format!("{name}(")));
        let names: Vec<String> = NATIVES
            .iter()
            .take_while(|n| !n.name.is_null())
            // SAFETY: as above.
            .map(|n| unsafe { core::ffi::CStr::from_ptr(n.name) }.to_string_lossy().into())
            .collect();
        let tsbot = &names[54..];
        assert_eq!(tsbot.len(), 35);
        for n in tsbot {
            assert!(declared("native ", n), "tsbot.inc doesn't declare native {n}");
        }
        for f in super::game::TsForward::ALL {
            let n = f.signature().0.to_string_lossy();
            assert!(declared("forward ", &n), "tsbot.inc doesn't declare forward {n}");
        }
        for n in ["ts_bot_add", "ts_bot_activate", "ts_bot_deactivate", "ts_bot_adopt"] {
            assert!(declared("forward ", n), "tsbot.inc doesn't declare forward {n}");
        }
    }
}
