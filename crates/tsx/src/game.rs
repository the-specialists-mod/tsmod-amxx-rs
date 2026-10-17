// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! What the module's logic needs from outside: the engine, the game library, AMX Mod X
//! forwards and the server's plugin API. The FFI layer implements [`Game`] for real; tests use
//! a fake.

pub use amxx::tsapi::backend::Call as BackendCall;
pub use ts_plugin_api::{
    BotCmd, BotCmdResult, DamageEvent, EntityInfo, FrameInfo, GameState, ItemInfo, ObjectiveInfo, PlayerInfo, PlayerSnapshot, WeaponEntry,
};

/// The server's plugin API (`ts_plugin_api::PluginApi`), as results. Errors are the API's
/// negative codes (`NOT_PLAYER`, `INVALID`, `UNSUPPORTED`); `Ok` covers `OK` and `DEFERRED`.
pub trait TsApi {
    fn player_info(&self, player: i32) -> Result<PlayerInfo, i32>;
    fn set_cash(&self, player: i32, cash: i32) -> Result<(), i32>;
    fn set_free_slots(&self, player: i32, slots: i32) -> Result<(), i32>;
    fn set_award(&self, player: i32, award: i32) -> Result<(), i32>;
    fn run_powerup(&self, player: i32, powerup: i32, duration: f32) -> Result<(), i32>;
    fn set_speed(&self, player: i32, speed: f32, aura: f32, duration: f32) -> Result<(), i32>;
    fn set_physics_speed(&self, player: i32, speed: f32) -> Result<(), i32>;
    fn set_bullet_trail(&self, player: i32, on: bool) -> Result<(), i32>;
    /// Sets the clip and reserve of the player's weapon `weapon` (negative: unchanged).
    fn set_weapon_ammo(&self, player: i32, weapon: i32, clip: i32, ammo: i32) -> Result<(), i32>;
    /// The player behind a projectile: `Ok(0)` for none.
    fn projectile_owner(&self, entity: i32) -> Result<i32, i32>;

    // ---- port addition: what tsbot.inc's natives use (plugin API versions 3 to 7) ----
    /// The frame's snapshot of a player slot (`players`): `None` for an index outside
    /// `1..=maxClients` or an empty slot.
    fn snapshot(&self, player: i32) -> Option<PlayerSnapshot>;
    /// `player_inventory`: the weapons the player owns.
    fn inventory(&self, player: i32) -> Result<Vec<WeaponEntry>, i32>;
    /// `items`: the map's items.
    fn items(&self) -> Vec<ItemInfo>;
    /// `objectives(player)`: the HUD's objective list, with `player`'s team's names.
    fn objectives(&self, player: i32) -> Result<Vec<ObjectiveInfo>, i32>;
    fn game_state(&self) -> GameState;
    fn frame_info(&self) -> FrameInfo;
    fn entity_info(&self, entity: i32) -> Result<EntityInfo, i32>;
    /// `entity_key`: a map key the server keeps outside `entvars_t`.
    fn entity_key(&self, entity: i32, key: &str) -> Result<String, i32>;
    fn master_triggered(&self, master: &str, activator: i32) -> bool;
    /// Attaches a plugin identity named `name` and registers bot backend slot `slot`'s
    /// trampolines under the same name (`attach_plugin` + `register_bot_backend`, detached again
    /// when the registration fails): the identity's id, or the API's status. The server may call
    /// the backend (`activate`) inside.
    fn attach_backend(&self, slot: usize, name: &str) -> Result<i32, i32>;
    /// `detach_plugin`: the server may call the backend's `deactivate` inside.
    fn detach(&self, plugin: i32) -> i32;
    fn data_dir(&self, plugin: i32) -> Result<String, i32>;
    /// `log(plugin, level, msg)`: the server console, with the plugin's prefix.
    fn api_log(&self, plugin: i32, level: i32, msg: &str);
    /// `create_bot`: the bot's index or a status. Joins and spawns the bot inside.
    fn create_bot(&self, plugin: i32, name: &[u8], model: &[u8], skill: f32) -> i32;
    fn kick_bot(&self, player: i32) -> i32;
    /// `run_cmd`: the bot's think, movement and post-think run inside.
    fn run_cmd(&self, player: i32, cmd: &BotCmd) -> Result<BotCmdResult, i32>;
    fn bot_command(&self, player: i32, line: &[u8]) -> i32;
    fn select_weapon(&self, player: i32, weapon: i32) -> i32;
    fn buy(&self, player: i32, weapon: i32, attachments: i32, akimbo: i32, ammo: i32) -> i32;
    fn buy_item(&self, player: i32, item: i32) -> i32;
    fn clear_loadout(&self, player: i32) -> i32;
    fn drop_weapon(&self, player: i32, weapon: i32) -> i32;
}

/// Port addition: tsbot.inc's multi-plugin forwards, from the server's plugin API callbacks
/// (`crate::bots`). Floats go as cells holding their bits, vectors as `Float:[3]` arrays.
#[derive(Clone, Debug, PartialEq)]
pub enum TsEvent {
    /// `ts_frame()`: the server's `start_frame`, where bots run their commands.
    Frame,
    /// `ts_weapon_fired(id, wid, Float:origin[3], Float:dir[3], silenced)`.
    WeaponFired {
        player: i32,
        weapon: i32,
        origin: [f32; 3],
        dir: [f32; 3],
        silenced: i32,
    },
    /// `ts_item_pickup(id, ent, kind, itemid)`.
    ItemPickup {
        player: i32,
        entity: i32,
        kind: i32,
        id: i32,
    },
    /// `ts_bot_touch(bot, other)`.
    BotTouch {
        bot: i32,
        other: i32,
    },
    /// `ts_sound(ent, Float:origin[3], const sample[], Float:vol, Float:attn, channel, source)`.
    Sound {
        entity: i32,
        origin: [f32; 3],
        sample: Vec<u8>,
        volume: f32,
        attenuation: f32,
        channel: i32,
        source: i32,
    },
    /// `ts_damage(victim, attacker, inflictor, Float:damage, bits, Float:health, Float:armor,
    /// hitgroup, wid, Float:dir[3], Float:pos[3])`.
    Damage(DamageEvent),
    /// `ts_player_killed(victim, attacker, inflictor)`.
    PlayerKilled {
        victim: i32,
        attacker: i32,
        inflictor: i32,
    },
    /// `ts_player_spawned(id)`.
    PlayerSpawned(i32),
    /// `ts_round_start()`, `ts_round_end()`.
    RoundStart,
    RoundEnd,
    /// `ts_weapon_changed(id, wid)`.
    WeaponChanged {
        player: i32,
        weapon: i32,
    },
    /// `ts_ammo_changed(id, wid, clip, ammo)`.
    AmmoChanged {
        player: i32,
        weapon: i32,
        clip: i32,
        ammo: i32,
    },
}

/// The single-plugin forwards of a Pawn bot backend (`ts_bot_add`, `ts_bot_activate`,
/// `ts_bot_deactivate`, `ts_bot_adopt`) in the plugin that registered it; -1 for a public the
/// plugin doesn't have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackendForwards {
    pub add: i32,
    pub activate: i32,
    pub deactivate: i32,
    pub adopt: i32,
}

impl BackendForwards {
    pub const NONE: BackendForwards = BackendForwards { add: -1, activate: -1, deactivate: -1, adopt: -1 };
}

/// A forward the logic asks the FFI layer to run once module state is released (plugins may
/// call the module's natives from it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// `client_damage(attacker, victim, damage, wpnindex, hitplace, TA)`.
    Damage { attacker: i32, victim: i32, damage: i32, weapon: i32, aim: i32, ta: i32 },
    /// `client_death(killer, victim, wpnindex, hitplace, TK)`.
    Death { killer: i32, victim: i32, weapon: i32, aim: i32, tk: i32 },
    /// `client_stunt(id, stunttype)` (tsfun.inc; port addition, `crate::forwards`).
    Stunt { player: i32, stunt: i32 },
    /// `client_powerup(id, powerup)` (tsfun.inc; port addition).
    Powerup { player: i32, powerup: i32 },
    /// `Melee_Attack(id, Float:time, Float:damage)` (tsfun.inc; port addition): its result says
    /// whether a plugin returned `PLUGIN_HANDLED`.
    MeleeAttack { player: i32, time: f32, damage: f32 },
}

/// Engine and game access by entity index. Entity 0 is the world; `None` stands for NULL.
pub trait Game {
    /// `gpGlobals->time`.
    fn time(&self) -> f32;
    /// `gpGlobals->maxClients`.
    fn max_clients(&self) -> i32;
    /// `FL_CLIENT | FL_FAKECLIENT` set on the entity.
    fn is_client(&self, ent: i32) -> bool;
    /// `FL_FAKECLIENT` set (bots).
    fn is_fake_client(&self, ent: i32) -> bool;
    /// `deadflag == DEAD_NO && health > 0`.
    fn is_alive(&self, ent: i32) -> bool;
    /// NULL or a free slot (`FNullEnt`).
    fn is_free(&self, ent: i32) -> bool;
    /// `(int)pev->frags`.
    fn frags(&self, ent: i32) -> i32;
    /// `pev->iuser4` (TS's stunt bits).
    fn iuser4(&self, ent: i32) -> i32;
    /// `pev->velocity`.
    fn velocity(&self, ent: i32) -> [f32; 3];
    /// `pev->friction`.
    fn friction(&self, ent: i32) -> f32;
    /// `pev->team`.
    fn team(&self, ent: i32) -> i32;
    /// `pev->dmg_inflictor` (`None` for NULL) and `pev->dmg_take`.
    fn damage(&self, ent: i32) -> (Option<i32>, f32);
    /// `pev->dmg_take = 0`.
    fn clear_dmg_take(&self, ent: i32);
    /// `pev->dmg_inflictor = NULL`.
    fn clear_dmg_inflictor(&self, ent: i32);
    /// `STRING(pev->classname)`.
    fn classname(&self, ent: i32) -> String;
    /// `pev->owner` (`None` for NULL).
    fn owner(&self, ent: i32) -> Option<i32>;
    /// `GETPLAYERAUTHID`.
    fn auth_id(&self, ent: i32) -> Option<String>;
    /// `STRING(pev->netname)`.
    fn netname(&self, ent: i32) -> String;
    /// `STRING(pev->weaponmodel)`.
    fn weapon_model(&self, ent: i32) -> String;
    /// `CVAR_GET_FLOAT`.
    fn cvar(&self, name: &str) -> f32;
    /// `CVAR_SET_FLOAT`.
    fn set_cvar(&self, name: &str, value: f32);
    /// `MF_IsPlayerIngame` (AMX Mod X's own notion).
    fn amxx_ingame(&self, player: i32) -> bool;
    /// The server's plugin API, if this server has it.
    fn api(&self) -> Option<&dyn TsApi>;
    /// A line in the AMX Mod X log.
    fn log(&self, msg: &str);
    /// `client_cmd(player, cmd)`: a command for the player's client to run (`CLIENT_COMMAND`;
    /// the engine drops it for bots).
    fn client_cmd(&self, player: i32, cmd: &str);

    /// `ts_giveweapon`'s entity dance: a `ts_groundweapon` with the keys, spawned at the
    /// player and used by them, then removed. False if the entity couldn't be created.
    fn give_weapon(&self, player: i32, weapon: i32, clips: i32, flags: i32) -> bool;
    /// `ts_createpwup`: a spawned `ts_powerup` of `kind` at `origin`; its index, or 0.
    fn create_powerup(&self, kind: i32, origin: [f32; 3]) -> i32;
    /// `ts_givepwup`: moves the power-up onto the player, touches it and removes it.
    fn give_powerup(&self, player: i32, ent: i32);
    /// Runs a forward (only call with module state released) and returns what `ExecuteForward`
    /// returned (`Melee_Attack`: the first `PLUGIN_HANDLED`, else 0).
    fn fire(&self, event: Event) -> i32;

    /// Port addition: runs a tsbot.inc forward (state released, as [`fire`](Self::fire)).
    fn fire_ts(&self, event: &TsEvent) -> i32;
    /// Port addition: registers a Pawn backend's single-plugin forwards in the plugin whose
    /// native is running (`amx`, [`amxx::native::Args::amx`]).
    fn backend_forwards(&self, amx: usize) -> BackendForwards;
    /// Drops forwards [`backend_forwards`](Self::backend_forwards) registered.
    fn drop_backend_forwards(&self, fwds: &BackendForwards);
    /// Runs one of a backend's forwards (`fwd` from [`BackendForwards`]; nothing for -1) with the
    /// call's arguments and returns what the public returned (state released).
    fn fire_backend(&self, fwd: i32, call: &BackendCall) -> i32;
}
