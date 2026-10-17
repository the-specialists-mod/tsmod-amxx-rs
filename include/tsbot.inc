// vim: set ts=4 sw=4 tw=99 noet:
//
// tsmod-amxx-rs: TS bot natives and forwards for the Rust TS 3.0 server (the-specialistsmod-rs).
// GPL-3.0-or-later with the AMX Mod X exceptions.
//
// A port addition with no upstream counterpart: the tsx module's natives that let a Pawn plugin
// be the server's bot backend (ts_bot_backend <name>) and read what a bot needs, through the
// server's plugin API (version 7, server v3.0.8 or later). Library: tsx (these natives live in
// the tsx module; #pragma reqlib tsx). Full documentation: docs/tsbot.md.
//
// How a backend works:
//  * plugin_init: ts_bot_register_backend("name") (every map; the first registration attaches a
//    server plugin identity "name", later maps bind it again). If ts_bot_backend names it, the
//    backend takes over at once (set the cvar before registering to avoid a map reload).
//  * ts_bot_activate(): the backend is the active one (delivered at a ts_frame, also after a map
//    change). ts_bot_deactivate(): it no longer is; the server kicks its bots.
//  * ts_bot_add(name, model, skill): the server's quota or addbot wants a bot: make it with
//    ts_bot_create and return its index (0 for none). ts_bot_adopt(id): a bot of this backend
//    the engine kept across a level change; pick it up again.
//  * ts_frame(): every server frame. Read the world (ts_get_player_snapshot, ts_get_items,
//    ts_get_frameinfo, ...), think, and call ts_bot_runcmd once per living or dead bot.
//  * A new bot starts observing, as a joining TS player does, and a dead one waits: send
//    ts_bot_command(id, "respawn") every few seconds while it isn't TSPF_ALIVE.
//
// Conventions:
//  * Weapon ids are the server's: TS 3.0's 1..37 and 0 for kung fu (TSBOT_KUNGFU). tsx's own
//    natives (ts_getuserwpn, ...) number kung fu 38 (36 with tsx_legacy_weaponids 1); these don't.
//  * Snapshots are built once per frame, before ts_frame: a change a native makes shows next frame.
//  * Floats in data arrays are Float: cells (use Float:data[i] or the Float-tagged getters).
//  * Natives return TSBOT_OK (0) or TSBOT_DEFERRED (1) on success and a negative TSBOT_* status
//    on failure (no native error), unless documented otherwise.

#if defined _tsbot_included
	#endinput
#endif
#define _tsbot_included

#pragma reqlib tsx
#if !defined AMXMODX_NOAUTOLOAD
	#pragma loadlib tsx
#endif

/* ---- Statuses ------------------------------------------------------------------------------ */

#define TSBOT_OK				0		// done
#define TSBOT_DEFERRED			1		// the player was inside its own callback: applies when that returns
#define TSBOT_NOT_PLAYER		-1		// not a connected player (or not a game entity)
#define TSBOT_INVALID			-2		// an argument is out of range; nothing changed
#define TSBOT_UNSUPPORTED		-3		// TS 3.0 has no such state, or the entity has no such key
#define TSBOT_BUSY				-4		// inside one of the bot's own callbacks: call ts_bot_runcmd from ts_frame
#define TSBOT_FULL				-5		// ts_bot_create: no free client slot
#define TSBOT_EXISTS			-6		// the name is taken (a server plugin, another AMXX plugin's backend)
#define TSBOT_NO_API			-100	// the server has no plugin API version 7
#define TSBOT_NO_BACKEND		-101	// the calling plugin registered no backend
#define TSBOT_ALREADY			-102	// the calling plugin registered a backend under another name
#define TSBOT_NO_SLOT			-103	// all 4 backend slots are taken

/* ---- ts_bot_runcmd ------------------------------------------------------------------------- */

// flags
#define TSBOT_SETVIEW			0x1		// turn as the native bots do (angles -> v_angle and angles; clears fixangle)
#define TSBOT_AUTOMSEC			0x2		// the server picks msec from the frame time
// events
#define TSBOT_EV_TELEPORTED		0x1		// moved further than it could walk: a teleport or respawn; drop the path
#define TSBOT_EV_FIXANGLE		0x2		// the game set the view: adopt out_vangle
#define TSBOT_EV_LANDED			0x4		// in the air before, on the ground after

// Buttons: hlsdk_const.inc's IN_* (IN_ATTACK, IN_JUMP, IN_DUCK, IN_USE, IN_ATTACK2, IN_RELOAD,
// IN_ALT1, ...).
#include <hlsdk_const>
// TS's stunt / dive key is IN_ALT1 (+alt1); IN_ATTACK2 is the secondary attack (kung-fu kick,
// fire mode on some guns).

/* ---- Player snapshot (ts_get_player_snapshot, ts_get_player_field) -------------------------- */

#define TSPS_USERID				0		// the engine's user id; 0 for an empty slot
#define TSPS_FLAGS				1		// TSPF_* bits
#define TSPS_TEAM				2		// rules team, 1.. in teamplay, else 0
#define TSPS_ORIGIN				3		// Float:[3]
#define TSPS_VELOCITY			6		// Float:[3]
#define TSPS_VANGLE				9		// Float:[3] view angles
#define TSPS_VIEWOFS			12		// Float:[3] eyes above the origin
#define TSPS_ABSMIN				15		// Float:[3]
#define TSPS_ABSMAX				18		// Float:[3]
#define TSPS_HEALTH				21		// Float:
#define TSPS_ARMOR				22		// Float:
#define TSPS_BUTTONS			23		// IN_* of the last command
#define TSPS_WEAPON				24		// weapon in hand (0 kung fu)
#define TSPS_CLIP				25		// its clip (the kung-fu bar with kung fu)
#define TSPS_AMMO				26		// its reserve
#define TSPS_FIREMODE			27
#define TSPS_ATTACHMENTS		28		// TSA_* bits
#define TSPS_STANCE				29		// TSState: 0 stand, 1 crouch, 2 prone or diving
#define TSPS_STUNTBITS			30		// TSSB_* bits (pev->iuser4)
#define TSPS_ACTIVE_PWUP		31		// TSPWUP_* running now
#define TSPS_STORED_PWUP		32		// TSPWUP_* held, not used yet
#define TSPS_PWUP_UNTIL			33		// Float: game time the running power-up ends
#define TSPS_TIMESCALE			34		// Float: 1.0 normal, below slowed
#define TSPS_FRAGS				35
#define TSPS_DEATHS				36
#define TSPS_FREESLOTS			37		// free inventory slots
#define TSPS_INVSERIAL			38		// changes whenever the inventory does: re-read ts_get_inventory then
#define TSPS_COMBAT				39		// TSCOMBAT_* follow, from here
#define TSPS_SIZE				49

// Combat timers (TSPS_COMBAT + n, or ts_get_player_combat): Float: game times they run out
// (0.0 not running), except the kung-fu power. TS counts most in the player's own time scale.
#define TSCOMBAT_MELEE_POWER	0		// kung-fu power 0..100
#define TSCOMBAT_NEXT_ATTACK	1		// no attack before
#define TSCOMBAT_NEXT_STUNT		2		// no stunt before
#define TSCOMBAT_RECOVER_UNTIL	3		// getting up, half speed
#define TSCOMBAT_PRONE_LOCK		4		// can't get up before
#define TSCOMBAT_DODGE_UNTIL	5		// slow-motion dodge: bullets do x0.7
#define TSCOMBAT_BLOCK_KUNGFU	6		// kung-fu blows blocked until
#define TSCOMBAT_BLOCK_KNIFE	7
#define TSCOMBAT_BLOCK_KATANA	8
#define TSCOMBAT_MELEE_WINDOW	9		// a delayed melee hit lands until
#define TSCOMBAT_SIZE			10

// Player flags (TSPS_FLAGS, ts_get_player_flags)
#define TSPF_JOINED				0x1		// past the team/MOTD menus, not spectating
#define TSPF_ALIVE				0x2
#define TSPF_BOT				0x4		// a bot of any backend
#define TSPF_ON_GROUND			0x8
#define TSPF_DUCKING			0x10
#define TSPF_ON_LADDER			0x20
#define TSPF_IN_WATER			0x40	// waist deep or more
#define TSPF_HAS_FLAG			0x80	// carries the capture-the-briefcase case
#define TSPF_SPECTATOR			0x100
#define TSPF_PRONE				0x200	// lying after a dive or slide
#define TSPF_STUNTING			0x400	// diving, flipping, rolling, on a wall, ducked for a stunt
#define TSPF_KUNG_FU			0x800	// permanent kung fu picked up this life
#define TSPF_SUPERJUMP			0x1000	// permanent superjump picked up this life

// Stunt bits (TSPS_STUNTBITS)
#define TSSB_RECOVER			0x1
#define TSSB_DEAD_LOCK			0x2
#define TSSB_GO_DUCK			0x4
#define TSSB_STAND_UP			0x8
#define TSSB_DIVING				0x10
#define TSSB_PRONE				0x20
#define TSSB_STEALTH_SHOES		0x200
#define TSSB_FLIP_HIGH			0x400
#define TSSB_FLIP_LOW			0x800
#define TSSB_WALLJUMPING		0x10000
#define TSSB_WALLCLIMBING		0x20000
#define TSSB_ROLLING			0x40000
#define TSSB_SUPERJUMP_GLIDE	0x100000
#define TSSB_WALLDIVE			0x800000
#define TSSB_STUNTING			0x870C14	// what TSPF_STUNTING tests

#define TSBOT_KUNGFU			0		// the server's weapon id of kung fu

/* ---- Lists ---------------------------------------------------------------------------------- */

// ts_get_inventory_ex: TSINV_SIZE cells per weapon
#define TSINV_WEAPON			0
#define TSINV_CLIP				1		// a stack's count for grenades and blades
#define TSINV_AMMO				2		// reserve of its ammo type (shared by every weapon of the type)
#define TSINV_FIREMODE			3
#define TSINV_ATTACHMENTS		4		// owned TSA_* bits
#define TSINV_SIZE				5

// ts_get_items: TSITM_SIZE cells per item (ground weapons, power-ups, dropped guns; taken ones too)
#define TSITM_ENT				0		// entity index
#define TSITM_SERIAL			1		// the engine's serial of the slot (another serial: another entity)
#define TSITM_KIND				2		// TSENT_GROUND_WEAPON, TSENT_POWERUP or TSENT_GUN
#define TSITM_WEAPON			3		// weapon id (ground weapon, gun)
#define TSITM_POWERUP			4		// TSPWUP_* bits (power-up)
#define TSITM_ORIGIN			5		// Float:[3]
#define TSITM_AVAILABLE			8		// can be picked up now
#define TSITM_RESPAWN_AT		9		// Float: game time it shows again; 0.0 available or next round
#define TSITM_SIZE				10

// ts_get_objectives: TSOBJ_SIZE cells per objective (names: ts_get_objective_name)
#define TSOBJ_ID				0		// the Objective message's list index
#define TSOBJ_ENT				1		// its entity
#define TSOBJ_ORIGIN			2		// Float:[3] the entity's centre
#define TSOBJ_ACTIVE			5		// can be completed now
#define TSOBJ_SIZE				6
#define TSOBJ_NAME_LEN			64

// ts_get_gamestate
#define TSGS_MODE				0		// TSMODE_*
#define TSGS_TEAMPLAY			1
#define TSGS_LMS				2		// last man standing
#define TSGS_THEONE				3		// The One mode
#define TSGS_CTB				4		// the map has a capture-the-briefcase case
#define TSGS_TEAMS				5		// teams in play (teamplay)
#define TSGS_ROUND				6		// TSROUND_*
#define TSGS_ROUND_ENDS			7		// Float: when the round's time runs out
#define TSGS_ROUND_RESTART		8		// Float: when the next round starts (TSROUND_OVER)
#define TSGS_GAMEOVER			9		// intermission
#define TSGS_TIME				10		// Float: gpGlobals->time
#define TSGS_MAXCLIENTS			11
#define TSGS_SCORES				12		// [4] team scores by team - 1
#define TSGS_SIZE				16

// ts_get_frameinfo
#define TSFI_TIME				0		// Float: time at StartFrame
#define TSFI_FRAMETIME			1		// Float:
#define TSFI_HIGHEST_EDICT		2		// scan entities up to it
#define TSFI_TEAMPLAY			3
#define TSFI_CTB				4
#define TSFI_MODE				5
#define TSFI_ROUND				6
#define TSFI_SCORES				7		// [4]
#define TSFI_SIZE				11

// ts_get_entity_info
#define TSEI_KIND				0		// TSENT_*
#define TSEI_WEAPON				1		// ground weapon / gun id (a held weapon_tsgun: the holder's in hand)
#define TSEI_POWERUP			2		// TSPWUP_* bits
#define TSEI_OWNER				3		// responsible player (thrower, holder, carrier), 0 none
#define TSEI_TEAM				4		// 0 none or any
#define TSEI_AVAILABLE			5		// can be picked up or used now
#define TSEI_SIZE				6

// ts_bot_get_result
#define TSRES_VANGLE			0		// Float:[3] the bot's view now
#define TSRES_ORIGIN			3		// Float:[3]
#define TSRES_VELOCITY			6		// Float:[3]
#define TSRES_EVENTS			9		// TSBOT_EV_*
#define TSRES_MSEC				10		// the msec the command ran with
#define TSRES_SIZE				11

// Entity kinds (TSITM_KIND, TSEI_KIND, ts_item_pickup's kind)
#define TSENT_OTHER				0
#define TSENT_PLAYER			1
#define TSENT_GROUND_WEAPON		2		// ts_groundweapon
#define TSENT_POWERUP			3		// ts_powerup
#define TSENT_PROJECTILE		4		// thrown grenade, knife or gun
#define TSENT_GUN				5		// a held weapon_tsgun or a dropped gun
#define TSENT_CTB_FLAG			6		// the briefcase lying in the map
#define TSENT_CTB_BASE			7		// a capture point
#define TSENT_OBJECTIVE			8		// ts_wingiver, ts_scoregiver

// Game modes (TSGS_MODE, TSFI_MODE) and round states
#define TSMODE_DEATHMATCH		0
#define TSMODE_TEAMDM			1
#define TSMODE_ROUND_TEAMDM		2		// unreachable in TS 3.0
#define TSMODE_OBJECTIVE		3
#define TSROUND_NONE			0		// no rounds
#define TSROUND_RUNNING			1
#define TSROUND_OVER			2

// Loadout items (ts_bot_buy_item)
#define TSLOAD_KEVLAR			0x10
#define TSLOAD_STEALTHSHOES		0x20

// ts_bot_log levels
#define TSLOG_INFO				0
#define TSLOG_WARNING			1
#define TSLOG_ERROR				2
#define TSLOG_DEBUG				3		// only with developer on

/* ---- World natives ------------------------------------------------------------------------- */

// The frame's snapshot of player id into data (len cells, TSPS_*; all zeroes for an empty slot).
// Returns the user id, 0 for an empty slot or without the API.
native ts_get_player_snapshot(id, data[], len = TSPS_SIZE);

// One TSPS_* cell (Float: fields: tag the call, Float:ts_get_player_field(id, TSPS_HEALTH)).
// 0 for an empty slot; a field outside 0..TSPS_SIZE-1 is a native error.
native any:ts_get_player_field(id, field);

// TSPF_* bits; 0 for an empty slot.
native ts_get_player_flags(id);

// The rules team (1.. in teamplay), 0 for none.
native ts_get_player_team(id);

// Health, and armour by reference.
native Float:ts_get_player_health(id, &Float:armor = 0.0);

// TSState (0 stand, 1 crouch, 2 prone or diving) and the TSSB_* bits; -1 for an empty slot.
native ts_get_player_stance(id, &stunt_bits = 0);

// The weapon in hand (0 kung fu, whose clip is the kung-fu bar) and its clip, reserve, fire mode
// and TSA_* bits; -1 for an empty slot.
native ts_get_player_weapon(id, &clip = 0, &ammo = 0, &fire_mode = 0, &attachments = 0);

// The running power-up (TSPWUP_*), the held one and when the running one ends.
native ts_get_player_powerup(id, &stored = 0, &Float:until = 0.0);

// Time scale: 1.0 normal, below 1.0 slowed; 0.0 for an empty slot.
native Float:ts_get_player_timescale(id);

// The TSCOMBAT_* timers. 1, or 0 (zeroes) for an empty slot.
native ts_get_player_combat(id, Float:combat[TSCOMBAT_SIZE]);

// The weapons the player owns (kung fu isn't one): up to max in each array. Returns how many
// it owns (may be more than max), or a status.
native ts_get_inventory(id, weapons[], clips[], ammo[], max);

// The same with every field: max entries of TSINV_SIZE cells. Returns the count or a status.
native ts_get_inventory_ex(id, data[], max);

// The map's items: max entries of TSITM_SIZE cells. Returns how many there are, or a status.
native ts_get_items(data[], max);

// The HUD's objectives: max entries of TSOBJ_SIZE cells; id picks the team whose names
// ts_get_objective_name reads (0 none). Returns how many there are, or a status.
native ts_get_objectives(id, data[], max);

// The text id's team reads for objective index (0-based, as ts_get_objectives lists them).
// Returns its length, or a status (TSBOT_INVALID: no such index).
native ts_get_objective_name(id, index, name[], len);

// The rules and round (TSGS_*). 1, or a status.
native ts_get_gamestate(data[TSGS_SIZE]);

// The frame (TSFI_*). 1, or a status.
native ts_get_frameinfo(data[TSFI_SIZE]);

// A map key the server keeps outside entvars (a door's or button's "master"). Returns its
// length, or a status (TSBOT_UNSUPPORTED: the entity has no such key; read entvars fields with
// fakemeta/engine; TSBOT_NOT_PLAYER: not a game entity).
native ts_entity_key(ent, const key[], out[], len);

// What a game entity is (TSEI_*). TSBOT_OK, or a status.
native ts_get_entity_info(ent, data[TSEI_SIZE]);

// 1 if the master entities named master (multisource, game_team_master) let activator through
// (an empty name is 1), else 0; or a status.
native ts_master_triggered(const master[], activator = 0);

// The player responsible for a thrown knife, grenade or placed charge: its index, 0 for none,
// or a status.
native ts_projectile_owner(ent);

/* ---- Bot natives --------------------------------------------------------------------------- */

// Makes the calling plugin the bot backend name (1-32 letters, digits, _ or -; also its data
// directory <gamedir>/tsmod/<name>/). One per plugin. Call it in plugin_init on every map.
// TSBOT_OK (also when it is already this plugin's), TSBOT_INVALID, TSBOT_EXISTS, TSBOT_ALREADY,
// TSBOT_NO_SLOT or TSBOT_NO_API.
native ts_bot_register_backend(const name[]);

// Gives the plugin's backend up for good (its bots are kicked if it was active; native takes
// over). No ts_bot_deactivate follows. TSBOT_OK or a status.
native ts_bot_unregister_backend();

// The backend's data directory, with a trailing slash (created on first use). Returns its
// length, or a status (out is then empty).
native ts_bot_data_dir(out[], len);

// A server console line "[<backend>] msg" (TSLOG_*).
native ts_bot_log(level, const msg[]);

// Makes a bot of this plugin's backend, joined (in teamplay on the team model names; the name
// and model are passed as the plugin's bytes) and observing until ts_bot_command(id, "respawn").
// The backend must be the active one. Returns its index, or TSBOT_FULL, TSBOT_INVALID,
// TSBOT_NO_BACKEND.
native ts_bot_create(const name[], const model[] = "", Float:skill = 0.0);

// Kicks a bot (queued). TSBOT_OK, TSBOT_NOT_PLAYER, or TSBOT_INVALID for a human.
native ts_bot_kick(id);

// Runs one usercmd for bot id now (its think, movement, post-think): once per ts_frame per bot.
// angles are the view (pitch, yaw, roll); fwd/side/up the movement (TS runs at up to ~400);
// buttons IN_*; flags TSBOT_SETVIEW|TSBOT_AUTOMSEC (the default) or msec 1..255.
// TSBOT_OK: out_vangle has the bot's view now (the game's after TSBOT_EV_FIXANGLE) and events
// the TSBOT_EV_* bits (ts_bot_get_result has origin and velocity). Else TSBOT_BUSY (inside the
// bot's own callback), TSBOT_INVALID (a human, a bad msec, impulse or angle), TSBOT_NOT_PLAYER.
native ts_bot_runcmd(id, const Float:angles[3], Float:fwd, Float:side, Float:up, buttons, impulse = 0, flags = TSBOT_SETVIEW | TSBOT_AUTOMSEC, msec = 0, Float:out_vangle[3] = {0.0, 0.0, 0.0}, &events = 0);

// What the bot's last successful ts_bot_runcmd did (TSRES_*). 1, or 0 for none since it joined.
native ts_bot_get_result(id, data[TSRES_SIZE]);

// Runs a client command as if the player typed it ("use", "usepowerup", "respawn", "say hi").
native ts_bot_command(id, const line[]);

// Switches to weapon wid (0 kung fu), as weapon_<id> does.
native ts_bot_select_weapon(id, wid);

// Adds a weapon to the loadout the bot buys at its next spawn (TSA_* attachments, akimbo for a
// second copy, ammo purchases). Death clears the loadout (as TS's Killed does): buy while dead.
native ts_bot_buy(id, weapon, attachments = 0, akimbo = 0, ammo = 0);

// Adds TSLOAD_KEVLAR or TSLOAD_STEALTHSHOES to the loadout.
native ts_bot_buy_item(id, item);

// Empties the loadout.
native ts_bot_clear_loadout(id);

// Drops weapon wid, or the one in hand for -1.
native ts_drop_weapon(id, wid = -1);

/* ---- Backend forwards (only to the plugin that registered the backend) --------------------- */

// The server wants a bot (quota, addbot, addcustombot): name and model may be empty (pick
// them; in teamplay the model is the team), skill <= 0.0 means pick one. Make it with
// ts_bot_create and return its index, or 0 for none.
forward ts_bot_add(const name[], const model[], Float:skill);

// The backend became the active one (delivered at a ts_frame). Also on each later map.
forward ts_bot_activate();

// It no longer is (another backend was chosen); its bots are being kicked.
forward ts_bot_deactivate();

// A bot of this backend the engine kept across a level change: pick it up again.
forward ts_bot_adopt(id);

/* ---- Game forwards (every plugin) ----------------------------------------------------------- */

// Every server frame (StartFrame), after the snapshots are built: run the bots here.
forward ts_frame();

// A player fired a shot of weapon wid (each round of a burst; one call for a shotgun):
// origin the gun position, dir the unit aim before spread.
forward ts_weapon_fired(id, wid, Float:origin[3], Float:dir[3], silenced);

// A player picked up ent: kind TSENT_GROUND_WEAPON (itemid the weapon), TSENT_POWERUP (itemid
// its TSPWUP_* bits) or TSENT_GUN (a dropped gun). A temporary item may be removed already.
forward ts_item_pickup(id, ent, kind, itemid);

// A bot (any backend) touched other or was touched by it.
forward ts_bot_touch(bot, other);

// The game played a sound at ent (0 the world): volume 0..1, attn 0..4, channel CHAN_*,
// source_player the player responsible (0 none).
forward ts_sound(ent, Float:origin[3], const sample[], Float:vol, Float:attn, channel, source_player);

// A player took damage: who and what did it (indices, 0 world, -1 none), the amount and DMG_*
// bits, what is left, the hitgroup (HIT_*, 0 generic), the weapon (24 grenade, -1 other), and the
// trace's direction and hit position (zero without one).
forward ts_damage(victim, attacker, inflictor, Float:damage, bits, Float:health, Float:armor, hitgroup, wid, Float:dir[3], Float:pos[3]);

// A player died.
forward ts_player_killed(victim, attacker, inflictor);

// A player spawned (first spawn or respawn).
forward ts_player_spawned(id);

// A round started (last man standing) / ended.
forward ts_round_start();
forward ts_round_end();

// The weapon in hand changed (0 kung fu), at most once a frame.
forward ts_weapon_changed(id, wid);

// The weapon in hand's clip or reserve changed, at most once a frame.
forward ts_ammo_changed(id, wid, clip, ammo);
