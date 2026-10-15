// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports the hook bookkeeping of
// modules/hamsandwich (hook.h's `Hook` pre/post lists, forward.h's `Forward` states) and the
// call protocol of hook_callbacks.cpp (`PRE_START` ... `CHECK_RETURN`), Copyright (C) The AMX
// Mod X Development Team.

//! Registered hooks and the calls in progress, and the dispatcher the server's hook runs.
//!
//! Stock patches a C++ vtable per class and runs, around the original function:
//! pre forwards (each sees the parameters as earlier ones changed them; the highest result wins,
//! `result`), the function unless `result` reached `HAM_SUPERCEDE`, post forwards, then returns
//! the plugins' value when the *last* forward that ran said `HAM_OVERRIDE` or more
//! (`thisresult`), else the function's. Here the server calls [`pre`] and [`post`] around its own
//! function (`ts-plugin-api`'s entity hooks, version 4); the state lives in a [`Frame`] per call, on a stack, so
//! nested calls (a forward dealing damage, `ExecuteHamB`) work as in stock. The server calls
//! post exactly when it called pre, innermost first, so post belongs to the top frame.
//!
//! The state is a thread-local `RefCell`; forwards run with it released, since plugins call the
//! data natives from them.

use crate::hooklist::HOOKS;
use crate::hooks::{FwdArg, Ty, Val, result};
use crate::host::Host;
use amxx::Cell;
use std::cell::RefCell;

/// `Forward`: one plugin public hooked to one function and class.
#[derive(Clone, Debug)]
pub struct Forward {
    /// AMX Mod X forward id.
    pub id: i32,
    /// `FSTATE_OK` (true) or `FSTATE_STOP`.
    pub enabled: bool,
}

/// A function's hooks for one class (stock's `Hook` on one vtable).
#[derive(Clone, Debug, Default)]
pub struct ClassHooks {
    pub class: i32,
    pub classname: String,
    /// Forward handles (indices into [`State::forwards`] + 1).
    pub pre: Vec<Cell>,
    pub post: Vec<Cell>,
}

/// One parameter of a call in progress (`Data` in stock's `ParamStack`).
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub ty: Ty,
    /// What the function gets.
    pub val: Val,
    /// For entities after `SetHamParamEntity` (not `...2`): the index forwards still see.
    pub fwd_ent: Option<i32>,
}

impl Param {
    pub fn new(ty: Ty, val: Val) -> Param {
        Param { ty, val, fwd_ent: None }
    }

    fn forward_arg(&self) -> FwdArg {
        match self.fwd_ent {
            Some(i) => FwdArg::Cell(i),
            None => self.val.forward_arg(),
        }
    }
}

/// A call in progress.
#[derive(Clone, Debug)]
pub struct Frame {
    /// `this` as the server passed it, which with `ham` matches [`post`] to its [`pre`] (the
    /// server's `HamCall` may move in between, so its address can't).
    pub this0: i32,
    pub ham: usize,
    pub class: i32,
    /// `params[0]` is `this` (stock's `MAKE_VECTOR` puts it first, so `SetHamParam*(1, ...)` is
    /// `this` and 2 the first argument).
    pub params: Vec<Param>,
    pub ret_ty: Ty,
    /// `ret` and `origret`.
    pub ret: Val,
    pub orig_ret: Val,
    /// The highest result so far (`GetHamReturnStatus`) and the last forward's.
    pub result: i32,
    pub thisresult: i32,
}

impl Frame {
    /// The arguments of the forward: `this` and the parameters.
    pub fn forward_args(&self) -> Vec<FwdArg> {
        self.params.iter().map(Param::forward_arg).collect()
    }
}

/// The module's state.
#[derive(Default)]
pub struct State {
    /// Handle `h` is `forwards[h - 1]`.
    pub forwards: Vec<Forward>,
    /// Per `Ham` function, per class.
    pub hooks: Vec<Vec<ClassHooks>>,
    /// Functions the server calls the hook for.
    pub watched: Vec<bool>,
    /// Whether the server has the function at all (`IsHamValid`), once asked.
    pub available: Vec<Option<bool>>,
    pub frames: Vec<Frame>,
    /// Log-once keys.
    pub logged: Vec<String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::new());
}

/// Runs `f` on the state; `None` if it is borrowed further up the stack.
pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.try_borrow_mut().ok().map(|mut s| f(&mut s)))
}

impl State {
    pub fn new() -> State {
        State {
            hooks: vec![Vec::new(); HOOKS.len()],
            watched: vec![false; HOOKS.len()],
            available: vec![None; HOOKS.len()],
            ..Default::default()
        }
    }

    /// Adds a forward to `ham`'s hooks for `class`; returns its handle.
    pub fn add_hook(&mut self, ham: usize, class: i32, classname: &str, post: bool, id: i32) -> Cell {
        self.forwards.push(Forward { id, enabled: true });
        let h = self.forwards.len() as Cell;
        let list = &mut self.hooks[ham];
        let i = match list.iter().position(|c| c.class == class) {
            Some(i) => i,
            None => {
                list.push(ClassHooks { class, classname: classname.into(), ..Default::default() });
                list.len() - 1
            }
        };
        if post {
            list[i].post.push(h)
        } else {
            list[i].pre.push(h)
        }
        h
    }

    /// The forward behind a handle.
    pub fn forward(&mut self, h: Cell) -> Option<&mut Forward> {
        usize::try_from(h).ok().and_then(|h| h.checked_sub(1)).and_then(|i| self.forwards.get_mut(i))
    }

    /// The pre or post forwards for a call.
    fn handles(&self, ham: usize, class: i32, post: bool) -> Vec<Cell> {
        self.hooks
            .get(ham)
            .and_then(|l| l.iter().find(|c| c.class == class))
            .map(|c| if post { c.post.clone() } else { c.pre.clone() })
            .unwrap_or_default()
    }

    /// `OnPluginsUnloaded`: every hook goes (the plugins' forwards are gone).
    pub fn clear(&mut self) {
        self.forwards.clear();
        for l in &mut self.hooks {
            l.clear();
        }
        self.frames.clear();
    }

    /// Whether an ItemInfo handle belongs to a call in progress.
    pub fn item_info_in_call(&self, h: Cell) -> bool {
        h != 0 && self.frames.iter().any(|f| f.params.iter().any(|p| p.ty == Ty::ItemInfo && p.val == Val::Handle(h)))
    }
}

/// What the server should do after a hook call.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Carry on (before) / keep the function's result (after).
    Continue,
    /// Before: skip the function and return this.
    Supercede(Val),
    /// After: return this instead.
    Override(Val),
}

/// Runs the pre or post forwards of the top frame, one at a time with the state released.
fn run_forwards(host: &dyn Host, post: bool) {
    let Some(handles) = with(|s| s.frames.last().map(|f| s.handles(f.ham, f.class, post))).flatten() else { return };
    for h in handles {
        let Some(job) = with(|s| {
            let fwd = s.forward(h).filter(|f| f.enabled).map(|f| f.id);
            fwd.and_then(|id| s.frames.last().map(|f| (id, f.forward_args())))
        }) else {
            return;
        };
        let ran = job.map(|(id, args)| host.execute_forward(id, &args));
        with(|s| {
            if let Some(f) = s.frames.last_mut() {
                if let Some(r) = ran {
                    f.thisresult = r;
                }
                // Stock compares even after a stopped forward (with the previous result).
                if f.thisresult > f.result {
                    f.result = f.thisresult;
                }
            }
        });
    }
}

/// Before the server runs `ham` on `this`: pushes the call and runs the pre forwards. The
/// arguments the function should get are in the returned frame's parameters.
pub fn pre(host: &dyn Host, ham: usize, this: i32, class: i32, args: Vec<Val>) -> (Action, Vec<Val>) {
    let Some(entry) = crate::hooks::entry(ham) else { return (Action::Continue, args) };
    let mut params = vec![Param::new(Ty::Cbase, Val::Ent(this))];
    params.extend(entry.args.iter().zip(args).map(|(&t, v)| Param::new(t, v)));
    let frame = Frame {
        this0: this,
        ham,
        class,
        params,
        ret_ty: entry.ret,
        ret: entry.ret.zero(),
        orig_ret: entry.ret.zero(),
        result: result::UNSET,
        thisresult: result::UNSET,
    };
    with(|s| s.frames.push(frame));
    run_forwards(host, false);
    with(|s| {
        let f = s.frames.last()?;
        let args: Vec<Val> = f.params.iter().skip(1).map(|p| p.val.clone()).collect();
        let action = if f.result >= result::SUPERCEDE { Action::Supercede(f.ret.clone()) } else { Action::Continue };
        Some((action, args))
    })
    .flatten()
    .unwrap_or((Action::Continue, Vec::new()))
}

/// After the function (or after a supercede, when `orig_ret` is the type's zero): runs the post
/// forwards, pops the call and decides the return value by the last forward's result, as stock's
/// `CHECK_RETURN`. `out_args` are the arguments as the function left them (pointer parameters).
pub fn post(host: &dyn Host, ham: usize, this: i32, orig_ret: Val, out_args: Option<Vec<Val>>) -> Action {
    let found = with(|s| {
        let i = s.frames.iter().rposition(|f| f.ham == ham && f.this0 == this)?;
        // A frame left over from a call whose post never came (shouldn't happen) is dropped.
        s.frames.truncate(i + 1);
        let f = s.frames.last_mut()?;
        f.orig_ret = orig_ret;
        if let Some(out) = out_args {
            for (p, v) in f.params.iter_mut().skip(1).zip(out) {
                if matches!(p.ty, Ty::PVector | Ty::PFloat) {
                    p.val = v;
                }
            }
        }
        Some(())
    })
    .flatten();
    if found.is_none() {
        return Action::Continue;
    }
    run_forwards(host, true);
    with(|s| {
        let f = s.frames.pop()?;
        Some(if f.thisresult < result::OVERRIDE { Action::Continue } else { Action::Override(f.ret) })
    })
    .flatten()
    .unwrap_or(Action::Continue)
}
