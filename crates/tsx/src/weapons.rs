// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports modules/ts/tsx/Utils.cpp
// (`weaponData`), Copyright (C) The AMX Mod X Development Team, Copyright (C) 2004 Lukasz
// Wlasinski.

//! The weapon table: names, log names, melee flags and frag bonuses by tsx weapon id.
//!
//! The ids are upstream tsx's, which plugins have compiled in (`tsconst.inc`'s `TSW_*`,
//! `TSMAX_WEAPONS` 44): the wire ids TS 3.0 sends, except kung fu, which tsx reports as 36
//! (`WeaponInfo` sends 0), and the thrown knife, 37. TS 3.0's own weapons 36 (Contender G2) and
//! 37 (Akimbo Skorpions) therefore share those ids, as they do with upstream tsx on TS 3.0;
//! real plugins treat 36 as kung fu. Names upstream got wrong for TS 3.0 where no id clashes
//! (2, 17) use TS 3.0's (`the-specialistsmod-rs` `crates/shared/data/weapons.toml`), and the log
//! names TS 3.0 writes (`log_weapon` in the port's `rules/scoring.rs`) resolve as aliases.

/// `TSMAX_WEAPONS`: 39 TS ids plus 5 custom slots.
pub const MAX_WEAPONS: usize = 44;
/// `TSMAX_CUSTOMWPNS`.
pub const CUSTOM_WEAPONS: usize = 5;
/// First custom slot.
pub const FIRST_CUSTOM: usize = MAX_WEAPONS - CUSTOM_WEAPONS;
/// The id tsx reports for kung fu (`WeaponInfo` id 0).
pub const KUNG_FU: usize = 36;
/// The M61 grenade (damage from a `grenade` entity).
pub const GRENADE: usize = 24;
/// The thrown knife (damage from a `knife` entity).
pub const THROWING_KNIFE: usize = 37;
/// Longest name a slot holds (`char name[24]` minus the terminator).
const NAME_MAX: usize = 23;

/// One `weapon_t`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Weapon {
    pub melee: bool,
    pub name: String,
    pub logname: String,
    /// Frags tsx expects for a kill (`lastFrag` before the multipliers).
    pub bonus: i32,
    /// A slot `custom_weapon_add` filled.
    pub custom: bool,
}

/// `weaponData` (Utils.cpp), ids 0 to 38, with the TS 3.0 names for ids 2 and 17.
const TABLE: [(bool, &str, &str, i32); FIRST_CUSTOM] = [
    (true, "Kung Fu", "kung_fu", 3), // id 0, as WeaponInfo and DeathMsg have it
    (false, "Glock-18", "glock-18", 1),
    (false, "Beretta 92F", "beretta_92f", 1), // upstream: "Unk1" (TS 2.x had nothing here)
    (false, "Mini-Uzi", "mini-uzi", 1),
    (false, "BENELLI-M3", "benelli_m3", 1),
    (false, "M4A1", "m4a1", 1),
    (false, "MP5SD", "mp5sd", 1),
    (false, "MP5K", "mp5k", 1),
    (false, "Akimbo Berettas", "akimbo_berettas", 1),
    (false, "SOCOM-MK23", "socom-mk23", 1),
    (false, "Akimbo MK23", "akimbo_mk23", 1),
    (false, "USAS-12", "usas-12", 1),
    (false, "Desert Eagle", "desert_eagle", 1),
    (false, "AK47", "ak47", 1),
    (false, "Five-seveN", "five-seven", 1),
    (false, "STEYR-AUG", "steyr-aug", 1),
    (false, "Akimbo Mini-Uzi", "akimbo_mini-uzi", 1),
    (false, "Skorpion", "skorpion", 1), // upstream: "STEYR-TMP" (TS 2.x)
    (false, "Barrett M82A1", "barrett_m82a1", 1),
    (false, "MP7-PDW", "mp7-pdw", 1),
    (false, "SPAS-12", "spas-12", 1),
    (false, "Golden Colts", "golden_colts", 1),
    (false, "Glock-20C", "glock-20c", 1),
    (false, "UMP", "ump", 1),
    (false, "M61 Grenade", "m61_grenade", 1),
    (true, "Combat Knife", "combat_knife", 1),
    (false, "MOSSBERG 500", "mossberg_500", 1),
    (false, "M16A4", "m16a4", 1),
    (false, "Ruger-MK1", "ruger-mk1", 1),
    (false, "C4", "c4", 0), // no TS 3.0 weapon has id 29
    (false, "Akimbo Five-seveN", "akimbo_five-seven", 1),
    (false, "Raging Bull", "raging_bull", 1),
    (false, "M60E3", "m60e3", 1),
    (false, "Sawed-off", "sawed-off", 1),
    (true, "Katana", "katana", 2),
    (true, "Seal Knife", "seal_knife", 1),
    (true, "Kung Fu", "kung_fu", 3),               // 36, also TS 3.0's Contender G2
    (true, "Throwing Knife", "throwing_knife", 2), // 37, also TS 3.0's Akimbo Skorpions
    (false, "breakable", "breakable", 1),
];

/// Log names TS 3.0 writes that differ from the table's (lower case, spaces as underscores).
const LOG_ALIASES: [(&str, usize); 10] = [
    ("unk1", 2),
    ("steyr-tmp", 17),
    ("kung-fu", KUNG_FU),
    ("contender_g2", 36),
    ("akimbo_skorpions", 37),
    ("throwing_combat_knife", THROWING_KNIFE),
    ("throwing_seal_knife", THROWING_KNIFE),
    ("throwing_katana", THROWING_KNIFE),
    ("benelli_m3", 4),
    ("mossberg_500", 26),
];

/// The table, including the custom slots.
#[derive(Clone, Debug)]
pub struct Weapons(Vec<Weapon>);

impl Default for Weapons {
    fn default() -> Self {
        let mut v: Vec<Weapon> = TABLE
            .iter()
            .map(|&(melee, name, logname, bonus)| Weapon { melee, name: name.into(), logname: logname.into(), bonus, custom: false })
            .collect();
        v.resize(MAX_WEAPONS, Weapon::default());
        Weapons(v)
    }
}

impl Weapons {
    /// The weapon with tsx id `id`.
    pub fn get(&self, id: usize) -> Option<&Weapon> {
        self.0.get(id)
    }

    /// Melee flag of `id` (false for an invalid id).
    pub fn melee(&self, id: usize) -> bool {
        self.get(id).is_some_and(|w| w.melee)
    }

    /// Frag bonus of `id`.
    pub fn bonus(&self, id: usize) -> i32 {
        self.get(id).map_or(0, |w| w.bonus)
    }

    /// `ts_wpnlogtoid`: the first id from 1 on whose log name matches, then TS 3.0's spellings.
    pub fn by_logname(&self, log: &str) -> Option<usize> {
        if log.is_empty() {
            return None;
        }
        (1..MAX_WEAPONS).find(|&i| self.0[i].logname == log).or_else(|| LOG_ALIASES.iter().find(|(n, _)| *n == log).map(|&(_, i)| i))
    }

    /// `custom_weapon_add`: the first free custom slot, or `None` when all five are taken.
    pub fn add_custom(&mut self, name: &str, melee: bool, logname: &str) -> Option<usize> {
        let i = (FIRST_CUSTOM..MAX_WEAPONS).find(|&i| !self.0[i].custom)?;
        let w = &mut self.0[i];
        w.name = truncate(name);
        // Upstream ignores the logname argument; filling it lets `ts_wpnlogtoid` find the slot.
        w.logname = truncate(logname);
        w.custom = true;
        w.melee = melee;
        Some(i)
    }

    /// Frees the custom slots (`ServerDeactivate`).
    pub fn clear_custom(&mut self) {
        for w in &mut self.0[FIRST_CUSTOM..] {
            w.custom = false;
        }
    }
}

fn truncate(s: &str) -> String {
    let mut end = s.len().min(NAME_MAX);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_tsconst() {
        let w = Weapons::default();
        // tsconst.inc's TSW_* ids.
        for (id, name) in [
            (1, "Glock-18"),
            (13, "AK47"),
            (24, "M61 Grenade"),
            (25, "Combat Knife"),
            (34, "Katana"),
            (36, "Kung Fu"),
            (37, "Throwing Knife"),
        ] {
            assert_eq!(w.get(id).unwrap().name, name, "id {id}");
        }
        assert_eq!(w.get(0).unwrap().name, "Kung Fu");
        assert_eq!(w.get(38).unwrap().logname, "breakable");
        assert!(w.get(MAX_WEAPONS).is_none());
        assert_eq!(FIRST_CUSTOM, 39);
    }

    #[test]
    fn melee_and_bonus_follow_upstream() {
        let w = Weapons::default();
        let melee: Vec<usize> = (0..MAX_WEAPONS).filter(|&i| w.melee(i)).collect();
        assert_eq!(melee, [0, 25, 34, 35, 36, 37]);
        assert_eq!((w.bonus(0), w.bonus(34), w.bonus(37), w.bonus(29), w.bonus(13)), (3, 2, 2, 0, 1));
    }

    #[test]
    fn log_names_resolve_including_ts30_spellings() {
        let w = Weapons::default();
        assert_eq!(w.by_logname("ak47"), Some(13));
        assert_eq!(w.by_logname("kung_fu"), Some(36), "id 0 is skipped like upstream (search starts at 1)");
        assert_eq!(w.by_logname("beretta_92f"), Some(2));
        assert_eq!(w.by_logname("steyr-tmp"), Some(17));
        assert_eq!(w.by_logname("contender_g2"), Some(36));
        assert_eq!(w.by_logname("throwing_katana"), Some(37));
        assert_eq!(w.by_logname("nope"), None);
        assert_eq!(w.by_logname(""), None, "empty custom slots don't match an empty name");
    }

    #[test]
    fn custom_slots() {
        let mut w = Weapons::default();
        let ids: Vec<_> = (0..6).map(|i| w.add_custom(&format!("Laser {i}"), i == 0, &format!("laser{i}"))).collect();
        assert_eq!(ids, [Some(39), Some(40), Some(41), Some(42), Some(43), None]);
        assert!(w.melee(39) && !w.melee(40));
        assert_eq!(w.by_logname("laser2"), Some(41));
        assert_eq!(w.add_custom(&"x".repeat(40), false, ""), None);
        w.clear_custom();
        let i = w.add_custom(&"x".repeat(40), false, "").unwrap();
        assert_eq!(w.get(i).unwrap().name.len(), 23, "names are cut to char[24]");
    }
}
