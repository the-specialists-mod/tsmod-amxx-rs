// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports
// modules/ts/tsx/usermsg.cpp and the message hooks of moduleconfig.cpp, Copyright (C) The AMX
// Mod X Development Team, Copyright (C) 2004 Lukasz Wlasinski.

//! The user messages tsx reads, as the game writes them (`MESSAGE_BEGIN`, `WRITE_*`,
//! `MESSAGE_END` pass through metamod's post hooks).
//!
//! Upstream handled each `WRITE_*` as it came; this collects a message's values and handles them
//! at `MESSAGE_END`, which reads the same fields. Layouts are TS 3.0's, as the Rust server
//! writes them (`the-specialistsmod-rs` `crates/shared/src/protocol`), checked in this module's
//! tests against the port's own encoders:
//!
//! | Message | Payload | Read |
//! |---|---|---|
//! | `ResetHUD` | byte 0 | at the end: spawn or death bookkeeping |
//! | `WeaponInfo` | byte id, byte clip, short reserve, byte mode, byte attachments | all (id 0 = kung fu: 38, or 36 with `tsx_legacy_weaponids`) |
//! | `ClipInfo` | byte clip | a drop counts a shot (TS 3.0 never sends it) |
//! | `ScoreInfo` | byte player, short frags, short deaths, short class, short team | player, team |
//! | `TSHealth` | byte health | at the end: damage and death |
//! | `TSState` | byte stance | stance |
//! | `WStatus` | byte id, byte owned, byte clip | owned 0: back to kung fu |
//! | `TSCash` | short money | money |
//! | `TSSpace` | byte free slots | slots |
//! | `PwUp` | short power-up bits, byte seconds | permanent kung fu / super jump, or the held one |
//! | `TSPAward` | byte player, byte award | award (port addition: upstream read `CBasePlayer`) |

use crate::game::Game;
use crate::player::{ITEM_KUNGFU, ITEM_SUPERJUMP};
use crate::state::{Hit, State, slot};

/// `TSPWUP_KUNGFU`, `TSPWUP_SJUMP`.
const PWUP_KUNGFU: i32 = 4;
const PWUP_SJUMP: i32 = 256;

/// A message tsx reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    ResetHud,
    WeaponInfo,
    ClipInfo,
    ScoreInfo,
    TsHealth,
    TsState,
    WStatus,
    TsCash,
    TsSpace,
    PwUp,
    TsPAward,
}

impl Kind {
    /// Every kind with its registered name.
    pub const ALL: [(Kind, &'static str); 11] = [
        (Kind::ResetHud, "ResetHUD"),
        (Kind::WeaponInfo, "WeaponInfo"),
        (Kind::ClipInfo, "ClipInfo"),
        (Kind::ScoreInfo, "ScoreInfo"),
        (Kind::TsHealth, "TSHealth"),
        (Kind::TsState, "TSState"),
        (Kind::WStatus, "WStatus"),
        (Kind::TsCash, "TSCash"),
        (Kind::TsSpace, "TSSpace"),
        (Kind::PwUp, "PwUp"),
        (Kind::TsPAward, "TSPAward"),
    ];

    pub fn from_name(name: &str) -> Option<Kind> {
        Self::ALL.iter().find(|(_, n)| *n == name).map(|&(k, _)| k)
    }
}

/// Message ids and the message being written.
#[derive(Default, Debug)]
pub struct Messages {
    ids: Vec<(i32, Kind)>,
    current: Option<(Kind, Option<usize>)>,
    args: Vec<i32>,
}

/// What a finished message asks the caller to do once the state is released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Followup {
    None,
    /// Run `client_damage`, then [`State::after_damage`].
    Damage(Hit),
}

impl Messages {
    /// `RegUserMsg_Post` (or metamod's id lookup for a module loaded after the game registered
    /// its messages): the first id registered for a name wins, as upstream.
    pub fn register(&mut self, name: &str, id: i32) {
        if let Some(k) = Kind::from_name(name)
            && !self.ids.iter().any(|&(_, kk)| kk == k)
        {
            self.ids.push((id, k));
        }
    }

    /// Whether `kind` has an id yet.
    pub fn has(&self, kind: Kind) -> bool {
        self.ids.iter().any(|&(_, k)| k == kind)
    }
}

impl State {
    /// `MessageBegin_Post`: `player` is the receiving entity (`ed`), `None` for broadcasts.
    pub fn message_begin(&mut self, msg_type: i32, player: Option<i32>) {
        let kind = self.msgs.ids.iter().find(|&&(id, _)| id == msg_type).map(|&(_, k)| k);
        let p = player.and_then(|e| slot(self, e));
        self.msgs.current = kind.map(|k| (k, p));
        self.msgs.args.clear();
    }

    /// `Write{Byte,Char,Short,Long,Entity}_Post`.
    pub fn message_int(&mut self, v: i32) {
        if self.msgs.current.is_some() {
            self.msgs.args.push(v);
        }
    }

    /// `WriteAngle_Post`, `WriteCoord_Post`: kept as the bits of the float, like upstream's
    /// `*(int*)&value`.
    pub fn message_float(&mut self, v: f32) {
        self.message_int(v.to_bits() as i32);
    }

    /// `WriteString_Post`: none of the messages read carries a string; a placeholder keeps the
    /// positions of the values after it.
    pub fn message_string(&mut self) {
        self.message_int(0);
    }

    /// `MessageEnd_Post`.
    pub fn message_end(&mut self, g: &dyn Game) -> Followup {
        let Some((kind, player)) = self.msgs.current.take() else { return Followup::None };
        let a = std::mem::take(&mut self.msgs.args);
        let arg = |i: usize| a.get(i).copied();
        // Broadcasts carry the player in the payload; the rest need the receiving player.
        match kind {
            Kind::ScoreInfo => {
                if let (Some(id), Some(team)) = (arg(0), arg(4))
                    && let Some(i) = slot(self, id)
                {
                    self.players[i].team_id = team;
                }
                return Followup::None;
            }
            Kind::TsPAward => {
                if let (Some(id), Some(award)) = (arg(0), arg(1))
                    && let Some(i) = slot(self, id)
                {
                    self.players[i].award = Some(award);
                }
                return Followup::None;
            }
            _ => {}
        }
        let Some(i) = player else { return Followup::None };
        match kind {
            Kind::ResetHud => self.reset_hud(g, i),
            Kind::WeaponInfo => {
                let Some(id) = arg(0) else { return Followup::None };
                let Some(w) = self.weapons.from_wire(id) else { return Followup::None };
                let p = &mut self.players[i];
                p.current = w;
                let pw = &mut p.weapons[w];
                pw.clip = arg(1).unwrap_or(pw.clip);
                pw.ammo = arg(2).unwrap_or(pw.ammo);
                pw.mode = arg(3).unwrap_or(pw.mode);
                pw.attach = arg(4).unwrap_or(pw.attach);
            }
            Kind::ClipInfo => {
                let Some(clip) = arg(0) else { return Followup::None };
                let w = self.players[i].current;
                if clip < self.players[i].weapons[w].clip && !crate::state::ignore_bots(self, g, i, None) {
                    self.players[i].save_shot(w);
                }
                self.players[i].weapons[w].clip = clip;
            }
            Kind::TsHealth => {
                return self.ts_health(g, i).map_or(Followup::None, Followup::Damage);
            }
            Kind::TsState => {
                if let Some(s) = arg(0) {
                    let p = &mut self.players[i];
                    p.oldstate = p.state;
                    p.checkstate = true;
                    p.state = s;
                }
            }
            Kind::WStatus => {
                // A weapon lost (thrown, or all of them at death): upstream's "fix for a dropped
                // weapon" puts the player back on kung fu.
                if arg(1) == Some(0) {
                    self.players[i].current = self.weapons.kung_fu();
                }
            }
            Kind::TsCash => self.players[i].money = arg(0).unwrap_or(self.players[i].money),
            Kind::TsSpace => self.players[i].space = arg(0).unwrap_or(self.players[i].space),
            Kind::PwUp => {
                let p = &mut self.players[i];
                match arg(0) {
                    Some(PWUP_KUNGFU) => p.items |= ITEM_KUNGFU,
                    Some(PWUP_SJUMP) => p.items |= ITEM_SUPERJUMP,
                    Some(t) => {
                        p.pwup = t;
                        p.pwup_value = arg(1).unwrap_or(0);
                    }
                    None => {}
                }
            }
            Kind::ScoreInfo | Kind::TsPAward => {}
        }
        Followup::None
    }

    /// `Client_ResetHUD_End`: alive, it is a spawn, so the life stats go to the rank shortly;
    /// dead, the per-life counters reset (and the frag count resyncs after `user_kill`, self
    /// kills and `addfrag`).
    fn reset_hud(&mut self, g: &dyn Game, i: usize) {
        let ent = i as i32;
        let p = &mut self.players[i];
        if g.is_alive(ent) {
            p.clear_stats = g.time() + 0.25;
        } else {
            p.items = 0;
            p.is_specialist = false;
            p.killing_spree = 0;
            p.kill_flags = 0;
            p.frags = g.frags(ent);
        }
    }
}
