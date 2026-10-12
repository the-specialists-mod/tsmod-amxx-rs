/*	TSX Addon Stocks
*
*	By: Avalanche, Twilight Suzuka and Harbu
*
*	Patched for tsmod-amxx-rs: ts_setuserammo is a tsx native (see below).
*/

#include <tsx>
#include <tsfun>
#include <fakemeta>
#include <engine>

#if defined _tsxaddon_included
  #endinput
#endif
#define _tsxaddon_included


 // ts_setuserammo is a tsx native in tsmod-amxx-rs (the Rust TS server's AMX Mod X modules).
 // The old function here wrote TS 2.x's private data, which the Rust server doesn't have; the
 // native keeps its behaviour: 0 for kung fu (36, after switching to it), invalid ids, C4 (29)
 // and the katana (34); otherwise it switches to the weapon and sets its reserve ammo (for the
 // grenade and the knives, 24/25/35, its clip) and returns 1. 0 when the server lacks the plugin
 // API. Recompile plugins against this file to use it.
 // Original code: Avalanche ("Avalanches Ammo Code").
native ts_setuserammo(id, weapon, ammo);

 public set_entity_health(door,Float:hp)
{
	if(hp == -1.0) {
		entity_set_float(door,EV_FL_max_health,9999.0)
		entity_set_float(door,EV_FL_health,9999.0)
		entity_set_float(door,EV_FL_dmg,0.0)
		entity_set_float(door,EV_FL_takedamage,0.0)
		return 1
	}
	entity_set_float(door,EV_FL_max_health,hp)
	entity_set_float(door,EV_FL_health,hp)
	return 1 
}

// The actual creating process


// Creating Ambients
public create_ambient(porigin[3],targetname[],vol[],pitch[],spawnflags,file[])
{
	new ambient = create_entity("ambient_generic")

	if(!ambient) return PLUGIN_HANDLED

	entity_set_string(ambient,EV_SZ_classname,"ambient_generic")
	entity_set_int(ambient,EV_INT_spawnflags,spawnflags)
	entity_set_float(ambient,EV_FL_health,10.0)
	entity_set_string(ambient,EV_SZ_targetname,targetname)
	entity_set_string(ambient,EV_SZ_message,file)

	DispatchKeyValue(ambient,"pitch",pitch)
	DispatchKeyValue(ambient,"volstart",vol)
	DispatchKeyValue(ambient,"pitchstart",pitch)
	DispatchSpawn(ambient)

	new Float:origin[3]
	origin[0] = float(porigin[0])
	origin[1] = float(porigin[1])
	origin[2] = float(porigin[2])

	entity_set_origin(ambient,origin)

	return PLUGIN_HANDLED
}