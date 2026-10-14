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
| TSX | `tsx`, class `xstats` | `tsx_amxx_i386.so` / `tsx_amxx.dll` | All the TS natives, the stats natives, `client_damage` / `client_death`, the rank. A metamod plugin too. |
| TSFun Wrapper | `tsfun` | `tsfun_amxx_i386.so` / `tsfun_amxx.dll` | Empty, as upstream: lets plugins with `#include <tsfun>` (`#pragma reqlib tsfun`) load. |
| Ham Sandwich | `hamsandwich` | `hamsandwich_amxx_i386.so` / `hamsandwich_amxx.dll` | `RegisterHam`, `ExecuteHam` and the rest, over the server's entity hooks (plugin API version 3). Replaces AMX Mod X's file of the same name. A metamod plugin too. |

## Requirements

* The Rust TS server ([the-specialistsmod-rs](https://github.com/the-specialists-mod/the-specialistsmod-rs))
  on Linux or Windows, with the plugin API (`main` from `f585a02` on; releases after v0.2.0).
  Older builds work too, but the natives that change players do nothing (see below).
  Both modules need version 3 of the API (`TSRS_PluginApi003`, the-specialistsmod-rs#104); a
  server with an older API, or the older modules on a newer server, find no table. Without it,
  `IsHamValid` is false and `RegisterHam`/`ExecuteHam` fail as stock does for functions missing
  from its gamedata.
* AMX Mod X 1.10 or 1.9 (1.8.2 should load: the module only requests functions that exist
  there).
* Metamod: Metamod-R or metamod-p (interface 5:13). AMX Mod X loads tsx as a metamod plugin
  by itself; don't list it in metamod's `plugins.ini`.
* A 32-bit server (AMX Mod X is 32-bit only): HLDS/ReHLDS on Linux or Windows.

## Installing

1. Download the archive for your platform from the
   [releases](https://github.com/the-specialists-mod/tsmod-amxx-rs/releases) and copy its
   `addons/` directory over your server's `ts/addons/`. That puts the three modules in
   `addons/amxmodx/modules/`, replacing AMX Mod X's own tsx, tsfun and hamsandwich (same file
   names), and the patched `tsxaddon.inc` in `addons/amxmodx/scripting/include/`.
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

## Native compatibility

Every native upstream tsx registers (`NBase.cpp`, `NRank.cpp`) is registered with the same name
and arguments, plus `ts_setuserammo`. tsx.inc, tsfun.inc, tsconst.inc and tsstats.inc are AMX
Mod X's own (use those shipped with AMX Mod X); only `tsxaddon.inc` needs this repository's
copy.

* **exact**: same behaviour as upstream.
* **API**: reads or writes the server through the plugin API. Getters fall back to what the
  game's messages carried when the server has no API; setters then do nothing, return 0 and
  log once.
* **approx.**: works, with a difference noted.

| Native | Status | Notes |
|---|---|---|
| `xmod_get_wpnname`, `get_weaponname`, `xmod_get_wpnlogname` | exact | Ids 2 and 17 use TS 3.0's names ("Beretta 92F", "Skorpion"); upstream's table had TS 2.x's ("Unk1", "STEYR-TMP") |
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
| `ts_force_run_powerup` | API, approx. | One power-up bit (1, 2, 8, 16, 256) for its normal length; 0 stops it. Upstream wrote any value; the server rejects combinations |
| `ts_is_running_powerup` | API | 0 without the API |
| `ts_is_in_slowmo` | API, approx. | When the player is slowed, the time their power-up ends (a float, as upstream); else 0 |
| `ts_set_speed`, `ts_set_physics_speed` | API | Speed 0-2 |
| `ts_setuserammo` | API, port addition | Was a function in `tsxaddon.inc` that wrote TS 2.x's private data; now a tsx native with its behaviour (see below) |
| `ts_set_bullettrail` | unsupported | TS 3.0 has no per-player bullet trails; returns 0 |
| `ts_giveweapon`, `ts_createpwup`, `ts_givepwup` | exact | Same entity approach (`ts_groundweapon` / `ts_powerup`, KeyValue, Spawn, Use/Touch). `ts_createpwup`'s origin is optional, as `tsfun.inc` declares one argument |
| `ts_getuserlastfrag`, `ts_getuserkillflags` | exact | tsx's own frag bookkeeping |
| `ts_setpddata` | approx. | Stored and unused: the thrown knife's owner comes from the API (or `pev->owner`) |
| `register_statsfwd` | exact | Returns 1 |
| `get_user_wstats`, `get_user_wrstats` | approx. | Shots are counted from the fire events (TS 3.0 never sends `ClipInfo`, upstream's source), so these return data |
| `get_user_stats`, `get_user_rstats`, `get_user_lstats`, `get_user_astats`, `get_user_vstats`, `reset_user_wstats`, `get_stats`, `get_statsnum` | exact | |
| `custom_weapon_add`, `custom_weapon_dmg`, `custom_weapon_shot` | exact | `custom_weapon_add` also keeps the log name |

Forwards `client_damage(attacker, victim, damage, wpnindex, hitplace, TA)` and
`client_death(killer, victim, wpnindex, hitplace, TK)` work as upstream, from the game's
`TSHealth` message and the victim's `dmg_inflictor`/`dmg_take`. Weapon ids are upstream's: kung
fu is 36 and a thrown knife 37. TS 3.0's own weapons 36 (Contender G2) and 37 (Akimbo
Skorpions) share those ids, as they did with upstream tsx on TS 3.0. The real plugins treat 36
as kung fu.

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
| `get_pdata_cbase`, `get_pdata_cbase_safe`, `set_pdata_cbase` | approx. | Entity and offset checks as stock; TS's C++ offsets don't exist on the Rust server, so reads give -1 (null) and writes do nothing (logged once) |

The server's own entities run what TS ran; functions the port never calls for a class (say,
`Ham_Think` on players) never fire, as with a vtable entry the game never calls.

### `ts_setuserammo` and `tsxaddon.inc`

`tsxaddon.inc` (a third-party include many TS plugins use) defined `ts_setuserammo(id, weapon,
ammo)` as a function that wrote TS 2.x's private data. tsx now provides it as a native with the
same visible behaviour: 0 for kung fu (36, after `client_cmd(id, "weapon_0")`), for ids outside
0-35, C4 (29) and the katana (34); otherwise it runs `client_cmd(id, "weapon_<id>")`, sets the
weapon's reserve ammo (for the grenade and knives, 24/25/35, its clip) through the plugin API
and returns 1 (0 if the player doesn't own the weapon, or without the API, logged once).

To use it, **recompile the plugin against [`include/tsxaddon.inc`](include/tsxaddon.inc)** from
this repository (the original minus that function, plus the native's declaration). Plugins
already compiled with the old include keep their own copy of the function, which on the Rust
server reads and writes zeroed private data and does nothing.

Not fixable in a module: other plugin code that reads or writes private data itself (fakemeta
`get_pdata_*`/`set_pdata_*` with TS offsets) gets zeros on the Rust server. Hooks through
hamsandwich work (above).

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
