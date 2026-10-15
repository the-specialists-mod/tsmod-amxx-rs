# Design

How the Rust tsx, tsfun and hamsandwich work, and where they differ from upstream AMX Mod X
(`modules/ts/tsx` and `modules/hamsandwich`, version 1.10). Upstream file names in sections 1-7
are tsx's; section 8 is hamsandwich.

## 1. Crates and the `unsafe` boundary

```
crates/amxx/    the AMX Mod X module SDK and metamod glue (all FFI): repr(C) ABI types, the
                AMXX_* and Meta_* export macros, native tables, core functions, engine and
                edict wrappers, the score-plugin loader, finding the game library's exports
crates/tsx/     the TSX module (cdylib + rlib): deny(unsafe_code) except `ffi`
crates/tsfun/   the TSFun wrapper (cdylib): module info only
crates/hamsandwich/  Ham Sandwich (cdylib + rlib): deny(unsafe_code) except `ffi` (section 8)
xtask/          build, dist, lint, test, e2e
e2e/            the end-to-end test plugin
```

`tsx` keeps its logic behind two traits so it can be tested without an engine:
`game::Game` (engine fields by entity index, the game library's entity calls, forwards, cvars,
logging) and `game::TsApi` (the plugin API). `ffi::game::RealGame` implements both for real;
`testing::FakeGame` for tests. Natives are `fn(&dyn Game, &mut dyn Args) -> Cell` with
`amxx::native::Args` over the AMX (or `FakeArgs`, an in-memory cell buffer).

State is a thread-local `RefCell<State>` (`state::with`). Plugins run inside forwards and inside
game calls (`ts_giveweapon`'s `Use` sends messages, which run tsx's hooks), so no borrow is held
across those: `TSHealth` handling returns the hit, the hook releases the state, runs
`client_damage`, then takes the state again for the death half (`State::after_damage`) and runs
`client_death`.

## 2. Loading

AMX Mod X sees `Meta_Attach` in tsx and loads it as a metamod plugin first
(`CModule::attachMetamod`), then calls `AMXX_Query`/`AMXX_Attach` on the same library:

1. `GiveFnptrsToDll` (stdcall on Windows; `crates/tsx/build.rs` makes MinGW export the plain
   name) copies the engine table. Only the entries every engine and metamod build has are
   copied (up to `pfnCheckParm`): the bindings' `enginefuncs_t` is FWGS's, which is longer.
2. `Meta_Query` / `Meta_Attach` (interface 5:13): registers the `tsstats_*` cvars (upstream
   `OnMetaAttach`). The hook tables are written field by field into metamod's tables for the
   same reason.
3. `AMXX_Attach`: requests only the core functions tsx calls (upstream's SDK requests ~80 and
   refuses to load if one is missing; `LoadAmxScriptEx` is optional, so 1.8.2 cores load it),
   adds the natives, loads the score plugin and `tsstats.dat`.
4. `AMXX_PluginsLoaded`: registers `client_damage` and `client_death` (and, port additions,
   tsfun.inc's `client_stunt`, `client_powerup` and `Melee_Attack`), then looks up the plugin
   API and registers tsx as its listener (section 3).

Hooks (as upstream): pre `ClientDisconnect`, `ServerDeactivate`; post `ClientConnect`,
`ClientPutInServer`, `ClientUserInfoChanged`, `ServerActivate`, `PlayerPreThink`; engine post
`TraceLine`, `MessageBegin` ... `MessageEnd`, `RegUserMsg`, plus (port additions)
`PrecacheEvent` and `PlaybackEvent` for shot counting.

## 3. The plugin API

The Rust server exports tier1's `CreateInterface`. tsx asks metamod for the game library's path
(`GET_GAME_INFO(PLID, GINFO_REALDLL_FULLPATH)`), finds the export in the already loaded library
(`dlopen(RTLD_NOLOAD)` + `dlsym` on Linux; `GetModuleHandleA` + `GetProcAddress` on Windows,
retrying with the bare file name) and calls it with `TSRS_PluginApi005` (version 5,
tsmod-amxx-rs#8, #10 and #15, the-specialistsmod-rs#197). The name carries the version and versions aren't compatible, so a
server with another version answers NULL; a table whose `version` isn't 5 or whose `size` is
smaller than this build's `PluginApi` is refused too (`amxx::tsapi::find`). The lookup runs at `AMXX_PluginsLoaded`
and again at each `ServerActivate` until it succeeds; a server without the API gets one warning
in the AMX Mod X log.

tsx and hamsandwich both need version 5; the modules use none of version 3's additions (bots,
entity info, host services) or version 4's (team scores, objectives, entity creation), only the
player functions, the entity hooks (whose version 4 additions hamsandwich passes through, section
8: the gun's functions, the queries, `Ham_TS_GiveSlowMul`'s float) and version 5's:

* **The listener** (`ffi/listener.rs`): tsx registers a `Plugin` descriptor with
  `add_listener` once it finds the API (removed at `AMXX_Detach`), with four callbacks:
  `player_stunt` fires `client_stunt(id, stunttype)`, `player_powerup` fires
  `client_powerup(id, powerup)`, `melee_attack` fires `Melee_Attack(id, Float:time,
  Float:damage)` (`forwards.rs`), and `kill_scored` finishes the death half of `TSHealth`
  (section 4). tsfun.inc declares the three forwards "UNAVAILABLE IN 1.70": no module fired them
  since TS 2.x. `stunttype` is tsconst.inc's `STUNT_ROLL` 2, `STUNT_DIVE` 3, `STUNT_GETUP` 4 or
  `STUNT_FLIP` 5, or 6 (wall jump) / 7 (wall climb), TS 3.0 stunts 2.x lacked; TS 3.0's
  `TSState` only says stand / crouch / prone, so the server reports the moves themselves.
  `client_powerup` fires when a held power-up is used and when one that works at once is picked
  up (health, armour, kung fu, superjump, grenade). `Melee_Attack` runs before a kung-fu move
  lands, with its delay and damage; `ts_set_fuattack(id, time, damage)` (commented out in
  tsfun.inc, a native again) changes them during the forward (1, else 0), and `PLUGIN_HANDLED`
  (the forward is `ET_STOP`) blocks the move. The repository's patched `include/tsfun.inc` drops
  the forwards' dummy `UNAVAILABLE` parameter and declares the native; tsx passes a 4th cell of 0,
  so plugins built against the stock file work too. Floats go as cells holding their bits.
* **The private-data mirror**: the server writes TS 3.0 members plugins read into its entities'
  private data at TS's offsets (the port's `docs/implementation/plugin-api.md` §4.1), so
  fakemeta's `get_pdata_*` / `set_pdata_*` work unchanged; hamsandwich's `get_pdata_cbase` /
  `set_pdata_cbase` read and write it within the block's size (`private_size`, section 8).

Contract (`ts-plugin-api` docs and the port's `docs/implementation/plugin-api.md`): entity
indices, engine thread only; `player_info` returns a snapshot refreshed each frame, so a getter
right after a setter shows the old value until the next frame; setters called inside the
player's own callback are queued (`DEFERRED`, which tsx counts as success).

How natives use it: getters prefer `player_info` and fall back to the message cache; setters
return 1 on `OK`/`DEFERRED`, 0 otherwise. A missing API, or a value the server rejects
(`INVALID`), is logged once per native and never raises a native error (upstream raised none:
it wrote whatever it got). `projectile_owner` names a thrown knife's thrower in `TSHealth`
handling, where upstream read the knife's private data at `ts_setpddata`'s offset.

## 4. Messages

tsx registers no messages; it reads the game's, matched by name at `RegUserMsg` (or through
metamod's `GET_USER_MSG_ID` at `ServerActivate`, for a module loaded after the game registered
them). Upstream handled each `WRITE_*` as it came; tsx collects a message's values and handles
them at `MESSAGE_END`. The layouts are TS 3.0's as the Rust server writes them (its
`crates/shared/src/protocol`), and `state_tests.rs` feeds the parsers through those very
encoders (`shared` is a dev-dependency).

| Message | Payload | tsx keeps |
|---|---|---|
| `ResetHUD` | byte | alive: commit life stats in 0.25 s; dead: reset spree, items, kill flags, resync frags |
| `WeaponInfo` | byte id, byte clip, short reserve, byte mode, byte attachments | weapon in hand (0 = kung fu = 38, or 36 with `tsx_legacy_weaponids`) and its ammo |
| `ClipInfo` | byte clip | a drop = a shot (TS 3.0 never sends it) |
| `ScoreInfo` (all) | byte player, short frags, deaths, class, team | team (the port sends team + 1) |
| `TSHealth` | byte health | damage and death (below) |
| `TSState` | byte stance | stance |
| `WStatus` | byte id, byte owned, byte clip | owned 0: back to kung fu (upstream's fix for dropped weapons) |
| `TSCash` | short | money |
| `TSSpace` | byte | free slots |
| `PwUp` | short bits, byte seconds | permanent kung fu (4) / super jump (256), else the held power-up |
| `TSPAward` (all) | byte player, byte award | award (port addition, for `ts_get_message` without the API) |
| `DeathMsg` (all) | byte killer, byte victim, string weapon | the victim's last death notice (port addition, for the kill's weapon) |

The server sends bots none of the per-player weapon messages, so `PlayerPreThink` refreshes the
weapon in hand from the API, or for bots without it from `pev->weaponmodel`
(`models/p_<name>[_sil].mdl`, names from the port's `assets.toml`).

`TSHealth` (upstream `Client_TSHealth_End`): the victim's `dmg_inflictor` and `dmg_take`
(consumed) give the hit. A player inflictor uses their weapon in hand and aim (the hit group
of their last `TraceLine` that hit a player); a `grenade` entity its owner (weapon 24); a
`knife` the API's `projectile_owner` (the thrown knife, 29 or with `tsx_legacy_weaponids` 37); anything else counts as self-damage with
weapon 0. Then `client_damage`, and if the victim is dead: the kill flags and frag
bookkeeping, the kill stats and `client_death`.

The death half follows TS 3.0's scoring (`scoring.rs`: `TSGetPointsForFrag` 0x89140 and
`PlayerKilled` 0x895e4 as the port's `rules/scoring.rs` has them), not upstream's TS 2.x model
(per-weapon bonus with katana 2 and kung fu 3, +2 for a stunt from `iuser4` 20/1028/2052, x2
within 1 s, x2 from a 10-kill spree before the +5):

* 1 point; 2 for a kung-fu frag or a thrown knife; a grenade (any other inflictor) 1, no stunt.
* +1 for a stunt (`iuser4 & 0x10c10` at over 80 u/s, `TSKF_STUNTKILL`) or a slide (`iuser4 &
  0x20`, `friction < 1`, over 80 u/s, `TSKF_SLIDINGKILL`), but not under `glast_closecombat`.
* x2 for a double kill (under 2 s since the killer's last scoring kill; a double kill resets
  that to 0, `TSKF_DOUBLEKILL`), +5 for a victim on a spree over 8 (`TSKF_KILLEDSPEC`), then x2
  from the killer's 10th consecutive kill. The spree counts every kill of another player, team
  kills included; 9 or more is a Specialist (`TSKF_ISSPEC`).

The kill's weapon, and so its kung-fu-ness, comes from TS itself: `PlayerKilled` broadcasts
`DeathMsg` just before `Killed` sends the victim's `TSHealth` 0, so tsx keeps it per victim and
the death half reads it. For a kill by the killer's own hand, "Kung Fu" (a close-combat hit,
also with a gun in hand) and "world" (kung fu outside one) are kung fu, and a weapon name is
that weapon (`<name> Stock`, a pistol-whip, is the gun). `ts_getuserlastfrag` is the change in
`pev->frags`, as upstream's corrected value was.

With the plugin API's kill reports (version 5, `kill_scored`; #15, the-specialistsmod-rs#197) tsx
uses the server's scoring instead: the death half of a `TSHealth` 0 waits for the report of that
death (`State::after_damage`, `kill_scored`, `player.pending_death`), which carries the killer
(the victim for a suicide, 0 for no player: both self kills), the frags it got, its spree, the
tags (diving, sliding, double kill, killed specialist) and the `DeathMsg` name. The server may
score a kill after the victim's `TSHealth` 0 (`PlayerKilled` deferred while the killer is out of
reach), when neither the frags nor the `DeathMsg` are there yet: the prediction made such a kill
a suicide and gave its frags to the killer's next kill. The report comes before the engine's next
callback; a death without one (a death the rules don't score, a teamplay team change) gets the
prediction above at the victim's next `PlayerPreThink` (`State::flush_death`), and a report
without a `TSHealth` 0 (a death without damage) is dropped, as upstream fired no `client_death`
then. The prediction stays the model for servers without the API.

Weapon ids (`weapons.rs`): TS 3.0's by default (wire ids 1-37, kung fu 38, thrown knife 29),
upstream's with `tsx_legacy_weaponids 1` (kung fu 36, thrown knife 37, shared with the Contender
G2 and Akimbo Skorpions). `State::sync_weapon_ids` reads the cvar at `ServerActivate` and at
every `PutInServer`, because `server.cfg` and `amxx.cfg` run after `ServerActivate`; a switch
rebuilds the TS slots (custom slots stay) and moves a player holding kung fu to its new id.

## 5. Stats and the rank

Per player, as upstream: stats per weapon (since connect and this round), against each
attacker and victim, and since the last spawn (`life`), committed to the rank at the next spawn
(`ResetHUD` + 0.25 s), at disconnect and at map change. Bots count unless `tsstats_rankbots 0`.

Shots: upstream counted a shot whenever `ClipInfo` showed fewer rounds, but TS 3.0 never sends
it (decomp `docs/server/architecture.md` §3), so with upstream tsx on TS 3.0 `get_user_wstats`
returned nothing. tsx counts one shot per `events/fire.sc` or `events/firelinked.sc` the shooter
plays (indices learnt at `PrecacheEvent`), plus upstream's melee shot per melee hit.

`tsstats.dat` (`rank.rs`): version 5, little endian, best first:
`i16 version; { i16 namelen; name\0; i16 uniquelen; unique\0; i32 kills, deaths, hs, tks,
damage, hits, shots; i32 bodyhits[8] }*; i16 0`. Byte-compatible with upstream's writer (a test
builds a file from `CRank.cpp`'s `fwrite`s, loads it and saves it back identically). The score
is `get_score(stats[8], body[8])` from `tsstats.amxx` when it loads, else kills - deaths;
entries scored `INT_MIN` aren't saved.

## 6. Deviations from upstream

Fixes:

* Upstream's `loadRank` reads the seven counters in another order than `saveRank` writes them
  (tks, damage, deaths, kills, shots, hits, hs), scrambling the rank on every load; tsx reads
  them as written.
* Upstream's IP rank lookup (`tsstats_rank 2`) never advances past a non-matching entry and hangs
  the server; tsx walks the list.
* `CPlayer::Disconnect` returned early for ignored bots, leaving them "in game"; fixed.
* Score-plugin errors name `tsstats_score` (upstream logged `csstats_score`).
* Natives check their argument count before writing by-reference arguments (`ts_getuserpwup`
  and `ts_createpwup` are declared with fewer arguments in `tsfun.inc` than upstream reads).
* Bounds: weapon and player indices, body parts (hit group 10 has no slot), custom weapon ids.

Differences:

* Weapon ids: TS 3.0's by default, so 36/37 are the Contender G2 / Akimbo Skorpions, kung fu
  is 38 (upstream 36) and the thrown knife 29 (upstream 37, TS 2.x's C4 there); the cvar
  `tsx_legacy_weaponids 1` (port addition) restores upstream's. `TSMAX_WEAPONS` (44) and the
  custom slots (39-43) don't move. The repository ships a patched `include/tsconst.inc`
  (`TSW_CONTENDER`, `TSW_ASKORPIONS`, the moved `TSW_KUNG_FU`/`TSW_TKNIFE`). TS 3.0's two guns
  have a frag bonus of 1 (TS 3.0's base frag).
* Weapon names 2 and 17, log-name aliases (`weapons.rs`).
* Shots from fire events; bots' weapons from their model without the API.
* Player state through the API instead of `CBasePlayer` offsets (section 3).
* `ts_get_message`: no award reads as `TSMSG_NORMAL` (6); `ts_set_message(6)` clears it.
* `ts_set_bullettrail` does nothing (the server answers UNSUPPORTED).
* `ts_force_run_powerup`: upstream wrote any value into TS 2.x's running-power-up field; the
  API's `run_powerup` runs exactly one of slow motion, infinite clip, slow pause, double fire
  rate and superjump, as TS 3.0 runs one power-up at a time. With several bits set tsx runs the
  lowest of those and logs once that the others were dropped; a value with none of them returns
  0 (logged once) without asking the server. Superjump has no running length in TS 3.0 (a
  picked-up one is a permanent item, `PlayerAddPowerUp` 0x97bc8; `SetupPowerUp` 0x98014 has no
  base time for it), so tsx runs it for 3 s (`SUPERJUMP_SECS`), `SetupPowerUp`'s base time of
  slow motion, infinite clip and double fire rate with `pwupmult` 1.
* `custom_weapon_add` stores its log name (upstream ignored the argument).
* Kills are scored as TS 3.0 scores them (section 4), so `ts_getuserkillflags` reports TS 3.0's
  double kills (2 s), stunts and slides, and specialists (spree 9). Upstream predicted TS 2.x's
  frags and, when `pev->frags` differed and the kill wasn't a slide, reported the kill as kung fu,
  which on TS 3.0 turned katana kills, stunt kills with a gun and kills 1-2 s apart into kung fu
  kills in `client_death` and the weapon stats. tsx takes the weapon from TS's `DeathMsg`; only
  for a kill without one does it still fall back to kung fu, and only when a close-combat
  kung-fu frag gives exactly the frags TS gave. The weapon table has no frag bonus column.
* A team kill resyncs tsx's frag count with `pev->frags` and counts in the killer's spree, as in
  TS 3.0; upstream left the count one high, so the killer's next kill looked like no frag and
  was reported as a suicide.
* Known limit: in The One mode a kill by a player who isn't The One scores 0 and is reported as
  upstream does, as a self kill, so it doesn't count in that player's tsx spree (TS counts it).
* `ts_setuserammo` (port addition): `tsxaddon.inc`'s private-data function as a native, through
  the API's `set_weapon_ammo`; the repository ships the patched `include/tsxaddon.inc`. With
  TS 3.0's ids it also takes 36 and 37 (the Contender G2 and Akimbo Skorpions).
* `client_stunt`, `client_powerup` and `Melee_Attack` fire (port addition; upstream declares
  them unavailable since 1.70), from the server's plugin API (section 3), and `ts_set_fuattack`
  is a native again. The repository ships a patched `include/tsfun.inc` for them. The stunt
  types 6 and 7 (wall jump, wall climb) are the port's.
* With the API, the death half of `TSHealth` uses the server's report of each scored kill
  (section 4) instead of predicting it, so a kill the server scores after the victim's
  `TSHealth` 0 is no longer a suicide (#15).

## 7. Testing

* Unit tests (`cargo xtask test`, host and i686): the ABI layouts (i686 only), module info and
  version checks, the weapon table, rank file round trips including the upstream-format
  fixture, message parsing through the port's encoders, the damage/death logic, and every
  native against `FakeArgs` and `FakeGame`; the tsfun forwards and `ts_set_fuattack`
  (`forwards.rs`), and the reported death half in both orders and without a report
  (`state_tests.rs`).
* `cargo xtask e2e [linux] [win]`: see AGENTS.md. On 2026-10-12, against the port at `14999e6`
  (API version 2), both ReHLDS roots (Linux and Windows under Wine) passed all 19 tsx checks and
  all 10 hamsandwich checks; against a build without the API, the 13 tsx checks that apply
  passed, the setters returned 0 with one log line each, and hamsandwich skipped.

## 8. Ham Sandwich

Stock hamsandwich creates an entity of the class, finds its C++ vtable through the private data
(gamedata offsets), and replaces the function's entry with a trampoline that runs the plugins'
forwards around the original. The Rust server's entities have no vtables (an edict's private
data is a zeroed block with a slab handle), so stock reads garbage (Linux) or crashes (Windows).

The port's plugin API (version 2 on, now 3) offers entity hooks instead: one hook function per plugin
(`ham_set_hook`), functions turned on by number (`ham_watch`, `Ham` enum numbers), classes by
id (`class_id`, `entity_class_id`), `ham_supported`, and `ham_execute`. The server calls the hook
before (`post` 0) and after (`post` 1) each watched function with a `HamCall`; arguments are
hamsandwich's signature for that function (`hook_native.cpp`), entities as indices.

**The table** (`hooklist.rs`): all 481 `Ham` functions with stock's hook names and signatures,
generated from `hook_native.cpp` and `ham_const.inc` by `gen_hooklist.py`. `V_REMOVED` entries
fail with HAM_FUNC_NOT_AVAILABLE, the `Deprecated` one with HAM_FUNC_NOT_CONFIGURED.

**Registration** (`natives.rs`): `CHECK_FUNCTION` as stock, with "configured in gamedata" meaning
"the server has the function": `ham_watch(ham, current setting)` answers without changing
anything (cached). The class comes from `class_id(classname)` (or `entity_class_id`); a class
whose `ham_supported` is UNSUPPORTED is the "Failed to retrieve vtable" native error. The forward
is `RegisterSPForwardByName(amx, callback, this, params...)` with stock's parameter list (vectors
as arrays, strings as strings, floats as cells holding the bits: the core's `ExecuteForward` reads
`FP_FLOAT` as a double). The first hook for a function turns it on with `ham_watch`. Handles are
indices + 1; `OnPluginsUnloaded` drops every hook and turns the functions off.

**Dispatch** (`state.rs`), stock's `hook_callbacks.cpp` protocol, on a stack of frames. The
server calls post exactly when it called pre, innermost first, so post belongs to the top frame
(checked by `ham` and `this`; the `HamCall` itself moves between the two calls, so its address
can't identify the call):

* before: push a frame (`this` as parameter 1, then the arguments, `ret`/`origret` zero), run the
  pre forwards one at a time with the module state released. Each sees the parameters as earlier
  forwards set them; `result` is the highest return, `thisresult` the last. Then the (possibly
  changed) arguments go back into the `HamCall`, and at `result >= HAM_SUPERCEDE` the hook answers
  SUPERCEDE (the function doesn't run).
* after (the server calls it after a supercede too, with `orig_ret` zero): record `orig_ret` and
  any pointer arguments the function wrote, run the post forwards, pop the frame, and answer
  OVERRIDE with `ret` when `thisresult >= HAM_OVERRIDE`, else CONTINUE (the function's value), as
  stock's `CHECK_RETURN`. So a superceded call with an ignoring post forward returns 0, as stock.

The data natives (`GetHamReturn*`, `SetHamParam*`, ...) work on the top frame with `Data`'s type
rules and messages (`"Wrong data type (data is of type float)"`, `"Data pointer is NULL!"`,
`"ParamStack is empty!"`). Strings handed to the server live until the call's post (a returned
string until the next one).

**ExecuteHam(B)** (`call_funcs.cpp`): the arguments by reference after `this`, `CHECK_ENTITY` on
`this` and entity arguments, a null TraceResult refused; `ham_execute` with `run_hooks` 0 or 1;
pointer arguments written back; Float/Vector results into the next by-reference parameter, Str
into a buffer and length, others returned. Errors from the server (BUSY: the entity is inside
one of its own functions) are native errors.

**ItemInfo**: handles are `ItemInfo *`, as stock. The module accepts only those it created or
that a call in progress passed, so a bad handle is a native error instead of a crash.

**pdata natives**: `get_pdata_cbase(_safe)` and `set_pdata_cbase` keep stock's checks (the
offset in ints, plus `linuxdiff` on Linux) and read or write the pointer at that offset, as stock.
The Rust server's private data is no C++ object, but since plugin API version 5 it mirrors the TS
members plugins read at TS's offsets (the port's `docs/implementation/plugin-api.md` §4.1: a
player's `m_pActiveItem` at int 639 + 5, the `weapon_tsgun`'s `m_pPlayer` at int 28 + 4, its
`m_iId`, clip, weapon id, reserves and weapon records, and `pev`), so `get_pdata_cbase(id, 639)`
is the player's `weapon_tsgun`. A pointer becomes an entity by finding the edict whose private
data it is (stock `_safe`'s scan): NULL is -1, a pointer to no entity -1 (`_safe`: -2, as stock;
stock's plain native would have followed it). Port changes: the access is bounded by the block's
size from the server (`private_size`), past it a native error where stock read or wrote past the
object, and `set_pdata_cbase` refuses bytes 8..16, the server's link to the entity (a native
error). The server takes no pointer writes back (its mirror rewrites them). Without the API: -1
and a no-op, with one log line. fakemeta's `get_pdata_int` / `set_pdata_int` / `get_pdata_ent`
are stock's and work on the same mirror (writes to the reserves and clips go back into the
game).

**Version 4's hooks** need nothing new in the dispatcher: the server reports the gun's
`Item_*` / `Weapon_*` functions (and `Ham_TS_Weapon_AlternateAttack`) as supported for the
`weapon_tsgun` class, `this` being the player's `weapon_tsgun` (its owner the player), and the
query functions (`ObjectCaps`, `IsPlayer`, `Center`, ...) now fire, so `RegisterHam` works for them
as for any other. One deviation from stock: the port's `Ham_TS_GiveSlowMul` returns its float
(stock declares it `Void_Void`), so `hooks::entry` gives it a `Float` return:
`SetHamReturnFloat` / `GetHamReturnFloat` (the override, 0 until set, as stock) and
`GetOrigHamReturnFloat` (the original's `max(fuser1, 0.01)`, in post) work in its hooks, and
`ExecuteHam` writes the result to an out parameter if the plugin passes one (a stock-compiled call
without one still runs).

**Without API version 5** the module loads, logs one warning and registers its natives;
`IsHamValid` is 0 and `RegisterHam`/`ExecuteHam` fail through `__fatal_ham_error` as stock does
for functions missing from its gamedata.

**Tests**: `src/tests.rs` drives the dispatcher with a fake host that is both AMX Mod X (forwards
are closures calling natives) and the server (a tiny game with health, following the hook
protocol): parameter changes, supercede and the last-result return rule, override in post,
`SetHamParamEntity` vs `Entity2`, nested `ExecuteHamB` from a forward, ExecuteHam without hooks,
type errors, ItemInfo, pdata (with and without the server's mirror: pointers to entities, NULL, a
pointer to nothing, the block's end, the server's link). `e2e/ham_e2e.sma` hooks Spawn,
TakeDamage (pre and post), Killed, Touch and Think on a live server and checks health after
halved and superceded damage, `Ham_Item_PreFrame` on `weapon_tsgun` (a player owner),
`Ham_TS_GiveSlowMul`'s float in a post hook, and the private-data mirror: `get_pdata_cbase` /
`_safe` / fakemeta's `get_pdata_ent` of `m_pActiveItem` (the player's `weapon_tsgun`), its
`m_pPlayer` back, `get_pdata_int` of its `m_iId` (16), a reserve written with `set_pdata_int`
taken by the game and a written `m_iId` overwritten.
