# tsbot.inc: a Pawn plugin as the TS bot backend

A port addition with no upstream counterpart. tsx registers natives and forwards
(`include/tsbot.inc`) that let an AMX Mod X plugin be the Rust TS server's bot backend
(`ts_bot_backend <name>`) and see what a bot needs: the server's per-frame snapshots of every
player, the map's items, the objectives, the rules and the entities, plus the shot, sound,
damage, pickup and touch events. The AMX Mod X-hosted RCBot (`the-specialists-mod/tsmod-amxx-rcbot`,
`rcbot_amxx.sma`, backend `rcbot_amxx`) is built on it.

Everything goes through the server's plugin API version 7 (the-specialistsmod-rs v3.0.8 or later;
`docs/implementation/plugin-api.md` and `bot-plugins.md` there): `attach_plugin` gives the Pawn
backend a server plugin identity, `register_bot_backend` makes it selectable, `create_bot` /
`run_cmd` / `bot_command` / the loadout functions drive its bots, and tsx's API listener hears the
game.

## 1. Library and conventions

* The natives live in the tsx module: `tsbot.inc` has `#pragma reqlib tsx` (and `loadlib`), so a
  plugin that includes it needs tsx loaded, as plugins of `tsx.inc` do. No new library name: the
  natives need tsx's API lookup and listener, and a second module would need its own.
* **Weapon ids are the server's**: TS 3.0's 1..37 and 0 for kung fu (`TSBOT_KUNGFU`), as the API
  has them. tsx's older natives (`ts_getuserwpn`, ...) report kung fu as 38 (36 with
  `tsx_legacy_weaponids 1`); tsbot.inc's never remap.
* **Snapshots are per frame**: the server builds them at `StartFrame`, before `ts_frame`. A
  change a native makes (a weapon switch, a buy) shows in the next frame's snapshot.
* **Structures are cell arrays** in the API's field order (every API field is 4 bytes): ints as
  cells, floats as `Float:` cells, indexed by `TSPS_*`, `TSCOMBAT_*`, `TSINV_*`, `TSITM_*`,
  `TSOBJ_*`, `TSGS_*`, `TSFI_*`, `TSEI_*`, `TSRES_*`. A unit test pins every index to the API's
  `offset_of` and to the include.
* **Statuses, not native errors**: natives return `TSBOT_OK` (0) or `TSBOT_DEFERRED` (1) on success
  and a negative `TSBOT_*` on failure (the API's codes -1..-6, and tsx's `TSBOT_NO_API` -100,
  `TSBOT_NO_BACKEND` -101, `TSBOT_ALREADY` -102, `TSBOT_NO_SLOT` -103), so a bot's think can
  test results without filling the error log. Only `ts_get_player_field` with a field index
  outside `0..TSPS_SIZE-1` raises a native error. Getters that return a value use -1 where 0 is a
  value (`ts_get_player_weapon`, `ts_get_player_stance`) and 0 elsewhere.
* **Bytes, not UTF-8**: bot names, models and command lines go to the server as the plugin's
  bytes (`Args::string_bytes`), so a name typed in CP1252 stays as typed (the port's #190).
* The bot natives act on any player the API accepts, as the API does (`ts_bot_command` on a
  human runs the command as that human); `ts_bot_runcmd` and `ts_bot_kick` refuse humans
  (`TSBOT_INVALID`).

## 2. A backend's life

```pawn
#include <amxmodx>
#include <tsbot>

public plugin_init()
{
    register_plugin("mybot", "1.0", "me");
    ts_bot_register_backend("mybot");     // every map
}

public ts_bot_activate()   { /* load waypoints from ts_bot_data_dir() */ }
public ts_bot_deactivate() { /* our bots are being kicked */ }

public ts_bot_add(const name[], const model[], Float:skill)
{
    return max(ts_bot_create(name[0] ? name : "MyBot", model, skill), 0);
}

public ts_frame()
{
    // read: ts_get_player_snapshot / ts_get_items / ts_get_frameinfo ...; think; then per bot:
    // ts_bot_runcmd(bot, angles, fwd, side, up, buttons);
}
```

* **Registration** (`ts_bot_register_backend(name)`): the first call attaches a server plugin
  identity `name` (`attach_plugin`: 1 to 32 letters, digits, `_`, `-`; its data directory is
  `<gamedir>/tsmod/<name>/`, its console prefix `[<name>]`) and registers one of tsx's four
  backend slots under it (`amxx::tsapi::backend`: the server's `BotBackend` callbacks have no
  context pointer, so each slot has its own trampolines). One backend per plugin
  (`TSBOT_ALREADY`); a name a `tsmod_*` library, another module or another plugin has is
  `TSBOT_EXISTS`.
* **Taking over**: the server reads `ts_bot_backend` at the map start, before `plugin_init`. A
  backend registered later takes over at once when the cvar names it (plugin API version 7), so
  set the cvar before registering (or in `server.cfg`): a cvar change after the registration is
  a change of backend, which reloads the map (port `bot-plugins.md` §3).
* **Every map**: AMX Mod X reloads its plugins. At `AMXX_PluginsUnloading` tsx unbinds the slots
  (the plugins' forwards are gone); the identity and the backend stay registered with the server.
  The next map's `ts_bot_register_backend(name)` binds the same slot again (`TSBOT_OK`, no new
  attach, same data directory).
* **Owed callbacks**: the server's `activate` for the next map comes before `plugin_init`, an
  `adopt` of a bot the engine kept across the level change too, and the `activate` of a takeover
  comes inside `ts_bot_register_backend`. tsx keeps them and delivers `ts_bot_activate` and then
  the `ts_bot_adopt`s at the first `ts_frame` after the plugin bound the slot (or just before the
  next `ts_bot_add`/`ts_bot_adopt` the server sends). A plugin gets `ts_bot_deactivate` only after
  it got `ts_bot_activate`; an activate and deactivate both while no plugin was bound reach
  nobody.
* **A new bot starts observing**, as a joining player does in TS (its `Spawn` puts it in observer
  mode until it respawns), and so does a dead one: send `ts_bot_command(id, "respawn")` (every
  few seconds) while it isn't `TSPF_ALIVE`, as the port's native bots' respawn module does (TS
  3.0's, 0x1006f5b8), and keep running `ts_bot_runcmd` for it (pressing `IN_ATTACK` now and then
  as RCBot does). Buy its loadout (`ts_bot_buy`, `ts_bot_buy_item`) while it is dead, before it
  respawns: death clears the loadout (TS's `CBasePlayer::Killed` runs `ResetArmament`; a client
  sends it again from the buy menu), so a loadout bought while alive is gone by the next spawn.
* **`ts_bot_add`** is the server's `add_bot` (the quota `ts_bot_quota`, `addbot`,
  `addcustombot`): make the bot with `ts_bot_create` and return its index. Without a bound plugin
  the add fails (logged once per backend).
* **`ts_bot_unregister_backend()`** detaches the identity: the server unregisters the backend,
  kicks its bots if it was active and activates native. No `ts_bot_deactivate` follows. The name
  is free again (a new registration gets a new identity).
* **Module unload** (`AMXX_Detach`) detaches every identity, so the server never calls into the
  unloaded library.

`ts_frame` is a multi-plugin forward from the API's `start_frame`. Call `ts_bot_runcmd` once per
bot per frame from it: a call inside one of the bot's own callbacks (a forward during its think)
is `TSBOT_BUSY`. With `TSBOT_SETVIEW | TSBOT_AUTOMSEC` (the default) the server turns the bot as
its own bots turn and picks `msec` from the frame time; adopt `out_vangle` after
`TSBOT_EV_FIXANGLE` (a spawn or teleport set the view) and drop the path after
`TSBOT_EV_TELEPORTED`. `ts_bot_get_result` has the last command's origin, velocity and msec.

## 3. Natives

World (`crates/tsx/src/natives/world.rs`):

| Native | Returns |
|---|---|
| `ts_get_player_snapshot(id, data[], len = TSPS_SIZE)` | the user id (0 empty slot / no API); `data` all zeroes for an empty slot |
| `any:ts_get_player_field(id, field)` | one `TSPS_*` cell (tag floats: `Float:ts_get_player_field(id, TSPS_HEALTH)`) |
| `ts_get_player_flags(id)` | `TSPF_*` |
| `ts_get_player_team(id)` | rules team (1.. in teamplay), 0 none |
| `Float:ts_get_player_health(id, &Float:armor = 0.0)` | health |
| `ts_get_player_stance(id, &stunt_bits = 0)` | `TSState` 0 stand / 1 crouch / 2 prone or diving, -1 empty |
| `ts_get_player_weapon(id, &clip = 0, &ammo = 0, &fire_mode = 0, &attachments = 0)` | weapon in hand (0 kung fu: clip is the kung-fu bar), -1 empty |
| `ts_get_player_powerup(id, &stored = 0, &Float:until = 0.0)` | running power-up bits |
| `Float:ts_get_player_timescale(id)` | 1.0 normal, below slowed, 0.0 empty |
| `ts_get_player_combat(id, Float:combat[TSCOMBAT_SIZE])` | 1, or 0 for an empty slot |
| `ts_get_inventory(id, weapons[], clips[], ammo[], max)` | count owned (kung fu isn't one), or a status |
| `ts_get_inventory_ex(id, data[], max)` | the same, `TSINV_SIZE` cells per weapon |
| `ts_get_items(data[], max)` | item count (`TSITM_SIZE` cells each) |
| `ts_get_objectives(id, data[], max)` | objective count (`TSOBJ_SIZE` cells each), or a status |
| `ts_get_objective_name(id, index, name[], len)` | name length, or `TSBOT_INVALID` |
| `ts_get_gamestate(data[TSGS_SIZE])` | 1, or a status |
| `ts_get_frameinfo(data[TSFI_SIZE])` | 1, or a status |
| `ts_entity_key(ent, const key[], out[], len)` | value length, or a status (`TSBOT_UNSUPPORTED`: no such key) |
| `ts_get_entity_info(ent, data[TSEI_SIZE])` | `TSBOT_OK` or a status |
| `ts_master_triggered(const master[], activator = 0)` | 1 / 0, or a status |
| `ts_projectile_owner(ent)` | the responsible player, 0 none, or a status |

Bots (`crates/tsx/src/natives/bot.rs`):

| Native | Returns |
|---|---|
| `ts_bot_register_backend(const name[])` | status |
| `ts_bot_unregister_backend()` | status |
| `ts_bot_data_dir(out[], len)` | length, or a status |
| `ts_bot_log(level, const msg[])` | status (`TSLOG_*` levels) |
| `ts_bot_create(const name[], const model[] = "", Float:skill = 0.0)` | bot index, or a status |
| `ts_bot_kick(id)` | status |
| `ts_bot_runcmd(id, const Float:angles[3], Float:fwd, Float:side, Float:up, buttons, impulse = 0, flags = TSBOT_SETVIEW \| TSBOT_AUTOMSEC, msec = 0, Float:out_vangle[3] = {0.0, 0.0, 0.0}, &events = 0)` | status |
| `ts_bot_get_result(id, data[TSRES_SIZE])` | 1, or 0 for no command yet |
| `ts_bot_command(id, const line[])` | status |
| `ts_bot_select_weapon(id, wid)` | status |
| `ts_bot_buy(id, weapon, attachments = 0, akimbo = 0, ammo = 0)` | status |
| `ts_bot_buy_item(id, item)` | status (`TSLOAD_*`) |
| `ts_bot_clear_loadout(id)` | status |
| `ts_drop_weapon(id, wid = -1)` | status |

## 4. Forwards

Backend forwards go only to the plugin that registered the backend (single-plugin forwards on its
AMX):

| Forward | When |
|---|---|
| `ts_bot_add(const name[], const model[], Float:skill)` | the server wants a bot; return its index or 0 |
| `ts_bot_activate()` | the backend is the active one (at a `ts_frame`) |
| `ts_bot_deactivate()` | it no longer is; its bots are being kicked |
| `ts_bot_adopt(id)` | a bot of this backend kept across a level change |

Game forwards go to every plugin (multi-plugin, `ET_IGNORE`, from tsx's API listener, owner
`NO_PLUGIN`; floats as cell bits, vectors as `Float:[3]` arrays):

| Forward | API callback |
|---|---|
| `ts_frame()` | `start_frame` |
| `ts_weapon_fired(id, wid, Float:origin[3], Float:dir[3], silenced)` | `weapon_fired` |
| `ts_item_pickup(id, ent, kind, itemid)` | `item_pickup` |
| `ts_bot_touch(bot, other)` | `entity_touch` |
| `ts_sound(ent, Float:origin[3], const sample[], Float:vol, Float:attn, channel, source_player)` | `sound` |
| `ts_damage(victim, attacker, inflictor, Float:damage, bits, Float:health, Float:armor, hitgroup, wid, Float:dir[3], Float:pos[3])` | `damage` (`DamageEvent`) |
| `ts_player_killed(victim, attacker, inflictor)` | `player_killed` |
| `ts_player_spawned(id)` | `player_spawned` |
| `ts_round_start()`, `ts_round_end()` | `round_start`, `round_end` |
| `ts_weapon_changed(id, wid)` | `weapon_changed` |
| `ts_ammo_changed(id, wid, clip, ammo)` | `ammo_changed` |

tsx holds no state across a forward (AGENTS.md): every forward runs with the module state
released, so plugins may call any tsx native from them.

## 5. Constants

All in `include/tsbot.inc`: statuses `TSBOT_*`; `ts_bot_runcmd` flags `TSBOT_SETVIEW`,
`TSBOT_AUTOMSEC` and events `TSBOT_EV_TELEPORTED`, `TSBOT_EV_FIXANGLE`, `TSBOT_EV_LANDED`;
player flags `TSPF_*` (version 7's `TSPF_KUNG_FU` 0x800, `TSPF_SUPERJUMP` 0x1000); stunt bits
`TSSB_*`; the layouts' indices; entity kinds `TSENT_*`; `TSMODE_*`, `TSROUND_*`;
`TSLOAD_KEVLAR`, `TSLOAD_STEALTHSHOES`; `TSLOG_*`; `TSBOT_KUNGFU`. Buttons are AMX Mod X's
`hlsdk_const.inc` `IN_*`, which tsbot.inc includes (`IN_ALT1` is TS's stunt / dive key).
tsconst.inc gained `STUNT_WALLJUMP` 6 and `STUNT_WALLCLIMB` 7 (`client_stunt`'s TS 3.0 kinds),
and keeps `TSPWUP_*` and `TSA_*`, which tsbot.inc's power-up and attachment fields use.

## 6. Tests

* Unit (`crates/tsx/src/bots.rs`, `natives/world.rs`, `natives/bot.rs`, `ffi/mod.rs`): every
  native against `FakeGame`/`FakeArgs` (snapshots and empty slots, lists past `max`, statuses
  without the API, the bytes of a CP1252 name, `ts_bot_runcmd`'s defaults, outputs and kept
  result); the backend's lifecycle (one per plugin, slots, a refused attach, the takeover's
  activate at the next frame, the next map's rebind with the owed activate and adopt, no
  deactivate without an activate, unregistering and the module's unload); the layouts against
  the API's struct offsets and the include; every constant against `ts-plugin-api`; the native
  table and the include's declarations.
* End to end (`e2e/tsbot_e2e.sma`, `cargo xtask e2e`): after the match, a server of its own
  without quota bots and with only this plugin: `ts_bot_backend tsbot_e2e` then registration
  (and the one-per-plugin and bad-name refusals, the data directory), `ts_bot_activate` at a
  frame, `addbot` through `ts_bot_add` / `ts_bot_create`, a direct `ts_bot_create`, the
  snapshot and getters of the new bot, walking (moved > 48 units, trying four directions),
  jumping (airborne, `TSBOT_EV_LANDED`), `ts_giveweapon` in the inventory (serial moved),
  `ts_bot_select_weapon` to kung fu and back (`ts_weapon_changed`), firing (`ts_weapon_fired`
  with a unit direction, `ts_sound` from the bot, `ts_ammo_changed`), a bought loadout after
  death and respawn (`ts_player_killed`, `ts_player_spawned`, `TSBOT_EV_TELEPORTED`), the world
  natives, `ts_bot_kick`, every `ts_bot_runcmd` `TSBOT_OK`, and `ts_bot_unregister_backend`
  kicking the active backend's bots without a `ts_bot_deactivate`.

## 7. Limits and what a Pawn bot still lacks

* Four Pawn backends at once (`amxx::tsapi::backend::SLOTS`), one per plugin.
* Traces, the PVS, point contents and entity searches aren't tsbot natives: AMX Mod X's engine
  and fakemeta modules have them (`trace_line`, `trace_hull`, `engfunc(EngFunc_TraceLine, ...)`,
  `EngFunc_PointContents`, `EngFunc_CheckVisibility`/`EngFunc_SetFatPVS`, `find_ent_in_sphere`,
  `find_ent_by_class`, `entity_get_*` / `pev`), and the `amxx` crate's
  `engine::{trace_line, trace_hull, in_pvs, entities_in_sphere, ...}` gives Rust modules (the
  nav/A* module) the same calls.
* No natives for the API's entity creation (`create_entity`, `entity_key_value`,
  `spawn_entity`; RCBot's `givetsweapon`): fakemeta's `create_entity`/`dllfunc(DLLFunc_KeyValue)`
  /`DLLFunc_Spawn` or tsx's `ts_giveweapon` do it.
* No forward for `kill_scored` / `on_kill` (the score of a kill), `player_stunt` and
  `player_powerup` beyond tsfun.inc's `client_stunt` / `client_powerup`, `client_command`, or
  `melee_attack` beyond `Melee_Attack`.
* `ts_get_player_snapshot` reads all of `players()` up to the slot each call (the API has no
  single-slot query); a plugin reading every slot every frame should call it once per slot.
* Learned data and waypoints are the plugin's own files in `ts_bot_data_dir()`; tsx doesn't
  manage them.
