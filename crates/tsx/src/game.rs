// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! What the module's logic needs from outside: the engine, the game library, AMX Mod X
//! forwards and the server's plugin API. The FFI layer implements [`Game`] for real; tests use
//! a fake.

pub use ts_plugin_api::PlayerInfo;

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
}

/// A forward the logic asks the FFI layer to run once module state is released (plugins may
/// call the module's natives from it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `client_damage(attacker, victim, damage, wpnindex, hitplace, TA)`.
    Damage { attacker: i32, victim: i32, damage: i32, weapon: i32, aim: i32, ta: i32 },
    /// `client_death(killer, victim, wpnindex, hitplace, TK)`.
    Death { killer: i32, victim: i32, weapon: i32, aim: i32, tk: i32 },
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
    /// Runs a forward (only call with module state released).
    fn fire(&self, event: Event);
}
