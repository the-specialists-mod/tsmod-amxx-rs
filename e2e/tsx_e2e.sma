// tsmod-amxx-rs end-to-end test plugin (cargo xtask e2e).
// GPL-3.0-or-later with the AMX Mod X exceptions.
//
// Runs on a dedicated server with bots: probes tsx/tsfun natives on living players and records
// the forwards, then writes one line per check to addons/amxmodx/logs/tsx_e2e.log:
//   CHECK <name> PASS|FAIL|SKIP <detail>
// and a final "DONE pass=N fail=N" that xtask waits for.
#include <amxmodx>
#include <engine>
#include <tsx>
#include <tsfun>
#include <tsxaddon> // the repo's patched copy: ts_setuserammo is a native
#include <fun>
#include <hamsandwich>

// Registered by tsx (NBase.cpp) but not declared in the stock includes.
native ts_getuseritems(index);

new const LOG[] = "tsx_e2e.log";

new g_pass, g_fail;
new g_damage, g_death, g_death_named;
new g_last_killer;
new bool:g_api;
new g_give_id, g_give_space;
new g_set_id;
// AMX Mod X 1.8.2's Linux core calls the game library on a stack the Rust server can't take
// (esp = 8 mod 16, then the server's SSE spills fault): its engclient_cmd crashes the server, so
// the checks that switch weapons are skipped there (tsmod-amxx-rs#2, the-specialistsmod-rs#199).
new bool:g_no_engclient;
// The thrown knife (#5).
new g_kn_thrower, g_kn_victim, g_kn_damage, g_kn_death, g_kn_tries;
// tsfun.inc's forwards, fired from the server's plugin API v5 (tsmod-amxx-rs#8).
new g_stunts, g_stunt_bad, g_pwups, g_pwup_id, g_pwup_last, g_melees, g_melee_bad, g_fuattack_ok;
new g_pwup_target, g_pwup_health, g_pwup_tries;

check(const name[], ok, const detail[] = "")
{
	if (ok) g_pass++; else g_fail++;
	log_to_file(LOG, "CHECK %s %s %s", name, ok ? "PASS" : "FAIL", detail);
}

skip(const name[], const detail[])
{
	log_to_file(LOG, "CHECK %s SKIP %s", name, detail);
}

public plugin_init()
{
	register_plugin("tsx_e2e", "1.0", "tsmod-amxx-rs");
	log_to_file(LOG, "start: maxplayers=%d", get_maxplayers());
	g_no_engclient = AMXX_VERSION_NUM < 183 && is_linux_server();
	// xtask's stats round trip: `tsx_e2e_dump <tag>` over rcon, or localinfo tsx_e2e_dump set
	// before a changelevel (the rank right after ServerDeactivate saved it).
	register_srvcmd("tsx_e2e_dump", "cmd_dump");
	new tag[16];
	get_localinfo("tsx_e2e_dump", tag, charsmax(tag));
	if (tag[0]) {
		set_localinfo("tsx_e2e_dump", "");
		dump_rank(tag);
	}
	set_task(20.0, "t_static");
	set_task(21.0, "t_getters");
	set_task(25.0, "t_give");
	set_task(26.5, "t_give_check");
	set_task(30.0, "t_pwup");
	set_task(32.0, "t_knife");
	set_task(49.0, "t_knife_check");
	set_task(50.0, "t_stats");
	set_task(51.0, "t_finish");
}

public cmd_dump()
{
	new tag[16];
	read_argv(1, tag, charsmax(tag));
	dump_rank(tag);
	return PLUGIN_HANDLED;
}

// The whole rank through get_stats, best first: "RANK <tag> <pos> <stats 0..6> <body 1..7> | <name>".
dump_rank(const tag[])
{
	new stats[8], body[8], name[32], i, n, next;
	do {
		name[0] = 0;
		next = get_stats(i, stats, body, name, charsmax(name));
		if (!name[0]) break;
		log_to_file(LOG, "RANK %s %d %d %d %d %d %d %d %d %d %d %d %d %d %d %d | %s", tag, i, stats[0], stats[1], stats[2], stats[3],
			stats[4], stats[5], stats[6], body[1], body[2], body[3], body[4], body[5], body[6], body[7], name);
		n++;
		i = next;
	} while (next);
	log_to_file(LOG, "DUMPED %s n=%d statsnum=%d", tag, n, get_statsnum());
}

public client_damage(attacker, victim, damage, wpnindex, hitplace, TA)
{
	g_damage++;
	if (wpnindex == TSW_TKNIFE) {
		log_to_file(LOG, "client_damage (thrown knife): att=%d vic=%d dmg=%d hit=%d", attacker, victim, damage, hitplace);
		if (attacker == g_kn_thrower && victim == g_kn_victim) g_kn_damage++;
	}
	if (g_damage <= 3)
		log_to_file(LOG, "client_damage: att=%d vic=%d dmg=%d wpn=%d hit=%d ta=%d", attacker, victim, damage, wpnindex, hitplace, TA);
}

public client_death(killer, victim, wpnindex, hitplace, TK)
{
	g_death++;
	new name[32];
	xmod_get_wpnname(wpnindex, name, charsmax(name));
	if (name[0]) g_death_named++;
	g_last_killer = killer;
	if (wpnindex == TSW_TKNIFE) {
		log_to_file(LOG, "client_death (thrown knife): killer=%d victim=%d (%s) hit=%d", killer, victim, name, hitplace);
		if (killer == g_kn_thrower && victim == g_kn_victim) g_kn_death++;
	}
	if (g_death <= 5)
		log_to_file(LOG, "client_death: killer=%d victim=%d wpn=%d (%s) hit=%d tk=%d lastfrag=%d flags=%d",
			killer, victim, wpnindex, name, hitplace, TK, ts_getuserlastfrag(killer), ts_getuserkillflags(killer));
}

public client_stunt(id, stunttype)
{
	g_stunts++;
	// STUNT_ROLL..STUNT_FLIP, and the port's 6 (wall jump) and 7 (wall climb).
	if (id < 1 || id > get_maxplayers() || stunttype < 2 || stunttype > 7) g_stunt_bad++;
	if (g_stunts <= 3) log_to_file(LOG, "client_stunt: id=%d type=%d", id, stunttype);
}

public client_powerup(id, powerup)
{
	g_pwups++;
	g_pwup_id = id;
	g_pwup_last = powerup;
	if (powerup == TSPWUP_HEALTH && !g_pwup_health) g_pwup_health = id;
	if (g_pwups <= 3) log_to_file(LOG, "client_powerup: id=%d powerup=%d", id, powerup);
}

public Melee_Attack(id, Float:time, Float:damage)
{
	g_melees++;
	if (id < 1 || id > get_maxplayers() || time <= 0.0 || damage <= 0.0) g_melee_bad++;
	// Changing nothing, through the native: 1 while the forward runs for that player.
	if (ts_set_fuattack(id, time, damage) == 1) g_fuattack_ok++;
	if (g_melees <= 3) log_to_file(LOG, "Melee_Attack: id=%d time=%.2f damage=%.1f", id, time, damage);
	return PLUGIN_CONTINUE;
}

Float:get_z(id)
{
	new Float:o[3];
	entity_get_vector(id, EV_VEC_origin, o);
	return o[2];
}

// TS hands over a gun only with the free slot weight for it (PickUpWeapon), so give to the
// living player with the most room.
roomiest_alive()
{
	new best, room = -1;
	for (new i = 1; i <= get_maxplayers(); i++)
		if (is_user_alive(i) && ts_getuserspace(i) > room) { best = i; room = ts_getuserspace(i); }
	return best;
}

first_alive()
{
	for (new i = 1; i <= get_maxplayers(); i++)
		if (is_user_alive(i)) return i;
	return 0;
}

// ---- The thrown knife (#5) ----
// Bots never throw knives, so the plugin makes one: the bots stand still (ts_bot_stop 1), a
// thrower gets the combat knife and faces a victim moved 96 units in front of it with 1 health,
// and ham's Weapon_SecondaryAttack on its weapon_tsgun throws (CTSGun::SecondaryAttack's knife
// branch). client_damage and client_death must name the thrower and TSW_TKNIFE, found through the
// API's projectile_owner.

public t_knife()
{
	if (g_kn_death || g_no_engclient) return;
	if (++g_kn_tries > 6) return;
	set_cvar_num("ts_bot_stop", 1);
	g_kn_thrower = 0; g_kn_victim = 0;
	for (new i = 1; i <= get_maxplayers(); i++) {
		if (!is_user_alive(i)) continue;
		if (!g_kn_thrower) g_kn_thrower = i;
		else if (!g_kn_victim) g_kn_victim = i;
	}
	if (!g_kn_victim) { set_task(2.0, "t_knife"); return; }
	ts_giveweapon(g_kn_thrower, TSW_CKNIFE, 0, 0);
	engclient_cmd(g_kn_thrower, "weapon_25");
	set_task(0.5, "t_knife_throw");
}

public t_knife_throw()
{
	new t = g_kn_thrower, v = g_kn_victim;
	if (!is_user_alive(t) || !is_user_alive(v)) { set_task(1.0, "t_knife"); return; }
	new clip, ammo, mode, extra;
	if (ts_getuserwpn(t, clip, ammo, mode, extra) != TSW_CKNIFE) {
		log_to_file(LOG, "knife: try %d: thrower %d holds %d, switching again", g_kn_tries, t, ts_getuserwpn(t, clip, ammo, mode, extra));
		engclient_cmd(t, "weapon_25");
		set_task(1.0, "t_knife");
		return;
	}
	// A free spot 96 units away in one of eight directions (eye-level line of sight to 160).
	new Float:o[3], Float:ofs[3], Float:eye[3], Float:to[3], Float:hit[3], Float:dest[3];
	entity_get_vector(t, EV_VEC_origin, o);
	entity_get_vector(t, EV_VEC_view_ofs, ofs);
	eye[0] = o[0] + ofs[0]; eye[1] = o[1] + ofs[1]; eye[2] = o[2] + ofs[2];
	new found;
	for (new k = 0; k < 8 && !found; k++) {
		new Float:yaw = float(k) * 45.0;
		new Float:dx = floatcos(yaw, degrees), Float:dy = floatsin(yaw, degrees);
		to[0] = eye[0] + dx * 160.0; to[1] = eye[1] + dy * 160.0; to[2] = eye[2];
		trace_line(t, eye, to, hit);
		if (get_distance_f(eye, hit) < 150.0) continue;
		to[2] = o[2] - 18.0; // and at the knees
		new Float:low[3]; low[0] = o[0]; low[1] = o[1]; low[2] = o[2] - 18.0;
		trace_line(t, low, to, hit);
		if (get_distance_f(low, hit) < 150.0) continue;
		dest[0] = o[0] + dx * 96.0; dest[1] = o[1] + dy * 96.0; dest[2] = o[2];
		found = 1;
	}
	if (!found) { log_to_file(LOG, "knife: try %d: no room around %d", g_kn_tries, t); set_task(1.0, "t_knife"); return; }
	new Float:zero[3];
	entity_set_vector(v, EV_VEC_velocity, zero);
	entity_set_origin(v, dest);
	set_user_health(v, 1);
	new Float:dir[3], Float:ang[3];
	dir[0] = dest[0] - eye[0]; dir[1] = dest[1] - eye[1]; dir[2] = dest[2] - eye[2];
	vector_to_angle(dir, ang);
	ang[0] = -ang[0]; // view angles look down with positive pitch
	entity_set_vector(t, EV_VEC_v_angle, ang);
	entity_set_vector(t, EV_VEC_angles, ang);
	new gun = find_ent_by_owner(-1, "weapon_tsgun", t);
	if (!gun) { log_to_file(LOG, "knife: try %d: no weapon_tsgun for %d", g_kn_tries, t); set_task(1.0, "t_knife"); return; }
	log_to_file(LOG, "knife: try %d: thrower %d at %.0f %.0f %.0f, victim %d at %.0f %.0f %.0f, gun %d",
		g_kn_tries, t, o[0], o[1], o[2], v, dest[0], dest[1], dest[2], gun);
	ExecuteHam(Ham_Weapon_SecondaryAttack, gun);
	set_task(1.5, "t_knife_done");
}

public t_knife_done()
{
	if (!g_kn_death) { set_task(0.1, "t_knife"); return; }
	set_cvar_num("ts_bot_stop", 0);
}

public t_knife_check()
{
	if (g_no_engclient) { skip("thrown_knife", "AMX Mod X 1.8.2 on Linux: engclient_cmd would crash the Rust server"); return; }
	set_cvar_num("ts_bot_stop", 0);
	new d[160], stats[8], body[8], name[32];
	xmod_get_wpnname(TSW_TKNIFE, name, charsmax(name));
	new w = g_kn_thrower ? get_user_wstats(g_kn_thrower, TSW_TKNIFE, stats, body) : 0;
	formatex(d, charsmax(d), "tries=%d thrower=%d victim=%d damage=%d death=%d wstats=%d kills=%d hits=%d name=%s",
		g_kn_tries, g_kn_thrower, g_kn_victim, g_kn_damage, g_kn_death, w, stats[0], stats[5], name);
	check("thrown_knife", g_kn_damage > 0 && g_kn_death > 0 && w && stats[0] > 0 && stats[5] > 0, d);
}

public t_static()
{
	new name[32], log[32];
	xmod_get_wpnname(13, name, charsmax(name));
	xmod_get_wpnlogname(13, log, charsmax(log));
	check("wpnname", equal(name, "AK47") && equal(log, "ak47"), name);
	// TS 3.0's weapon ids (tsx_legacy_weaponids 0, the repo's tsconst.inc).
	check("wpnlogtoid", ts_wpnlogtoid("desert_eagle") == 12 && ts_wpnlogtoid("kung_fu") == TSW_KUNG_FU
		&& TSW_KUNG_FU == 38 && ts_wpnlogtoid("contender_g2") == TSW_CONTENDER && ts_wpnlogtoid("throwing_knife") == TSW_TKNIFE);
	xmod_get_wpnname(TSW_ASKORPIONS, name, charsmax(name));
	check("wpnname_ts30", equal(name, "Akimbo Skorpions") && xmod_is_melee_wpn(TSW_KUNG_FU) == 1 && xmod_is_melee_wpn(TSW_CONTENDER) == 0, name);
	ts_wpnlogtoname("m4a1", name, charsmax(name));
	check("wpnlogtoname", equal(name, "M4A1"), name);
	check("maxweapons", xmod_get_maxweapons() == 44 && xmod_get_stats_size() == 8);
	check("is_melee", xmod_is_melee_wpn(34) == 1 && xmod_is_melee_wpn(13) == 0);
	new id = custom_weapon_add("E2E Laser", 0, "e2e_laser");
	check("custom_weapon_add", id >= 39 && xmod_is_custom_wpn(id) == 1);
	check("set_fuattack_outside", ts_set_fuattack(1, 1.0, 1.0) == 0, "no Melee_Attack running");
}

// A health power-up works at pickup: client_powerup(id, TSPWUP_HEALTH).
public t_pwup()
{
	if (!g_api) { skip("client_powerup", "no plugin API"); return; }
	g_pwup_target = first_alive();
	if (!g_pwup_target) { skip("client_powerup", "no living player"); return; }
	g_pwup_id = 0;
	g_pwup_health = 0;
	new ent = ts_createpwup(TSPWUP_HEALTH);
	if (!ent) { check("client_powerup", 0, "ts_createpwup failed"); return; }
	// "create_pwup -> !wait! -> give_pwup" (upstream): a ts_powerup takes touches only once it has
	// landed, so drop it at the player's feet and give it a second later (if the player hasn't
	// walked into it first, which fires the same forward).
	new Float:o[3];
	entity_get_vector(g_pwup_target, EV_VEC_origin, o);
	entity_set_origin(ent, o);
	new p[1]; p[0] = ent;
	set_task(1.0, "t_pwup_give", 0, p, 1);
}

public t_pwup_give(const p[])
{
	if (!is_user_alive(g_pwup_target)) { skip("client_powerup", "player died"); return; }
	if (is_valid_ent(p[0])) {
		new Float:o[3];
		entity_get_vector(p[0], EV_VEC_origin, o);
		log_to_file(LOG, "givepwup: ent=%d onground=%d z=%.0f, player z=%.0f", p[0],
			(entity_get_int(p[0], EV_INT_flags) & FL_ONGROUND) != 0, o[2], get_z(g_pwup_target));
		ts_givepwup(g_pwup_target, p[0]);
	} else {
		log_to_file(LOG, "givepwup: ent=%d gone", p[0]);
	}
	set_task(0.3, "t_pwup_check");
}

public t_pwup_check()
{
	new d[96];
	formatex(d, charsmax(d), "target=%d health_to=%d last id=%d powerup=%d", g_pwup_target, g_pwup_health, g_pwup_id, g_pwup_last);
	// A bot that walks over the dropped power-up first takes it (and fires the forward), and one
	// that hasn't landed yet (dropped on a player in the air) takes no touch: try again.
	if (g_pwup_health != g_pwup_target && ++g_pwup_tries < 3)
	{
		log_to_file(LOG, "client_powerup: %s, retrying", d);
		t_pwup();
		return;
	}
	check("client_powerup", g_pwup_health == g_pwup_target, d);
}

public t_getters()
{
	new id = first_alive();
	if (!id) { check("getters", 0, "no living player"); return; }
	new clip, ammo, mode, extra, d[160];
	new wpn = ts_getuserwpn(id, clip, ammo, mode, extra);
	new cash = ts_getusercash(id), space = ts_getuserspace(id), slots = ts_getuserslots(id);
	new stance = ts_getuserstate(id), msg = ts_get_message(id), streak = ts_getkillingstreak(id);
	new pw = ts_getuserpwup(id), items = ts_getuseritems(id);
	formatex(d, charsmax(d), "id=%d wpn=%d clip=%d ammo=%d mode=%d extra=%d cash=%d space=%d slots=%d state=%d msg=%d streak=%d pwup=%d items=%d sj=%d fu=%d",
		id, wpn, clip, ammo, mode, extra, cash, space, slots, stance, msg, streak, pw, items, ts_has_superjump(id), ts_has_fupowerup(id));
	log_to_file(LOG, "getters: %s", d);
	check("getters_plausible", wpn >= 0 && wpn < 44 && cash >= 0 && space >= 0 && msg >= 0 && msg <= 16, d);

	// The plugin API: a setter returns 1 only when the server has it (ts_set_bullettrail can't
	// tell: TS 3.0 has no per-player trail, so the API answers UNSUPPORTED).
	new r = ts_setuserslots(id, 50);
	g_api = r == 1;
	log_to_file(LOG, "plugin api: %s", g_api ? "present" : "absent");
	new r2 = ts_setusercash(id, 1234);
	if (g_api) {
		g_set_id = id;
		check("in_game_values", wpn > 0 && ts_get_message(id) > 0, d);
		// ts_setusercash returns 0 like upstream; its effect is checked below.
		formatex(d, charsmax(d), "set_message=%d fakeslowmo=%d speed=%d physics=%d bullettrail=%d",
			ts_set_message(id, TSMSG_SPECIALIST), ts_set_fakeslowmo(id, 5.0), ts_set_speed(id, 0.5, 0.0, 4.0),
			ts_set_physics_speed(id, 1.0), ts_set_bullettrail(id, 1));
		check("setters_return", contain(d, "=0 ") == -1 && contain(d, "bullettrail=0") != -1, d);
		// The API's player snapshot refreshes every frame: look a little later.
		set_task(0.3, "t_setters_check");
	} else {
		formatex(d, charsmax(d), "setuserslots=%d setusercash=%d (no API: expect 0 and one log line each)", r, r2);
		check("setters_without_api", r == 0 && r2 == 0, d);
	}
}

public t_setters_check()
{
	new id = g_set_id, d[160];
	if (!is_user_alive(id)) { skip("setters_visible", "player died"); return; }
	new Float:until = Float:ts_is_in_slowmo(id);
	formatex(d, charsmax(d), "id=%d cash=%d slots=%d message=%d running=%d in_slowmo_until=%.1f now=%.1f",
		id, ts_getusercash(id), ts_getuserslots(id), ts_get_message(id), ts_is_running_powerup(id), until, get_gametime());
	check("setters_visible", ts_getusercash(id) == 1234 && ts_getuserslots(id) == 50 && ts_get_message(id) == TSMSG_SPECIALIST
		&& ts_is_running_powerup(id) == TSPWUP_SLOWMO && until > get_gametime(), d);
	// Several bits: tsx runs the lowest runnable one (TS 3.0 runs one at a time) for its normal
	// length and logs once that superjump was dropped.
	new f = ts_force_run_powerup(id, TSPWUP_DFIRERATE | TSPWUP_SUPERJUMP);
	check("force_run_powerup", f == 1, "");
	set_task(0.3, "t_powerup_check");
}

public t_powerup_check()
{
	new id = g_set_id, d[64];
	if (!is_user_alive(id)) { skip("powerup_visible", "player died"); return; }
	formatex(d, charsmax(d), "running=%d", ts_is_running_powerup(id));
	check("powerup_visible", ts_is_running_powerup(id) == TSPWUP_DFIRERATE, d);
	ts_force_run_powerup(id, 0);
}

public t_give()
{
	g_give_id = roomiest_alive();
	if (!g_give_id) { check("giveweapon", 0, "no living player"); return; }
	g_give_space = ts_getuserspace(g_give_id);
	new ok = ts_giveweapon(g_give_id, 13, 60, 0);
	check("giveweapon_returns", ok == 1);
	if (g_no_engclient) {
		skip("giveweapon_visible", "AMX Mod X 1.8.2 on Linux: engclient_cmd would crash the Rust server");
		g_give_id = 0;
		return;
	}
	engclient_cmd(g_give_id, "weapon_13");
}

// The bot AI may switch away again (to a grenade, say), so switch back and look a few times.
public t_give_check()
{
	static tries;
	if (g_no_engclient) return;
	new id = g_give_id;
	if (!id || !is_user_alive(id)) { skip("giveweapon_visible", "player died"); return; }
	new clip, ammo, mode, extra, model[64], d[128];
	new wpn = ts_getuserwpn(id, clip, ammo, mode, extra);
	entity_get_string(id, EV_SZ_weaponmodel, model, charsmax(model));
	formatex(d, charsmax(d), "id=%d try=%d ts_getuserwpn=%d clip=%d ammo=%d weaponmodel=%s", id, tries, wpn, clip, ammo, model);
	if (wpn != 13 && ++tries < 8) {
		log_to_file(LOG, "giveweapon: %s, switching again", d);
		engclient_cmd(id, "weapon_13");
		set_task(0.3, "t_give_check");
		return;
	}
	if (wpn != 13 && ts_getuserspace(id) == g_give_space) {
		// The roomiest player had too little room for an AK47 (no slots taken): TS refused it.
		skip("giveweapon_visible", d);
		return;
	}
	// Without the API, tsx reads a bot's weapon from its player model (bots get no WeaponInfo).
	check("giveweapon_visible", wpn == 13, d);
	if (wpn != 13) return;
	// ts_setuserammo (tsxaddon.inc's function, now a tsx native): the reserve of the gun in hand.
	new r = ts_setuserammo(id, 13, 77);
	if (g_api) {
		check("setuserammo_returns", r == 1, "");
		set_task(0.3, "t_ammo_check");
	} else {
		check("setuserammo_without_api", r == 0, "");
	}
}

public t_ammo_check()
{
	new id = g_give_id, clip, ammo, mode, extra, d[96];
	if (!is_user_alive(id)) { skip("setuserammo_visible", "player died"); return; }
	new wpn = ts_getuserwpn(id, clip, ammo, mode, extra);
	formatex(d, charsmax(d), "wpn=%d clip=%d ammo=%d", wpn, clip, ammo);
	if (wpn != 13) { skip("setuserammo_visible", d); return; }
	check("setuserammo_visible", ammo == 77, d);
}

public t_stats()
{
	new d[160];
	formatex(d, charsmax(d), "client_damage=%d client_death=%d (named weapon %d)", g_damage, g_death, g_death_named);
	check("forwards", g_death > 0 && g_damage > 0 && g_death_named == g_death, d);

	// The bots' own stunts and kung fu: not every run has them.
	formatex(d, charsmax(d), "client_stunt=%d (bad %d) Melee_Attack=%d (bad %d, set_fuattack ok %d) client_powerup=%d",
		g_stunts, g_stunt_bad, g_melees, g_melee_bad, g_fuattack_ok, g_pwups);
	log_to_file(LOG, "v5 forwards: %s", d);
	if (!g_api) skip("client_stunt", "no plugin API");
	else if (!g_stunts) skip("client_stunt", "no stunt in this run");
	else check("client_stunt", g_stunt_bad == 0, d);
	if (!g_api) skip("melee_attack", "no plugin API");
	else if (!g_melees) skip("melee_attack", "no kung-fu move in this run");
	else check("melee_attack", g_melee_bad == 0 && g_fuattack_ok == g_melees, d);

	new stats[8], body[8], n = get_statsnum();
	new ranked, wstats, vstats, rstats;
	for (new i = 1; i <= get_maxplayers(); i++) {
		if (!is_user_connected(i)) continue;
		if (get_user_stats(i, stats, body) > 0) ranked++;
		if (get_user_wstats(i, 0, stats, body)) wstats++;
		if (get_user_vstats(i, 0, stats, body)) vstats++;
		if (get_user_rstats(i, stats, body)) rstats++;
	}
	formatex(d, charsmax(d), "statsnum=%d ranked=%d wstats=%d vstats=%d rstats=%d", n, ranked, wstats, vstats, rstats);
	check("stats", n > 0 && ranked > 0 && wstats > 0 && vstats > 0 && rstats > 0, d);
	new name[32];
	new next = get_stats(0, stats, body, name, charsmax(name));
	formatex(d, charsmax(d), "top=%s kills=%d deaths=%d next=%d", name, stats[0], stats[1], next);
	check("get_stats", name[0] != 0, d);
	if (g_last_killer)
		log_to_file(LOG, "last killer %d: lastfrag=%d killflags=%d streak=%d", g_last_killer,
			ts_getuserlastfrag(g_last_killer), ts_getuserkillflags(g_last_killer), ts_getkillingstreak(g_last_killer));
}

public t_finish()
{
	log_to_file(LOG, "DONE pass=%d fail=%d api=%d", g_pass, g_fail, g_api);
}
