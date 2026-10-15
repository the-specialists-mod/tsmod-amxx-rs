// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. `give_weapon`,
// `create_powerup` and `give_powerup` port the entity code of NBase.cpp's `give_weapon`,
// `create_pwup` and `give_pwup`, Copyright (C) The AMX Mod X Development Team.

//! The real [`Game`]: the engine through metamod, the game library's tables, AMX Mod X, and
//! the server's plugin API (looked up through the game library's `CreateInterface`).

use crate::game::{Event, Game, PlayerInfo, TsApi};
use amxx::core::Forward;
use amxx::engine::{Edict, engine};
use amxx::meta::gamedll;
use core::ffi::c_int;
use core::sync::atomic::{AtomicI32, AtomicPtr, Ordering};
use goldsrc_sys::{DEAD_NO, FL_CLIENT, FL_FAKECLIENT};
use std::ffi::CString;
use ts_plugin_api::PluginApi;

/// Forward ids (`client_damage`, `client_death`), -1 before `AMXX_PluginsLoaded`.
pub static FWD_DAMAGE: AtomicI32 = AtomicI32::new(-1);
pub static FWD_DEATH: AtomicI32 = AtomicI32::new(-1);
/// Port addition: tsfun.inc's `client_stunt`, `client_powerup`, `Melee_Attack` (`crate::forwards`).
pub static FWD_STUNT: AtomicI32 = AtomicI32::new(-1);
pub static FWD_POWERUP: AtomicI32 = AtomicI32::new(-1);
pub static FWD_MELEE: AtomicI32 = AtomicI32::new(-1);

static API: AtomicPtr<PluginApi> = AtomicPtr::new(core::ptr::null_mut());

/// The plugin API table, once [`find_api`] found it.
pub fn api_table() -> Option<&'static PluginApi> {
    // SAFETY: NULL or the server's static table (`find_api` checked its version and size), which
    // lives as long as the game library.
    unsafe { API.load(Ordering::Relaxed).as_ref() }
}

/// Looks the API up in the loaded game library. Returns why it isn't available.
pub fn find_api() -> Result<(), String> {
    if api_table().is_some() {
        return Ok(());
    }
    let t = amxx::tsapi::find()?;
    API.store(t as *const PluginApi as *mut PluginApi, Ordering::Relaxed);
    super::listener::register(t);
    Ok(())
}

/// The API as [`TsApi`].
pub struct RealApi(&'static PluginApi);

fn rc(r: c_int) -> Result<(), i32> {
    if r >= 0 { Ok(()) } else { Err(r) }
}

impl TsApi for RealApi {
    fn player_info(&self, player: i32) -> Result<PlayerInfo, i32> {
        let mut out = PlayerInfo::default();
        // SAFETY: `out` is valid for writes, as the API requires.
        let r = unsafe { (self.0.player_info)(player, &mut out) };
        rc(r).map(|()| out)
    }
    fn set_cash(&self, player: i32, cash: i32) -> Result<(), i32> {
        rc((self.0.set_cash)(player, cash))
    }
    fn set_free_slots(&self, player: i32, slots: i32) -> Result<(), i32> {
        rc((self.0.set_free_slots)(player, slots))
    }
    fn set_award(&self, player: i32, award: i32) -> Result<(), i32> {
        rc((self.0.set_award)(player, award))
    }
    fn run_powerup(&self, player: i32, powerup: i32, duration: f32) -> Result<(), i32> {
        rc((self.0.run_powerup)(player, powerup, duration))
    }
    fn set_speed(&self, player: i32, speed: f32, aura: f32, duration: f32) -> Result<(), i32> {
        rc((self.0.set_speed)(player, speed, aura, duration))
    }
    fn set_physics_speed(&self, player: i32, speed: f32) -> Result<(), i32> {
        rc((self.0.set_physics_speed)(player, speed))
    }
    fn set_bullet_trail(&self, player: i32, on: bool) -> Result<(), i32> {
        rc((self.0.set_bullet_trail)(player, on as c_int))
    }
    fn set_weapon_ammo(&self, player: i32, weapon: i32, clip: i32, ammo: i32) -> Result<(), i32> {
        rc((self.0.set_weapon_ammo)(player, weapon, clip, ammo))
    }
    fn projectile_owner(&self, entity: i32) -> Result<i32, i32> {
        let r = (self.0.projectile_owner)(entity);
        if r >= 0 { Ok(r) } else { Err(r) }
    }
}

/// The real game. Holds the API wrapper so `api()` can lend it.
pub struct RealGame {
    api: Option<RealApi>,
}

impl RealGame {
    pub fn new() -> Self {
        RealGame { api: api_table().map(RealApi) }
    }

    fn ent(&self, i: i32) -> Edict {
        if i < 0 {
            return Edict::NULL;
        }
        engine().map_or(Edict::NULL, |e| e.edict(i))
    }

    fn index(&self, e: Edict) -> Option<i32> {
        (!e.is_null()).then(|| engine().map_or(0, |g| g.ent_index(e)))
    }
}

impl Game for RealGame {
    fn time(&self) -> f32 {
        engine().map_or(0.0, |e| e.time())
    }
    fn max_clients(&self) -> i32 {
        engine().map_or(0, |e| e.max_clients())
    }
    fn is_client(&self, ent: i32) -> bool {
        self.ent(ent).flags() & (FL_CLIENT | FL_FAKECLIENT) as i32 != 0
    }
    fn is_fake_client(&self, ent: i32) -> bool {
        self.ent(ent).flags() & FL_FAKECLIENT as i32 != 0
    }
    fn is_alive(&self, ent: i32) -> bool {
        let e = self.ent(ent);
        !e.is_null() && e.deadflag() == DEAD_NO as i32 && e.health() > 0.0
    }
    fn is_free(&self, ent: i32) -> bool {
        self.ent(ent).is_free()
    }
    fn frags(&self, ent: i32) -> i32 {
        self.ent(ent).frags() as i32
    }
    fn iuser4(&self, ent: i32) -> i32 {
        self.ent(ent).iuser4()
    }
    fn velocity(&self, ent: i32) -> [f32; 3] {
        self.ent(ent).velocity()
    }
    fn friction(&self, ent: i32) -> f32 {
        self.ent(ent).friction()
    }
    fn team(&self, ent: i32) -> i32 {
        self.ent(ent).team()
    }
    fn damage(&self, ent: i32) -> (Option<i32>, f32) {
        let e = self.ent(ent);
        (self.index(e.dmg_inflictor()), e.dmg_take())
    }
    fn clear_dmg_take(&self, ent: i32) {
        self.ent(ent).set_dmg_take(0.0);
    }
    fn clear_dmg_inflictor(&self, ent: i32) {
        self.ent(ent).set_dmg_inflictor(Edict::NULL);
    }
    fn classname(&self, ent: i32) -> String {
        self.ent(ent).classname()
    }
    fn owner(&self, ent: i32) -> Option<i32> {
        self.index(self.ent(ent).owner())
    }
    fn auth_id(&self, ent: i32) -> Option<String> {
        engine()?.auth_id(self.ent(ent))
    }
    fn netname(&self, ent: i32) -> String {
        self.ent(ent).netname()
    }
    fn weapon_model(&self, ent: i32) -> String {
        self.ent(ent).weapon_model()
    }
    fn cvar(&self, name: &str) -> f32 {
        let n = CString::new(name).unwrap_or_default();
        engine().map_or(0.0, |e| e.cvar_float(&n))
    }
    fn set_cvar(&self, name: &str, value: f32) {
        let n = CString::new(name).unwrap_or_default();
        if let Some(e) = engine() {
            e.set_cvar_float(&n, value);
        }
    }
    fn amxx_ingame(&self, player: i32) -> bool {
        amxx::core::is_player_ingame(player)
    }
    fn api(&self) -> Option<&dyn TsApi> {
        self.api.as_ref().map(|a| a as &dyn TsApi)
    }
    fn log(&self, msg: &str) {
        amxx::core::log(msg);
    }
    fn client_cmd(&self, player: i32, cmd: &str) {
        if let Some(e) = engine() {
            e.client_command(self.ent(player), cmd);
        }
    }

    fn give_weapon(&self, player: i32, weapon: i32, clips: i32, flags: i32) -> bool {
        let Some(eng) = engine() else { return false };
        let p = self.ent(player);
        let e = eng.create_named_entity(c"ts_groundweapon");
        if e.is_free() {
            return false;
        }
        gamedll::key_value(e, c"tsweaponid", &weapon.to_string());
        gamedll::key_value(e, c"wduration", "180");
        gamedll::key_value(e, c"wextraclip", &clips.to_string());
        gamedll::key_value(e, c"spawnflags", &flags.to_string());
        gamedll::spawn(e);
        e.set_origin_field(p.origin());
        gamedll::use_(e, p);
        // The game may have removed it already (upstream removes it regardless).
        if !e.is_free() {
            eng.remove_entity(e);
        }
        true
    }

    fn create_powerup(&self, kind: i32, origin: [f32; 3]) -> i32 {
        let Some(eng) = engine() else { return 0 };
        let e = eng.create_named_entity(c"ts_powerup");
        if e.is_free() {
            return 0;
        }
        gamedll::key_value(e, c"pwuptype", &kind.to_string());
        gamedll::key_value(e, c"pwupduration", "60");
        gamedll::spawn(e);
        e.set_origin_field(origin);
        eng.ent_index(e)
    }

    fn give_powerup(&self, player: i32, ent: i32) {
        let Some(eng) = engine() else { return };
        let (p, e) = (self.ent(player), self.ent(ent));
        e.set_origin_field(p.origin());
        gamedll::touch(e, p);
        if !e.is_free() {
            eng.remove_entity(e);
        }
    }

    fn fire(&self, event: Event) -> i32 {
        // Floats go as cells holding their bits (`ExecuteForward` reads `FP_FLOAT` as a double).
        let bits = |f: f32| f.to_bits() as i32;
        let (fwd, cells) = match event {
            Event::Damage { attacker, victim, damage, weapon, aim, ta } => (&FWD_DAMAGE, vec![attacker, victim, damage, weapon, aim, ta]),
            Event::Death { killer, victim, weapon, aim, tk } => (&FWD_DEATH, vec![killer, victim, weapon, aim, tk]),
            // Stock tsfun.inc declares a 4th parameter, UNAVAILABLE: 0 for plugins built with it.
            Event::Stunt { player, stunt } => (&FWD_STUNT, vec![player, stunt, 0]),
            Event::Powerup { player, powerup } => (&FWD_POWERUP, vec![player, powerup, 0]),
            Event::MeleeAttack { player, time, damage } => (&FWD_MELEE, vec![player, bits(time), bits(damage), 0]),
        };
        let id = fwd.load(Ordering::Relaxed);
        if id >= 0 { amxx::core::execute_forward(Forward(id), &cells) } else { 0 }
    }
}
