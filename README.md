# tsmod-amxx-rs: AMX Mod X's TS modules for the Rust TS server

AMX Mod X's modules for The Specialists, **tsx** and **tsfun**, and **hamsandwich**, ported to
Rust for servers running [the Rust port of The Specialists 3.0](https://github.com/the-specialists-mod/the-specialistsmod-rs).
Existing compiled plugins (`.amxx`) that use `tsx.inc`, `tsfun.inc`, `tsstats.inc` or
`hamsandwich.inc` run unchanged: the modules register the same natives, forwards, library names
and cvars, and read and write the same `tsstats.dat`. AMX Mod X's stock hamsandwich can't work on
the Rust server (it patches C++ vtables the server doesn't have, and crashes on Windows); this one
replaces it.

Upstream tsx read and wrote TS's C++ `CBasePlayer` at fixed offsets (from TS 2.x). The Rust
server has no such object, so this tsx reads the game's user messages, as upstream also did,
and uses the server's **plugin API** (`ts-plugin-api`, a C table the server library exports
through `CreateInterface`) for everything else: money, slots, awards, power-ups, speed, and
the weapon in hand.

| Module | Library | File (Linux / Windows) | What |
|---|---|---|---|
| TSX | `tsx`, class `xstats` | `tsx_amxx_i386.so` / `tsx_amxx.dll` | All the TS natives, the stats natives, `client_damage` / `client_death`, tsfun.inc's `client_stunt` / `client_powerup` / `Melee_Attack`, the rank. A metamod plugin too. |
| TSFun Wrapper | `tsfun` | `tsfun_amxx_i386.so` / `tsfun_amxx.dll` | Empty, as upstream: lets plugins with `#include <tsfun>` (`#pragma reqlib tsfun`) load. |
| Ham Sandwich | `hamsandwich` | `hamsandwich_amxx_i386.so` / `hamsandwich_amxx.dll` | `RegisterHam`, `ExecuteHam` and the rest, over the server's entity hooks (plugin API version 5, including the gun's `Item_*` / `Weapon_*` functions on `weapon_tsgun`), and `get_pdata_cbase` over the server's private-data mirror. Replaces AMX Mod X's file of the same name. A metamod plugin too. |

## Requirements

* The Rust TS server ([the-specialistsmod-rs](https://github.com/the-specialists-mod/the-specialistsmod-rs))
  with plugin API version 7 (v3.0.8 or later), on Linux or Windows. Older builds work too, but the natives that change
  players do nothing (see below). Both modules need version 7 of the API (`TSRS_PluginApi007`; version 5 brought what
  they use: kill reports, the tsfun forwards and the private-data mirror, tsmod-amxx-rs#8, #10, #15,
  the-specialistsmod-rs#197; versions 6 and 7 brought what tsx's bot natives use, `tsbot.inc`); a
  server with an older API, or the older modules on a newer server, find no table. Without it,
  `IsHamValid` is false and `RegisterHam`/`ExecuteHam` fail as stock does for functions missing
  from its gamedata.
* AMX Mod X 1.10 or 1.9. 1.8.2 works on Windows; on Linux its core calls the Rust server
  library on a misaligned stack, so natives that reach the game library (e.g. `engclient_cmd`)
  crash the server until the-specialistsmod-rs#199 is fixed (`docs/design.md` §7). 1.8.2's
  fakemeta can't read `Ham_Keyvalue`'s KeyValueData (stock 1.8.2 has the same limit).
* Metamod: Metamod-R or metamod-p (interface 5:13). AMX Mod X loads tsx as a metamod plugin
  by itself; don't list it in metamod's `plugins.ini`.
* A 32-bit server (AMX Mod X is 32-bit only): HLDS/ReHLDS on Linux or Windows. The end-to-end
  tests pass on ReHLDS 3.15 with Metamod-R 1.3 or metamod-p 1.21p109, and on HLDS 25th
  anniversary with metamod-p (`docs/design.md` §7).

## Installing

1. Download the archive for your platform from the
   [releases](https://github.com/the-specialists-mod/tsmod-amxx-rs/releases) and copy its
   `addons/` directory over your server's `ts/addons/`. That puts the three modules in
   `addons/amxmodx/modules/`, replacing AMX Mod X's own tsx, tsfun and hamsandwich (same file
   names), and the patched `tsxaddon.inc` and `tsconst.inc` in
   `addons/amxmodx/scripting/include/`.
2. Plugins that include `tsx.inc`/`tsfun.inc` load the modules automatically. Otherwise add
   `tsx` and `tsfun` lines to `addons/amxmodx/configs/modules.ini`.
3. Optional, for the stats plugins (`statsx`, `tsstats`): `addons/amxmodx/data/tsstats.amxx`
   (the score calculator, compiled from AMX Mod X's `plugins/ts/tsstats.sma`) and the cvars
   below.
4. Check with `amxx modules` (TSX and TSFun Wrapper "running") and in the AMX Mod X log. On a
   server without the plugin API, tsx logs one warning:
   `[TSX] WARNING: no TS plugin API (...)`.

hamsandwich needs no gamedata files (`data/gamedata/.../ts/offsets*`); it ignores them.

Cvars (as upstream): `tsstats_maxsize` (3500: the rank is cleared at map change when it is
this long), `tsstats_reset` (1: clear at the next map change), `tsstats_rank` (0 name, 1 Steam
ID, 2 IP), `tsstats_rankbots` (1), `tsstats_pause` (1 stops stats). Localinfo `tsstats`
(`addons/amxmodx/data/tsstats.dat`) and `tsstats_score` (`addons/amxmodx/data/tsstats.amxx`).
Port addition: `tsx_legacy_weaponids` (0: TS 3.0's weapon ids; 1: upstream's, for plugins that
hard-code kung fu as 36; see [Weapon ids](#weapon-ids)). Set it in `server.cfg` or `amxx.cfg`;
tsx reads it at map start and when a player enters the game.

## Native compatibility

Every native upstream tsx registers (`NBase.cpp`, `NRank.cpp`) is registered with the same name
and arguments, plus `ts_setuserammo` and `ts_set_fuattack`. tsx.inc and tsstats.inc are AMX Mod
X's own (use those shipped with AMX Mod X); `tsxaddon.inc` needs this repository's copy,
`tsconst.inc` does for TS 3.0's weapon ids (`TSW_CONTENDER`, `TSW_ASKORPIONS`, and kung fu and
the thrown knife where tsx reports them by default), and `tsfun.inc` does for `client_stunt`,
`client_powerup`, `Melee_Attack` and `ts_set_fuattack` (the stock file marks them unavailable;
plugins built against it still get the forwards).

* **exact**: same behaviour as upstream.
* **API**: reads or writes the server through the plugin API. Getters fall back to what the
  game's messages carried when the server has no API; setters then do nothing, return 0 and
  log once.
* **approx.**: works, with a difference noted.

| Native | Status | Notes |
|---|---|---|
| `xmod_get_wpnname`, `get_weaponname`, `xmod_get_wpnlogname` | approx. | TS 3.0's weapon ids by default: 36 Contender G2, 37 Akimbo Skorpions, 38 kung fu, 29 thrown knife (upstream: 36 kung fu, 37 thrown knife, with `tsx_legacy_weaponids 1`). Ids 2 and 17 use TS 3.0's names ("Beretta 92F", "Skorpion"); upstream's table had TS 2.x's ("Unk1", "STEYR-TMP") |
| `xmod_is_melee_wpn`, `xmod_get_maxweapons` (44), `xmod_get_stats_size` (8), `xmod_is_custom_wpn` | exact | |
| `ts_wpnlogtoname`, `ts_wpnlogtoid` | exact | Also resolve TS 3.0's own log spellings (`contender_g2`, `throwing_katana`, `kung-fu`, ...) |
| `ts_getuserwpn`, `get_user_weapon` | API | The weapon in hand with its clip, reserve, mode, attachments. Without the API: the last `WeaponInfo` (players) or the player model (bots, which get no `WeaponInfo`) |
| `ts_getusercash`, `ts_getuserspace`, `ts_getuserslots` | API | Without: `TSCash` / `TSSpace`. Slots are free inventory slots, as `TSSpace` |
| `ts_getuserstate` | API | The stance `TSState` carries |
| `ts_getuserpwup`, `ts_getuseritems`, `ts_has_superjump`, `ts_has_fupowerup` | API | Without: from `PwUp` |
| `ts_getkillingstreak` | API | TS's own streak. Without: tsx's count, as upstream |
| `ts_get_message` | API, approx. | TS 3.0's award byte (`TSMSG_*`); no award reads as `TSMSG_NORMAL` (6), so the `ts_is_normal` stock works. Without the API: from `TSPAward` |
| `ts_set_message` | API, approx. | 1-16 as upstream; `TSMSG_NORMAL` clears the award |
| `ts_setusercash`, `ts_setuserslots` | API | Returns as upstream (`ts_setusercash` always 0). The server accepts cash 0-30000, slots 0-81 |
| `ts_set_fakeslowmo`, `ts_set_fakeslowpause` | API | Run slow motion / slow pause for the given time without using a held power-up |
| `ts_force_run_powerup` | API, approx. | Runs one power-up (1, 2, 8, 16, 256) for its normal length; 0 stops it. Upstream wrote any value; TS 3.0 runs one power-up at a time, so with several bits set tsx runs the lowest of those and logs once that it dropped the rest. Superjump, which TS 3.0 never runs for a time (picked up, it is a permanent item), runs 3 s, the base time of TS 3.0's timed power-ups. A value with none of those bits returns 0 (logged once) |
| `ts_is_running_powerup` | API | 0 without the API |
| `ts_is_in_slowmo` | API, approx. | When the player is slowed, the time their power-up ends (a float, as upstream); else 0 |
| `ts_set_speed`, `ts_set_physics_speed` | API | Speed 0-2 |
| `ts_setuserammo` | API, port addition | Was a function in `tsxaddon.inc` that wrote TS 2.x's private data; now a tsx native with its behaviour (see below) |
| `ts_set_bullettrail` | unsupported | TS 3.0 has no per-player bullet trails; returns 0 |
| `ts_giveweapon`, `ts_createpwup`, `ts_givepwup` | exact | Same entity approach (`ts_groundweapon` / `ts_powerup`, KeyValue, Spawn, Use/Touch). `ts_createpwup`'s origin is optional, as `tsfun.inc` declares one argument |
| `ts_getuserlastfrag`, `ts_getuserkillflags` | API, approx. | TS 3.0's scoring (2 s double kills, +1 stunts and slides, specialists at a 9-kill spree), not upstream's TS 2.x model: the server's report of the kill, else predicted; the last frag is the frags the kill gave ([design §4](docs/design.md#4-messages)) |
| `ts_set_fuattack` | API, port addition | Commented out in stock tsfun.inc ("Alters a fu attack"): during `Melee_Attack` for its player, sets the move's time and damage and returns 1; else 0 |
| `ts_setpddata` | approx. | Stored and unused: the thrown knife's owner comes from the API (or `pev->owner`) |
| `register_statsfwd` | exact | Returns 1 |
| `get_user_wstats`, `get_user_wrstats` | approx. | Shots are counted from the fire events (TS 3.0 never sends `ClipInfo`, upstream's source), so these return data |
| `get_user_stats`, `get_user_rstats`, `get_user_lstats`, `get_user_astats`, `get_user_vstats`, `reset_user_wstats`, `get_stats`, `get_statsnum` | exact | |
| `custom_weapon_add`, `custom_weapon_dmg`, `custom_weapon_shot` | exact | `custom_weapon_add` also keeps the log name |

Forwards `client_damage(attacker, victim, damage, wpnindex, hitplace, TA)` and
`client_death(killer, victim, wpnindex, hitplace, TK)` work as upstream, from the game's
`TSHealth` message and the victim's `dmg_inflictor`/`dmg_take`. With the plugin API, the death
waits for the server's report of how it scored the kill (killer, frags, tags, weapon), so a kill
the server scores late is no longer a suicide; without it, the kill's weapon comes from TS's
`DeathMsg` (kung fu when TS says "Kung Fu" or "world"), where upstream reported every kill whose
frags differed from its TS 2.x prediction as kung fu.

Port additions: tsfun.inc's `client_stunt(id, stunttype)` (`STUNT_ROLL` 2, `STUNT_DIVE` 3,
`STUNT_GETUP` 4, `STUNT_FLIP` 5, and 6 wall jump / 7 wall climb), `client_powerup(id, powerup)`
(a held power-up used, or one that works at once picked up) and `Melee_Attack(id, Float:time,
Float:damage)` (before a kung-fu move; `ts_set_fuattack` changes it, `PLUGIN_HANDLED` blocks it)
fire from the server's plugin API (version 5). Upstream declared them "UNAVAILABLE IN 1.70".

### Weapon ids

Upstream tsx numbers weapons as TS 2.x did, with kung fu (which `WeaponInfo` sends as 0) as 36
and a thrown knife as 37 (`tsconst.inc`'s `TSW_KUNG_FU`, `TSW_TKNIFE`). TS 3.0 added the
Contender G2 as 36 and the Akimbo Skorpions as 37, so on TS 3.0 upstream reports them as kung
fu and thrown knife. By default this tsx reports **TS 3.0's ids**: 1-37 as TS 3.0 sends them,
kung fu as **38** and a thrown knife as **29** (the two ids below the custom slots TS 3.0 never
uses: upstream's unused "breakable" and TS 2.x's C4). `TSMAX_WEAPONS` stays 44 and the custom
weapons stay 39-43, so plugins' `[TSMAX_WEAPONS]` arrays fit; `tsstats.dat` keeps no
per-weapon data and is unaffected. Plugins that use `TSW_KUNG_FU`/`TSW_TKNIFE` or 36/37:
recompile against [`include/tsconst.inc`](include/tsconst.inc), or set
`tsx_legacy_weaponids 1` for upstream's numbering (the Contender and Akimbo Skorpions then read
as kung fu and thrown knife again). The weapon ids every other native takes and returns
(`get_user_wstats`, `ts_getuserwpn`, `ts_setuserammo`, ...) follow the same numbering.

### Ham Sandwich

Every native of AMX Mod X 1.10's hamsandwich is registered with stock's name, arguments, return
values and error messages. Functions are `ham_const.inc`'s `Ham` enum; the server decides which it
has (`IsHamValid`) and for which classes.

| Natives | Status | Notes |
|---|---|---|
| `RegisterHam`, `RegisterHamFromEntity` (and the `RegisterHamPlayer` stock) | exact | Hooks are per class, as stock's per-vtable hooks. A function the server lacks fails the plugin through `__fatal_ham_error`/`HamFilter` like stock's "not configured in gamedata"; an unknown class or one without the function is a native error. `specialbot` needs nothing (every player is one class) |
| `EnableHamForward`, `DisableHamForward`, `IsHamValid` | exact | `IsHamValid` is "the server has this function" |
| `ExecuteHam`, `ExecuteHamB` | exact | Arguments by reference as stock; float, vector and string results go to the extra by-reference parameters; pointer arguments are written back. `ExecuteHamB` runs the hooks |
| `GetHamReturnStatus`, `GetHamReturn*`, `GetOrigHamReturn*`, `SetHamReturn*`, `SetHamParam*` | exact | Parameter 1 is `this`. Pre forwards see earlier forwards' changes; the highest result skips the function at `HAM_SUPERCEDE`; post forwards run even then; the last forward's result decides whether the return value is the plugin's (`HAM_OVERRIDE` or more) or the function's, as stock. `SetHamParamEntity` keeps the index later forwards see, `SetHamParamEntity2` updates it |
| `SetHamParamTraceResult`, `SetHamParamItemInfo` | exact | TraceResults are handles fakemeta's `get_tr2`/`set_tr2` take |
| `CreateHamItemInfo`, `FreeHamItemInfo`, `GetHamItemInfo`, `SetHamItemInfo` | approx. | An unknown handle is a native error (stock dereferences any value) |
| `get_pdata_cbase`, `get_pdata_cbase_safe`, `set_pdata_cbase` | approx. | Entity and offset checks as stock. The Rust server mirrors the TS members plugins read at TS's offsets (`m_pActiveItem` at 639, +5 on Linux, is the player's `weapon_tsgun`; its `m_pPlayer` at 28, +4 on Linux); reads and writes are bounded by the block (a native error past it, and for writes to the server's link at bytes 8..16); a pointer to no entity is -1 (`_safe`: -2). Without the API: -1 and no write, logged once |

The server's own entities run what TS ran; functions the port never calls for a class (say,
`Ham_Think` on players) never fire, as with a vtable entry the game never calls.

### `ts_setuserammo` and `tsxaddon.inc`

`tsxaddon.inc` (a third-party include many TS plugins use) defined `ts_setuserammo(id, weapon,
ammo)` as a function that wrote TS 2.x's private data. tsx now provides it as a native with the
same visible behaviour: 0 for kung fu (38, or 36 with `tsx_legacy_weaponids 1`, after
`client_cmd(id, "weapon_0")`), for ids outside 0-37 (0-35, TS 2.x's weapons, with
`tsx_legacy_weaponids 1`), 29 (C4, or TS 3.0's thrown knife) and the katana (34); otherwise it runs `client_cmd(id, "weapon_<id>")`, sets the
weapon's reserve ammo (for the grenade and knives, 24/25/35, its clip) through the plugin API
and returns 1 (0 if the player doesn't own the weapon, or without the API, logged once).

To use it, **recompile the plugin against [`include/tsxaddon.inc`](include/tsxaddon.inc)** from
this repository (the original minus that function, plus the native's declaration). Plugins
already compiled with the old include keep their own copy of the function, which writes the
`weapon_tsgun` at `mp.dll`'s offsets: on a Windows server the reserve (int 49 + ammo type) and
clip (`m_iClip`, int 41) it writes go into the game through the server's private-data mirror
(below); on Linux its default shift of 5 ints lands one int past TS's (`CTSGun` is shifted by 4),
as it did on TS's own Linux server.

### Private data (fakemeta, `get_pdata_cbase`)

The Rust server's entities aren't C++ objects, but since plugin API version 5 the server writes
the TS 3.0 members plugins read into their private data at TS's offsets, for its platform, and
takes some writes back (the port's `docs/implementation/plugin-api.md` §4.1): `pev`; a player's
`m_pActiveItem` (its `weapon_tsgun`); the `weapon_tsgun`'s `m_pPlayer`, `m_iId` (16), `m_iClip`
(written: the gun in hand's clip), current weapon id, reserve ammo per ammo type (written: taken
into the game) and the 38 weapon records' owned / ammo type / fire mode, clip (written: taken) and
attachments. So stock fakemeta's `get_pdata_int`/`set_pdata_int` and this
hamsandwich's `get_pdata_cbase` read them (values up to a frame old; a write takes effect within a
frame). Other offsets read zeros (or what a plugin wrote) and their writes do nothing.

## Building

Requirements: Rust stable (edition 2024) with the `i686-unknown-linux-gnu` and
`i686-pc-windows-gnu` targets, clang 18 or newer (bindgen), 32-bit glibc/libgcc, MinGW-w64
(`i686-w64-mingw32-*`), and an [xash3d-fwgs](https://github.com/FWGS/xash3d-fwgs) checkout next to
this repository with its `3rdparty/library_suffix` submodule (or set `FWGS_DIR`): the engine
bindings come from the port's `goldsrc-sys` crate, which generates them from the FWGS headers.

```sh
cargo xtask build [--release]    # tsx, tsfun, hamsandwich for i686 Linux and Windows
cargo xtask dist                 # dist/<platform>/addons/amxmodx/{modules,scripting/include}/, stripped
cargo xtask test                 # unit tests (host and i686)
cargo xtask lint                 # rustfmt check, clippy -D warnings for both targets
cargo xtask e2e [linux] [win]    # end-to-end on the amxx-testenv roots (see AGENTS.md)
```

Pushing a `v*` tag builds both platforms and publishes `tsmod-amxx-rs-<tag>-<platform>.zip`
(`.github/workflows/release.yml`).

## Documentation

| Doc | Contents |
|---|---|
| [AGENTS.md](AGENTS.md) | Working in this repository: layout, the `unsafe` boundary, commands, conventions, pitfalls |
| [docs/design.md](docs/design.md) | How the modules work: loading, the plugin API lookup, message parsing, stats and the rank file, hamsandwich's dispatch, deviations from upstream |

## License and credits

GPL-3.0-or-later, with AMX Mod X's linking exceptions ([LICENSE](LICENSE),
[GPL-EXCEPTION.txt](GPL-EXCEPTION.txt)). This is a port of AMX Mod X's TS modules,
hamsandwich and module SDK: Copyright (C) The AMX Mod X Development Team; tsx originally by Lukasz Wlasinski
(2004), based on AMX Mod by Aleksander Naszko ("OLO"). `include/tsxaddon.inc`: Avalanche, Twilight Suzuka and Harbu. Metamod's interface: Will Day
(metamod), the metamod-p and Metamod-R teams. The Specialists by The Specialists team.

The port's plugin API crate `ts-plugin-api` (a dependency from the-specialistsmod-rs) is
MIT-licensed. MIT is compatible with the GPL, so the built modules are is distributed under the
GPL as a whole. Keep the API crate's MIT notice with it.
