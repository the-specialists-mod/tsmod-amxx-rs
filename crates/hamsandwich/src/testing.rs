// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! A fake host that is AMX Mod X (forwards are Rust closures, which may call natives) and the
//! server (a tiny game with health, run through the hook protocol the server follows: pre, the
//! function unless superceded, post deciding the return).

use crate::hooks::{FwdArg, FwdType, Ty, Val};
use crate::host::{EntState, HamApi, Host, ItemVal};
use crate::state::{self, Action};
use amxx::Cell;
use std::cell::{Cell as StdCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use ts_plugin_api as api;

pub type Public = Rc<dyn Fn(&FakeHost, &[FwdArg]) -> Cell>;

pub const TAKEDAMAGE: i32 = 9;
pub const SPAWN: i32 = 0;
pub const KILLED: i32 = 11;
pub const THINK: i32 = 41;
pub const TOUCH: i32 = 42;
pub const CENTER: i32 = 48;
pub const GIVESLOWMUL: i32 = 123;
pub const PLAYER: i32 = 1;
pub const BREAKABLE: i32 = 2;

#[derive(Default)]
pub struct FakeHost {
    pub has_api: bool,
    pub publics: RefCell<HashMap<String, Public>>,
    pub forwards: RefCell<Vec<String>>,
    pub failed: RefCell<Vec<(i32, i32, String)>>,
    pub logs: RefCell<Vec<String>>,
    pub health: RefCell<HashMap<i32, f32>>,
    pub watched: RefCell<Vec<i32>>,
    /// Calls of the real function (ham, this).
    pub ran: RefCell<Vec<(i32, i32)>>,
    pub item_infos: RefCell<HashMap<Cell, [ItemVal; 11]>>,
    pub next_ii: StdCell<Cell>,
}

impl FakeHost {
    pub fn new() -> Self {
        let h = FakeHost { has_api: true, ..Default::default() };
        for e in [1, 2, 3, 50] {
            h.health.borrow_mut().insert(e, 100.0);
        }
        h
    }

    pub fn public(&self, name: &str, f: impl Fn(&FakeHost, &[FwdArg]) -> Cell + 'static) {
        self.publics.borrow_mut().insert(name.into(), Rc::new(f));
    }

    pub fn health(&self, e: i32) -> f32 {
        self.health.borrow()[&e]
    }

    fn class_of(&self, e: i32) -> i32 {
        match e {
            1..=32 => PLAYER,
            50 => BREAKABLE,
            _ => api::NOT_PLAYER,
        }
    }

    /// The function itself.
    fn run(&self, ham: i32, this: i32, args: &[Val]) -> Val {
        self.ran.borrow_mut().push((ham, this));
        match ham {
            TAKEDAMAGE => {
                let Val::Float(d) = args[2] else { return Val::Int(0) };
                *self.health.borrow_mut().get_mut(&this).unwrap() -= d;
                Val::Int(1)
            }
            SPAWN => {
                self.health.borrow_mut().insert(this, 100.0);
                Val::Void
            }
            CENTER => Val::Vector([1.0, 2.0, 3.0]),
            GIVESLOWMUL => Val::Float(0.5),
            _ => Val::Void,
        }
    }

    /// The game calling `ham` on `this` (hooks run when watched).
    pub fn game_call(&self, ham: i32, this: i32, args: Vec<Val>) -> Val {
        let ret_ty = crate::hooks::entry(ham as usize).map_or(Ty::Void, |e| e.ret);
        if !self.watched.borrow().contains(&ham) {
            return self.run(ham, this, &args);
        }
        let (action, args) = state::pre(self, ham as usize, this, self.class_of(this), args);
        let orig = match action {
            Action::Supercede(_) => ret_ty.zero(),
            _ => self.run(ham, this, &args),
        };
        match state::post(self, ham as usize, this, orig.clone(), Some(args)) {
            Action::Override(v) => v,
            _ => orig,
        }
    }
}

impl HamApi for FakeHost {
    fn set_hook(&self, _on: bool) {}
    fn watch(&self, ham: i32, on: bool) -> i32 {
        if ![SPAWN, TAKEDAMAGE, KILLED, THINK, TOUCH, CENTER, GIVESLOWMUL].contains(&ham) {
            return api::UNSUPPORTED;
        }
        let mut w = self.watched.borrow_mut();
        w.retain(|&h| h != ham);
        if on {
            w.push(ham);
        }
        api::OK
    }
    fn supported(&self, ham: i32, class: i32) -> i32 {
        if ham == THINK && class == PLAYER { api::UNSUPPORTED } else { 1 }
    }
    fn class_id(&self, classname: &str) -> i32 {
        match classname {
            "player" => PLAYER,
            "func_breakable" => BREAKABLE,
            _ => api::INVALID,
        }
    }
    fn entity_class_id(&self, ent: i32) -> i32 {
        self.class_of(ent)
    }
    fn execute(&self, ham: i32, this: i32, args: &[(Ty, Val)], _ret: Ty, run_hooks: bool) -> Result<(Val, Vec<Val>), i32> {
        let vals: Vec<Val> = args.iter().map(|(_, v)| v.clone()).collect();
        let r = if run_hooks { self.game_call(ham, this, vals.clone()) } else { self.run(ham, this, &vals) };
        Ok((r, vals))
    }
}

impl Host for FakeHost {
    fn api(&self) -> Option<&dyn HamApi> {
        self.has_api.then_some(self as &dyn HamApi)
    }
    fn register_forward(&self, _amx: usize, name: &str, _params: &[FwdType]) -> i32 {
        if !self.publics.borrow().contains_key(name) {
            return -1;
        }
        self.forwards.borrow_mut().push(name.into());
        self.forwards.borrow().len() as i32 - 1
    }
    fn execute_forward(&self, id: i32, args: &[FwdArg]) -> Cell {
        let name = self.forwards.borrow()[id as usize].clone();
        let f = self.publics.borrow()[&name].clone();
        f(self, args)
    }
    fn fail_plugin(&self, _amx: usize, ham: i32, err: i32, reason: &str) {
        self.failed.borrow_mut().push((ham, err, reason.into()));
    }
    fn max_entities(&self) -> i32 {
        900
    }
    fn entity_state(&self, ent: i32) -> EntState {
        if self.health.borrow().contains_key(&ent) || ent == 0 { EntState::Ok } else { EntState::Free }
    }
    fn log(&self, msg: &str) {
        self.logs.borrow_mut().push(msg.into());
    }
    fn item_info_create(&self) -> Cell {
        let h = self.next_ii.get() + 0x1000;
        self.next_ii.set(h);
        self.item_infos
            .borrow_mut()
            .insert(h, std::array::from_fn(|i| if [2, 4, 6].contains(&i) { ItemVal::Str(String::new()) } else { ItemVal::Int(0) }));
        h
    }
    fn item_info_free(&self, h: Cell) -> bool {
        self.item_infos.borrow_mut().remove(&h).is_some()
    }
    fn item_info_owned(&self, h: Cell) -> bool {
        self.item_infos.borrow().contains_key(&h)
    }
    fn item_info_get(&self, h: Cell, field: i32) -> Option<ItemVal> {
        self.item_infos.borrow().get(&h).and_then(|f| f.get(field as usize).cloned())
    }
    fn item_info_set(&self, h: Cell, field: i32, v: ItemVal) -> bool {
        self.item_infos.borrow_mut().get_mut(&h).and_then(|f| f.get_mut(field as usize)).map(|x| *x = v).is_some()
    }
}

/// The cell of a forward argument.
pub fn cell(a: &FwdArg) -> Cell {
    match a {
        FwdArg::Cell(c) => *c,
        _ => panic!("not a cell: {a:?}"),
    }
}
