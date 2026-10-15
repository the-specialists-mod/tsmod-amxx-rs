// tsmod-amxx-rs end-to-end score plugin (cargo xtask e2e's stats round trip).
// GPL-3.0-or-later with the AMX Mod X exceptions.
//
// Installed as addons/amxmodx/data/tsstats.amxx (tsx's `tsstats_score` default) for the restart
// that loads tsstats.dat: it ranks by deaths - kills, the reverse of upstream's tsstats.sma, so
// the loaded rank's order shows tsx called this get_score.
#include <amxmodx>

public get_score(stats[8], body[8])
{
	return stats[1] - stats[0];
}
