// tsmod-amxx-rs end-to-end test plugin for hamsandwich (cargo xtask e2e).
// GPL-3.0-or-later with the AMX Mod X exceptions.
//
// Hooks Ham_Spawn, Ham_TakeDamage (pre and post), Ham_Killed, Ham_Touch on players and
// Ham_Think on an entity, then checks the effects through game state (health), and writes
// "CHECK <name> PASS|FAIL|SKIP <detail>" lines and "DONE ..." to addons/amxmodx/logs/ham_e2e.log.
#include <amxmodx>
#include <fun>
#include <engine>
#include <hamsandwich>

new const LOG[] = "ham_e2e.log";

new g_pass, g_fail;
new g_spawns, g_killed, g_touches, g_thinks, g_td_pre, g_td_post;
new g_target, g_mode; // 1: halve the damage, 2: supercede
new Float:g_post_damage;
new g_post_orig, g_post_status;
new g_filter_err = -1;
new g_think_ent;
// Plugin API v4 (the-specialistsmod-rs#99): the gun's functions on weapon_tsgun and the queries.
new g_preframes, g_preframe_bad, g_slowmul;
new Float:g_slowmul_ret = -1.0;

check(const name[], ok, const detail[] = "")
{
	if (ok) g_pass++; else g_fail++;
	log_to_file(LOG, "CHECK %s %s %s", name, ok ? "PASS" : "FAIL", detail);
}

skip(const name[], const detail[])
{
	log_to_file(LOG, "CHECK %s SKIP %s", name, detail);
}

// Fatal hamsandwich errors (RegisterHam/ExecuteHam on a function the server lacks) come here
// instead of failing the plugin.
public HamFilter(Ham:id, HamError:err, const reason[])
{
	g_filter_err = _:err;
	log_to_file(LOG, "HamFilter: ham=%d err=%d %s", _:id, _:err, reason);
	return PLUGIN_HANDLED;
}

public plugin_init()
{
	register_plugin("ham_e2e", "1.0", "tsmod-amxx-rs");
	log_to_file(LOG, "start");
	if (!IsHamValid(Ham_TakeDamage)) {
		skip("ham", "IsHamValid(Ham_TakeDamage) is false: the server has no plugin API v2 (entity hooks)");
		set_task(5.0, "t_finish");
		return;
	}
	RegisterHamPlayer(Ham_Spawn, "fw_spawn_post", 1);
	RegisterHam(Ham_TakeDamage, "player", "fw_td_pre");
	RegisterHam(Ham_TakeDamage, "player", "fw_td_post", 1);
	RegisterHam(Ham_Killed, "player", "fw_killed");
	RegisterHam(Ham_Touch, "player", "fw_touch");
	if (IsHamValid(Ham_Item_PreFrame)) {
		RegisterHam(Ham_Item_PreFrame, "weapon_tsgun", "fw_preframe");
		RegisterHam(Ham_TS_GiveSlowMul, "player", "fw_slowmul_post", 1);
	}
	set_task(15.0, "t_valid");
	set_task(20.0, "t_damage");
	set_task(22.0, "t_think_setup");
	set_task(50.0, "t_events");
	set_task(51.0, "t_finish");
}

public fw_spawn_post(id) { g_spawns++; }
public fw_killed(victim, attacker, shouldgib) { g_killed++; }
public fw_touch(id, other) { g_touches++; }
public fw_think(ent) { if (ent == g_think_ent) g_thinks++; }

// The gun's ItemPreFrame: `this` is a weapon_tsgun whose owner is a player.
public fw_preframe(gun)
{
	new owner = entity_get_edict(gun, EV_ENT_owner);
	if (owner < 1 || owner > get_maxplayers()) g_preframe_bad++;
	g_preframes++;
}

// The port's Ham_TS_GiveSlowMul returns its float (stock declares it void): max(fuser1, 0.01).
// As in stock, GetHamReturnFloat is the override (0 unless a hook set one); the result of the
// original is GetOrigHamReturnFloat.
public fw_slowmul_post(id)
{
	g_slowmul++;
	GetOrigHamReturnFloat(g_slowmul_ret);
}

public fw_td_pre(victim, inflictor, attacker, Float:damage, bits)
{
	g_td_pre++;
	if (victim != g_target) return HAM_IGNORED;
	if (g_mode == 1) {
		SetHamParamFloat(4, damage / 2.0);
		return HAM_HANDLED;
	}
	if (g_mode == 2) {
		SetHamReturnInteger(0);
		return HAM_SUPERCEDE;
	}
	return HAM_IGNORED;
}

public fw_td_post(victim, inflictor, attacker, Float:damage, bits)
{
	g_td_post++;
	if (victim == g_target) {
		g_post_damage = damage;
		GetOrigHamReturnInteger(g_post_orig);
		g_post_status = GetHamReturnStatus();
	}
	return HAM_IGNORED;
}

public t_valid()
{
	new d[96];
	formatex(d, charsmax(d), "TakeDamage=%d Killed=%d CS_RoundRespawn=%d SC_IsPhysX=%d",
		IsHamValid(Ham_TakeDamage), IsHamValid(Ham_Killed), IsHamValid(Ham_CS_RoundRespawn), IsHamValid(Ham_SC_IsPhysX));
	check("ishamvalid", IsHamValid(Ham_TakeDamage) && IsHamValid(Ham_Killed) && !IsHamValid(Ham_CS_RoundRespawn) && !IsHamValid(Ham_SC_IsPhysX), d);
	// An unsupported function fails "like stock": HamFilter gets HAM_FUNC_NOT_CONFIGURED (2).
	new HamHook:h = RegisterHam(Ham_CS_RoundRespawn, "player", "fw_spawn_post");
	formatex(d, charsmax(d), "handle=%d filter_err=%d", _:h, g_filter_err);
	check("register_unsupported", _:h == 0 && g_filter_err == 2, d);
	// An unknown class is a native error (in the AMXX error log) and no handle.
	h = RegisterHam(Ham_TakeDamage, "no_such_class_e2e", "fw_td_pre");
	check("register_unknown_class", _:h == 0, "");
}

first_alive()
{
	for (new i = 1; i <= get_maxplayers(); i++)
		if (is_user_alive(i)) return i;
	return 0;
}

public t_damage()
{
	new id = first_alive(), d[160];
	if (!id) { skip("takedamage", "no living player"); return; }
	g_target = id;
	set_user_armor(id, 0);

	// 1. The pre forward halves 40 damage: post sees 20, the player loses less than the full hit.
	set_user_health(id, 100);
	g_mode = 1;
	new r1 = ExecuteHamB(Ham_TakeDamage, id, id, id, 40.0, 0);
	new halved = 100 - get_user_health(id);
	formatex(d, charsmax(d), "ret=%d health_drop=%d post_damage=%.1f post_orig=%d status=%d", r1, halved, g_post_damage, g_post_orig, g_post_status);
	check("takedamage_param", g_post_damage == 20.0 && halved > 0 && g_post_status == _:HAM_HANDLED, d);

	// 2. Full damage without the hooks (ExecuteHam): more than the halved hit.
	set_user_health(id, 100);
	g_mode = 2; // would supercede, but ExecuteHam runs no hooks
	new pre_before = g_td_pre;
	ExecuteHam(Ham_TakeDamage, id, id, id, 40.0, 0);
	new full = 100 - get_user_health(id);
	formatex(d, charsmax(d), "health_drop=%d halved_drop=%d hooks_ran=%d", full, halved, g_td_pre - pre_before);
	check("executeham_no_hooks", full > halved && g_td_pre == pre_before, d);

	// 3. Superceded through ExecuteHamB: no damage; the post forward ignores, so (stock) the
	// return is the original value, 0 when the function didn't run.
	set_user_health(id, 100);
	new r3 = ExecuteHamB(Ham_TakeDamage, id, id, id, 40.0, 0);
	formatex(d, charsmax(d), "ret=%d health=%d status=%d", r3, get_user_health(id), g_post_status);
	check("takedamage_supercede", get_user_health(id) == 100 && r3 == 0 && g_post_status == _:HAM_SUPERCEDE, d);

	g_mode = 0;
	g_target = 0;
	set_user_health(id, 100);
}

public t_think_setup()
{
	g_think_ent = create_entity("info_target");
	if (!g_think_ent) { skip("think", "create_entity(info_target) failed"); return; }
	new HamHook:h = RegisterHamFromEntity(Ham_Think, g_think_ent, "fw_think");
	if (!h) { skip("think", "RegisterHamFromEntity(Ham_Think, info_target) failed"); return; }
	entity_set_float(g_think_ent, EV_FL_nextthink, get_gametime() + 0.5);
}

public t_events()
{
	new d[160];
	formatex(d, charsmax(d), "spawns=%d killed=%d touches=%d td_pre=%d td_post=%d thinks=%d", g_spawns, g_killed, g_touches, g_td_pre, g_td_post, g_thinks);
	log_to_file(LOG, "events: %s", d);
	check("spawn_post", g_spawns > 0, d);
	check("killed", g_killed > 0, d);
	check("touch", g_touches > 0, d);
	check("takedamage_by_game", g_td_pre > 3 && g_td_post > 3, d);
	if (g_think_ent)
		check("think", g_thinks > 0, d);
	if (!IsHamValid(Ham_Item_PreFrame)) {
		skip("gun_hooks", "the server has no plugin API v4 (Item_*/Weapon_* hooks)");
		return;
	}
	formatex(d, charsmax(d), "preframes=%d bad=%d slowmul=%d ret=%.3f", g_preframes, g_preframe_bad, g_slowmul, g_slowmul_ret);
	log_to_file(LOG, "v4: %s", d);
	check("item_preframe", g_preframes > 0 && g_preframe_bad == 0, d);
	check("giveslowmul_float", g_slowmul > 0 && g_slowmul_ret >= 0.01 && g_slowmul_ret <= 2.0, d);
}

public t_finish()
{
	log_to_file(LOG, "DONE pass=%d fail=%d", g_pass, g_fail);
}
