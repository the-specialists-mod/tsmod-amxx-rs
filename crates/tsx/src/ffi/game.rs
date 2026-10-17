// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. `give_weapon`,
// `create_powerup` and `give_powerup` port the entity code of NBase.cpp's `give_weapon`,
// `create_pwup` and `give_pwup`, Copyright (C) The AMX Mod X Development Team.

//! The real [`Game`]: the engine through metamod, the game library's tables, AMX Mod X, and
//! the server's plugin API (looked up through the game library's `CreateInterface`).

use crate::game::{
    BackendCall, BackendForwards, BotCmd, BotCmdResult, EntityInfo, Event, FrameInfo, Game, GameState, ItemInfo, ObjectiveInfo, PlayerInfo,
    PlayerSnapshot, TsApi, TsEvent, WeaponEntry,
};
use amxx::core::{ExecType, Forward, FwdValue, Param};
use amxx::engine::{Edict, engine};
use amxx::meta::gamedll;
use amxx::tsapi::{Api, backend, c_bytes};
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

/// Port addition: tsbot.inc's multi-plugin forwards, by [`TsForward`], -1 before
/// `AMXX_PluginsLoaded` and after `AMXX_PluginsUnloading` (the core drops them with the plugins).
pub static TS_FWDS: [AtomicI32; TsForward::COUNT] = [const { AtomicI32::new(-1) }; TsForward::COUNT];

/// tsbot.inc's multi-plugin forwards: their names and parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TsForward {
    Frame,
    WeaponFired,
    ItemPickup,
    BotTouch,
    Sound,
    Damage,
    PlayerKilled,
    PlayerSpawned,
    RoundStart,
    RoundEnd,
    WeaponChanged,
    AmmoChanged,
}

impl TsForward {
    pub const COUNT: usize = 12;
    pub const ALL: [TsForward; Self::COUNT] = [
        TsForward::Frame,
        TsForward::WeaponFired,
        TsForward::ItemPickup,
        TsForward::BotTouch,
        TsForward::Sound,
        TsForward::Damage,
        TsForward::PlayerKilled,
        TsForward::PlayerSpawned,
        TsForward::RoundStart,
        TsForward::RoundEnd,
        TsForward::WeaponChanged,
        TsForward::AmmoChanged,
    ];

    /// The public's name and parameters (tsbot.inc).
    pub fn signature(self) -> (&'static core::ffi::CStr, &'static [Param]) {
        use Param::{Array as A, Cell as C, Float as F, String as S};
        match self {
            TsForward::Frame => (c"ts_frame", &[]),
            TsForward::WeaponFired => (c"ts_weapon_fired", &[C, C, A, A, C]),
            TsForward::ItemPickup => (c"ts_item_pickup", &[C, C, C, C]),
            TsForward::BotTouch => (c"ts_bot_touch", &[C, C]),
            TsForward::Sound => (c"ts_sound", &[C, A, S, F, F, C, C]),
            TsForward::Damage => (c"ts_damage", &[C, C, C, F, C, F, F, C, C, A, A]),
            TsForward::PlayerKilled => (c"ts_player_killed", &[C, C, C]),
            TsForward::PlayerSpawned => (c"ts_player_spawned", &[C]),
            TsForward::RoundStart => (c"ts_round_start", &[]),
            TsForward::RoundEnd => (c"ts_round_end", &[]),
            TsForward::WeaponChanged => (c"ts_weapon_changed", &[C, C]),
            TsForward::AmmoChanged => (c"ts_ammo_changed", &[C, C, C, C]),
        }
    }

    fn of(e: &TsEvent) -> TsForward {
        match e {
            TsEvent::Frame => TsForward::Frame,
            TsEvent::WeaponFired { .. } => TsForward::WeaponFired,
            TsEvent::ItemPickup { .. } => TsForward::ItemPickup,
            TsEvent::BotTouch { .. } => TsForward::BotTouch,
            TsEvent::Sound { .. } => TsForward::Sound,
            TsEvent::Damage(_) => TsForward::Damage,
            TsEvent::PlayerKilled { .. } => TsForward::PlayerKilled,
            TsEvent::PlayerSpawned(_) => TsForward::PlayerSpawned,
            TsEvent::RoundStart => TsForward::RoundStart,
            TsEvent::RoundEnd => TsForward::RoundEnd,
            TsEvent::WeaponChanged { .. } => TsForward::WeaponChanged,
            TsEvent::AmmoChanged { .. } => TsForward::AmmoChanged,
        }
    }
}

/// Registers tsbot.inc's forwards (`AMXX_PluginsLoaded`).
pub fn register_ts_forwards() {
    for f in TsForward::ALL {
        let (name, params) = f.signature();
        let id = amxx::core::register_forward_params(name, ExecType::Ignore, params).map_or(-1, |f| f.0);
        TS_FWDS[f as usize].store(id, Ordering::Relaxed);
    }
}

/// Forgets them (`AMXX_PluginsUnloading`: the core is about to clear its forwards).
pub fn forget_ts_forwards() {
    for f in &TS_FWDS {
        f.store(-1, Ordering::Relaxed);
    }
}

/// The dispatcher of the backend slots' trampolines (`amxx::tsapi::backend`).
pub fn backend_dispatch(slot: usize, call: BackendCall) -> c_int {
    crate::bots::backend_call(&RealGame::new(), slot, call)
}

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
    fn snapshot(&self, player: i32) -> Option<PlayerSnapshot> {
        self.api().player_snapshot(player)
    }
    fn inventory(&self, player: i32) -> Result<Vec<WeaponEntry>, i32> {
        self.api().player_inventory(player)
    }
    fn items(&self) -> Vec<ItemInfo> {
        self.api().items()
    }
    fn objectives(&self, player: i32) -> Result<Vec<ObjectiveInfo>, i32> {
        self.api().objectives(player)
    }
    fn game_state(&self) -> GameState {
        self.api().game_state()
    }
    fn frame_info(&self) -> FrameInfo {
        self.api().frame_info()
    }
    fn entity_info(&self, entity: i32) -> Result<EntityInfo, i32> {
        self.api().entity_info(entity)
    }
    fn entity_key(&self, entity: i32, key: &str) -> Result<String, i32> {
        self.api().entity_key(entity, &c_bytes(key.as_bytes()))
    }
    fn master_triggered(&self, master: &str, activator: i32) -> bool {
        self.api().master_triggered(&c_bytes(master.as_bytes()), activator)
    }
    fn attach_backend(&self, slot: usize, name: &str) -> Result<i32, i32> {
        let api = self.api();
        let Some(table) = backend::table(slot) else { return Err(ts_plugin_api::INVALID) };
        let id = api.attach_plugin(name)?;
        let r = api.register_bot_backend(id, &c_bytes(name.as_bytes()), &table);
        if r != ts_plugin_api::OK {
            api.detach_plugin(id);
            return Err(r);
        }
        Ok(id)
    }
    fn detach(&self, plugin: i32) -> i32 {
        self.api().detach_plugin(plugin)
    }
    fn data_dir(&self, plugin: i32) -> Result<String, i32> {
        self.api().data_dir(plugin)
    }
    fn api_log(&self, plugin: i32, level: i32, msg: &str) {
        self.api().log(plugin, level, msg);
    }
    fn create_bot(&self, plugin: i32, name: &[u8], model: &[u8], skill: f32) -> i32 {
        self.api().create_bot(plugin, name, model, skill)
    }
    fn kick_bot(&self, player: i32) -> i32 {
        (self.0.kick_bot)(player)
    }
    fn run_cmd(&self, player: i32, cmd: &BotCmd) -> Result<BotCmdResult, i32> {
        self.api().run_cmd(player, cmd)
    }
    fn bot_command(&self, player: i32, line: &[u8]) -> i32 {
        self.api().bot_command(player, line)
    }
    fn select_weapon(&self, player: i32, weapon: i32) -> i32 {
        (self.0.select_weapon)(player, weapon)
    }
    fn buy(&self, player: i32, weapon: i32, attachments: i32, akimbo: i32, ammo: i32) -> i32 {
        (self.0.buy)(player, weapon, attachments, akimbo, ammo)
    }
    fn buy_item(&self, player: i32, item: i32) -> i32 {
        (self.0.buy_item)(player, item)
    }
    fn clear_loadout(&self, player: i32) -> i32 {
        (self.0.clear_loadout)(player)
    }
    fn drop_weapon(&self, player: i32, weapon: i32) -> i32 {
        (self.0.drop_weapon)(player, weapon)
    }
}

impl RealApi {
    fn api(&self) -> Api {
        Api::new(self.0)
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

    fn fire_ts(&self, event: &TsEvent) -> i32 {
        let id = TS_FWDS[TsForward::of(event) as usize].load(Ordering::Relaxed);
        if id < 0 {
            return 0;
        }
        let v3 = |v: [f32; 3]| v.map(|f| f.to_bits() as amxx::Cell);
        let fwd = Forward(id);
        use FwdValue::{Array, Cell as C, Float as F};
        match event {
            TsEvent::Frame | TsEvent::RoundStart | TsEvent::RoundEnd => amxx::core::execute_forward_values(fwd, &mut []),
            TsEvent::WeaponFired { player, weapon, origin, dir, silenced } => {
                let (mut o, mut d) = (v3(*origin), v3(*dir));
                amxx::core::execute_forward_values(fwd, &mut [C(*player), C(*weapon), Array(&mut o), Array(&mut d), C(*silenced)])
            }
            TsEvent::ItemPickup { player, entity, kind, id } => {
                amxx::core::execute_forward_values(fwd, &mut [C(*player), C(*entity), C(*kind), C(*id)])
            }
            TsEvent::BotTouch { bot, other } => amxx::core::execute_forward_values(fwd, &mut [C(*bot), C(*other)]),
            TsEvent::Sound { entity, origin, sample, volume, attenuation, channel, source } => {
                let mut o = v3(*origin);
                let s = c_bytes(sample);
                amxx::core::execute_forward_values(
                    fwd,
                    &mut [C(*entity), Array(&mut o), FwdValue::Str(&s), F(*volume), F(*attenuation), C(*channel), C(*source)],
                )
            }
            TsEvent::Damage(d) => {
                let (mut dir, mut pos) = (v3(d.dir), v3(d.pos));
                amxx::core::execute_forward_values(
                    fwd,
                    &mut [
                        C(d.victim),
                        C(d.attacker),
                        C(d.inflictor),
                        F(d.damage),
                        C(d.bits),
                        F(d.health),
                        F(d.armor),
                        C(d.hitgroup),
                        C(d.weapon),
                        Array(&mut dir),
                        Array(&mut pos),
                    ],
                )
            }
            TsEvent::PlayerKilled { victim, attacker, inflictor } => {
                amxx::core::execute_forward_values(fwd, &mut [C(*victim), C(*attacker), C(*inflictor)])
            }
            TsEvent::PlayerSpawned(p) => amxx::core::execute_forward_values(fwd, &mut [C(*p)]),
            TsEvent::WeaponChanged { player, weapon } => amxx::core::execute_forward_values(fwd, &mut [C(*player), C(*weapon)]),
            TsEvent::AmmoChanged { player, weapon, clip, ammo } => {
                amxx::core::execute_forward_values(fwd, &mut [C(*player), C(*weapon), C(*clip), C(*ammo)])
            }
        }
    }

    fn backend_forwards(&self, amx: usize) -> BackendForwards {
        use Param::{Cell as C, Float as F, String as S};
        // SAFETY: `amx` is the AMX of the plugin whose native (ts_bot_register_backend) is
        // running, which the core passed to it this map.
        let reg = |name: &str, params: &[Param]| unsafe { amxx::core::register_sp_forward_params(amx, name, params) };
        BackendForwards {
            add: reg("ts_bot_add", &[S, S, F]),
            activate: reg("ts_bot_activate", &[]),
            deactivate: reg("ts_bot_deactivate", &[]),
            adopt: reg("ts_bot_adopt", &[C]),
        }
    }

    fn drop_backend_forwards(&self, f: &BackendForwards) {
        for id in [f.add, f.activate, f.deactivate, f.adopt] {
            if id >= 0 {
                amxx::core::unregister_sp_forward(id);
            }
        }
    }

    fn fire_backend(&self, fwd: i32, call: &BackendCall) -> i32 {
        if fwd < 0 {
            return 0;
        }
        match *call {
            BackendCall::Add { name, model, skill } => {
                let (n, m) = (c_bytes(name), c_bytes(model));
                amxx::core::execute_sp_forward(fwd, &mut [FwdValue::Str(&n), FwdValue::Str(&m), FwdValue::Float(skill)])
            }
            BackendCall::Activate | BackendCall::Deactivate => amxx::core::execute_sp_forward(fwd, &mut []),
            BackendCall::Adopt(p) => amxx::core::execute_sp_forward(fwd, &mut [FwdValue::Cell(p)]),
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
