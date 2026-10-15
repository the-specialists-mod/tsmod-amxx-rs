// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Replaces the frag model of
// modules/ts/tsx/usermsg.cpp (`Client_TSHealth_End`), Copyright (C) The AMX Mod X Development
// Team, Copyright (C) 2004 Lukasz Wlasinski.

//! TS 3.0's frags for a kill, as tsx predicts them (port addition).
//!
//! Upstream's `Client_TSHealth_End` predicted TS 2.x's frags: a per-weapon bonus (katana 2, kung
//! fu 3), +2 for a stunt (`iuser4` 20/1028/2052), x2 within 1 s of the last kill, x2 from a
//! 10-kill spree on, +5 for killing a specialist. TS 3.0 scores differently, so this models
//! `TSGetPointsForFrag` (0x89140) and `CHalfLifeMultiplay::PlayerKilled` (0x895e4) as the Rust
//! server ports them (`the-specialistsmod-rs` `crates/server/src/rules/scoring.rs`, decomp
//! `docs/server/gamerules-entities.md` §2.3):
//!
//! * 1 point; 2 for a kung-fu frag (the killer its own inflictor, no simulated bullet, kung fu
//!   out or `glast_closecombat`) or a thrown knife. Any other inflictor (a grenade) scores 1
//!   with no stunt test at all.
//! * +1 for a stunt (`iuser4 & 0x10c10`, faster than 80 u/s: the "diving" tag) or else a slide
//!   (`iuser4 & 0x20`, `friction < 1`, faster than 80: "sliding"), except under
//!   `glast_closecombat`, which keeps the tag but not the point.
//! * Then, for a kill worth more than 0: x2 for a double kill (under 2 s since the killer's last
//!   scoring kill, which a double kill sets to 0, so a third quick kill doesn't double), +5 when
//!   the victim's spree was over 8, x2 when the killer's spree is 10 or more with this kill.
//!
//! The killer's spree counts every kill of another player, team kills and 0-point kills too.

/// `iuser4` bits that count as stunting for `TSGetPointsForFrag`: diving, the flips, the wall
/// jump (the port's `STUNTING`).
pub const STUNTING: i32 = 0x10c10;
/// `iuser4` bit of lying prone after a dive or a slide.
pub const PRONE: i32 = 0x20;
/// `PlayerKilled`'s double-kill window, seconds since the killer's last scoring kill.
pub const DOUBLE_KILL_SECS: f32 = 2.0;
/// A spree above this makes a Specialist (spree 9, `is_specialist`); killing one is worth +5.
pub const SPECIALIST_SPREE: i32 = 8;
/// From this spree on (`is_unstoppable`) a kill's points double.
pub const UNSTOPPABLE_SPREE: i32 = 10;
/// The speed above which a stunt or a slide counts.
const STUNT_SPEED: f32 = 80.0;

/// What dealt a kill, as `TSGetPointsForFrag` tells kills apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blow {
    /// The killer was its own inflictor: a bullet, a slash, a kick. `kung_fu` is TS's kung-fu frag;
    /// `close_combat` is `glast_closecombat` (no stunt point).
    Own { kung_fu: bool, close_combat: bool },
    /// A `knife` entity the killer threw.
    ThrownKnife,
    /// Any other inflictor (a grenade).
    Other,
}

impl Blow {
    /// The blow of a kill by its own inflictor from TS's `DeathMsg` weapon (`PlayerKilled`'s
    /// `own_weapon_name`): "Kung Fu" only under `glast_closecombat`, "world" for kung fu outside
    /// one, else the weapon. Without the message, the weapon in hand decides.
    pub fn own(death_msg_weapon: Option<&str>, kung_fu_in_hand: bool) -> Blow {
        match death_msg_weapon {
            Some("Kung Fu") => Blow::Own { kung_fu: true, close_combat: true },
            Some("world") => Blow::Own { kung_fu: true, close_combat: false },
            Some(_) => Blow::Own { kung_fu: false, close_combat: false },
            None => Blow::Own { kung_fu: kung_fu_in_hand, close_combat: false },
        }
    }

    /// Whether TS scored this as kung fu.
    pub fn kung_fu(self) -> bool {
        matches!(self, Blow::Own { kung_fu: true, .. })
    }
}

/// `TSGetPointsForFrag`'s stunt test on the killer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stunt {
    /// The ` (diving)` tag: stunting (`iuser4 & 0x10c10`) and fast.
    pub diving: bool,
    /// The ` (sliding)` tag: otherwise prone, `friction < 1` and fast.
    pub sliding: bool,
}

impl Stunt {
    /// The test on the killer's `pev->iuser4`, `velocity` and `friction`.
    pub fn of(iuser4: i32, velocity: [f32; 3], friction: f32) -> Stunt {
        let [x, y, z] = velocity;
        let fast = (x * x + y * y + z * z).sqrt() > STUNT_SPEED;
        let diving = iuser4 & STUNTING != 0 && fast;
        let sliding = !diving && iuser4 & PRONE != 0 && friction < 1.0 && fast;
        Stunt { diving, sliding }
    }
}

/// `TSGetPointsForFrag` (0x89140) for a kill of another player outside a team kill and The One
/// mode's 0: the points and the stunt tags (none for [`Blow::Other`], which returns first).
pub fn points_for_frag(blow: Blow, stunt: Stunt) -> (i32, Stunt) {
    let (base, close) = match blow {
        Blow::Other => return (1, Stunt::default()),
        Blow::ThrownKnife => (2, false),
        Blow::Own { kung_fu, close_combat } => (if kung_fu { 2 } else { 1 }, close_combat),
    };
    let bonus = (stunt.diving || stunt.sliding) && !close;
    (base + bonus as i32, stunt)
}

/// The rest of `PlayerKilled` (0x895e4) on [`points_for_frag`]'s `points`: the double kill, the
/// killed specialist (`victim_spree` before the death) and the unstoppable spree
/// (`killer_spree` counting this kill). Nothing changes a kill worth 0 or less.
pub fn player_killed_points(points: i32, double_kill: bool, victim_spree: i32, killer_spree: i32) -> i32 {
    if points <= 0 {
        return points;
    }
    let mut p = points;
    if double_kill {
        p *= 2;
    }
    if victim_spree > SPECIALIST_SPREE {
        p += 5;
    }
    if killer_spree >= UNSTOPPABLE_SPREE {
        p *= 2;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    const STILL: Stunt = Stunt { diving: false, sliding: false };
    const DIVE: Stunt = Stunt { diving: true, sliding: false };
    const GUN: Blow = Blow::Own { kung_fu: false, close_combat: false };

    #[test]
    fn base_points_follow_the_blow_not_the_weapon_table() {
        // A katana or knife slash is no kung-fu frag: 1, as a gun.
        assert_eq!(points_for_frag(Blow::own(Some("Katana"), false), STILL).0, 1);
        assert_eq!(points_for_frag(GUN, STILL).0, 1);
        assert_eq!(points_for_frag(Blow::own(Some("Kung Fu"), false), STILL).0, 2);
        assert_eq!(points_for_frag(Blow::own(Some("world"), false), STILL).0, 2);
        assert_eq!(points_for_frag(Blow::own(None, true), STILL).0, 2, "kung fu in hand, no DeathMsg");
        assert_eq!(points_for_frag(Blow::ThrownKnife, STILL).0, 2);
        assert_eq!(points_for_frag(Blow::Other, STILL).0, 1);
    }

    #[test]
    fn a_stunt_is_one_point_and_none_in_close_combat_or_for_a_grenade() {
        assert_eq!(points_for_frag(GUN, DIVE), (2, DIVE));
        assert_eq!(points_for_frag(Blow::ThrownKnife, DIVE), (3, DIVE));
        assert_eq!(points_for_frag(Blow::own(Some("world"), false), DIVE), (3, DIVE));
        // glast_closecombat keeps the tag, not the point.
        assert_eq!(points_for_frag(Blow::own(Some("Kung Fu"), false), DIVE), (2, DIVE));
        assert_eq!(points_for_frag(Blow::Other, DIVE), (1, STILL));
    }

    #[test]
    fn the_stunt_test_needs_speed() {
        let fast = [60.0, 60.0, 0.0];
        assert_eq!(Stunt::of(0x10, fast, 1.0), DIVE);
        assert_eq!(Stunt::of(0x400 | 0x20, fast, 0.5), DIVE, "a flip wins over a slide");
        assert_eq!(Stunt::of(0x10000, fast, 1.0), DIVE, "wall jump");
        assert_eq!(Stunt::of(0x10, [50.0, 60.0, 0.0], 1.0), STILL, "78 u/s");
        assert_eq!(Stunt::of(0x20, fast, 0.5), Stunt { diving: false, sliding: true });
        assert_eq!(Stunt::of(0x20, fast, 1.0), STILL, "prone without the slide's friction");
        // TS 2.x's values upstream tested for: 20 has 0x10, 36 has 0x20.
        assert_eq!(Stunt::of(20, [0.0; 3], 1.0), STILL);
    }

    #[test]
    fn double_kills_specialists_and_sprees() {
        assert_eq!(player_killed_points(1, false, 0, 1), 1);
        assert_eq!(player_killed_points(1, true, 0, 2), 2);
        assert_eq!(player_killed_points(2, false, 9, 1), 7, "killed a specialist");
        assert_eq!(player_killed_points(2, false, 8, 1), 2, "spree 8 is no specialist");
        assert_eq!(player_killed_points(1, false, 0, 9), 1, "spree 9: specialist, no x2");
        assert_eq!(player_killed_points(1, false, 0, 10), 2, "unstoppable");
        // The order: x2 double, +5, then x2 unstoppable.
        assert_eq!(player_killed_points(2, true, 9, 10), 18);
        assert_eq!(player_killed_points(0, true, 9, 10), 0, "The One mode's 0 stays 0");
        assert_eq!(player_killed_points(-1, true, 9, 10), -1, "team kill");
    }
}
