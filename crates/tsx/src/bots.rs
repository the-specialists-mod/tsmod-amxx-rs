// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Port addition: a Pawn plugin as the server's bot backend (`include/tsbot.inc`).
//!
//! `ts_bot_register_backend(name)` attaches a plugin identity under `name` (plugin API version 7,
//! `attach_plugin`) and registers one of the [`SLOTS`] backend slots' trampolines
//! (`amxx::tsapi::backend`) under the same name. The server's backend callbacks then run the
//! registering plugin's single-plugin forwards: `ts_bot_add`, `ts_bot_activate`,
//! `ts_bot_deactivate` and `ts_bot_adopt`. The server's `start_frame` fires the multi-plugin
//! forward `ts_frame`, where bots run their commands (`ts_bot_runcmd`).
//!
//! **Lifetimes.** The identity and the backend are the server's for the process, but AMX Mod X
//! reloads its plugins every map: `AMXX_PluginsUnloading` unbinds every slot (its forwards are
//! gone), and the plugin's `ts_bot_register_backend` on the next map binds it again (same name,
//! same identity and data directory). The next map's start chose the backend before the plugins'
//! `plugin_init`, so the server's `activate` (and an `adopt` of a bot kept across the level
//! change) can come while the slot is unbound: they are kept and delivered at the first
//! `ts_frame` after the plugin bound the slot. The same goes for an `activate` inside
//! `ts_bot_register_backend` itself (a backend registered after the map start takes over at once
//! when `ts_bot_backend` names it). A plugin only gets `ts_bot_deactivate` after its
//! `ts_bot_activate`.
//!
//! As everywhere in this crate, no state is held across a forward or an API call that can run a
//! plugin (`create_bot`, `run_cmd`, `detach`): take, release, call, take again.

use crate::game::{BackendCall, BackendForwards, BotCmdResult, Game, TsEvent};
use crate::state::with;
use std::collections::VecDeque;

/// How many Pawn backends tsx can register at once (one per plugin).
pub const SLOTS: usize = amxx::tsapi::backend::SLOTS;

/// tsbot.inc's statuses beyond the API's (`TSBOT_*`, below the API's own codes).
pub mod status {
    /// The server has no plugin API (version 7).
    pub const NO_API: i32 = -100;
    /// The calling plugin registered no backend (`ts_bot_register_backend`).
    pub const NO_BACKEND: i32 = -101;
    /// The calling plugin registered a backend under another name already.
    pub const ALREADY: i32 = -102;
    /// Every backend slot is taken.
    pub const NO_SLOT: i32 = -103;
}

/// One registered Pawn backend.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub name: String,
    /// The identity's id (0 while being registered).
    pub plugin: i32,
    /// The plugin bound to it this map (its AMX), 0 when unbound.
    pub owner: usize,
    pub fwds: BackendForwards,
    /// The server says the backend is the active one.
    pub active: bool,
    /// The bound plugin got `ts_bot_activate` for that.
    pub told: bool,
    /// `ts_bot_activate` is owed to the bound plugin (delivered at the next `ts_frame`).
    pub announce: bool,
    /// `ts_bot_adopt`s owed to the bound plugin.
    pub adopt: VecDeque<i32>,
    /// Inside `ts_bot_register_backend`'s API calls: callbacks wait for the next frame.
    pub registering: bool,
}

impl Slot {
    fn deliverable(&self) -> bool {
        self.owner != 0 && !self.registering
    }
}

/// tsx's Pawn bot state.
#[derive(Debug)]
pub struct Bots {
    pub slots: [Option<Slot>; SLOTS],
    /// The last `ts_bot_runcmd` result per player slot (`ts_bot_get_result`).
    pub results: Vec<Option<BotCmdResult>>,
}

impl Default for Bots {
    fn default() -> Self {
        Bots { slots: Default::default(), results: vec![None; 33] }
    }
}

impl Bots {
    fn owned_by(&self, amx: usize) -> Option<usize> {
        (amx != 0).then(|| self.slots.iter().position(|s| s.as_ref().is_some_and(|s| s.owner == amx))).flatten()
    }

    fn named(&self, name: &str) -> Option<usize> {
        self.slots.iter().position(|s| s.as_ref().is_some_and(|s| s.name.eq_ignore_ascii_case(name)))
    }

    /// The identity of the backend `amx` registered, once registered.
    pub fn plugin_of(&self, amx: usize) -> Option<i32> {
        self.owned_by(amx).and_then(|i| self.slots[i].as_ref()).map(|s| s.plugin).filter(|&p| p > 0)
    }

    /// `AMXX_PluginsUnloading`: the plugins and their forwards go; the backends stay.
    pub fn unbind_all(&mut self) {
        for s in self.slots.iter_mut().flatten() {
            (s.owner, s.fwds, s.told, s.announce) = (0, BackendForwards::NONE, false, false);
        }
    }
}

/// `ts_bot_register_backend(name)` for the plugin `amx`: [`ts_plugin_api::OK`] or a status.
pub fn register(g: &dyn Game, amx: usize, name: &str) -> i32 {
    let Some(api) = g.api() else { return status::NO_API };
    if !amxx::tsapi::valid_plugin_name(name) {
        return ts_plugin_api::INVALID;
    }
    enum Plan {
        Done(i32),
        Rebind(usize),
        New(usize),
    }
    let plan = with(|s| {
        if let Some(i) = s.bots.owned_by(amx) {
            let same = s.bots.slots[i].as_ref().is_some_and(|sl| sl.name.eq_ignore_ascii_case(name));
            return Plan::Done(if same { ts_plugin_api::OK } else { status::ALREADY });
        }
        if let Some(i) = s.bots.named(name) {
            return if s.bots.slots[i].as_ref().is_some_and(|sl| sl.owner != 0) {
                Plan::Done(ts_plugin_api::EXISTS)
            } else {
                Plan::Rebind(i)
            };
        }
        s.bots.slots.iter().position(Option::is_none).map_or(Plan::Done(status::NO_SLOT), Plan::New)
    });
    let Some(plan) = plan else { return ts_plugin_api::BUSY };
    match plan {
        Plan::Done(r) => r,
        Plan::Rebind(i) => {
            let fwds = g.backend_forwards(amx);
            with(|s| {
                if let Some(sl) = s.bots.slots[i].as_mut() {
                    (sl.owner, sl.fwds, sl.told, sl.announce) = (amx, fwds, false, sl.active);
                }
            });
            ts_plugin_api::OK
        }
        Plan::New(i) => {
            let fwds = g.backend_forwards(amx);
            with(|s| {
                s.bots.slots[i] = Some(Slot {
                    name: name.to_owned(),
                    plugin: 0,
                    owner: amx,
                    fwds,
                    active: false,
                    told: false,
                    announce: false,
                    adopt: VecDeque::new(),
                    registering: true,
                })
            });
            // The server may call the backend (`activate`) inside: the slot is `registering`.
            match api.attach_backend(i, name) {
                Ok(id) => {
                    with(|s| {
                        if let Some(sl) = s.bots.slots[i].as_mut() {
                            (sl.plugin, sl.registering) = (id, false);
                        }
                    });
                    ts_plugin_api::OK
                }
                Err(rc) => {
                    with(|s| s.bots.slots[i] = None);
                    g.drop_backend_forwards(&fwds);
                    rc
                }
            }
        }
    }
}

/// `ts_bot_unregister_backend()`: gives the calling plugin's identity up (its backend goes; if
/// it was active, the server kicks its bots and native takes over). No `ts_bot_deactivate`.
pub fn unregister(g: &dyn Game, amx: usize) -> i32 {
    let Some(api) = g.api() else { return status::NO_API };
    let Some(Some(slot)) = with(|s| s.bots.owned_by(amx).and_then(|i| s.bots.slots[i].take())) else { return status::NO_BACKEND };
    g.drop_backend_forwards(&slot.fwds);
    // The server may call the slot's `deactivate` inside; the slot is gone, so nothing runs.
    api.detach(slot.plugin)
}

/// Module unload: gives every identity up, so the server never calls into the unloaded library.
pub fn detach_all(g: &dyn Game) {
    let slots: Vec<Slot> = with(|s| s.bots.slots.iter_mut().filter_map(Option::take).collect()).unwrap_or_default();
    if let Some(api) = g.api() {
        for s in slots.iter().filter(|s| s.plugin > 0) {
            api.detach(s.plugin);
        }
    }
}

/// The identity of the backend the calling plugin registered.
pub fn plugin_of(amx: usize) -> Option<i32> {
    with(|s| s.bots.plugin_of(amx)).flatten()
}

/// Delivers what slot `i` owes its bound plugin (`ts_bot_activate`, then the `ts_bot_adopt`s),
/// one forward at a time with the state released.
fn flush(g: &dyn Game, i: usize) {
    loop {
        let next = with(|s| {
            let sl = s.bots.slots[i].as_mut()?;
            if !sl.deliverable() {
                return None;
            }
            if sl.announce {
                (sl.announce, sl.told) = (false, true);
                return Some((sl.fwds.activate, BackendCall::Activate));
            }
            if sl.told
                && let Some(p) = sl.adopt.pop_front()
            {
                return Some((sl.fwds.adopt, BackendCall::Adopt(p)));
            }
            None
        })
        .flatten();
        let Some((fwd, call)) = next else { break };
        g.fire_backend(fwd, &call);
    }
}

/// A server callback of slot `i`'s backend ([`amxx::tsapi::backend`]'s dispatcher): `Add`'s
/// result is the bot's index (0 for none).
pub fn backend_call(g: &dyn Game, i: usize, call: BackendCall) -> i32 {
    if i >= SLOTS {
        return 0;
    }
    match call {
        BackendCall::Activate => {
            let fwd = with(|s| {
                let sl = s.bots.slots[i].as_mut()?;
                sl.active = true;
                if sl.deliverable() {
                    (sl.told, sl.announce) = (true, false);
                    Some(sl.fwds.activate)
                } else {
                    sl.announce = true;
                    None
                }
            })
            .flatten();
            if let Some(f) = fwd {
                g.fire_backend(f, &BackendCall::Activate);
            }
            0
        }
        BackendCall::Deactivate => {
            let fwd = with(|s| {
                let sl = s.bots.slots[i].as_mut()?;
                let told = sl.told && sl.deliverable();
                (sl.active, sl.told, sl.announce) = (false, false, false);
                sl.adopt.clear();
                told.then_some(sl.fwds.deactivate)
            })
            .flatten();
            if let Some(f) = fwd {
                g.fire_backend(f, &BackendCall::Deactivate);
            }
            0
        }
        BackendCall::Adopt(p) => {
            flush(g, i);
            let fwd = with(|s| {
                let sl = s.bots.slots[i].as_mut()?;
                if sl.deliverable() && sl.told {
                    Some(sl.fwds.adopt)
                } else {
                    sl.adopt.push_back(p);
                    None
                }
            })
            .flatten();
            if let Some(f) = fwd {
                g.fire_backend(f, &BackendCall::Adopt(p));
            }
            0
        }
        BackendCall::Add { .. } => {
            flush(g, i);
            let target = with(|s| s.bots.slots[i].as_ref().map(|sl| (sl.deliverable().then_some(sl.fwds.add), sl.name.clone()))).flatten();
            match target {
                Some((Some(f), _)) => g.fire_backend(f, &call).max(0),
                Some((None, name)) => {
                    crate::natives::log_once(g, &format!("tsbot:{name}:unbound"), || {
                        format!("bot backend {name}: the server asked for a bot but no plugin has registered the backend this map")
                    });
                    0
                }
                None => 0,
            }
        }
    }
}

/// The server's `start_frame`: what the backends owe their plugins, then `ts_frame`.
pub fn frame(g: &dyn Game) {
    for i in 0..SLOTS {
        flush(g, i);
    }
    g.fire_ts(&TsEvent::Frame);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeGame;

    const PLUGIN: usize = 0x1000;

    fn calls(g: &FakeGame) -> Vec<String> {
        g.backend_fired.borrow().clone()
    }

    #[test]
    fn registering_attaches_and_takes_a_slot_per_plugin() {
        let g = FakeGame::with_api();
        assert_eq!(register(&FakeGame::new(), PLUGIN, "x"), status::NO_API);
        assert_eq!(register(&g, PLUGIN, "bad name"), ts_plugin_api::INVALID);
        assert_eq!(register(&g, PLUGIN, "rcbot_amxx"), ts_plugin_api::OK);
        assert_eq!(g.api.as_ref().unwrap().calls.borrow().last().unwrap(), "attach_backend 0 rcbot_amxx");
        assert_eq!(plugin_of(PLUGIN), Some(101));
        assert_eq!(register(&g, PLUGIN, "RCBOT_AMXX"), ts_plugin_api::OK, "again: idempotent, case ignored");
        assert_eq!(register(&g, PLUGIN, "other"), status::ALREADY);
        assert_eq!(register(&g, PLUGIN + 1, "rcbot_amxx"), ts_plugin_api::EXISTS, "bound to another plugin");
        for (k, name) in ["b", "c", "d"].iter().enumerate() {
            assert_eq!(register(&g, PLUGIN + 1 + k, name), ts_plugin_api::OK);
        }
        assert_eq!(register(&g, PLUGIN + 9, "e"), status::NO_SLOT);
        // A refused attach frees the slot again.
        assert_eq!(unregister(&g, PLUGIN + 3), ts_plugin_api::OK);
        g.api.as_ref().unwrap().attach_result.set(ts_plugin_api::EXISTS);
        assert_eq!(register(&g, PLUGIN + 9, "native"), ts_plugin_api::EXISTS);
        assert!(with(|s| s.bots.slots[3].is_none()).unwrap());
        assert_eq!(unregister(&g, PLUGIN + 9), status::NO_BACKEND);
    }

    #[test]
    fn an_activate_during_registration_waits_for_the_next_frame() {
        let g = FakeGame::with_api();
        // The server takes over inside register_bot_backend (ts_bot_backend names it).
        g.api.as_ref().unwrap().activate_on_attach.set(true);
        assert_eq!(register(&g, PLUGIN, "tsbot_e2e"), ts_plugin_api::OK);
        assert!(calls(&g).is_empty(), "not inside the native");
        frame(&g);
        assert_eq!(calls(&g), ["activate"]);
        assert_eq!(g.ts_fired.borrow().last(), Some(&TsEvent::Frame));
        frame(&g);
        assert_eq!(calls(&g), ["activate"], "once");
        // Bots are added through the plugin's forward, which returns the index.
        g.backend_result.set(5);
        assert_eq!(backend_call(&g, 0, BackendCall::Add { name: b"Neo", model: b"", skill: 0.0 }), 5);
        assert_eq!(calls(&g).last().unwrap(), "add Neo  0");
        backend_call(&g, 0, BackendCall::Adopt(3));
        backend_call(&g, 0, BackendCall::Deactivate);
        assert_eq!(calls(&g)[2..], ["adopt 3".to_string(), "deactivate".into()]);
    }

    #[test]
    fn the_next_maps_plugin_hears_what_happened_before_it_bound() {
        let g = FakeGame::with_api();
        register(&g, PLUGIN, "rcbot_amxx");
        backend_call(&g, 0, BackendCall::Activate);
        assert_eq!(calls(&g), ["activate"], "bound: at once");
        // Map change: the plugins unload, the next map start selects the backend again and a
        // bot kept across the level change is adopted, all before plugin_init.
        with(|s| s.bots.unbind_all());
        backend_call(&g, 0, BackendCall::Deactivate);
        backend_call(&g, 0, BackendCall::Activate);
        backend_call(&g, 0, BackendCall::Adopt(4));
        g.backend_result.set(7);
        assert_eq!(backend_call(&g, 0, BackendCall::Add { name: b"", model: b"", skill: 1.0 }), 0, "no plugin to make it");
        assert_eq!(g.logs.borrow().len(), 1);
        assert_eq!(calls(&g), ["activate"], "nothing reached the unloaded plugin");
        // The new plugin instance binds (same name, same identity, no new attach).
        let attaches = g.api.as_ref().unwrap().calls.borrow().len();
        assert_eq!(register(&g, PLUGIN + 7, "rcbot_amxx"), ts_plugin_api::OK);
        assert_eq!(g.api.as_ref().unwrap().calls.borrow().len(), attaches);
        assert_eq!(plugin_of(PLUGIN + 7), Some(101));
        frame(&g);
        assert_eq!(calls(&g)[1..], ["activate".to_string(), "adopt 4".into()]);
    }

    #[test]
    fn deactivate_only_follows_a_delivered_activate() {
        let g = FakeGame::with_api();
        g.api.as_ref().unwrap().activate_on_attach.set(true);
        register(&g, PLUGIN, "x");
        // A callback after the registration delivers the owed activate first.
        backend_call(&g, 0, BackendCall::Adopt(2));
        backend_call(&g, 0, BackendCall::Deactivate);
        assert_eq!(calls(&g), ["activate", "adopt 2", "deactivate"]);
        // Activated and deactivated again while no plugin is bound (between maps): the next
        // plugin hears nothing of it, and the queued adopt is dropped.
        with(|s| s.bots.unbind_all());
        backend_call(&g, 0, BackendCall::Activate);
        backend_call(&g, 0, BackendCall::Adopt(5));
        backend_call(&g, 0, BackendCall::Deactivate);
        register(&g, PLUGIN + 1, "x");
        frame(&g);
        assert_eq!(calls(&g).len(), 3);
    }

    #[test]
    fn unregistering_detaches_without_a_deactivate_and_unload_detaches_all() {
        let g = FakeGame::with_api();
        register(&g, PLUGIN, "a");
        register(&g, PLUGIN + 1, "b");
        backend_call(&g, 0, BackendCall::Activate);
        assert_eq!(unregister(&g, PLUGIN), ts_plugin_api::OK);
        backend_call(&g, 0, BackendCall::Deactivate); // the server's, inside detach
        assert_eq!(calls(&g), ["activate"]);
        assert!(g.api.as_ref().unwrap().calls.borrow().contains(&"detach 101".to_string()));
        assert_eq!(g.dropped.get(), 1, "its forwards are released");
        detach_all(&g);
        assert!(g.api.as_ref().unwrap().calls.borrow().contains(&"detach 102".to_string()));
        assert!(with(|s| s.bots.slots.iter().all(Option::is_none)).unwrap());
        assert_eq!(backend_call(&g, 9, BackendCall::Activate), 0, "no such slot");
    }
}
