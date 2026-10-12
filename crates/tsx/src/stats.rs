// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports `Stats` and the
// `CPlayer` stats bookkeeping (modules/ts/tsx/CRank.h, CMisc.cpp), Copyright (C) The AMX Mod X
// Development Team, Copyright (C) 2004 Lukasz Wlasinski.

//! Hit/kill counters.

use amxx::Cell;

/// `struct Stats`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub hits: i32,
    pub shots: i32,
    pub damage: i32,
    pub hs: i32,
    pub tks: i32,
    pub kills: i32,
    pub deaths: i32,
    /// Hits per body part (`HIT_*`, 1 to 7; 0 is "generic" and isn't kept in the rank).
    pub body_hits: [i32; 8],
}

impl Stats {
    /// `Stats::commit`: adds `a` (body part 0 excluded, as upstream).
    pub fn commit(&mut self, a: &Stats) {
        self.hits += a.hits;
        self.shots += a.shots;
        self.damage += a.damage;
        self.hs += a.hs;
        self.tks += a.tks;
        self.kills += a.kills;
        self.deaths += a.deaths;
        for i in 1..8 {
            self.body_hits[i] += a.body_hits[i];
        }
    }

    /// The first seven cells of a natives' `stats[8]` (`STATSX_*` order): kills, deaths,
    /// headshots, team kills, shots, hits, damage.
    pub fn cells(&self) -> [Cell; 7] {
        [self.kills, self.deaths, self.hs, self.tks, self.shots, self.hits, self.damage]
    }

    /// `bodyhits[1..8]` as natives write them (index 0 untouched).
    pub fn body(&self) -> &[i32] {
        &self.body_hits[1..]
    }

    pub(crate) fn hit(&mut self, damage: i32, body: usize) {
        self.hits += 1;
        self.damage += damage;
        if let Some(b) = self.body_hits.get_mut(body) {
            *b += 1;
        }
    }

    pub(crate) fn kill(&mut self, hs: i32, tk: i32) {
        self.kills += 1;
        self.hs += hs;
        self.tks += tk;
    }

    pub(crate) fn victim(&mut self, hs: i32, tk: i32) {
        self.deaths += 1;
        self.hs += hs;
        self.tks += tk;
    }
}
