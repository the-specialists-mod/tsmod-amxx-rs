// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Test doubles: a fake [`Game`] and [`TsApi`], and [`Feed`], which runs the port's own message
//! encoders (`shared::protocol`) into the module's message hooks the way the engine's
//! `WRITE_*` calls would.

use crate::game::{
    BackendCall, BackendForwards, BotCmd, BotCmdResult, EntityInfo, Event, FrameInfo, Game, GameState, ItemInfo, ObjectiveInfo, PlayerInfo,
    PlayerSnapshot, TsApi, TsEvent, WeaponEntry,
};
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
    pub velocity: [f32; 3],
    /// `pev->friction`; [`FakeGame::player`] sets 1.
    pub friction: f32,
    pub team: i32,
    pub inflictor: Option<i32>,
    pub dmg_take: f32,
    pub class: String,
    pub owner: Option<i32>,
    pub name: String,
    pub auth: Option<String>,
    pub weapon_model: String,
}

#[derive(Default)]
pub struct FakeApi {
    pub info: RefCell<HashMap<i32, PlayerInfo>>,
    pub calls: RefCell<Vec<String>>,
    /// What setters return.
    pub result: Cell<i32>,
    pub knife_owner: Cell<i32>,
    // tsbot.inc's world and bot functions.
    pub snapshots: RefCell<HashMap<i32, PlayerSnapshot>>,
    pub inventories: RefCell<HashMap<i32, Vec<WeaponEntry>>>,
    pub items: RefCell<Vec<ItemInfo>>,
    pub objectives: RefCell<Vec<ObjectiveInfo>>,
    pub game_state: Cell<GameState>,
    pub frame: Cell<FrameInfo>,
    pub entities: RefCell<HashMap<i32, EntityInfo>>,
    pub keys: RefCell<HashMap<(i32, String), String>>,
    /// What `attach_backend` returns when not `OK` (0: a new id, 101 on).
    pub attach_result: Cell<i32>,
    /// `attach_backend` calls the slot's `activate` inside, as the server does for a backend
    /// `ts_bot_backend` names.
    pub activate_on_attach: Cell<bool>,
    pub names: RefCell<HashMap<i32, String>>,
    /// What `create_bot` and `run_cmd` give.
    pub create_result: Cell<i32>,
    pub run_result: Cell<i32>,
    pub cmd_result: Cell<BotCmdResult>,
    pub last_cmd: Cell<Option<(i32, BotCmd)>>,
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
    fn set_weapon_ammo(&self, p: i32, w: i32, c: i32, a: i32) -> Result<(), i32> {
        self.call(format!("set_weapon_ammo {p} {w} {c} {a}"))
    }
    fn projectile_owner(&self, _e: i32) -> Result<i32, i32> {
        Ok(self.knife_owner.get())
    }
    fn snapshot(&self, p: i32) -> Option<PlayerSnapshot> {
        self.snapshots.borrow().get(&p).copied().filter(|s| s.user_id != 0)
    }
    fn inventory(&self, p: i32) -> Result<Vec<WeaponEntry>, i32> {
        self.inventories.borrow().get(&p).cloned().ok_or(ts_plugin_api::NOT_PLAYER)
    }
    fn items(&self) -> Vec<ItemInfo> {
        self.items.borrow().clone()
    }
    fn objectives(&self, p: i32) -> Result<Vec<ObjectiveInfo>, i32> {
        if p != 0 && !self.snapshots.borrow().contains_key(&p) {
            return Err(ts_plugin_api::NOT_PLAYER);
        }
        Ok(self.objectives.borrow().clone())
    }
    fn game_state(&self) -> GameState {
        self.game_state.get()
    }
    fn frame_info(&self) -> FrameInfo {
        self.frame.get()
    }
    fn entity_info(&self, e: i32) -> Result<EntityInfo, i32> {
        self.entities.borrow().get(&e).copied().ok_or(ts_plugin_api::NOT_PLAYER)
    }
    fn entity_key(&self, e: i32, key: &str) -> Result<String, i32> {
        if !self.entities.borrow().contains_key(&e) {
            return Err(ts_plugin_api::NOT_PLAYER);
        }
        self.keys.borrow().get(&(e, key.to_owned())).cloned().ok_or(ts_plugin_api::UNSUPPORTED)
    }
    fn master_triggered(&self, master: &str, activator: i32) -> bool {
        self.calls.borrow_mut().push(format!("master_triggered {master} {activator}"));
        master.is_empty() || master == "open"
    }
    fn attach_backend(&self, slot: usize, name: &str) -> Result<i32, i32> {
        self.calls.borrow_mut().push(format!("attach_backend {slot} {name}"));
        let r = self.attach_result.get();
        if r != 0 {
            return Err(r);
        }
        let id = 101 + self.names.borrow().len() as i32;
        self.names.borrow_mut().insert(id, name.to_owned());
        if self.activate_on_attach.get() {
            // Inside register_bot_backend, as the server's takeover does.
            crate::bots::backend_call(&FakeGame::new(), slot, BackendCall::Activate);
        }
        Ok(id)
    }
    fn detach(&self, plugin: i32) -> i32 {
        self.call(format!("detach {plugin}")).map_or_else(|e| e, |()| 0)
    }
    fn data_dir(&self, plugin: i32) -> Result<String, i32> {
        self.names.borrow().get(&plugin).map(|n| format!("ts/tsmod/{n}/")).ok_or(ts_plugin_api::INVALID)
    }
    fn api_log(&self, plugin: i32, level: i32, msg: &str) {
        self.calls.borrow_mut().push(format!("log {plugin} {level} {msg}"));
    }
    fn create_bot(&self, plugin: i32, name: &[u8], model: &[u8], skill: f32) -> i32 {
        self.calls.borrow_mut().push(format!("create_bot {plugin} {:?} {:?} {skill}", name, model));
        self.create_result.get()
    }
    fn kick_bot(&self, p: i32) -> i32 {
        self.call(format!("kick_bot {p}")).map_or_else(|e| e, |()| 0)
    }
    fn run_cmd(&self, p: i32, cmd: &BotCmd) -> Result<BotCmdResult, i32> {
        self.last_cmd.set(Some((p, *cmd)));
        let r = self.run_result.get();
        if r == 0 { Ok(self.cmd_result.get()) } else { Err(r) }
    }
    fn bot_command(&self, p: i32, line: &[u8]) -> i32 {
        self.call(format!("bot_command {p} {:?}", line)).map_or_else(|e| e, |()| 0)
    }
    fn select_weapon(&self, p: i32, w: i32) -> i32 {
        self.call(format!("select_weapon {p} {w}")).map_or_else(|e| e, |()| 0)
    }
    fn buy(&self, p: i32, w: i32, att: i32, akimbo: i32, ammo: i32) -> i32 {
        self.call(format!("buy {p} {w} {att} {akimbo} {ammo}")).map_or_else(|e| e, |()| 0)
    }
    fn buy_item(&self, p: i32, item: i32) -> i32 {
        self.call(format!("buy_item {p} {item}")).map_or_else(|e| e, |()| 0)
    }
    fn clear_loadout(&self, p: i32) -> i32 {
        self.call(format!("clear_loadout {p}")).map_or_else(|e| e, |()| 0)
    }
    fn drop_weapon(&self, p: i32, w: i32) -> i32 {
        self.call(format!("drop_weapon {p} {w}")).map_or_else(|e| e, |()| 0)
    }
}

/// A plugin's forward in [`FakeGame::fire`].
pub type OnFire = Box<dyn Fn(&Event)>;

#[derive(Default)]
pub struct FakeGame {
    pub time: Cell<f32>,
    pub ents: RefCell<HashMap<i32, Ent>>,
    pub cvars: RefCell<HashMap<String, f32>>,
    pub api: Option<FakeApi>,
    pub logs: RefCell<Vec<String>>,
    pub fired: RefCell<Vec<Event>>,
    /// What `fire` returns (the forward's result), and what runs inside it (a plugin).
    pub fire_result: Cell<i32>,
    pub on_fire: RefCell<Option<OnFire>>,
    pub given: RefCell<Vec<(i32, i32, i32, i32)>>,
    pub commands: RefCell<Vec<(i32, String)>>,
    /// tsbot.inc's forwards: the multi-plugin ones fired, the backend's (as text), what the
    /// backend's return, and how many backend forward sets were dropped.
    pub ts_fired: RefCell<Vec<TsEvent>>,
    pub backend_fired: RefCell<Vec<String>>,
    pub backend_result: Cell<i32>,
    pub dropped: Cell<usize>,
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
        self.ents.borrow_mut().insert(
            i,
            Ent { client: true, alive: true, class: "player".into(), name: format!("p{i}"), friction: 1.0, ..Default::default() },
        );
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
    fn velocity(&self, e: i32) -> [f32; 3] {
        self.get(e, |x| x.velocity)
    }
    fn friction(&self, e: i32) -> f32 {
        self.get(e, |x| x.friction)
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
    fn weapon_model(&self, e: i32) -> String {
        self.get(e, |x| x.weapon_model.clone())
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
    fn client_cmd(&self, p: i32, cmd: &str) {
        self.commands.borrow_mut().push((p, cmd.into()));
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
    fn fire_ts(&self, event: &TsEvent) -> i32 {
        self.ts_fired.borrow_mut().push(event.clone());
        0
    }
    fn backend_forwards(&self, _amx: usize) -> BackendForwards {
        BackendForwards { add: 1, activate: 2, deactivate: 3, adopt: 4 }
    }
    fn drop_backend_forwards(&self, _f: &BackendForwards) {
        self.dropped.set(self.dropped.get() + 1);
    }
    fn fire_backend(&self, fwd: i32, call: &BackendCall) -> i32 {
        if fwd < 0 {
            return 0;
        }
        let text = match call {
            BackendCall::Add { name, model, skill } => {
                format!("add {} {} {skill}", String::from_utf8_lossy(name), String::from_utf8_lossy(model))
            }
            BackendCall::Activate => "activate".into(),
            BackendCall::Deactivate => "deactivate".into(),
            BackendCall::Adopt(p) => format!("adopt {p}"),
        };
        self.backend_fired.borrow_mut().push(text);
        self.backend_result.get()
    }
    fn fire(&self, event: Event) -> i32 {
        self.fired.borrow_mut().push(event);
        // A plugin's forward: may call natives (the module state is released).
        if let Some(f) = self.on_fire.borrow().as_ref() {
            f(&event);
        }
        self.fire_result.get()
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
    fn string(&mut self, s: &[u8]) {
        self.s.message_string(&String::from_utf8_lossy(s));
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
    fn string(&mut self, s: &core::ffi::CStr) {
        self.s.message_string(&s.to_string_lossy());
    }
}
