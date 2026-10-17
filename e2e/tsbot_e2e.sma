// tsmod-amxx-rs end-to-end test plugin for tsbot.inc (cargo xtask e2e).
// GPL-3.0-or-later with the AMX Mod X exceptions.
//
// Runs alone on a dedicated server without quota bots: registers the bot backend tsbot_e2e
// (with ts_bot_backend set to it first, so it takes over at once), makes one bot through the
// server's addbot (ts_bot_add) and one directly, drives them every ts_frame with ts_bot_runcmd
// (walk, jump, weapons, fire, death, respawn with a bought loadout), and checks the snapshot,
// world natives and forwards. One line per check in addons/amxmodx/logs/tsbot_e2e.log:
//   CHECK <name> PASS|FAIL|SKIP <detail>
// and a final "DONE pass=N fail=N".
#include <amxmodx>
#include <tsfun>
#include <tsbot>

new const LOG[] = "tsbot_e2e.log";
new const NAME[] = "tsbot_e2e";

#define AK47	13
#define GLOCK	1

new g_pass, g_fail;
new g_activated, g_deactivated;
new g_add_calls, g_add_name[32], g_added;   // the bot ts_bot_add made
new g_direct;                                // the bot ts_bot_create made in ts_frame
new g_step, Float:g_step_at;
new Float:g_o0[3];
new g_try;
new bool:g_airborne, bool:g_landed, bool:g_teleported;
new g_fired, g_fired_bad, g_sound_from_bot, g_wchanged, g_achanged;
new g_killed, g_spawned;
new g_runcmd_ok, g_runcmd_bad, g_runcmd_last_bad;
new g_serial0;
new bool:g_done;
new Float:g_respawn_at[33];
new bool:g_bought;

check(const name[], ok, const detail[] = "")
{
	if (ok) g_pass++; else g_fail++;
	log_to_file(LOG, "CHECK %s %s %s", name, ok ? "PASS" : "FAIL", detail);
}

fmt_int(v)
{
	new s[16];
	num_to_str(v, s, charsmax(s));
	return s;
}

public plugin_init()
{
	register_plugin(NAME, "1.0", "tsmod-amxx-rs");
	log_to_file(LOG, "start: maxplayers=%d", get_maxplayers());
	// Name the backend first: registering then takes over at once (no map reload).
	set_cvar_string("ts_bot_backend", NAME);
	new r = ts_bot_register_backend(NAME);
	check("register", r == TSBOT_OK, fmt_int(r));
	r = ts_bot_register_backend("tsbot_other");
	check("register_one_per_plugin", r == TSBOT_ALREADY, fmt_int(r));
	r = ts_bot_register_backend(NAME);
	check("register_again", r == TSBOT_OK, fmt_int(r));
	r = ts_bot_register_backend("bad name");
	check("register_bad_name", r == TSBOT_ALREADY || r == TSBOT_INVALID, fmt_int(r));
	new dir[128];
	new n = ts_bot_data_dir(dir, charsmax(dir));
	check("data_dir", n > 0 && containi(dir, "tsmod") >= 0 && containi(dir, NAME) >= 0, dir);
	ts_bot_log(TSLOG_INFO, "tsbot_e2e registered");
	set_task(110.0, "t_timeout");
	g_step_at = get_gametime();
}

public ts_bot_activate()
{
	g_activated++;
	log_to_file(LOG, "activate at %.1f", get_gametime());
}

public ts_bot_deactivate()
{
	g_deactivated++;
}

public ts_bot_adopt(id)
{
	log_to_file(LOG, "adopt %d", id);
}

public ts_bot_add(const name[], const model[], Float:skill)
{
	g_add_calls++;
	copy(g_add_name, charsmax(g_add_name), name);
	new id = ts_bot_create(name[0] ? name : "tsbot_add", model, skill);
	log_to_file(LOG, "ts_bot_add(^"%s^", ^"%s^", %.2f) -> %d", name, model, skill, id);
	if (id > 0) g_added = id;
	return id > 0 ? id : 0;
}

public ts_weapon_fired(id, wid, Float:origin[3], Float:dir[3], silenced)
{
	if (id != g_added || g_added == 0) return;
	g_fired++;
	new Float:len = floatsqroot(dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]);
	if (wid != AK47 || len < 0.9 || len > 1.1 || (origin[0] == 0.0 && origin[1] == 0.0 && origin[2] == 0.0)) g_fired_bad++;
}

public ts_sound(ent, Float:origin[3], const sample[], Float:vol, Float:attn, channel, source_player)
{
	if (g_added && source_player == g_added) g_sound_from_bot++;
}

public ts_weapon_changed(id, wid)
{
	if (id == g_added && wid == AK47) g_wchanged++;
}

public ts_ammo_changed(id, wid, clip, ammo)
{
	if (id == g_added) g_achanged++;
}

public ts_player_killed(victim, attacker, inflictor)
{
	if (victim == g_added) g_killed++;
}

public ts_player_spawned(id)
{
	if (id == g_added) g_spawned++;
}

public ts_item_pickup(id, ent, kind, itemid) {}
public ts_bot_touch(bot, other) {}
public ts_damage(victim, attacker, inflictor, Float:damage, bits, Float:health, Float:armor, hitgroup, wid, Float:dir[3], Float:pos[3]) {}

next_step()
{
	g_step++;
	g_step_at = get_gametime();
	g_try = 0;
}

Float:since()
{
	return get_gametime() - g_step_at;
}

alive(id)
{
	return id > 0 && (ts_get_player_flags(id) & TSPF_ALIVE) != 0;
}

origin_of(id, Float:o[3])
{
	new data[TSPS_SIZE];
	ts_get_player_snapshot(id, data);
	for (new k = 0; k < 3; k++) o[k] = Float:data[TSPS_ORIGIN + k];
}

Float:dist(const Float:a[3], const Float:b[3])
{
	return floatsqroot((a[0] - b[0]) * (a[0] - b[0]) + (a[1] - b[1]) * (a[1] - b[1]) + (a[2] - b[2]) * (a[2] - b[2]));
}

// A new bot starts observing, as a joining player does in TS (Spawn puts it in observer mode until
// its respawn): like the native bots' respawn module, send "respawn" while it isn't alive.
keep_alive(id)
{
	if (id <= 0 || !is_user_connected(id) || alive(id)) return;
	new Float:now = get_gametime();
	if (now >= g_respawn_at[id]) {
		ts_bot_command(id, "respawn");
		g_respawn_at[id] = now + 2.0;
	}
}

// One command for a bot; counts the results and records the events.
drive(id, Float:yaw, Float:fwd, Float:side, buttons)
{
	if (id <= 0 || !is_user_connected(id)) return;
	new Float:ang[3];
	ang[1] = yaw;
	new Float:vang[3], events;
	new r = ts_bot_runcmd(id, ang, fwd, side, 0.0, buttons, 0, TSBOT_SETVIEW | TSBOT_AUTOMSEC, 0, vang, events);
	if (r == TSBOT_OK) {
		g_runcmd_ok++;
		if (id == g_added) {
			if (events & TSBOT_EV_LANDED) g_landed = true;
			if (events & TSBOT_EV_TELEPORTED) g_teleported = true;
		}
	} else {
		g_runcmd_bad++;
		g_runcmd_last_bad = r;
	}
}

public ts_frame()
{
	if (g_done) return;
	// Death clears the loadout (TS's Killed: ResetArmament): buy the next spawn's once the bot is
	// seen dead, before keep_alive asks for the respawn.
	if (g_step == 7 && !g_bought && g_added && !alive(g_added)) {
		g_bought = true;
		new r1 = ts_bot_clear_loadout(g_added), r2 = ts_bot_buy(g_added, GLOCK, 0, 0, 1), r3 = ts_bot_buy_item(g_added, TSLOAD_KEVLAR);
		new msg[64];
		formatex(msg, charsmax(msg), "clear %d buy %d item %d", r1, r2, r3);
		check("buy_status", r1 >= 0 && r2 >= 0 && r3 >= 0, msg);
	}
	keep_alive(g_added);
	keep_alive(g_direct);
	// Bots this step doesn't drive stand still (every bot gets one command per frame).
	new bool:drove_added;
	switch (g_step) {
		case 0: {
			// ts_bot_activate comes at a ts_frame after the registration took over.
			if (g_activated) {
				check("activate", g_activated == 1, fmt_int(g_activated));
				server_cmd("addbot");
				next_step();
			} else if (since() > 10.0) {
				check("activate", 0, "no ts_bot_activate within 10 s (is ts_bot_backend tsbot_e2e?)");
				finish();
			}
		}
		case 1: {
			if (g_added && g_add_calls) {
				new msg[64];
				formatex(msg, charsmax(msg), "addbot -> ts_bot_add(%s) -> %d", g_add_name, g_added);
				check("add_forward", is_user_connected(g_added) && is_user_bot(g_added), msg);
				g_direct = ts_bot_create("tsbot_direct", "", 0.5);
				check("create", g_direct > 0 && g_direct != g_added, fmt_int(g_direct));
				new r = ts_bot_create("");
				check("create_empty_name", r == TSBOT_INVALID, fmt_int(r));
				next_step();
			} else if (since() > 10.0) {
				check("add_forward", 0, "addbot didn't reach ts_bot_add / ts_bot_create");
				finish();
			}
		}
		case 2: {
			// The snapshot of the new bot, once it lives.
			if (alive(g_added) && since() > 1.0) {
				new data[TSPS_SIZE];
				new uid = ts_get_player_snapshot(g_added, data);
				new flags = data[TSPS_FLAGS];
				new msg[96];
				formatex(msg, charsmax(msg), "uid=%d flags=%x hp=%.0f team=%d weapon=%d ts=%.2f", uid, flags, Float:data[TSPS_HEALTH],
					data[TSPS_TEAM], data[TSPS_WEAPON], Float:data[TSPS_TIMESCALE]);
				check("snapshot", uid > 0 && uid == get_user_userid(g_added) && (flags & (TSPF_JOINED | TSPF_ALIVE | TSPF_BOT)) == (TSPF_JOINED | TSPF_ALIVE | TSPF_BOT)
					&& Float:data[TSPS_HEALTH] > 0.0 && Float:data[TSPS_TIMESCALE] > 0.0, msg);
				check("field", ts_get_player_field(g_added, TSPS_USERID) == uid && ts_get_player_flags(g_added) == flags, "");
				new bits, st = ts_get_player_stance(g_added, bits);
				new clip, w = ts_get_player_weapon(g_added, clip);
				new Float:armor, Float:hp = ts_get_player_health(g_added, armor);
				formatex(msg, charsmax(msg), "stance=%d bits=%x weapon=%d clip=%d hp=%.0f", st, bits, w, clip, hp);
				check("getters", st >= 0 && w >= 0 && hp == Float:data[TSPS_HEALTH], msg);
				new Float:combat[TSCOMBAT_SIZE];
				check("combat", ts_get_player_combat(g_added, combat) == 1, "");
				check("empty_slot", ts_get_player_snapshot(get_maxplayers(), data) == 0 || is_user_connected(get_maxplayers()), "");
				g_serial0 = ts_get_player_field(g_added, TSPS_INVSERIAL);
				origin_of(g_added, g_o0);
				next_step();
			} else if (since() > 10.0) {
				check("snapshot", 0, "the added bot isn't alive after 10 s of respawn commands");
				finish();
			}
		}
		case 3: {
			// Walk: forward, then back, then sideways, 1.5 s each, until the bot got 48 units.
			new Float:yaw = 90.0 * float(g_try);
			drive(g_added, yaw, 400.0, 0.0, 0);
			drove_added = true;
			if (since() > 1.5) {
				new Float:o[3];
				origin_of(g_added, o);
				new Float:d = dist(o, g_o0);
				if (d > 48.0 || g_try == 3) {
					new msg[96], res[TSRES_SIZE];
					ts_bot_get_result(g_added, res);
					new Float:ro[3];
					for (new k = 0; k < 3; k++) ro[k] = Float:res[TSRES_ORIGIN + k];
					formatex(msg, charsmax(msg), "moved %.0f units (yaw %.0f); result origin %.0f from snapshot", d, yaw, dist(ro, o));
					check("walk", d > 48.0, msg);
					check("runcmd_result", dist(ro, o) < 64.0, msg);
					next_step();
				} else {
					g_try++;
					g_step_at = get_gametime();
					g_o0 = o;
				}
			}
		}
		case 4: {
			// Jump, once standing.
			new flags = ts_get_player_flags(g_added);
			if (g_try == 0) {
				if (flags & TSPF_ON_GROUND) {
					drive(g_added, 0.0, 0.0, 0.0, IN_JUMP);
					drove_added = true;
					g_try = 1;
					g_landed = false;
				}
			} else if (!(flags & TSPF_ON_GROUND)) {
				g_airborne = true;
			}
			if (since() > 2.0) {
				new msg[64];
				formatex(msg, charsmax(msg), "airborne=%d landed=%d", _:g_airborne, _:g_landed);
				check("jump", g_airborne && g_landed, msg);
				ts_giveweapon(g_added, AK47, 3, 0);
				next_step();
			}
		}
		case 5: {
			// The given AK47 shows in the inventory (its serial moved), then select kung fu.
			if (since() > 0.5 && g_try == 0) {
				new w[8], c[8], a[8];
				new n = ts_get_inventory(g_added, w, c, a, sizeof w);
				new bool:has;
				for (new k = 0; k < n && k < sizeof w; k++) if (w[k] == AK47) has = true;
				new msg[64];
				formatex(msg, charsmax(msg), "%d weapons, serial %d -> %d", n, g_serial0, ts_get_player_field(g_added, TSPS_INVSERIAL));
				check("inventory", has && ts_get_player_field(g_added, TSPS_INVSERIAL) != g_serial0, msg);
				new r = ts_bot_select_weapon(g_added, TSBOT_KUNGFU);
				check("select_kungfu_status", r == TSBOT_OK || r == TSBOT_DEFERRED, fmt_int(r));
				g_try = 1;
			} else if (since() > 1.0 && g_try == 1) {
				check("select_kungfu", ts_get_player_weapon(g_added) == TSBOT_KUNGFU, fmt_int(ts_get_player_weapon(g_added)));
				g_wchanged = 0;
				ts_bot_select_weapon(g_added, AK47);
				g_try = 2;
			} else if (since() > 1.5 && g_try == 2) {
				new clip, ammo, w = ts_get_player_weapon(g_added, clip, ammo);
				new msg[64];
				formatex(msg, charsmax(msg), "weapon %d clip %d ammo %d, ts_weapon_changed %d", w, clip, ammo, g_wchanged);
				check("select_weapon", w == AK47, msg);
				check("weapon_changed_forward", g_wchanged > 0, msg);
				g_achanged = 0;
				next_step();
			}
		}
		case 6: {
			// Fire: hold attack (released every other frame for single-shot modes).
			drive(g_added, 0.0, 0.0, 0.0, (g_try++ & 1) ? 0 : IN_ATTACK);
			drove_added = true;
			if (since() > 1.5) {
				new msg[96];
				formatex(msg, charsmax(msg), "shots %d (bad %d), sounds from the bot %d, ammo changes %d", g_fired, g_fired_bad, g_sound_from_bot, g_achanged);
				check("weapon_fired_forward", g_fired > 0 && g_fired_bad == 0, msg);
				check("sound_forward", g_sound_from_bot > 0, msg);
				check("ammo_changed_forward", g_achanged > 0, msg);
				user_kill(g_added);
				g_teleported = false;
				next_step();
			}
		}
		case 7: {
			// Dead: keep_alive sends "respawn"; press attack too.
			if (!alive(g_added)) {
				drive(g_added, 0.0, 0.0, 0.0, (floatround(since() * 10.0) & 1) ? IN_ATTACK : 0);
				drove_added = true;
			} else if (since() > 1.0) {
				drive(g_added, 0.0, 0.0, 0.0, 0);
				drove_added = true;
				new w[8], c[8], a[8];
				new n = ts_get_inventory(g_added, w, c, a, sizeof w);
				new bool:has;
				for (new k = 0; k < n && k < sizeof w; k++) if (w[k] == GLOCK) has = true;
				new msg[128];
				new data[TSPS_SIZE];
				ts_get_player_snapshot(g_added, data);
				formatex(msg, charsmax(msg), "killed %d spawned %d, %d weapons, glock %d, armor %.0f, slots %d, weapon %d", g_killed, g_spawned, n, _:has,
					Float:data[TSPS_ARMOR], data[TSPS_FREESLOTS], data[TSPS_WEAPON]);
				check("killed_forward", g_killed > 0, msg);
				check("spawned_forward", g_spawned > 0, msg);
				check("buy", has && Float:data[TSPS_ARMOR] > 0.0, msg);
				next_step();
			}
			if (since() > 20.0) {
				check("respawn", 0, "the bot didn't respawn within 20 s");
				next_step();
			}
		}
		case 8: {
			check("respawn_teleported", g_teleported, "the first command after the respawn says TSBOT_EV_TELEPORTED");
			world_checks();
			new r = ts_bot_kick(g_direct);
			check("kick_status", r == TSBOT_OK, fmt_int(r));
			next_step();
		}
		case 9: {
			if (since() > 2.0) {
				check("kick", !is_user_connected(g_direct), "");
				new msg[64];
				formatex(msg, charsmax(msg), "%d ok, %d failed (last %d)", g_runcmd_ok, g_runcmd_bad, g_runcmd_last_bad);
				check("runcmd", g_runcmd_ok > 100 && g_runcmd_bad == 0, msg);
				new r = ts_bot_unregister_backend();
				check("unregister", r == TSBOT_OK, fmt_int(r));
				next_step();
			}
		}
		case 10: {
			if (since() > 3.0) {
				check("unregister_kicks", !is_user_connected(g_added), "the active backend's bots go with it");
				check("no_deactivate_on_unregister", g_deactivated == 0, fmt_int(g_deactivated));
				finish();
			}
		}
	}
	if (!drove_added && g_step < 10) drive(g_added, 0.0, 0.0, 0.0, 0);
	if (g_step < 10) drive(g_direct, 0.0, 0.0, 0.0, 0);
}

world_checks()
{
	new fi[TSFI_SIZE];
	ts_get_frameinfo(fi);
	new msg[96];
	formatex(msg, charsmax(msg), "time %.1f frametime %.3f highest %d", Float:fi[TSFI_TIME], Float:fi[TSFI_FRAMETIME], fi[TSFI_HIGHEST_EDICT]);
	check("frameinfo", Float:fi[TSFI_TIME] > 0.0 && fi[TSFI_HIGHEST_EDICT] > get_maxplayers(), msg);
	new gs[TSGS_SIZE];
	ts_get_gamestate(gs);
	formatex(msg, charsmax(msg), "mode %d maxclients %d time %.1f", gs[TSGS_MODE], gs[TSGS_MAXCLIENTS], Float:gs[TSGS_TIME]);
	check("gamestate", gs[TSGS_MAXCLIENTS] == get_maxplayers() && gs[TSGS_MODE] >= 0 && gs[TSGS_MODE] <= 3, msg);
	new items[TSITM_SIZE * 64];
	new n = ts_get_items(items, 64);
	if (n > 0) {
		new kind = items[TSITM_KIND];
		formatex(msg, charsmax(msg), "%d items, first: ent %d kind %d", n, items[TSITM_ENT], kind);
		check("items", kind == TSENT_GROUND_WEAPON || kind == TSENT_POWERUP || kind == TSENT_GUN, msg);
	} else {
		check("items", n == 0, fmt_int(n));
	}
	new ei[TSEI_SIZE];
	new r = ts_get_entity_info(g_added, ei);
	formatex(msg, charsmax(msg), "status %d kind %d", r, ei[TSEI_KIND]);
	check("entity_info", r == TSBOT_OK && ei[TSEI_KIND] == TSENT_PLAYER, msg);
	new val[32];
	r = ts_entity_key(g_added, "master", val, charsmax(val));
	check("entity_key_absent", r == TSBOT_UNSUPPORTED, fmt_int(r));
	check("master_empty", ts_master_triggered("") == 1, "");
	new objs[TSOBJ_SIZE * 8];
	r = ts_get_objectives(0, objs, 8);
	check("objectives", r >= 0, fmt_int(r));
	r = ts_bot_command(g_added, "");
	check("command_empty", r == TSBOT_INVALID, fmt_int(r));
}

finish()
{
	if (g_done) return;
	g_done = true;
	log_to_file(LOG, "DONE pass=%d fail=%d", g_pass, g_fail);
}

public t_timeout()
{
	if (!g_done) {
		new msg[32];
		formatex(msg, charsmax(msg), "stuck at step %d", g_step);
		check("timeout", 0, msg);
		finish();
	}
}
