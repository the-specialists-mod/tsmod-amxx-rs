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

// Registered by tsx (NBase.cpp) but not declared in the stock includes.
native ts_getuseritems(index);

new const LOG[] = "tsx_e2e.log";

new g_pass, g_fail;
new g_damage, g_death, g_death_named;
new g_last_killer;
new bool:g_api;
new g_give_id;
new g_set_id;

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
	set_task(20.0, "t_static");
	set_task(21.0, "t_getters");
	set_task(25.0, "t_give");
	set_task(26.5, "t_give_check");
	set_task(50.0, "t_stats");
	set_task(51.0, "t_finish");
}

public client_damage(attacker, victim, damage, wpnindex, hitplace, TA)
{
	g_damage++;
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
	if (g_death <= 5)
		log_to_file(LOG, "client_death: killer=%d victim=%d wpn=%d (%s) hit=%d tk=%d lastfrag=%d flags=%d",
			killer, victim, wpnindex, name, hitplace, TK, ts_getuserlastfrag(killer), ts_getuserkillflags(killer));
}

first_alive()
{
	for (new i = 1; i <= get_maxplayers(); i++)
		if (is_user_alive(i)) return i;
	return 0;
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
	// One power-up bit for its normal length.
	new f = ts_force_run_powerup(id, TSPWUP_DFIRERATE);
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
	g_give_id = first_alive();
	if (!g_give_id) { check("giveweapon", 0, "no living player"); return; }
	new ok = ts_giveweapon(g_give_id, 13, 60, 0);
	check("giveweapon_returns", ok == 1);
	engclient_cmd(g_give_id, "weapon_13");
}

// The bot AI may switch away again (to a grenade, say), so switch back and look a few times.
public t_give_check()
{
	static tries;
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
