// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Port addition: tsx as a listener of the server's plugin API (version 5, `add_listener`; owner
//! `NO_PLUGIN` since version 7). The server calls these back when a player starts a stunt, a
//! power-up takes effect, a kung-fu move is about to land and a death is scored; they fire
//! tsfun.inc's forwards (`crate::forwards`) and finish the death half of `TSHealth`
//! (`State::kill_scored`). The rest (version 6) fire tsbot.inc's forwards (`crate::bots`):
//! `start_frame` delivers what the Pawn backends are owed and fires `ts_frame`.

use super::game::RealGame;
use crate::forwards::{self, KillReport};
use crate::game::{DamageEvent, Game, TsEvent};
use crate::state::with;
use core::ffi::{CStr, c_int};
use core::sync::atomic::{AtomicI32, Ordering};
use ts_plugin_api::{KillInfo, Plugin, PluginApi};

/// The listener's handle, 0 for none.
static HANDLE: AtomicI32 = AtomicI32::new(0);

/// Registers tsx's callbacks with the server (once).
pub fn register(api: &'static PluginApi) {
    if HANDLE.load(Ordering::Relaxed) > 0 {
        return;
    }
    let desc = Plugin {
        name: c"TSX".as_ptr(),
        player_stunt: Some(player_stunt),
        player_powerup: Some(player_powerup),
        melee_attack: Some(melee_attack),
        kill_scored: Some(kill_scored),
        start_frame: Some(start_frame),
        weapon_fired: Some(weapon_fired),
        item_pickup: Some(item_pickup),
        entity_touch: Some(entity_touch),
        sound: Some(sound),
        damage: Some(damage),
        player_killed: Some(player_killed),
        player_spawned: Some(player_spawned),
        round_start: Some(round_start),
        round_end: Some(round_end),
        weapon_changed: Some(weapon_changed),
        ammo_changed: Some(ammo_changed),
        ..Plugin::EMPTY
    };
    // SAFETY: the server copies the descriptor during the call; its callbacks are this
    // library's, which stays loaded until `unregister` (at `AMXX_Detach`).
    let h = unsafe { (api.add_listener)(ts_plugin_api::NO_PLUGIN, &desc) };
    if h > 0 {
        HANDLE.store(h, Ordering::Relaxed);
        with(|s| s.kill_reports = true);
    } else {
        amxx::core::log(&format!(
            "WARNING: the server refused tsx's plugin API listener ({h}); client_stunt, client_powerup, Melee_Attack and tsbot.inc's \
             forwards (ts_frame, ...) won't fire"
        ));
    }
}

/// Removes the callbacks (the library may be unloaded next).
pub fn unregister(api: Option<&'static PluginApi>) {
    let h = HANDLE.swap(0, Ordering::Relaxed);
    if let Some(api) = api
        && h > 0
    {
        (api.remove_listener)(h);
    }
    with(|s| s.kill_reports = false);
}

extern "C" fn player_stunt(player: c_int, stunt: c_int) {
    forwards::stunt(&RealGame::new(), player, stunt);
}

extern "C" fn player_powerup(player: c_int, powerup: c_int) {
    forwards::powerup(&RealGame::new(), player, powerup);
}

/// Guard: NULL pointers leave the move alone and only block or not.
unsafe extern "C" fn melee_attack(player: c_int, time: *mut f32, damage: *mut f32) -> c_int {
    if time.is_null() || damage.is_null() {
        return 0;
    }
    // SAFETY: the server passes two floats valid for the call (the API's contract).
    let (t, d) = unsafe { (time.read(), damage.read()) };
    let (t, d, handled) = forwards::melee_attack(&RealGame::new(), player, t, d);
    // SAFETY: as above.
    unsafe {
        time.write(t);
        damage.write(d);
    }
    handled as c_int
}

/// Guard: NULL is ignored.
unsafe extern "C" fn kill_scored(kill: *const KillInfo) {
    if kill.is_null() {
        return;
    }
    // SAFETY: the server passes a `KillInfo` valid for the call (the API's contract).
    let k = unsafe { kill.read_unaligned() };
    let name = CStr::from_bytes_until_nul(name_bytes(&k.weapon_name)).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let report =
        KillReport { killer: k.killer, victim: k.victim, points: k.points, tags: k.tags, killer_spree: k.killer_spree, weapon_name: name };
    let g = RealGame::new();
    if let Some(Some(death)) = with(|s| s.kill_scored(&g, report)) {
        g.fire(death);
    }
}

extern "C" fn start_frame() {
    crate::bots::frame(&RealGame::new());
}

/// Three floats the server passes, zero for NULL.
///
/// # Safety
/// `p` is NULL or valid for three floats during the call.
unsafe fn vec3(p: *const f32) -> [f32; 3] {
    if p.is_null() {
        return [0.0; 3];
    }
    // SAFETY: the caller's contract.
    unsafe { [p.read_unaligned(), p.add(1).read_unaligned(), p.add(2).read_unaligned()] }
}

unsafe extern "C" fn weapon_fired(player: c_int, weapon: c_int, origin: *const f32, dir: *const f32, silenced: c_int) {
    // SAFETY: the server passes three floats each, valid for the call (the API's contract).
    let (origin, dir) = unsafe { (vec3(origin), vec3(dir)) };
    RealGame::new().fire_ts(&TsEvent::WeaponFired { player, weapon, origin, dir, silenced });
}

extern "C" fn item_pickup(player: c_int, entity: c_int, kind: c_int, id: c_int) {
    RealGame::new().fire_ts(&TsEvent::ItemPickup { player, entity, kind, id });
}

extern "C" fn entity_touch(bot: c_int, other: c_int) {
    RealGame::new().fire_ts(&TsEvent::BotTouch { bot, other });
}

unsafe extern "C" fn sound(
    entity: c_int,
    origin: *const f32,
    sample: *const core::ffi::c_char,
    volume: f32,
    attenuation: f32,
    channel: c_int,
    source: c_int,
) {
    // SAFETY: three floats and a NUL-terminated sample name (or NULL), valid for the call.
    let (origin, sample) =
        unsafe { (vec3(origin), if sample.is_null() { Vec::new() } else { CStr::from_ptr(sample).to_bytes().to_vec() }) };
    RealGame::new().fire_ts(&TsEvent::Sound { entity, origin, sample, volume, attenuation, channel, source });
}

unsafe extern "C" fn damage(event: *const DamageEvent) {
    if event.is_null() {
        return;
    }
    // SAFETY: the server passes a `DamageEvent` valid for the call.
    let d = unsafe { event.read_unaligned() };
    RealGame::new().fire_ts(&TsEvent::Damage(d));
}

extern "C" fn player_killed(victim: c_int, attacker: c_int, inflictor: c_int) {
    RealGame::new().fire_ts(&TsEvent::PlayerKilled { victim, attacker, inflictor });
}

extern "C" fn player_spawned(player: c_int) {
    RealGame::new().fire_ts(&TsEvent::PlayerSpawned(player));
}

extern "C" fn round_start() {
    RealGame::new().fire_ts(&TsEvent::RoundStart);
}

extern "C" fn round_end() {
    RealGame::new().fire_ts(&TsEvent::RoundEnd);
}

extern "C" fn weapon_changed(player: c_int, weapon: c_int) {
    RealGame::new().fire_ts(&TsEvent::WeaponChanged { player, weapon });
}

extern "C" fn ammo_changed(player: c_int, weapon: c_int, clip: c_int, ammo: c_int) {
    RealGame::new().fire_ts(&TsEvent::AmmoChanged { player, weapon, clip, ammo });
}

/// The name's bytes (`c_char` is `i8` or `u8` by target).
fn name_bytes(name: &[core::ffi::c_char]) -> &[u8] {
    // SAFETY: `c_char` and `u8` have the same size and alignment, and every bit pattern is valid
    // for both.
    unsafe { core::slice::from_raw_parts(name.as_ptr().cast::<u8>(), name.len()) }
}
