// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Test doubles: a fake [`Game`] and [`TsApi`], and [`Feed`], which runs the port's own message
//! encoders (`shared::protocol`) into the module's message hooks the way the engine's
//! `WRITE_*` calls would.

use crate::game::{Event, Game, PlayerInfo, TsApi};
use crate::msgs::{Followup, Kind};
use crate::state::State;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// One entity's fields.
#[derive(Clone, Debug, Default)]
pub struct Ent {
    pub client: bool,
    pub bot: bool,
    pub alive: bool,
    pub frags: i32,
    pub iuser4: i32,
    pub team: i32,
    pub inflictor: Option<i32>,
    pub dmg_take: f32,
    pub class: String,
    pub owner: Option<i32>,
    pub name: String,
    pub auth: Option<String>,
}

#[derive(Default)]
pub struct FakeApi {
    pub info: RefCell<HashMap<i32, PlayerInfo>>,
    pub calls: RefCell<Vec<String>>,
    /// What setters return.
    pub result: Cell<i32>,
    pub knife_owner: Cell<i32>,
}

impl FakeApi {
    fn call(&self, s: String) -> Result<(), i32> {
        self.calls.borrow_mut().push(s);
        let r = self.result.get();
        if r >= 0 { Ok(()) } else { Err(r) }
    }
}

impl TsApi for FakeApi {
    fn player_info(&self, player: i32) -> Result<PlayerInfo, i32> {
        self.info.borrow().get(&player).copied().ok_or(ts_plugin_api::NOT_PLAYER)
    }
    fn set_cash(&self, p: i32, v: i32) -> Result<(), i32> {
        self.call(format!("set_cash {p} {v}"))
    }
    fn set_free_slots(&self, p: i32, v: i32) -> Result<(), i32> {
        self.call(format!("set_free_slots {p} {v}"))
    }
    fn set_award(&self, p: i32, v: i32) -> Result<(), i32> {
        self.call(format!("set_award {p} {v}"))
    }
    fn run_powerup(&self, p: i32, bits: i32, d: f32) -> Result<(), i32> {
        self.call(format!("run_powerup {p} {bits} {d}"))
    }
    fn set_speed(&self, p: i32, s: f32, a: f32, d: f32) -> Result<(), i32> {
        self.call(format!("set_speed {p} {s} {a} {d}"))
    }
    fn set_physics_speed(&self, p: i32, s: f32) -> Result<(), i32> {
        self.call(format!("set_physics_speed {p} {s}"))
    }
    fn set_bullet_trail(&self, p: i32, on: bool) -> Result<(), i32> {
        self.call(format!("set_bullet_trail {p} {on}"))
    }
    fn projectile_owner(&self, _e: i32) -> Result<i32, i32> {
        Ok(self.knife_owner.get())
    }
}

#[derive(Default)]
pub struct FakeGame {
    pub time: Cell<f32>,
    pub ents: RefCell<HashMap<i32, Ent>>,
    pub cvars: RefCell<HashMap<String, f32>>,
    pub api: Option<FakeApi>,
    pub logs: RefCell<Vec<String>>,
    pub fired: RefCell<Vec<Event>>,
    pub given: RefCell<Vec<(i32, i32, i32, i32)>>,
}

impl FakeGame {
    pub fn new() -> Self {
        let g = FakeGame::default();
        g.cvars.borrow_mut().insert("tsstats_rankbots".into(), 1.0);
        g.cvars.borrow_mut().insert("tsstats_maxsize".into(), 3500.0);
        g
    }

    pub fn with_api() -> Self {
        FakeGame { api: Some(FakeApi::default()), ..Self::new() }
    }

    /// Adds a living player `i` named `p<i>`.
    pub fn player(&self, i: i32) {
        self.ents
            .borrow_mut()
            .insert(i, Ent { client: true, alive: true, class: "player".into(), name: format!("p{i}"), ..Default::default() });
    }

    pub fn edit(&self, i: i32, f: impl FnOnce(&mut Ent)) {
        f(self.ents.borrow_mut().entry(i).or_default());
    }

    fn get<R: Default>(&self, i: i32, f: impl FnOnce(&Ent) -> R) -> R {
        self.ents.borrow().get(&i).map(f).unwrap_or_default()
    }
}

impl Game for FakeGame {
    fn time(&self) -> f32 {
        self.time.get()
    }
    fn max_clients(&self) -> i32 {
        32
    }
    fn is_client(&self, e: i32) -> bool {
        self.get(e, |x| x.client)
    }
    fn is_fake_client(&self, e: i32) -> bool {
        self.get(e, |x| x.bot)
    }
    fn is_alive(&self, e: i32) -> bool {
        self.get(e, |x| x.alive)
    }
    fn is_free(&self, e: i32) -> bool {
        !self.ents.borrow().contains_key(&e)
    }
    fn frags(&self, e: i32) -> i32 {
        self.get(e, |x| x.frags)
    }
    fn iuser4(&self, e: i32) -> i32 {
        self.get(e, |x| x.iuser4)
    }
    fn team(&self, e: i32) -> i32 {
        self.get(e, |x| x.team)
    }
    fn damage(&self, e: i32) -> (Option<i32>, f32) {
        self.get(e, |x| (x.inflictor, x.dmg_take))
    }
    fn clear_dmg_take(&self, e: i32) {
        self.edit(e, |x| x.dmg_take = 0.0);
    }
    fn clear_dmg_inflictor(&self, e: i32) {
        self.edit(e, |x| x.inflictor = None);
    }
    fn classname(&self, e: i32) -> String {
        self.get(e, |x| x.class.clone())
    }
    fn owner(&self, e: i32) -> Option<i32> {
        self.get(e, |x| x.owner)
    }
    fn auth_id(&self, e: i32) -> Option<String> {
        self.get(e, |x| x.auth.clone())
    }
    fn netname(&self, e: i32) -> String {
        self.get(e, |x| x.name.clone())
    }
    fn cvar(&self, name: &str) -> f32 {
        self.cvars.borrow().get(name).copied().unwrap_or(0.0)
    }
    fn set_cvar(&self, name: &str, value: f32) {
        self.cvars.borrow_mut().insert(name.into(), value);
    }
    fn amxx_ingame(&self, p: i32) -> bool {
        self.get(p, |x| x.client)
    }
    fn api(&self) -> Option<&dyn TsApi> {
        self.api.as_ref().map(|a| a as &dyn TsApi)
    }
    fn log(&self, msg: &str) {
        self.logs.borrow_mut().push(msg.into());
    }
    fn give_weapon(&self, p: i32, w: i32, c: i32, f: i32) -> bool {
        self.given.borrow_mut().push((p, w, c, f));
        true
    }
    fn create_powerup(&self, kind: i32, _origin: [f32; 3]) -> i32 {
        self.ents.borrow_mut().insert(100, Ent { class: "ts_powerup".into(), frags: kind, ..Default::default() });
        100
    }
    fn give_powerup(&self, p: i32, e: i32) {
        self.given.borrow_mut().push((p, e, -1, -1));
    }
    fn fire(&self, event: Event) {
        self.fired.borrow_mut().push(event);
    }
}

/// Message ids for the tests (any distinct numbers; the game assigns them at `RegUserMsg`).
pub fn register_all(s: &mut State) {
    for (i, (_, name)) in Kind::ALL.iter().enumerate() {
        s.msgs.register(name, 64 + i as i32);
    }
}

fn id(kind: Kind) -> i32 {
    64 + Kind::ALL.iter().position(|&(k, _)| k == kind).unwrap() as i32
}

/// The engine side of one message: `MESSAGE_BEGIN` with the receiving player, the encoder's
/// `WRITE_*` calls, `MESSAGE_END`. Then, like the FFI hook, `client_damage` and the death half.
pub struct Feed<'a> {
    pub s: &'a mut State,
}

impl Feed<'_> {
    pub fn send(s: &mut State, g: &FakeGame, kind: Kind, to: Option<i32>, write: impl FnOnce(&mut Feed)) {
        s.message_begin(id(kind), to);
        write(&mut Feed { s });
        if let Followup::Damage(hit) = s.message_end(g) {
            g.fire(hit.event());
            if let Some(d) = s.after_damage(g, hit) {
                g.fire(d);
            }
        }
    }
}

impl shared::protocol::hud_wire::Wire for Feed<'_> {
    fn byte(&mut self, v: i32) {
        self.s.message_int(v);
    }
    fn char(&mut self, v: i32) {
        self.s.message_int(v);
    }
    fn short(&mut self, v: i32) {
        self.s.message_int(v);
    }
    fn long(&mut self, v: i32) {
        self.s.message_int(v);
    }
    fn coord(&mut self, v: f32) {
        self.s.message_float(v);
    }
    fn string(&mut self, _s: &str) {
        self.s.message_string();
    }
}

impl shared::protocol::weapons::MsgSink for Feed<'_> {
    fn byte(&mut self, v: i32) {
        self.s.message_int(v);
    }
    fn char(&mut self, v: i32) {
        self.s.message_int(v);
    }
    fn short(&mut self, v: i32) {
        self.s.message_int(v);
    }
    fn string(&mut self, _s: &core::ffi::CStr) {
        self.s.message_string();
    }
}
