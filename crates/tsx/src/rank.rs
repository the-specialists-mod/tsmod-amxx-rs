// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports `RankSystem`
// (modules/ts/tsx/CRank.cpp, CRank.h), Copyright (C) The AMX Mod X Development Team, Copyright
// (C) 2004 Lukasz Wlasinski.

//! The persistent rank (`tsstats.dat`).
//!
//! Upstream keeps a doubly linked list from the best entry (`head`, position 1) to the worst
//! (`tail`); here it is a vector in the same order plus stable keys, so a player's handle stays
//! valid while entries move and simply stops resolving after [`RankSystem::clear`] (upstream's
//! pointer would dangle).
//!
//! File format (version 5, little endian), as `RankSystem::saveRank` writes it, best first:
//! `i16 version`, then per entry `i16 namelen, name\0, i16 uniquelen, unique\0, i32 kills,
//! deaths, hs, tks, damage, hits, shots, i32 bodyhits[8]`, then `i16 0`. Upstream's loader reads
//! the seven counters in another order (tks, damage, deaths, kills, shots, hits, hs), so every
//! load scrambled them; this one reads them in the order they are written.

use crate::stats::Stats;
use std::io::{self, Read, Write};

/// `RANK_VERSION`.
pub const RANK_VERSION: i16 = 5;

/// A stable handle to an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RankKey(u64);

/// `RankSystem::RankStats`.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    key: RankKey,
    pub stats: Stats,
    pub name: String,
    pub unique: String,
    pub score: i32,
}

/// The score of an entry: upstream calls the `get_score` public of `tsstats.amxx` when it is
/// loaded, else `kills - deaths`.
pub type ScoreFn = Box<dyn FnMut(&Stats) -> i32>;

/// `RankSystem`.
#[derive(Default)]
pub struct RankSystem {
    /// Best first.
    order: Vec<Entry>,
    next_key: u64,
    calc: Option<ScoreFn>,
}

impl RankSystem {
    /// Number of entries (`getRankNum`).
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Sets (or removes) the score function.
    pub fn set_calc(&mut self, calc: Option<ScoreFn>) {
        self.calc = calc;
    }

    /// Entries best first.
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.order.iter()
    }

    fn index(&self, key: RankKey) -> Option<usize> {
        self.order.iter().position(|e| e.key == key)
    }

    /// The entry of `key`.
    pub fn get(&self, key: RankKey) -> Option<&Entry> {
        self.index(key).map(|i| &self.order[i])
    }

    /// 1-based position of `key` (`getPosition`).
    pub fn position(&self, key: RankKey) -> Option<usize> {
        self.index(key).map(|i| i + 1)
    }

    /// The entry at 1-based `position`.
    pub fn at(&self, position: usize) -> Option<&Entry> {
        position.checked_sub(1).and_then(|i| self.order.get(i))
    }

    /// `findEntryInRank`: the entry for `unique`, created at the bottom if there is none. With
    /// `ip`, a stored unique also matches when it is `unique` followed by `:port`, as older
    /// files kept the port. (Upstream's IP search never advances past a non-matching entry and
    /// hangs; this one walks the list.)
    pub fn find_or_add(&mut self, unique: &str, name: &str, ip: bool) -> RankKey {
        let hit = self.order.iter().find(|e| {
            if ip { e.unique.strip_prefix(unique).is_some_and(|rest| rest.is_empty() || rest.starts_with(':')) } else { e.unique == unique }
        });
        if let Some(e) = hit {
            return e.key;
        }
        let key = RankKey(self.next_key);
        self.next_key += 1;
        self.order.push(Entry { key, stats: Stats::default(), name: name.into(), unique: unique.into(), score: 0 });
        key
    }

    /// `setName`.
    pub fn set_name(&mut self, key: RankKey, name: &str) {
        if let Some(i) = self.index(key) {
            self.order[i].name = name.into();
        }
    }

    /// `updatePos`: adds `s`, rescores the entry and moves it. It moves up past every better
    /// entry whose score is not above its own (so the latest updater wins a tie), otherwise down
    /// past every worse entry that scores more.
    pub fn update(&mut self, key: RankKey, s: &Stats) {
        let Some(mut i) = self.index(key) else { return };
        self.order[i].stats.commit(s);
        let st = self.order[i].stats;
        let score = match self.calc.as_mut() {
            Some(f) => f(&st),
            None => st.kills - st.deaths,
        };
        self.order[i].score = score;
        let mut up = i;
        while up > 0 && self.order[up - 1].score <= score {
            up -= 1;
        }
        if up != i {
            let e = self.order.remove(i);
            self.order.insert(up, e);
            return;
        }
        while i + 1 < self.order.len() && self.order[i + 1].score > score {
            self.order.swap(i, i + 1);
            i += 1;
        }
    }

    /// `clear`.
    pub fn clear(&mut self) {
        self.order.clear();
    }

    /// `saveRank` to a writer.
    pub fn save(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(&RANK_VERSION.to_le_bytes())?;
        for e in &self.order {
            // The C++ compares with `1 << 31`: entries scored `INT_MIN` are dropped.
            if e.score == i32::MIN {
                continue;
            }
            for s in [&e.name, &e.unique] {
                let mut b = s.as_bytes().to_vec();
                b.truncate(i16::MAX as usize - 1);
                b.push(0);
                w.write_all(&(b.len() as i16).to_le_bytes())?;
                w.write_all(&b)?;
            }
            let st = &e.stats;
            for v in [st.kills, st.deaths, st.hs, st.tks, st.damage, st.hits, st.shots] {
                w.write_all(&v.to_le_bytes())?;
            }
            for v in st.body_hits {
                w.write_all(&v.to_le_bytes())?;
            }
        }
        w.write_all(&0i16.to_le_bytes())
    }

    /// `loadRank` from a reader: each entry is found or added by its unique and its counters are
    /// added with [`update`](Self::update). A short or foreign file loads what it can, like
    /// upstream.
    pub fn load(&mut self, r: &mut impl Read) {
        fn short(r: &mut impl Read) -> Option<i16> {
            let mut b = [0; 2];
            r.read_exact(&mut b).ok()?;
            Some(i16::from_le_bytes(b))
        }
        fn int(r: &mut impl Read) -> Option<i32> {
            let mut b = [0; 4];
            r.read_exact(&mut b).ok()?;
            Some(i32::from_le_bytes(b))
        }
        fn text(r: &mut impl Read, n: i16) -> Option<String> {
            let mut b = vec![0; usize::try_from(n).ok()?];
            r.read_exact(&mut b).ok()?;
            let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
            Some(String::from_utf8_lossy(&b[..end]).into_owned())
        }
        let entry = |r: &mut _, namelen: i16| -> Option<(String, String, Stats)> {
            let name = text(r, namelen)?;
            let n = short(r)?;
            let unique = text(r, n)?;
            let mut d = Stats::default();
            for f in [&mut d.kills, &mut d.deaths, &mut d.hs, &mut d.tks, &mut d.damage, &mut d.hits, &mut d.shots] {
                *f = int(r)?;
            }
            for b in &mut d.body_hits {
                *b = int(r)?;
            }
            Some((name, unique, d))
        };
        if short(r) != Some(RANK_VERSION) {
            return;
        }
        let mut n = short(r).unwrap_or(0);
        while n > 0 {
            let Some((name, unique, d)) = entry(r, n) else { break };
            let k = self.find_or_add(&unique, &name, false);
            self.update(k, &d);
            n = short(r).unwrap_or(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(kills: i32, deaths: i32) -> Stats {
        Stats { kills, deaths, hs: 1, tks: 2, damage: 300, hits: 9, shots: 40, body_hits: [0, 1, 2, 3, 4, 5, 6, 7] }
    }

    /// A file as upstream's `saveRank` writes it, built byte by byte from CRank.cpp's `fwrite`s.
    fn upstream_file(entries: &[(&str, &str, Stats)]) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend(5i16.to_le_bytes());
        for (name, unique, s) in entries {
            for t in [name, unique] {
                f.extend(((t.len() + 1) as i16).to_le_bytes());
                f.extend(t.as_bytes());
                f.push(0);
            }
            for v in [s.kills, s.deaths, s.hs, s.tks, s.damage, s.hits, s.shots] {
                f.extend(v.to_le_bytes());
            }
            for v in s.body_hits {
                f.extend(v.to_le_bytes());
            }
        }
        f.extend(0i16.to_le_bytes());
        f
    }

    #[test]
    fn loads_an_upstream_file_and_saves_it_byte_for_byte() {
        let file = upstream_file(&[("Alice", "STEAM_0:0:1", stats(10, 2)), ("Bob", "STEAM_0:0:2", stats(5, 5))]);
        let mut r = RankSystem::default();
        r.load(&mut &file[..]);
        assert_eq!(r.len(), 2);
        let a = r.at(1).unwrap();
        assert_eq!((a.name.as_str(), a.unique.as_str(), a.score), ("Alice", "STEAM_0:0:1", 8));
        // Body part 0 isn't kept (`Stats::commit` starts at 1).
        assert_eq!(a.stats, Stats { body_hits: [0, 1, 2, 3, 4, 5, 6, 7], ..stats(10, 2) });
        let mut out = Vec::new();
        r.save(&mut out).unwrap();
        assert_eq!(out, file);
    }

    #[test]
    fn round_trip_keeps_order_and_counters() {
        let mut r = RankSystem::default();
        for (i, k) in [3, 7, 1].iter().enumerate() {
            let key = r.find_or_add(&format!("u{i}"), &format!("n{i}"), false);
            r.update(key, &stats(*k, 0));
        }
        let mut out = Vec::new();
        r.save(&mut out).unwrap();
        let mut back = RankSystem::default();
        back.load(&mut &out[..]);
        let names: Vec<_> = back.iter().map(|e| (e.name.clone(), e.score)).collect();
        assert_eq!(names, [("n1".to_owned(), 7), ("n0".to_owned(), 3), ("n2".to_owned(), 1)]);
    }

    #[test]
    fn positions_follow_score_with_latest_winning_ties() {
        let mut r = RankSystem::default();
        let a = r.find_or_add("a", "a", false);
        let b = r.find_or_add("b", "b", false);
        assert_eq!((r.position(a), r.position(b)), (Some(1), Some(2)));
        r.update(b, &Stats { kills: 1, ..Default::default() });
        assert_eq!(r.position(b), Some(1));
        r.update(a, &Stats { kills: 1, ..Default::default() });
        assert_eq!(r.position(a), Some(1), "equal score: the updated entry moves up");
        r.update(a, &Stats { deaths: 3, ..Default::default() });
        assert_eq!((r.position(a), r.position(b)), (Some(2), Some(1)), "moves down past better entries");
    }

    #[test]
    fn ip_uniques_match_with_or_without_a_stored_port() {
        let mut r = RankSystem::default();
        let old = r.find_or_add("4.2.2.24:27005", "x", false);
        let other = r.find_or_add("4.2.2.2:27005", "y", false);
        assert_eq!(r.find_or_add("4.2.2.2", "y", true), other, "4.2.2.2 doesn't match 4.2.2.24");
        assert_eq!(r.find_or_add("4.2.2.24", "x", true), old);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn score_function_and_min_score_entries() {
        let mut r = RankSystem::default();
        r.set_calc(Some(Box::new(|s: &Stats| if s.kills == 0 { i32::MIN } else { s.kills * 10 })));
        let a = r.find_or_add("a", "a", false);
        r.update(a, &Stats { kills: 2, ..Default::default() });
        let b = r.find_or_add("b", "b", false);
        r.update(b, &Stats::default());
        assert_eq!(r.get(a).unwrap().score, 20);
        let mut out = Vec::new();
        r.save(&mut out).unwrap();
        let mut back = RankSystem::default();
        back.load(&mut &out[..]);
        assert_eq!(back.len(), 1, "INT_MIN scores aren't saved");
    }

    #[test]
    fn foreign_or_truncated_files_load_what_they_can() {
        let mut r = RankSystem::default();
        r.load(&mut &4i16.to_le_bytes()[..]);
        assert!(r.is_empty());
        let file = upstream_file(&[("A", "a", stats(1, 0)), ("B", "b", stats(2, 0))]);
        r.load(&mut &file[..file.len() - 10]);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn handles_stop_resolving_after_clear() {
        let mut r = RankSystem::default();
        let a = r.find_or_add("a", "a", false);
        r.clear();
        assert!(r.get(a).is_none());
        r.update(a, &Stats::default());
        assert!(r.is_empty());
    }
}
