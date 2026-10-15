// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports modules/ts/tsx/Utils.cpp
// (`weaponData`), Copyright (C) The AMX Mod X Development Team, Copyright (C) 2004 Lukasz
// Wlasinski.

//! The weapon table: names, log names and melee flags by tsx weapon id.
//!
//! Two numberings, chosen by the cvar `tsx_legacy_weaponids` (port addition):
//!
//! * **TS 3.0's (0, the default).** Ids 1 to 37 are the wire ids TS 3.0 sends
//!   (`the-specialistsmod-rs` `crates/shared/data/weapons.toml`, `MAX_WEAPONS` 38), so 36 is the
//!   Contender G2 and 37 the Akimbo Skorpions. Kung fu (`WeaponInfo` sends 0) is 38 and the thrown
//!   knife 29: the two ids below the custom slots that TS 3.0 never sends (upstream's "breakable",
//!   which nothing reported, and TS 2.x's C4). `TSMAX_WEAPONS` stays 44 and the custom slots stay
//!   39 to 43, so plugins' `[TSMAX_WEAPONS]` arrays and `xmod_get_maxweapons()` loops still fit.
//! * **Upstream's (1).** `weaponData` (Utils.cpp) as plugins compiled against stock `tsconst.inc`
//!   expect it: kung fu 36 (`usermsg.cpp`'s `if (!wpn) wpn = 36`) and the thrown knife 37, which
//!   TS 3.0's Contender G2 and Akimbo Skorpions then share, as with upstream tsx on TS 3.0.
//!
//! Names upstream got wrong for TS 3.0 where no id clashes (2, 17) use TS 3.0's in both, and the
//! log names TS 3.0 writes (`log_weapon` in the port's `rules/scoring.rs`) resolve as aliases.
//! `tsstats.dat` keeps no per-weapon data (`CRank.cpp`), so the numbering doesn't touch it.

/// `TSMAX_WEAPONS`: 39 TS ids plus 5 custom slots.
pub const MAX_WEAPONS: usize = 44;
/// `TSMAX_CUSTOMWPNS`.
pub const CUSTOM_WEAPONS: usize = 5;
/// First custom slot.
pub const FIRST_CUSTOM: usize = MAX_WEAPONS - CUSTOM_WEAPONS;
/// Kung fu (`WeaponInfo` id 0) in TS 3.0's numbering.
pub const KUNG_FU: usize = 38;
/// The thrown knife (damage from a `knife` entity) in TS 3.0's numbering.
pub const THROWN_KNIFE: usize = 29;
/// Kung fu in upstream's numbering (`tsconst.inc` `TSW_KUNG_FU`).
pub const LEGACY_KUNG_FU: usize = 36;
/// The thrown knife in upstream's numbering (`TSW_TKNIFE`).
pub const LEGACY_THROWN_KNIFE: usize = 37;
/// The M61 grenade (damage from a `grenade` entity).
pub const GRENADE: usize = 24;
/// The cvar that selects upstream's numbering (port addition).
pub const LEGACY_CVAR: &str = "tsx_legacy_weaponids";
/// Longest name a slot holds (`char name[24]` minus the terminator).
const NAME_MAX: usize = 23;

/// One `weapon_t`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Weapon {
    pub melee: bool,
    pub name: String,
    pub logname: String,
    /// A slot `custom_weapon_add` filled.
    pub custom: bool,
}

/// A `weaponData` row: melee, name, log name. Upstream's fourth column, TS 2.x's frag bonus, is
/// gone: TS 3.0's frags don't follow the weapon table (`crate::scoring`).
type Entry = (bool, &'static str, &'static str);

/// `weaponData` (Utils.cpp), ids 0 to 38, with the TS 3.0 names for ids 2 and 17: upstream's
/// numbering. [`TS30`] overrides it for TS 3.0's.
const TABLE: [Entry; FIRST_CUSTOM] = [
    (true, "Kung Fu", "kung_fu"), // id 0, as WeaponInfo and DeathMsg have it
    (false, "Glock-18", "glock-18"),
    (false, "Beretta 92F", "beretta_92f"), // upstream: "Unk1" (TS 2.x had nothing here)
    (false, "Mini-Uzi", "mini-uzi"),
    (false, "BENELLI-M3", "benelli_m3"),
    (false, "M4A1", "m4a1"),
    (false, "MP5SD", "mp5sd"),
    (false, "MP5K", "mp5k"),
    (false, "Akimbo Berettas", "akimbo_berettas"),
    (false, "SOCOM-MK23", "socom-mk23"),
    (false, "Akimbo MK23", "akimbo_mk23"),
    (false, "USAS-12", "usas-12"),
    (false, "Desert Eagle", "desert_eagle"),
    (false, "AK47", "ak47"),
    (false, "Five-seveN", "five-seven"),
    (false, "STEYR-AUG", "steyr-aug"),
    (false, "Akimbo Mini-Uzi", "akimbo_mini-uzi"),
    (false, "Skorpion", "skorpion"), // upstream: "STEYR-TMP" (TS 2.x)
    (false, "Barrett M82A1", "barrett_m82a1"),
    (false, "MP7-PDW", "mp7-pdw"),
    (false, "SPAS-12", "spas-12"),
    (false, "Golden Colts", "golden_colts"),
    (false, "Glock-20C", "glock-20c"),
    (false, "UMP", "ump"),
    (false, "M61 Grenade", "m61_grenade"),
    (true, "Combat Knife", "combat_knife"),
    (false, "MOSSBERG 500", "mossberg_500"),
    (false, "M16A4", "m16a4"),
    (false, "Ruger-MK1", "ruger-mk1"),
    (false, "C4", "c4"), // no TS 3.0 weapon has id 29
    (false, "Akimbo Five-seveN", "akimbo_five-seven"),
    (false, "Raging Bull", "raging_bull"),
    (false, "M60E3", "m60e3"),
    (false, "Sawed-off", "sawed-off"),
    (true, "Katana", "katana"),
    (true, "Seal Knife", "seal_knife"),
    (true, "Kung Fu", "kung_fu"),               // 36, also TS 3.0's Contender G2
    (true, "Throwing Knife", "throwing_knife"), // 37, also TS 3.0's Akimbo Skorpions
    (false, "breakable", "breakable"),
];

/// TS 3.0's numbering: the slots that differ from [`TABLE`].
const TS30: [(usize, Entry); 4] = [
    (THROWN_KNIFE, (true, "Throwing Knife", "throwing_knife")),
    (36, (false, "Contender G2", "contender_g2")),
    (37, (false, "Akimbo Skorpions", "akimbo_skorpions")),
    (KUNG_FU, (true, "Kung Fu", "kung_fu")),
];

/// Log names TS 3.0 writes that differ from the table's (lower case, spaces as underscores), by
/// fixed id; kung fu's and the thrown knife's follow the numbering ([`Weapons::by_logname`]).
const LOG_ALIASES: [(&str, usize); 6] =
    [("unk1", 2), ("steyr-tmp", 17), ("contender_g2", 36), ("akimbo_skorpions", 37), ("benelli_m3", 4), ("mossberg_500", 26)];
/// TS 3.0's log names for kung fu.
const KUNG_FU_ALIASES: [&str; 1] = ["kung-fu"];
/// TS 3.0's log names for a thrown knife (`CTSKnife::GetDescription`).
const THROWN_KNIFE_ALIASES: [&str; 3] = ["throwing_combat_knife", "throwing_seal_knife", "throwing_katana"];

/// TS 3.0's player-model names (`models/p_<name>.mdl`, `_sil` when silenced) by wire id, from
/// the port's `crates/shared/data/assets.toml`.
const P_MODELS: [(&str, usize); 36] = [
    ("glock18", 1),
    ("beretta", 2),
    ("uzi", 3),
    ("m3", 4),
    ("m4", 5),
    ("mp5sd", 6),
    ("mp5k", 7),
    ("berettas", 8),
    ("mk23", 9),
    ("mk23_akimbo", 10),
    ("usas", 11),
    ("desert", 12),
    ("ak47", 13),
    ("fnh", 14),
    ("aug", 15),
    ("uzi_akimbo", 16),
    ("skorpion", 17),
    ("m82", 18),
    ("pdw", 19),
    ("spas12", 20),
    ("gold", 21),
    ("glock22", 22),
    ("ump", 23),
    ("m61", 24),
    ("knife", 25),
    ("mossberg", 26),
    ("m16", 27),
    ("ruger", 28),
    ("fnh_akimbo", 30),
    ("bull", 31),
    ("m60", 32),
    ("sawedoff", 33),
    ("katana", 34),
    ("sealknife", 35),
    ("contender", 36),
    ("skorpion_ak", 37),
];

/// The table, including the custom slots, in one of the two numberings.
#[derive(Clone, Debug)]
pub struct Weapons {
    list: Vec<Weapon>,
    legacy: bool,
}

impl Default for Weapons {
    fn default() -> Self {
        Weapons::new(false)
    }
}

fn weapon(&(melee, name, logname): &Entry) -> Weapon {
    Weapon { melee, name: name.into(), logname: logname.into(), custom: false }
}

impl Weapons {
    /// The table in upstream's numbering (`legacy`) or TS 3.0's, with free custom slots.
    pub fn new(legacy: bool) -> Self {
        let mut w = Weapons { list: vec![Weapon::default(); MAX_WEAPONS], legacy: !legacy };
        w.set_legacy(legacy);
        w
    }

    /// Whether this is upstream's numbering (`tsx_legacy_weaponids 1`).
    pub fn legacy(&self) -> bool {
        self.legacy
    }

    /// Switches the numbering of the TS ids; the custom slots stay as they are.
    pub fn set_legacy(&mut self, legacy: bool) {
        if legacy == self.legacy {
            return;
        }
        self.legacy = legacy;
        for (slot, entry) in self.list.iter_mut().zip(TABLE.iter()) {
            *slot = weapon(entry);
        }
        if !legacy {
            for (i, entry) in &TS30 {
                self.list[*i] = weapon(entry);
            }
        }
    }

    /// The id kung fu has in this numbering.
    pub fn kung_fu(&self) -> usize {
        if self.legacy { LEGACY_KUNG_FU } else { KUNG_FU }
    }

    /// The id a thrown knife has in this numbering.
    pub fn thrown_knife(&self) -> usize {
        if self.legacy { LEGACY_THROWN_KNIFE } else { THROWN_KNIFE }
    }

    /// The tsx id of TS weapon `wire` (`WeaponInfo`, the API's `weapon`): 0 is kung fu, as
    /// `Client_WeaponInfo` maps it; `None` past the table.
    pub fn from_wire(&self, wire: i32) -> Option<usize> {
        let id = if wire == 0 { self.kung_fu() } else { usize::try_from(wire).ok()? };
        (id < MAX_WEAPONS).then_some(id)
    }

    /// The weapon a player holds, from `pev->weaponmodel`: the server sends bots no
    /// `WeaponInfo`, so without the plugin API this is how tsx learns a bot's weapon. No model
    /// is kung fu. Port addition.
    pub fn id_from_p_model(&self, path: &str) -> Option<usize> {
        if path.is_empty() {
            return Some(self.kung_fu());
        }
        let file = path.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
        let name = file.strip_prefix("p_")?.strip_suffix(".mdl")?;
        let base = name.strip_suffix("_sil").unwrap_or(name);
        P_MODELS.iter().find(|(m, _)| *m == base).map(|&(_, id)| id)
    }

    /// The weapon with tsx id `id`.
    pub fn get(&self, id: usize) -> Option<&Weapon> {
        self.list.get(id)
    }

    /// Melee flag of `id` (false for an invalid id).
    pub fn melee(&self, id: usize) -> bool {
        self.get(id).is_some_and(|w| w.melee)
    }

    /// `ts_wpnlogtoid`: the first id from 1 on whose log name matches, then TS 3.0's spellings.
    pub fn by_logname(&self, log: &str) -> Option<usize> {
        if log.is_empty() {
            return None;
        }
        (1..MAX_WEAPONS).find(|&i| self.list[i].logname == log).or_else(|| {
            if KUNG_FU_ALIASES.contains(&log) {
                Some(self.kung_fu())
            } else if THROWN_KNIFE_ALIASES.contains(&log) {
                Some(self.thrown_knife())
            } else {
                LOG_ALIASES.iter().find(|(n, _)| *n == log).map(|&(_, i)| i)
            }
        })
    }

    /// `custom_weapon_add`: the first free custom slot, or `None` when all five are taken.
    pub fn add_custom(&mut self, name: &str, melee: bool, logname: &str) -> Option<usize> {
        let i = (FIRST_CUSTOM..MAX_WEAPONS).find(|&i| !self.list[i].custom)?;
        let w = &mut self.list[i];
        w.name = truncate(name);
        // Upstream ignores the logname argument; filling it lets `ts_wpnlogtoid` find the slot.
        w.logname = truncate(logname);
        w.custom = true;
        w.melee = melee;
        Some(i)
    }

    /// Frees the custom slots (`ServerDeactivate`).
    pub fn clear_custom(&mut self) {
        for w in &mut self.list[FIRST_CUSTOM..] {
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
    fn ts30_ids_by_default() {
        let w = Weapons::default();
        assert!(!w.legacy());
        for (id, name) in [
            (1, "Glock-18"),
            (13, "AK47"),
            (24, "M61 Grenade"),
            (25, "Combat Knife"),
            (29, "Throwing Knife"),
            (34, "Katana"),
            (36, "Contender G2"),
            (37, "Akimbo Skorpions"),
            (38, "Kung Fu"),
        ] {
            assert_eq!(w.get(id).unwrap().name, name, "id {id}");
        }
        assert_eq!((w.kung_fu(), w.thrown_knife()), (38, 29));
        assert_eq!(w.get(0).unwrap().name, "Kung Fu");
        assert!(w.get(MAX_WEAPONS).is_none());
        assert_eq!(FIRST_CUSTOM, 39, "the custom slots don't move");
    }

    #[test]
    fn legacy_ids_match_tsconst() {
        let w = Weapons::new(true);
        // Stock tsconst.inc's TSW_* ids.
        for (id, name) in [(1, "Glock-18"), (29, "C4"), (34, "Katana"), (36, "Kung Fu"), (37, "Throwing Knife")] {
            assert_eq!(w.get(id).unwrap().name, name, "id {id}");
        }
        assert_eq!((w.kung_fu(), w.thrown_knife()), (36, 37));
        assert_eq!(w.get(38).unwrap().logname, "breakable");
    }

    #[test]
    fn melee() {
        let w = Weapons::default();
        let melee: Vec<usize> = (0..MAX_WEAPONS).filter(|&i| w.melee(i)).collect();
        assert_eq!(melee, [0, 25, 29, 34, 35, 38]);
        let l = Weapons::new(true);
        let melee: Vec<usize> = (0..MAX_WEAPONS).filter(|&i| l.melee(i)).collect();
        assert_eq!(melee, [0, 25, 34, 35, 36, 37], "upstream's");
    }

    #[test]
    fn wire_ids() {
        let (w, l) = (Weapons::default(), Weapons::new(true));
        assert_eq!((w.from_wire(0), w.from_wire(36), w.from_wire(37)), (Some(38), Some(36), Some(37)));
        assert_eq!((l.from_wire(0), l.from_wire(36)), (Some(36), Some(36)), "upstream: the Contender shares kung fu's id");
        assert_eq!((w.from_wire(44), w.from_wire(-1)), (None, None));
    }

    #[test]
    fn switching_keeps_the_custom_slots() {
        let mut w = Weapons::default();
        let id = w.add_custom("Laser", false, "laser").unwrap();
        w.set_legacy(true);
        assert_eq!((w.get(36).unwrap().name.as_str(), w.get(id).unwrap().name.as_str()), ("Kung Fu", "Laser"));
        w.set_legacy(false);
        assert_eq!((w.get(36).unwrap().name.as_str(), w.get(38).unwrap().name.as_str()), ("Contender G2", "Kung Fu"));
        assert!(w.get(id).unwrap().custom);
    }

    #[test]
    fn log_names_resolve_including_ts30_spellings() {
        let w = Weapons::default();
        assert_eq!(w.by_logname("ak47"), Some(13));
        assert_eq!(w.by_logname("kung_fu"), Some(38), "id 0 is skipped like upstream (search starts at 1)");
        assert_eq!(w.by_logname("kung-fu"), Some(38));
        assert_eq!(w.by_logname("beretta_92f"), Some(2));
        assert_eq!(w.by_logname("steyr-tmp"), Some(17));
        assert_eq!(w.by_logname("contender_g2"), Some(36));
        assert_eq!(w.by_logname("throwing_knife"), Some(29));
        assert_eq!(w.by_logname("throwing_katana"), Some(29));
        assert_eq!(w.by_logname("nope"), None);
        assert_eq!(w.by_logname(""), None, "empty custom slots don't match an empty name");
        let l = Weapons::new(true);
        assert_eq!((l.by_logname("kung_fu"), l.by_logname("kung-fu")), (Some(36), Some(36)));
        assert_eq!((l.by_logname("contender_g2"), l.by_logname("akimbo_skorpions")), (Some(36), Some(37)));
        assert_eq!(l.by_logname("throwing_seal_knife"), Some(37));
    }

    #[test]
    fn player_models_name_the_weapon() {
        let w = Weapons::default();
        assert_eq!(w.id_from_p_model("models/p_ak47.mdl"), Some(13));
        assert_eq!(w.id_from_p_model("models/p_glock18_sil.mdl"), Some(1));
        assert_eq!(w.id_from_p_model("models/p_uzi_akimbo_sil.mdl"), Some(16));
        assert_eq!(w.id_from_p_model("models/p_PDW_sil.mdl"), Some(19), "the shipped spelling of the MP7's silenced model");
        assert_eq!(w.id_from_p_model("models/p_contender.mdl"), Some(36));
        assert_eq!(w.id_from_p_model(""), Some(KUNG_FU));
        assert_eq!(Weapons::new(true).id_from_p_model(""), Some(LEGACY_KUNG_FU));
        assert_eq!(w.id_from_p_model("models/v_ak47.mdl"), None);
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
