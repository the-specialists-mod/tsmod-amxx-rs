// vim: set ts=4 sw=4 tw=99 noet:
//
// AMX Mod X, based on AMX Mod by Aleksander Naszko ("OLO").
// Copyright (C) The AMX Mod X Development Team.
// Copyright (C) 2005 Suzuka.
//
// This software is licensed under the GNU General Public License, version 3 or higher.
// Additional exceptions apply. For full license details, see LICENSE.txt or visit:
//     https://alliedmods.net/amxmodx-license

//
// TSFUN Constants
//
// Patched for tsmod-amxx-rs (the Rust TS 3.0 server's AMX Mod X modules): the weapon ids are
// TS 3.0's, as tsx reports them by default. 36 is the Contender G2 and 37 the Akimbo
// Skorpions (TS 3.0's own ids); kung fu moved from 36 to 38 and the thrown knife from 37 to 29
// (TS 3.0 has no C4). TSMAX_WEAPONS and the custom weapon slots (39 to 43) are unchanged.
// A server with tsx_legacy_weaponids 1 reports stock tsconst.inc's ids (kung fu 36, thrown
// knife 37) instead: compile against the stock file for that.
//

#if defined _tsconst_included
  #endinput
#endif
#define _tsconst_included

#define TSMAX_WEAPONS 44	// TS 3.0's 0..37 + kung fu (38) + 5 custom weapon slots

#define TSPWUP_NONE			0
#define TSPWUP_RANDOM			0
#define TSPWUP_SLOWMO 			1
#define TSPWUP_INFAMMO		 	2
#define TSPWUP_KUNGFU		 	4
#define TSPWUP_SLOWPAUSE 		8
#define TSPWUP_DFIRERATE		16
#define TSPWUP_GRENADE			32
#define TSPWUP_HEALTH			64
#define TSPWUP_ARMOR			128
#define TSPWUP_SUPERJUMP		256

#define TSITEM_KUNGFU			1<<0
#define TSITEM_SUPERJUMP		1<<1

#define TSKF_STUNTKILL			1<<0
#define TSKF_SLIDINGKILL		1<<1
#define TSKF_DOUBLEKILL			1<<2
#define TSKF_ISSPEC			1<<3
#define TSKF_KILLEDSPEC			1<<4

#define TSA_SILENCER			1
#define TSA_LASERSIGHT			2
#define TSA_FLASHLIGHT			4
#define TSA_SCOPE				8

#define TSMSG_NORMAL			6
#define TSMSG_WAITING			11
#define TSMSG_DEAD			1
#define TSMSG_KILLER			2
#define TSMSG_DEMOLITION		3
#define TSMSG_SPECIALIST		4
#define TSMSG_UNSTOPPABLE		5
#define TSMSG_THEONE			10

#define STUNT_NONE 0
#define STUNT_DUCK 1
#define STUNT_ROLL 2
#define STUNT_DIVE 3
#define STUNT_GETUP 4
#define STUNT_FLIP 5
// tsmod-amxx-rs: TS 3.0 stunts 2.x lacked, as client_stunt reports them.
#define STUNT_WALLJUMP 6
#define STUNT_WALLCLIMB 7

enum {
  TSW_GLOCK18 = 1,
  TSW_UNK1,
  TSW_UZI,
  TSW_M3,
  TSW_M4A1,
  TSW_MP5SD,
  TSW_MP5K,
  TSW_ABERETTAS,
  TSW_MK23,
  TSW_AMK23,
  TSW_USAS,
  TSW_DEAGLE,
  TSW_AK47,
  TSW_57,
  TSW_AUG,
  TSW_AUZI,
  TSW_TMP,
  TSW_M82A1,
  TSW_MP7,
  TSW_SPAS,
  TSW_GCOLTS,
  TSW_GLOCK20,
  TSW_UMP,
  TSW_M61GRENADE,
  TSW_CKNIFE,
  TSW_MOSSBERG,
  TSW_M16A4,
  TSW_MK1,
  TSW_C4,		// 29: no TS 3.0 weapon; the thrown knife (TSW_TKNIFE)
  TSW_A57,
  TSW_RBULL,
  TSW_M60E3,
  TSW_SAWED_OFF,
  TSW_KATANA,
  TSW_SKNIFE,
  TSW_CONTENDER,	// 36, TS 3.0
  TSW_ASKORPIONS,	// 37, TS 3.0
  TSW_KUNG_FU,		// 38 (stock: 36)
  TSW_TKNIFE = 29,	// stock: 37
};

/* 
valid tsweaponid in TS_GiveWeapon

1: "Glock 18"
3: "Mini Uzi"
4: "Benelli M3"
5: "M4A1"
6: "MP5SD"
7: "MP5K"
8: "Akimbo Berettas"
9: "Socom Mk23"
11: "Usas12"
12: "Desert Eagle"
13: "Ak47"
14: "FiveSeven"
15: "Steyr Aug"
17: "Steyr Tmp"
18: "Barrett M82"
19: "HK Pdw"
20: "Spas12"
21: "Akimbo colts"
22: "Glock 20"
23: "Mac10"
25: "Combat Knife"
26: "Mossberg 500"
27: "M16A4"
28: "Ruger Mk1"
24: "M61 Grenade"
29: "C4"
31: "Raging Bull"
32: "M60"
33: "Sawed off"
34: "Katana"
35: "Seal Knife"

valid pwuptype in TS_GivePwUp

0: "Random"
1: "Slow Motion"
2: "Infinite Clip"
4: "Kung Fu"
8: "Slow Pause"
16: "Double Firerate"
32: "Grenade"
64: "Health"
128: "Armor"
256: "Superjump"

*/


