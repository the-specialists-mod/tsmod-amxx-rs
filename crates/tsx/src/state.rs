// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports the hook bodies of
// modules/ts/tsx/moduleconfig.cpp and CMisc.cpp (`PutInServer`, `Disconnect`), Copyright (C)
// The AMX Mod X Development Team, Copyright (C) 2004 Lukasz Wlasinski.

//! The module's state and what the engine and game hooks do to it.
//!
//! The state lives in a thread-local `RefCell` (the engine calls everything on one thread).
//! Plugins run inside forwards and inside game calls like `ts_giveweapon`'s `Use`, and they may
//! call this module's natives, so no borrow is held across such a call: hooks and natives take
//! the state in short [`with`] sections and run forwards and game calls outside them.

use crate::forwards::KillReport;
use crate::game::{Event, Game};
use crate::msgs::Messages;
use crate::player::{self, Player};
use crate::rank::RankSystem;
use crate::scoring::{self, Blow, Stunt};
use crate::weapons::{LEGACY_CVAR, Weapons};
use std::cell::RefCell;

/// Everything the module keeps.
pub struct State {
    /// Index 0 is unused (entity 0 is the world), as upstream's `players[33]`.
    pub players: Vec<Player>,
    pub weapons: Weapons,
    pub rank: RankSystem,
    pub msgs: Messages,
    /// `mp_theonemode` at map start (every non-One player is on one side).
    pub theone: bool,
    /// `tsstats_rankbots` at map start.
    pub rank_bots: bool,
    /// `ts_setpddata`'s knife offset: stored and unused (the API gives the knife's owner).
    pub knife_offset: i32,
    /// Indices of the weapon-firing events (`events/fire.sc`, `events/firelinked.sc`).
    pub fire_events: Vec<u16>,
    /// Natives that already logged an API problem (no API, a rejected value).
    pub api_warned: Vec<String>,
    /// The kung-fu move of the `Melee_Attack` forward running now (port addition,
    /// `crate::forwards`).
    pub melee: Option<crate::forwards::MeleeAttack>,
    /// The server reports each death it scores (plugin API version 5, `kill_scored`; port
    /// addition): the death half of `TSHealth` waits for the report instead of predicting.
    pub kill_reports: bool,
    /// Pawn bot backends and the bots' last commands (port addition, `crate::bots`).
    pub bots: crate::bots::Bots,
}

impl Default for State {
    fn default() -> Self {
        State {
            players: (0..33).map(Player::new).collect(),
            weapons: Weapons::default(),
            rank: RankSystem::default(),
            msgs: Messages::default(),
            theone: false,
            rank_bots: true,
            knife_offset: 0,
            fire_events: Vec::new(),
            api_warned: Vec::new(),
            melee: None,
            kill_reports: false,
            bots: crate::bots::Bots::default(),
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Runs `f` on the state. `None` if the state is already borrowed further up the stack (a
/// plugin re-entering through a path that should have released it); callers treat that as
/// "no data".
pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.try_borrow_mut().ok().map(|mut s| f(&mut s)))
}

/// `ignoreBots`: stats skip bots unless `tsstats_rankbots` is on.
pub fn ignore_bots(s: &State, g: &dyn Game, a: usize, b: Option<usize>) -> bool {
    !s.rank_bots && (g.is_fake_client(a as i32) || b.is_some_and(|b| g.is_fake_client(b as i32)))
}

/// `isModuleActive`: `tsstats_pause` is 0.
pub fn active(g: &dyn Game) -> bool {
    g.cvar("tsstats_pause") as i32 == 0
}

/// A player index from an entity index, if it is a player slot.
pub fn slot(s: &State, ent: i32) -> Option<usize> {
    let i = usize::try_from(ent).ok()?;
    (i >= 1 && i < s.players.len()).then_some(i)
}

/// What the victim's `dmg_inflictor` was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inflictor {
    /// The attacking player itself.
    Player,
    /// A `grenade` (its owner the attacker).
    Grenade,
    /// A thrown `knife`.
    Knife,
    /// Anything else: self-damage.
    Other,
}

/// Damage the `TSHealth` handler found, between the `client_damage` forward and the death
/// bookkeeping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub attacker: usize,
    pub victim: usize,
    pub damage: i32,
    pub weapon: usize,
    pub aim: i32,
    pub ta: i32,
    pub by: Inflictor,
}

impl Hit {
    pub fn event(&self) -> Event {
        Event::Damage {
            attacker: self.attacker as i32,
            victim: self.victim as i32,
            damage: self.damage,
            weapon: self.weapon as i32,
            aim: self.aim,
            ta: self.ta,
        }
    }
}

impl State {
    /// `ServerActivate_Post`.
    pub fn server_activate(&mut self, g: &dyn Game) {
        self.theone = g.cvar("mp_theonemode") as i32 != 0;
        self.rank_bots = g.cvar("tsstats_rankbots") as i32 != 0;
        for p in self.players.iter_mut().skip(1) {
            p.init();
        }
        self.sync_weapon_ids(g);
    }

    /// Port addition: follows `tsx_legacy_weaponids` (see [`crate::weapons`]). Read when a map
    /// starts and when a player enters the game, so a value from `server.cfg` or `amxx.cfg`
    /// (executed after `ServerActivate`) holds from the first player on. A player holding kung
    /// fu keeps holding it under its new id; recorded stats stay under the ids they were made with.
    pub fn sync_weapon_ids(&mut self, g: &dyn Game) {
        let legacy = g.cvar(LEGACY_CVAR) as i32 != 0;
        if legacy == self.weapons.legacy() {
            return;
        }
        let old = self.weapons.kung_fu();
        self.weapons.set_legacy(legacy);
        let new = self.weapons.kung_fu();
        for p in &mut self.players {
            if p.current == old {
                p.current = new;
            }
        }
    }

    /// `ClientConnect_Post`.
    pub fn client_connect(&mut self, ent: i32, address: &str) {
        if let Some(i) = slot(self, ent) {
            self.players[i].connect(address);
        }
    }

    /// `CPlayer::PutInServer` (`ClientPutInServer_Post`, and for bots `ClientUserInfoChanged`).
    pub fn put_in_server(&mut self, g: &dyn Game, ent: i32) {
        let Some(i) = slot(self, ent) else { return };
        self.sync_weapon_ids(g);
        let p = &mut self.players[i];
        p.restart_stats(true);
        p.ingame = true;
        p.killing_spree = 0;
        p.items = 0;
        p.last_frag = 0;
        p.last_kill = 0.0;
        p.is_specialist = false;
        p.frags = 0;
        if ignore_bots(self, g, i, None) {
            return;
        }
        let name = g.netname(ent);
        let (unique, ip) = match g.cvar("tsstats_rank") as i32 {
            1 => (g.auth_id(ent).unwrap_or_else(|| name.clone()), false),
            2 => (self.players[i].ip.clone(), true),
            _ => (name.clone(), false),
        };
        self.players[i].rank = Some(self.rank.find_or_add(&unique, &name, ip));
    }

    /// `CPlayer::Disconnect`: commits the life stats.
    fn disconnect_player(&mut self, g: &dyn Game, i: usize) {
        if !ignore_bots(self, g, i, None)
            && active(g)
            && let Some(k) = self.players[i].rank
        {
            let life = self.players[i].life;
            self.rank.update(k, &life);
        }
        // Upstream returned before this for ignored bots, leaving them "in game".
        self.players[i].ingame = false;
    }

    /// `ClientDisconnect`.
    pub fn client_disconnect(&mut self, g: &dyn Game, ent: i32) {
        if let Some(i) = slot(self, ent) {
            // Port addition: the slot's next bot has no last command yet.
            self.bots.results[i] = None;
            if self.players[i].ingame {
                self.disconnect_player(g, i);
            }
        }
    }

    /// `ClientUserInfoChanged_Post`: follows name changes (the rank is keyed by name with
    /// `tsstats_rank 0`), and puts bots in, which may skip `ClientPutInServer`.
    pub fn userinfo_changed(&mut self, g: &dyn Game, ent: i32, new_name: &str) {
        let Some(i) = slot(self, ent) else { return };
        let old = g.netname(ent);
        if self.players[i].ingame {
            if old != new_name {
                if g.cvar("tsstats_rank") as i32 == 0 {
                    self.players[i].rank = Some(self.rank.find_or_add(new_name, new_name, false));
                } else if let Some(k) = self.players[i].rank {
                    self.rank.set_name(k, new_name);
                }
            }
        } else if g.auth_id(ent).as_deref() == Some("BOT") {
            self.put_in_server(g, ent);
        }
    }

    /// `PlayerPreThink_Post`: a spawn's delayed stats commit. Also refreshes the weapon in hand
    /// from the plugin API, or for bots without it from their player model (the server sends
    /// bots no `WeaponInfo`).
    pub fn pre_think(&mut self, g: &dyn Game, ent: i32) {
        let Some(i) = slot(self, ent) else { return };
        self.players[i].checkstate = false;
        if let Some(api) = g.api() {
            if let Ok(info) = api.player_info(ent)
                && let Some(w) = self.weapons.from_wire(info.weapon)
            {
                let p = &mut self.players[i];
                p.current = w;
                let pw = &mut p.weapons[w];
                (pw.clip, pw.ammo, pw.mode, pw.attach) = (info.clip, info.ammo, info.fire_mode, info.attachments);
            }
        } else if g.is_fake_client(ent)
            && g.is_alive(ent)
            && let Some(w) = self.weapons.id_from_p_model(&g.weapon_model(ent))
        {
            // Port addition: bots get no `WeaponInfo`; their model shows what they hold.
            self.players[i].current = w;
        }
        if !active(g) {
            return;
        }
        let p = &self.players[i];
        if p.clear_stats != 0.0 && p.clear_stats < g.time() && p.ingame {
            self.players[i].clear_stats = 0.0;
            if let Some(k) = self.players[i].rank {
                let life = self.players[i].life;
                self.rank.update(k, &life);
            }
            self.players[i].restart_stats(false);
        }
    }

    /// `ServerDeactivate`: commits everyone, trims or resets the rank, and returns whether to
    /// save it (always, as upstream).
    pub fn server_deactivate(&mut self, g: &dyn Game) {
        for i in 1..self.players.len() {
            if self.players[i].rank.is_some() {
                self.disconnect_player(g, i);
            }
        }
        if self.rank.len() as i32 >= g.cvar("tsstats_maxsize") as i32 || g.cvar("tsstats_reset") as i32 == 1 {
            g.set_cvar("tsstats_reset", 0.0);
            self.rank.clear();
        }
        self.weapons.clear_custom();
    }

    /// `TraceLine_Post`: a player's trace that hit a player records the hit group.
    pub fn trace_line(&mut self, shooter: i32, hit: i32, hitgroup: i32, g: &dyn Game) {
        if let Some(i) = slot(self, shooter)
            && g.is_client(shooter)
            && g.is_client(hit)
        {
            self.players[i].aiming = hitgroup;
        }
    }

    /// `pfnPrecacheEvent_Post`: remembers the weapon-firing events.
    pub fn precache_event(&mut self, name: &str, index: u16) {
        let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
        if (file.eq_ignore_ascii_case("fire.sc") || file.eq_ignore_ascii_case("firelinked.sc")) && !self.fire_events.contains(&index) {
            self.fire_events.push(index);
        }
    }

    /// `pfnPlaybackEvent`: one shot of the invoker's weapon. Port addition: upstream counted
    /// shots from `ClipInfo`, which TS 3.0 never sends (decomp architecture.md §3).
    pub fn playback_event(&mut self, g: &dyn Game, invoker: i32, index: u16) {
        if !self.fire_events.contains(&index) {
            return;
        }
        if let Some(i) = slot(self, invoker)
            && !ignore_bots(self, g, i, None)
        {
            let w = self.players[i].current;
            self.players[i].save_shot(w);
        }
    }

    /// The first half of `Client_TSHealth_End`: who hurt `victim`, with what. Records the hit
    /// and returns it for `client_damage`; the death half is [`State::after_damage`].
    pub fn ts_health(&mut self, g: &dyn Game, victim: usize) -> Option<Hit> {
        let vent = victim as i32;
        let (inflictor, take) = g.damage(vent);
        let enemy = inflictor?;
        let damage = take as i32;
        if damage == 0 {
            return None;
        }
        g.clear_dmg_take(vent);
        let mut attacker = None;
        let (mut weapon, mut aim) = (0usize, 0);
        let mut by = Inflictor::Other;
        if g.is_client(enemy) {
            let a = slot(self, enemy)?;
            attacker = Some(a);
            by = Inflictor::Player;
            weapon = self.players[a].current;
            aim = self.players[a].aiming;
            if !ignore_bots(self, g, a, Some(victim)) {
                player::save_hit(&mut self.players, a, victim, weapon, damage, aim as usize);
            }
        } else {
            let class = g.classname(enemy);
            if class.starts_with('g') {
                if let Some(o) = g.owner(enemy).filter(|&o| g.is_client(o)).and_then(|o| slot(self, o)) {
                    attacker = Some(o);
                    by = Inflictor::Grenade;
                    weapon = crate::weapons::GRENADE;
                    if o != victim && !ignore_bots(self, g, o, Some(victim)) {
                        player::save_hit(&mut self.players, o, victim, weapon, damage, 0);
                    }
                }
            } else if class.starts_with('k') {
                // Upstream read the thrower from the knife's private data (`ts_setpddata`'s
                // offset); the Rust server's private data is a handle, so ask its API, else
                // fall back to `pev->owner`.
                let owner = match g.api().map(|a| a.projectile_owner(enemy)) {
                    Some(Ok(o)) => Some(o),
                    Some(Err(_)) => None,
                    None => g.owner(enemy),
                };
                let o = owner.filter(|&o| o > 0 && !g.is_free(o)).and_then(|o| slot(self, o))?;
                attacker = Some(o);
                by = Inflictor::Knife;
                weapon = self.weapons.thrown_knife();
                aim = self.players[o].aiming;
                if !ignore_bots(self, g, o, Some(victim)) {
                    player::save_hit(&mut self.players, o, victim, weapon, damage, aim as usize);
                }
            }
        }
        let attacker = attacker.unwrap_or(victim);
        let ta = ((self.players[victim].team_id != 0 || self.theone)
            && self.players[victim].team_id == self.players[attacker].team_id
            && victim != attacker) as i32;
        if self.weapons.melee(weapon) && !ignore_bots(self, g, attacker, None) {
            self.players[attacker].save_shot(weapon);
        }
        Some(Hit { attacker, victim, damage, weapon, aim, ta, by })
    }

    /// The death half of `Client_TSHealth_End`, after `client_damage` ran: kill flags, frag
    /// bookkeeping and the kill stats, and the `client_death` event if the victim died.
    ///
    /// Port change: the flags and the frag prediction follow TS 3.0's scoring
    /// ([`crate::scoring`]) instead of upstream's TS 2.x model, and the weapon of a kill by the
    /// killer's own hand comes from TS's `DeathMsg` (sent just before this `TSHealth`,
    /// [`player::DeathNotice`]): "Kung Fu" or "world" is kung fu, a named weapon that weapon.
    /// Upstream relabelled every kill whose frags it mispredicted as kung fu; tsx does so only
    /// without a `DeathMsg`, and only when a close-combat kung-fu frag explains the frags exactly.
    ///
    /// Port addition (#15): when the server reports the deaths it scores (plugin API version 5,
    /// [`State::kill_reports`]), the death waits for its report ([`State::kill_scored`]), which
    /// tells the killer, the frags and the tags: the server may score a kill after this `TSHealth`
    /// 0 (a killer out of reach), when the frags and the `DeathMsg` haven't happened yet. A death
    /// the rules don't score gets the prediction at the victim's next PreThink
    /// ([`State::flush_death`]).
    pub fn after_damage(&mut self, g: &dyn Game, hit: Hit) -> Option<Event> {
        if g.is_alive(hit.victim as i32) {
            return None;
        }
        if !self.kill_reports {
            return self.predict_death(g, hit);
        }
        let v = &mut self.players[hit.victim];
        match v.pending_kill.take() {
            Some(k) => self.reported_death(g, hit, &k),
            None => {
                v.pending_death = Some(hit);
                None
            }
        }
    }

    /// The server scored a death ([`crate::forwards::KillReport`]): the death half of the
    /// victim's `TSHealth` 0 if that came first, else the report waits for it.
    pub fn kill_scored(&mut self, g: &dyn Game, k: KillReport) -> Option<Event> {
        let v = slot(self, k.victim)?;
        match self.players[v].pending_death.take() {
            Some(hit) => self.reported_death(g, hit, &k),
            None => {
                self.players[v].pending_kill = Some(k);
                None
            }
        }
    }

    /// The victim's PlayerPreThink after a death: every report has come by then (the server
    /// sends it before the engine's next callback). A death without one (rules that don't score
    /// it, such as a teamplay team change) gets the prediction; a report without a `TSHealth` 0
    /// (a death without damage) is dropped, as upstream fired no `client_death` without one.
    pub fn flush_death(&mut self, g: &dyn Game, victim: i32) -> Option<Event> {
        let v = slot(self, victim)?;
        self.players[v].pending_kill = None;
        let hit = self.players[v].pending_death.take()?;
        self.predict_death(g, hit)
    }

    /// The death half from the server's report: its killer (a suicide or a death by no player
    /// is a self kill), the weapon its `DeathMsg` name gives a kill by the killer's own hand, its
    /// frags, spree and tags for the kill flags.
    fn reported_death(&mut self, g: &dyn Game, hit: Hit, k: &KillReport) -> Option<Event> {
        use ts_plugin_api::kill;
        let Hit { victim, mut weapon, aim, ta, by, .. } = hit;
        self.players[victim].death_notice = None;
        let attacker = slot(self, k.killer).filter(|&a| a != victim).unwrap_or(victim);
        let mut kill_flags = 0;
        if attacker != victim {
            let now = g.time();
            if ta == 0 && by == Inflictor::Player {
                weapon = self.death_msg_weapon(&k.weapon_name).unwrap_or(weapon);
            }
            let a = &mut self.players[attacker];
            a.frags = g.frags(attacker as i32);
            a.killing_spree = k.killer_spree;
            a.is_specialist = k.killer_spree > scoring::SPECIALIST_SPREE;
            if ta == 0 {
                for (bit, flag) in [
                    (kill::DIVING, player::KF_STUNTKILL),
                    (kill::SLIDING, player::KF_SLIDINGKILL),
                    (kill::DOUBLE_KILL, player::KF_DOUBLEKILL),
                    (kill::KILLED_SPECIALIST, player::KF_KILLEDSPEC),
                ] {
                    if k.tags & bit != 0 {
                        kill_flags |= flag;
                    }
                }
                if a.is_specialist {
                    kill_flags |= player::KF_ISSPEC;
                }
                if k.points > 0 {
                    a.last_kill = if k.tags & kill::DOUBLE_KILL != 0 { 0.0 } else { now };
                }
                a.last_frag = k.points;
            }
        }
        self.players[attacker].kill_flags = kill_flags;
        if !ignore_bots(self, g, attacker, Some(victim)) {
            player::save_kill(&mut self.players, attacker, victim, weapon, (aim == 1) as i32, ta);
        }
        Some(Event::Death { killer: attacker as i32, victim: victim as i32, weapon: weapon as i32, aim, tk: ta })
    }

    /// The death half without the server's report: the kill predicted from the frags,
    /// `DeathMsg` and TS 3.0's scoring ([`crate::scoring`]).
    fn predict_death(&mut self, g: &dyn Game, hit: Hit) -> Option<Event> {
        let Hit { mut attacker, victim, mut weapon, aim, ta, by, .. } = hit;
        let notice = self.players[victim].death_notice.take().filter(|n| n.killer == attacker as i32);
        // No frag for the attacker (e.g. a grenade's owner killed themselves): self kill.
        if g.frags(attacker as i32) - self.players[attacker].frags == 0 {
            attacker = victim;
        }
        let mut kill_flags = 0;
        if attacker != victim {
            let now = g.time();
            let victim_spree = self.players[victim].killing_spree;
            let real = g.frags(attacker as i32);
            let kung_fu = self.weapons.kung_fu();
            let a = &mut self.players[attacker];
            let gained = real - a.frags;
            // Also after a team kill, which upstream left out of sync, so that the next kill
            // looked like no frag at all (a self kill).
            a.frags = real;
            // TS 3.0 counts every kill of another player in the spree, team kills too.
            a.killing_spree += 1;
            a.is_specialist = a.killing_spree > scoring::SPECIALIST_SPREE;
            if ta == 0 {
                let named = notice.as_ref().filter(|_| by == Inflictor::Player).map(|n| n.weapon.as_str());
                let blow = match by {
                    Inflictor::Player => Blow::own(named, weapon == kung_fu),
                    Inflictor::Knife => Blow::ThrownKnife,
                    Inflictor::Grenade | Inflictor::Other => Blow::Other,
                };
                let ent = attacker as i32;
                let (points, stunt) = scoring::points_for_frag(blow, Stunt::of(g.iuser4(ent), g.velocity(ent), g.friction(ent)));
                let double = now - a.last_kill < scoring::DOUBLE_KILL_SECS;
                let spree = a.killing_spree;
                let predict = |points| scoring::player_killed_points(points, double, victim_spree, spree);
                if let Some(name) = named {
                    weapon = self.death_msg_weapon(name).unwrap_or(weapon);
                } else if by == Inflictor::Player && !blow.kung_fu() && predict(points) != gained {
                    // No `DeathMsg`: a kick with a gun in hand (`glast_closecombat`) if that is
                    // what the frags show.
                    let (kick, _) = scoring::points_for_frag(Blow::Own { kung_fu: true, close_combat: true }, stunt);
                    if predict(kick) == gained {
                        weapon = kung_fu;
                    }
                }
                let a = &mut self.players[attacker];
                for (on, flag) in [
                    (stunt.diving, player::KF_STUNTKILL),
                    (stunt.sliding, player::KF_SLIDINGKILL),
                    (double, player::KF_DOUBLEKILL),
                    (a.is_specialist, player::KF_ISSPEC),
                    (victim_spree > scoring::SPECIALIST_SPREE, player::KF_KILLEDSPEC),
                ] {
                    if on {
                        kill_flags |= flag;
                    }
                }
                // Only a kill worth points restarts the double-kill clock, and a double kill
                // stops it (`PlayerKilled` stores 0).
                if points > 0 {
                    a.last_kill = if double { 0.0 } else { now };
                }
                // `pev->frags` is the truth (upstream corrected its prediction to it too).
                a.last_frag = gained;
            }
        }
        self.players[attacker].kill_flags = kill_flags;
        if !ignore_bots(self, g, attacker, Some(victim)) {
            player::save_kill(&mut self.players, attacker, victim, weapon, (aim == 1) as i32, ta);
        }
        Some(Event::Death { killer: attacker as i32, victim: victim as i32, weapon: weapon as i32, aim, tk: ta })
    }

    /// The tsx id of a `DeathMsg` weapon (TS's `get_weapon_info` name, `<name> Stock` for a
    /// pistol-whip): kung fu for "Kung Fu" and "world", else the weapon of that log name.
    fn death_msg_weapon(&self, name: &str) -> Option<usize> {
        if name == "Kung Fu" || name == "world" {
            return Some(self.weapons.kung_fu());
        }
        let name = name.strip_suffix(" Stock").unwrap_or(name);
        self.weapons.by_logname(&name.to_ascii_lowercase().replace(' ', "_"))
    }
}
