// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports the natives of
// modules/hamsandwich: hook_native.cpp (`RegisterNatives`), call_funcs.cpp (`Call_*`),
// DataHandler.cpp (`ReturnNatives`) and pdata.cpp, Copyright (C) The AMX Mod X Development Team.

//! The natives, as `fn(&dyn Host, &mut dyn Args) -> Cell`. Names, arguments, return values and
//! error messages are stock's; `ffi` binds them.

use crate::hooklist::HOOKS;
use crate::hooks::{Ty, Val, error};
use crate::host::{EntState, Host, ItemVal};
use crate::state::{Param, with};
use amxx::Cell;
use amxx::native::Args;
use ts_plugin_api as api;

fn log_once(host: &dyn Host, key: &str, msg: impl FnOnce() -> String) {
    let first = with(|s| {
        let first = !s.logged.iter().any(|k| k == key);
        if first {
            s.logged.push(key.to_owned());
        }
        first
    });
    if first.unwrap_or(false) {
        host.log(&msg());
    }
}

/// Whether the server has `ham` for some class (stock: "configured in gamedata"). Asking
/// `ham_watch` with the current setting changes nothing.
pub fn available(host: &dyn Host, ham: usize) -> bool {
    let Some(entry) = HOOKS.get(ham) else { return false };
    if entry.removed || entry.deprecated {
        return false;
    }
    if let Some(Some(v)) = with(|s| s.available[ham]) {
        return v;
    }
    let Some(api) = host.api() else { return false };
    let on = with(|s| s.watched[ham]).unwrap_or(false);
    let ok = api.watch(ham as i32, on) == api::OK;
    with(|s| s.available[ham] = Some(ok));
    ok
}

/// `CHECK_FUNCTION`: a function the server has, else the plugin fails (`FailPlugin`).
fn check_function(host: &dyn Host, a: &mut dyn Args, func: Cell) -> Option<usize> {
    let max = HOOKS.len() as Cell;
    let fail = |err, msg: String| {
        host.fail_plugin(a.amx(), func, err, &msg);
        None
    };
    if !(0..max).contains(&func) {
        return fail(error::INVALID_FUNC, format!("Function out of bounds.  Got: {func}  Max: {}", max - 1));
    }
    let e = &HOOKS[func as usize];
    if e.removed {
        return fail(error::FUNC_NOT_AVAILABLE, format!("Function {} is no more available in the mod.", e.name));
    }
    if !available(host, func as usize) {
        if host.api().is_none() {
            log_once(host, "noapi", || {
                format!(
                    "this server has no TS plugin API version {} (entity hooks): hamsandwich can't hook or call anything",
                    ts_plugin_api::VERSION
                )
            });
        }
        return fail(error::FUNC_NOT_CONFIGURED, format!("Function {} is not configured in gamedata.", e.name));
    }
    Some(func as usize)
}

/// `CHECK_ENTITY`.
fn check_entity(host: &dyn Host, a: &mut dyn Args, ent: Cell) -> bool {
    if ent < 0 || ent > host.max_entities() {
        a.error(&format!("Entity out of range ({ent})"));
        return false;
    }
    match host.entity_state(ent) {
        EntState::Free => {
            a.error(&format!("Invalid entity ({ent})"));
            false
        }
        EntState::NoPrivateData => {
            a.error(&format!("Entity has null private data ({ent})"));
            false
        }
        EntState::Ok => true,
    }
}

/// The class a hook is for, and how stock's messages name it.
struct Target<'a> {
    class: i32,
    classname: &'a str,
    what: &'a str,
}

/// `Ham:function`'s hook for a class (shared by RegisterHam and RegisterHamFromEntity).
fn register(host: &dyn Host, a: &mut dyn Args, ham: usize, t: Target, function: &str, post: bool) -> Cell {
    let Target { class, classname, what } = t;
    let Some(api) = host.api() else { return 0 };
    if api.supported(ham as i32, class) == api::UNSUPPORTED {
        a.error(&format!("Failed to retrieve vtable for {what}, hook for \"{function}\" not active."));
        return 0;
    }
    let id = host.register_forward(a.amx(), function, &HOOKS[ham].forward_params());
    if id == -1 {
        a.error(&format!("Function {function} not found."));
        return 0;
    }
    let need_watch = with(|s| !s.watched[ham]).unwrap_or(false);
    if need_watch && api.watch(ham as i32, true) == api::OK {
        with(|s| s.watched[ham] = true);
    }
    with(|s| s.add_hook(ham, class, classname, post, id)).unwrap_or(0)
}

/// `RegisterHam(Ham:function, const EntityClass[], const Callback[], Post=0, bool:specialbot=false)`.
/// Every player is one class on the Rust server, so `specialbot` needs nothing.
pub fn register_ham(host: &dyn Host, a: &mut dyn Args) -> Cell {
    let func = a.cell(1);
    let Some(ham) = check_function(host, a, func) else { return 0 };
    let (function, classname, post) = (a.string(3), a.string(2), a.cell(4) != 0);
    let class = host.api().map_or(api::INVALID, |api| api.class_id(&classname));
    if class <= 0 {
        a.error(&format!("Failed to retrieve classtype for \"{classname}\", hook for \"{function}\" not active."));
        return 0;
    }
    let what = format!("\"{classname}\"");
    register(host, a, ham, Target { class, classname: &classname, what: &what }, &function, post)
}

/// `RegisterHamFromEntity(Ham:function, EntityId, const Callback[], Post=0)`.
pub fn register_ham_from_entity(host: &dyn Host, a: &mut dyn Args) -> Cell {
    let func = a.cell(1);
    let Some(ham) = check_function(host, a, func) else { return 0 };
    let (ent, function, post) = (a.cell(2), a.string(3), a.cell(4) != 0);
    let class = host.api().map_or(api::NOT_PLAYER, |api| api.entity_class_id(ent));
    if class <= 0 {
        a.error(&format!("Failed to retrieve classtype for entity id \"{ent}\", hook for \"{function}\" not active."));
        return 0;
    }
    let (classname, what) = (format!("entity {ent}"), format!("entity id \"{ent}\""));
    register(host, a, ham, Target { class, classname: &classname, what: &what }, &function, post)
}

fn set_forward_state(a: &mut dyn Args, enabled: bool) -> Cell {
    let h = a.cell(1);
    let ok = h != 0 && with(|s| s.forward(h).map(|f| f.enabled = enabled).is_some()).unwrap_or(false);
    if !ok {
        a.error("Invalid HamHook handle.");
        return -1;
    }
    0
}

/// `DisableHamForward(HamHook:fwd)`.
pub fn disable_ham_forward(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_forward_state(a, false)
}

/// `EnableHamForward(HamHook:fwd)`.
pub fn enable_ham_forward(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_forward_state(a, true)
}

/// `IsHamValid(Ham:function)`.
pub fn is_ham_valid(host: &dyn Host, a: &mut dyn Args) -> Cell {
    usize::try_from(a.cell(1)).is_ok_and(|f| f < HOOKS.len() && available(host, f)) as Cell
}

fn floats(cells: &[Cell]) -> [f32; 3] {
    [0, 1, 2].map(|i| f32::from_bits(cells.get(i).copied().unwrap_or(0) as u32))
}

fn cells(v: [f32; 3]) -> [Cell; 3] {
    v.map(|x| x.to_bits() as Cell)
}

/// `ExecuteHam` / `ExecuteHamB` (`call_funcs.cpp`): the arguments come by reference after
/// `this`, then the out parameters for a float, vector or string result.
fn execute(host: &dyn Host, a: &mut dyn Args, run_hooks: bool) -> Cell {
    let func = a.cell(1);
    let Some(ham) = check_function(host, a, func) else { return 0 };
    let Some(e) = crate::hooks::entry(ham) else { return 0 };
    // A deviating return's out parameter is optional (a stock-compiled call doesn't pass one).
    let ret_params = if crate::hooks::ret_deviates(ham) { 0 } else { e.ret.execute_ret_params() };
    let need = e.args.len() + ret_params + 2;
    if a.count() < need {
        a.error(&format!("Bad arg count.  Expected {need}, got {}.", a.count()));
        return 0;
    }
    let this = a.cell(2);
    if !check_entity(host, a, this) {
        return 0;
    }
    let mut args = Vec::with_capacity(e.args.len());
    for (k, &t) in e.args.iter().enumerate() {
        let i = 3 + k;
        let v = match t {
            Ty::Int | Ty::Bool | Ty::Short => Val::Int(a.read_array(i, 1)[0]),
            Ty::Float | Ty::PFloat => Val::Float(f32::from_bits(a.read_array(i, 1)[0] as u32)),
            Ty::Vector | Ty::PVector => Val::Vector(floats(&a.read_array(i, 3))),
            Ty::Str => Val::Str(a.string(i)),
            Ty::Cbase | Ty::Entvar | Ty::Edict => {
                let id = a.read_array(i, 1)[0];
                if !check_entity(host, a, id) {
                    return 0;
                }
                Val::Ent(id)
            }
            Ty::Trace => {
                let h = a.read_array(i, 1)[0];
                if h == 0 {
                    a.error("Null traceresult provided.");
                    return 0;
                }
                Val::Handle(h)
            }
            Ty::ItemInfo => Val::Handle(a.read_array(i, 1)[0]),
            Ty::Void => Val::Void,
        };
        args.push((t, v));
    }
    let Some(api) = host.api() else { return 0 };
    let (ret, out) = match api.execute(ham as i32, this, &args, e.ret, run_hooks) {
        Ok(r) => r,
        Err(api::NOT_PLAYER) => {
            a.error(&format!("Invalid entity ({this})"));
            return 0;
        }
        Err(api::BUSY) => {
            // The server answers BUSY for both: it doesn't say which.
            a.error(&format!("Entity {this} is busy (inside one of its own functions, or hooks nested too deep); {} didn't run.", e.name));
            return 0;
        }
        Err(api::UNSUPPORTED) => {
            a.error(&format!("Function {} isn't available for entity {this}.", e.name));
            return 0;
        }
        Err(code) => {
            a.error(&format!("Function {} refused its arguments (error {code}).", e.name));
            return 0;
        }
    };
    for (k, (&t, v)) in e.args.iter().zip(out).enumerate() {
        match (t, v) {
            (Ty::PVector, Val::Vector(v)) => a.write_array(3 + k, 0, &cells(v)),
            (Ty::PFloat, Val::Float(f)) => a.set_ref(3 + k, f.to_bits() as Cell),
            _ => {}
        }
    }
    let out_at = 3 + e.args.len();
    match (e.ret, ret) {
        (Ty::Float, Val::Float(f)) => {
            if a.count() >= out_at {
                a.set_ref(out_at, f.to_bits() as Cell);
            }
            1
        }
        (Ty::Vector, Val::Vector(v)) => {
            a.write_array(out_at, 0, &cells(v));
            1
        }
        (Ty::Str, Val::Str(s)) => {
            let max = a.read_array(out_at + 1, 1)[0];
            a.set_string(out_at, &s, max)
        }
        (Ty::Void, _) => 1,
        (_, Val::Int(i) | Val::Ent(i) | Val::Handle(i)) => i,
        _ => 0,
    }
}

/// `ExecuteHam(Ham:function, this, any:...)`: runs the function without the plugins' hooks.
pub fn execute_ham(host: &dyn Host, a: &mut dyn Args) -> Cell {
    execute(host, a, false)
}

/// `ExecuteHamB(Ham:function, this, any:...)`: with the hooks, as if the game called it.
pub fn execute_ham_b(host: &dyn Host, a: &mut dyn Args) -> Cell {
    execute(host, a, true)
}

// ---- DataHandler.cpp ----

/// Stock's `PARSE_RETURN`: 0 done, -2 "Data pointer is NULL!", -1 wrong type.
fn parse(a: &mut dyn Args, r: Result<(), Option<Ty>>) -> Cell {
    match r {
        Ok(()) => 0,
        Err(None) => {
            a.error("Data pointer is NULL!");
            -2
        }
        Err(Some(t)) => {
            a.error(&format!("Wrong data type (data is of type {})", t.data_name()));
            -1
        }
    }
}

#[derive(Clone, Copy)]
enum Which {
    Ret,
    Orig,
}

/// The return value (or original return value) of the call in progress.
fn return_value(a: &mut dyn Args, which: Which) -> Option<(Ty, Val)> {
    let r =
        with(|s| s.frames.last().map(|f| (f.ret_ty, if let Which::Ret = which { f.ret.clone() } else { f.orig_ret.clone() }))).flatten();
    if r.is_none() {
        a.error(if let Which::Ret = which { "ReturnStack is empty!" } else { "OrigReturnStack is empty!" });
    }
    r
}

fn get_int(a: &mut dyn Args, which: Which) -> Cell {
    let Some((t, v)) = return_value(a, which) else { return 0 };
    let r = match (t, v) {
        (Ty::Void, _) => Err(None),
        (Ty::Int | Ty::Bool | Ty::Short, Val::Int(i)) => {
            a.set_ref(1, i);
            Ok(())
        }
        (t, _) => Err(Some(t)),
    };
    parse(a, r)
}

fn get_float(a: &mut dyn Args, which: Which) -> Cell {
    let Some((t, v)) = return_value(a, which) else { return 0 };
    let r = match (t, v) {
        (Ty::Void, _) => Err(None),
        (Ty::Float, Val::Float(f)) => {
            a.set_ref(1, f.to_bits() as Cell);
            Ok(())
        }
        (t, _) => Err(Some(t)),
    };
    parse(a, r)
}

fn get_vector(a: &mut dyn Args, which: Which) -> Cell {
    let Some((t, v)) = return_value(a, which) else { return 0 };
    let r = match (t, v) {
        (Ty::Void, _) => Err(None),
        (Ty::Vector, Val::Vector(v)) => {
            a.write_array(1, 0, &cells(v));
            Ok(())
        }
        (t, _) => Err(Some(t)),
    };
    parse(a, r)
}

fn get_entity(a: &mut dyn Args, which: Which) -> Cell {
    let Some((t, v)) = return_value(a, which) else { return 0 };
    let r = match (t, v) {
        (Ty::Void, _) => Err(None),
        (Ty::Cbase | Ty::Entvar | Ty::Edict, Val::Ent(i)) => {
            a.set_ref(1, i);
            Ok(())
        }
        (t, _) => Err(Some(t)),
    };
    parse(a, r)
}

fn get_string(a: &mut dyn Args, which: Which) -> Cell {
    let Some((t, v)) = return_value(a, which) else { return 0 };
    let r = match (t, v) {
        (Ty::Void, _) => Err(None),
        (Ty::Str, Val::Str(s)) => {
            let len = a.cell(2);
            a.set_string(1, &s, len);
            Ok(())
        }
        (t, _) => Err(Some(t)),
    };
    parse(a, r)
}

macro_rules! getters {
    ($($name:ident => $f:ident, $w:ident;)*) => {$(
        #[doc = concat!("`", stringify!($name), "`.")]
        pub fn $name(_host: &dyn Host, a: &mut dyn Args) -> Cell {
            $f(a, Which::$w)
        }
    )*};
}

getters! {
    get_ham_return_integer => get_int, Ret;
    get_ham_return_float => get_float, Ret;
    get_ham_return_vector => get_vector, Ret;
    get_ham_return_entity => get_entity, Ret;
    get_ham_return_string => get_string, Ret;
    get_orig_ham_return_integer => get_int, Orig;
    get_orig_ham_return_float => get_float, Orig;
    get_orig_ham_return_vector => get_vector, Orig;
    get_orig_ham_return_entity => get_entity, Orig;
    get_orig_ham_return_string => get_string, Orig;
}

/// Sets the return value of the call in progress if `conv` accepts its type.
fn set_return(a: &mut dyn Args, conv: impl FnOnce(Ty, &mut dyn Args) -> Option<Val>) -> Cell {
    let Some(t) = with(|s| s.frames.last().map(|f| f.ret_ty)).flatten() else {
        a.error("ReturnStack is empty!");
        return 0;
    };
    let r = if t == Ty::Void {
        Err(None)
    } else {
        match conv(t, a) {
            Some(v) => {
                with(|s| {
                    if let Some(f) = s.frames.last_mut() {
                        f.ret = v;
                    }
                });
                Ok(())
            }
            None => Err(Some(t)),
        }
    };
    parse(a, r)
}

/// `SetHamReturnInteger(value)`.
pub fn set_ham_return_integer(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_return(a, |t, a| matches!(t, Ty::Int | Ty::Bool | Ty::Short).then(|| Val::Int(conv_int(t, a.cell(1)))))
}

/// `SetHamReturnFloat(Float:value)`.
pub fn set_ham_return_float(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_return(a, |t, a| (t == Ty::Float).then(|| Val::Float(a.float(1))))
}

/// `SetHamReturnVector(const Float:value[3])`.
pub fn set_ham_return_vector(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_return(a, |t, a| (t == Ty::Vector).then(|| Val::Vector(floats(&a.read_array(1, 3)))))
}

/// `SetHamReturnEntity(value)`.
pub fn set_ham_return_entity(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_return(a, |t, a| t.is_entity().then(|| Val::Ent(a.cell(1))))
}

/// `SetHamReturnString(const value[])`.
pub fn set_ham_return_string(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_return(a, |t, a| (t == Ty::Str).then(|| Val::Str(a.string(1))))
}

/// `GetHamReturnStatus()`: the highest result so far.
pub fn get_ham_return_status(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    match with(|s| s.frames.last().map(|f| f.result)).flatten() {
        Some(r) => r,
        None => {
            a.error("ReturnStatus is empty!");
            0
        }
    }
}

/// `Data::SetInt` on a bool or short.
fn conv_int(t: Ty, v: Cell) -> i32 {
    match t {
        Ty::Bool => (v > 0) as i32,
        Ty::Short => v as i16 as i32,
        _ => v,
    }
}

/// Sets parameter `which` (1 = `this`) of the call in progress if `conv` accepts its type.
fn set_param(a: &mut dyn Args, check_low: bool, conv: impl FnOnce(&mut Param, &mut dyn Args) -> bool) -> Cell {
    let which = a.cell(1);
    let Some(mut p) = with(|s| {
        s.frames.last().map(|f| (f.params.len(), usize::try_from(which).ok().and_then(|w| f.params.get(w.checked_sub(1)?).cloned())))
    })
    .flatten() else {
        a.error("ParamStack is empty!");
        return 0;
    };
    let Some(mut param) = p.1.take().filter(|_| !(check_low && which < 1)) else {
        a.error(&format!("Invalid parameter number, got {which}, expected {}", p.0));
        return 0;
    };
    let ty = param.ty;
    let r = if conv(&mut param, a) {
        with(|s| {
            if let Some(f) = s.frames.last_mut() {
                f.params[which as usize - 1] = param;
            }
        });
        Ok(())
    } else {
        Err(Some(ty))
    };
    parse(a, r)
}

/// `SetHamParamInteger(which, value)`: int, bool, short (and, as stock, trace and ItemInfo
/// handles).
pub fn set_ham_param_integer(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_param(a, false, |p, a| match p.ty {
        Ty::Int | Ty::Bool | Ty::Short => {
            p.val = Val::Int(conv_int(p.ty, a.cell(2)));
            true
        }
        Ty::Trace | Ty::ItemInfo => {
            p.val = Val::Handle(a.cell(2));
            true
        }
        _ => false,
    })
}

/// `SetHamParamFloat(which, Float:value)`.
pub fn set_ham_param_float(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_param(a, true, |p, a| {
        let ok = matches!(p.ty, Ty::Float | Ty::PFloat);
        if ok {
            p.val = Val::Float(a.float(2));
        }
        ok
    })
}

/// `SetHamParamVector(which, const Float:value[3])`.
pub fn set_ham_param_vector(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_param(a, false, |p, a| {
        let ok = matches!(p.ty, Ty::Vector | Ty::PVector);
        if ok {
            p.val = Val::Vector(floats(&a.read_array(2, 3)));
        }
        ok
    })
}

fn set_entity(a: &mut dyn Args, update_index: bool) -> Cell {
    set_param(a, false, |p, a| {
        if !p.ty.is_entity() {
            return false;
        }
        let old = match p.val {
            Val::Ent(i) => i,
            _ => -1,
        };
        p.val = Val::Ent(a.cell(2));
        // Stock's Entity keeps the index forwards see; Entity2 updates it too.
        p.fwd_ent = if update_index { None } else { Some(p.fwd_ent.unwrap_or(old)) };
        true
    })
}

/// `SetHamParamEntity(which, value)`: the function gets the new entity; later forwards still
/// see the old index (stock).
pub fn set_ham_param_entity(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_entity(a, false)
}

/// `SetHamParamEntity2(which, value)`: also updates the index later forwards see.
pub fn set_ham_param_entity2(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_entity(a, true)
}

/// `SetHamParamString(which, const output[])`.
pub fn set_ham_param_string(_host: &dyn Host, a: &mut dyn Args) -> Cell {
    set_param(a, false, |p, a| {
        let ok = p.ty == Ty::Str;
        if ok {
            p.val = Val::Str(a.string(2));
        }
        ok
    })
}

/// `SetHamParamTraceResult(which, tr_handle)`.
pub fn set_ham_param_trace_result(host: &dyn Host, a: &mut dyn Args) -> Cell {
    if a.cell(2) == 0 {
        a.error("Null traceresult provided.");
        return 0;
    }
    set_ham_param_integer(host, a)
}

/// `SetHamParamItemInfo(which, iteminfo_handle)`.
pub fn set_ham_param_item_info(host: &dyn Host, a: &mut dyn Args) -> Cell {
    if a.cell(2) == 0 {
        a.error("Null ItemInfo handle provided.");
        return 0;
    }
    set_ham_param_integer(host, a)
}

// ---- ItemInfo ----

const II_AMMO1: i32 = 2;
const II_AMMO2: i32 = 4;
const II_NAME: i32 = 6;
const II_LAST: i32 = 10;

/// A handle the module may read: one it created, or one a call in progress passed. Stock
/// dereferences any value; an unknown handle is a native error here instead of a crash.
fn valid_item_info(host: &dyn Host, a: &mut dyn Args, h: Cell) -> bool {
    if h == 0 {
        a.error("Null iteminfo handle provided.");
        return false;
    }
    if host.item_info_owned(h) || with(|s| s.item_info_in_call(h)).unwrap_or(false) {
        return true;
    }
    a.error(&format!("Invalid iteminfo handle ({h})."));
    false
}

/// `GetHamItemInfo(iteminfo_handle, HamItemInfo:type, any:...)`.
pub fn get_ham_item_info(host: &dyn Host, a: &mut dyn Args) -> Cell {
    let (h, field) = (a.cell(1), a.cell(2));
    if !valid_item_info(host, a, h) {
        return 0;
    }
    let is_str = matches!(field, II_AMMO1 | II_AMMO2 | II_NAME);
    if is_str && a.count() != 4 {
        a.error(&format!("Bad arg count.  Expected 4, got {}.", a.count()));
        return 0;
    }
    match host.item_info_get(h, field) {
        Some(ItemVal::Int(i)) => i,
        Some(ItemVal::Str(s)) => {
            let max = a.cell(4);
            a.set_string(3, &s, max)
        }
        None => {
            a.error(&format!("Unknown ItemInfo type {field}"));
            0
        }
    }
}

/// `SetHamItemInfo(iteminfo_handle, HamItemInfo:type, any:...)`: the value comes by reference.
pub fn set_ham_item_info(host: &dyn Host, a: &mut dyn Args) -> Cell {
    let (h, field) = (a.cell(1), a.cell(2));
    if !valid_item_info(host, a, h) {
        return 0;
    }
    if !(0..=II_LAST).contains(&field) {
        a.error(&format!("Unknown ItemInfo type {field}"));
        return 0;
    }
    if matches!(field, II_AMMO1 | II_AMMO2 | II_NAME) {
        let s = a.string(3);
        let n = s.len() as Cell;
        host.item_info_set(h, field, ItemVal::Str(s));
        return n;
    }
    let v = a.read_array(3, 1)[0];
    host.item_info_set(h, field, ItemVal::Int(v));
    1
}

/// `CreateHamItemInfo()`.
pub fn create_ham_item_info(host: &dyn Host, _a: &mut dyn Args) -> Cell {
    host.item_info_create()
}

/// `FreeHamItemInfo(itemInfo_handle)`.
pub fn free_ham_item_info(host: &dyn Host, a: &mut dyn Args) -> Cell {
    let h = a.cell(1);
    (h != 0 && host.item_info_free(h)) as Cell
}

// ---- pdata.cpp ----

/// `FM_CHECK_ENTITY`.
fn fm_check_entity(host: &dyn Host, a: &mut dyn Args, ent: Cell) -> bool {
    if ent < 0 || ent > host.max_entities() {
        a.error(&format!("Entity out of range ({ent})"));
        return false;
    }
    if ent != 0 && host.entity_state(ent) == EntState::Free {
        a.error(&format!("Invalid entity {ent}"));
        return false;
    }
    true
}

/// The offset in ints, with the Linux difference on Linux, as stock.
fn pdata_offset(a: &mut dyn Args, offset_arg: usize, diff_arg: usize) -> Option<Cell> {
    let mut off = a.cell(offset_arg);
    if cfg!(target_os = "linux") {
        off = off.wrapping_add(a.cell(diff_arg));
    }
    if off < 0 {
        a.error(&format!("Invalid offset provided. (got: {off})"));
        return None;
    }
    Some(off)
}

fn pdata_note(host: &dyn Host) {
    log_once(host, "pdata", || {
        format!(
            "get/set_pdata_cbase: the server has no TS plugin API (version {}), so its private data can't be checked; reads give -1 \
             (null), writes do nothing",
            ts_plugin_api::VERSION
        )
    });
}

/// Whether the server's private data holds pointers: the Rust server mirrors them into TS's 32-bit
/// words on 32-bit builds only (its `pdata.rs`), so on a 64-bit server `get_pdata_cbase` has
/// nothing to read and `set_pdata_cbase` nothing to write.
const POINTERS: bool = cfg!(target_pointer_width = "32");

fn no_pointers_note(host: &dyn Host) {
    log_once(host, "pdata64", || {
        "get/set_pdata_cbase: a 64-bit server's private data holds no pointers, so reads give -1 (null) and writes do nothing".into()
    });
}

/// Bytes 8..16 of the Rust server's private data hold its link to the entity
/// (`ts_plugin_api::PluginApi::private_size`).
const SERVER_LINK: core::ops::Range<usize> = 8..16;

/// The size of `ent`'s private data from the server (plugin API version 5), `None` without it.
fn pdata_size(host: &dyn Host, ent: Cell) -> Option<usize> {
    usize::try_from(host.api()?.private_size(ent)).ok()
}

fn past_end(a: &mut dyn Args, ent: Cell, off: Cell, size: usize) {
    a.error(&format!("Offset {off} is past the end of entity {ent}'s private data ({size} bytes)"));
}

/// `get_pdata_cbase(id, offset, linuxdiff=5, macdiff=5)` / `get_pdata_cbase_safe`: the entity
/// whose object the pointer at `offset` (ints) points to, -1 for NULL. The Rust server's private
/// data is no C++ object, but it mirrors the TS members plugins read at TS's offsets (plugin API
/// version 5: a player's `m_pActiveItem`, the `weapon_tsgun`'s `m_pPlayer`). Port changes: the
/// read is bounded by the block's size (a native error past it, where stock read past the
/// object), and a pointer to no entity is -1 (`_safe`: -2, as stock) where stock's would have
/// followed it. On a 64-bit server, -1 (see [`POINTERS`]).
fn get_cbase(host: &dyn Host, a: &mut dyn Args, safe: bool) -> Cell {
    let ent = a.cell(1);
    if !fm_check_entity(host, a, ent) {
        return 0;
    }
    let Some(off) = pdata_offset(a, 2, 3) else { return 0 };
    if !POINTERS {
        no_pointers_note(host);
        return -1;
    }
    let Some(size) = pdata_size(host, ent) else {
        pdata_note(host);
        return -1;
    };
    let Some(ptr) = host.pdata_word(ent, (off as usize).saturating_mul(4), size) else {
        past_end(a, ent, off, size);
        return 0;
    };
    if ptr == 0 {
        return -1;
    }
    host.entity_of_private(ptr as usize).unwrap_or(if safe { -2 } else { -1 })
}

pub fn get_pdata_cbase(host: &dyn Host, a: &mut dyn Args) -> Cell {
    get_cbase(host, a, false)
}

pub fn get_pdata_cbase_safe(host: &dyn Host, a: &mut dyn Args) -> Cell {
    get_cbase(host, a, true)
}

/// `set_pdata_cbase(id, offset, value, linuxdiff=5, macdiff=5)`: writes the pointer to `value`'s
/// object (NULL for -1) at `offset`, as stock. The server takes no pointer writes back (its
/// mirror rewrites `m_pActiveItem` and `m_pPlayer`), so only what plugins read sees it. Port
/// changes: bounded by the block's size, and the server's link (bytes 8..16) is refused, both
/// native errors. On a 64-bit server it writes nothing and returns 1 (see [`POINTERS`]).
pub fn set_pdata_cbase(host: &dyn Host, a: &mut dyn Args) -> Cell {
    let ent = a.cell(1);
    if !fm_check_entity(host, a, ent) {
        return 0;
    }
    let target = a.cell(3);
    if target != -1 && !fm_check_entity(host, a, target) {
        return 0;
    }
    // The linux difference comes after the value here.
    let Some(off) = pdata_offset(a, 2, 4) else { return 0 };
    if !POINTERS {
        no_pointers_note(host);
        return 1;
    }
    let Some(size) = pdata_size(host, ent) else {
        pdata_note(host);
        return 1;
    };
    let byte = (off as usize).saturating_mul(4);
    if byte < SERVER_LINK.end && byte + 4 > SERVER_LINK.start {
        a.error(&format!("Offset {off} would overwrite the server's link to entity {ent}"));
        return 0;
    }
    let value = if target == -1 { 0 } else { host.private_address(target) as u32 };
    if !host.set_pdata_word(ent, byte, size, value) {
        past_end(a, ent, off, size);
        return 0;
    }
    1
}
