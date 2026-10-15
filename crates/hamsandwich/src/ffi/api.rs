// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! The server's entity hooks: the table, conversions between [`Val`] and the API's `HamArg`,
//! the hook the server calls, and [`HamApi`] for the natives.

use super::host::RealHost;
use crate::hooks::{Ty, Val};
use crate::host::HamApi;
use crate::state::{self, Action};
use core::ffi::{CStr, c_int};
use core::sync::atomic::{AtomicPtr, Ordering};
use std::cell::RefCell;
use std::ffi::CString;
use ts_plugin_api::{HamArg, HamCall, PluginApi, ham};

static API: AtomicPtr<PluginApi> = AtomicPtr::new(core::ptr::null_mut());

/// The table, once found.
pub fn table() -> Option<&'static PluginApi> {
    // SAFETY: NULL or the server's static table, checked by `find` (its version and size); it
    // lives as long as the game library.
    if let Some(t) = unsafe { API.load(Ordering::Relaxed).as_ref() } {
        return Some(t);
    }
    // Natives can run before `AMXX_PluginsLoaded` (a plugin's `plugin_precache`): look now.
    find().ok()
}

/// Finds the table and installs the hook.
pub fn find() -> Result<&'static PluginApi, String> {
    // SAFETY: as in `table`.
    if let Some(t) = unsafe { API.load(Ordering::Relaxed).as_ref() } {
        return Ok(t);
    }
    let t = amxx::tsapi::find()?;
    API.store(t as *const PluginApi as *mut PluginApi, Ordering::Relaxed);
    t.ham_set_hook_on();
    Ok(t)
}

/// Installing the module's hook.
pub trait SetHook {
    fn ham_set_hook_on(&self);
}

impl SetHook for PluginApi {
    fn ham_set_hook_on(&self) {
        (self.ham_set_hook)(Some(hook));
    }
}

thread_local! {
    /// Strings handed to the server for each call in progress, innermost last (pre pushes, post
    /// pops, as the frames).
    static CALL_STRINGS: RefCell<Vec<Vec<CString>>> = const { RefCell::new(Vec::new()) };
    /// The last string return value, which the server reads after the hook returns.
    static RET_STRING: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn kind(t: Ty) -> c_int {
    match t {
        Ty::Void => ham::kind::VOID,
        Ty::Int | Ty::Bool | Ty::Short => ham::kind::INT,
        Ty::Float => ham::kind::FLOAT,
        Ty::Vector => ham::kind::VECTOR,
        Ty::PVector => ham::kind::VECTOR_OUT,
        Ty::PFloat => ham::kind::FLOAT_OUT,
        Ty::Str => ham::kind::STRING,
        Ty::Cbase | Ty::Entvar | Ty::Edict => ham::kind::ENTITY,
        Ty::Trace => ham::kind::TRACE,
        Ty::ItemInfo => ham::kind::ITEMINFO,
    }
}

/// A value as a `HamArg`. A string's bytes are kept in `keep` until the caller drops it.
fn to_arg(t: Ty, v: &Val, keep: &mut Vec<CString>) -> HamArg {
    let mut a = HamArg { kind: kind(t), ..Default::default() };
    match v {
        Val::Int(i) | Val::Ent(i) => a.i = *i,
        Val::Float(f) => a.f = *f,
        Val::Vector(x) => a.v = *x,
        Val::Handle(h) => a.p = *h as usize as *const core::ffi::c_void,
        Val::Str(s) => {
            let c = CString::new(s.replace('\0', "")).unwrap_or_default();
            a.p = c.as_ptr().cast();
            keep.push(c);
        }
        Val::Void => {}
    }
    a
}

/// A `HamArg` as a value of type `t`.
fn from_arg(t: Ty, a: &HamArg) -> Val {
    match t {
        Ty::Void => Val::Void,
        Ty::Int | Ty::Bool | Ty::Short => Val::Int(a.i),
        Ty::Float | Ty::PFloat => Val::Float(a.f),
        Ty::Vector | Ty::PVector => Val::Vector(a.v),
        Ty::Cbase | Ty::Entvar | Ty::Edict => Val::Ent(a.i),
        // Pointers are 32 bits in every AMX Mod X build: a cell holds them.
        Ty::Trace | Ty::ItemInfo => Val::Handle(a.p as usize as amxx::Cell),
        Ty::Str if a.p.is_null() => Val::Str(String::new()),
        // SAFETY: the API passes a NUL-terminated string valid for the call.
        Ty::Str => Val::Str(unsafe { CStr::from_ptr(a.p.cast()) }.to_string_lossy().into_owned()),
    }
}

/// The hook the server calls around every watched function (`ham::Hook`).
unsafe extern "C" fn hook(call: *mut HamCall, post: c_int) -> c_int {
    // SAFETY: the server passes the call it is running, valid and exclusively ours during the
    // hook.
    let Some(call) = (unsafe { call.as_mut() }) else { return ham::action::CONTINUE };
    let Some(entry) = usize::try_from(call.ham).ok().and_then(crate::hooks::entry) else { return ham::action::CONTINUE };
    let n = (call.argc.max(0) as usize).min(entry.args.len()).min(ham::MAX_ARGS);
    let args: Vec<Val> = entry.args[..n].iter().zip(&call.args[..n]).map(|(&t, a)| from_arg(t, a)).collect();
    let host = RealHost;
    if post == 0 {
        let (action, new_args) = state::pre(&host, call.ham as usize, call.this, call.class, args);
        CALL_STRINGS.with(|k| {
            let mut strings = Vec::new();
            for (i, (&t, v)) in entry.args.iter().zip(&new_args).enumerate() {
                let orig_p = call.args[i].p;
                call.args[i] = to_arg(t, v, &mut strings);
                // An INT argument may carry a pointer too (Keyvalue's KeyValueData*): keep it.
                if call.args[i].kind == ham::kind::INT {
                    call.args[i].p = orig_p;
                }
            }
            k.borrow_mut().push(strings);
        });
        match action {
            Action::Supercede(v) => {
                set_ret(call, entry.ret, &v);
                ham::action::SUPERCEDE
            }
            _ => ham::action::CONTINUE,
        }
    } else {
        let orig = from_arg(entry.ret, &call.orig_ret);
        let action = state::post(&host, call.ham as usize, call.this, orig, Some(args));
        CALL_STRINGS.with(|k| k.borrow_mut().pop());
        match action {
            Action::Override(v) => {
                set_ret(call, entry.ret, &v);
                ham::action::OVERRIDE
            }
            _ => ham::action::CONTINUE,
        }
    }
}

fn set_ret(call: &mut HamCall, t: Ty, v: &Val) {
    let mut strings = Vec::new();
    call.ret = to_arg(t, v, &mut strings);
    if let Some(s) = strings.pop() {
        RET_STRING.with(|k| *k.borrow_mut() = Some(s));
    }
}

/// [`HamApi`] over the table.
pub struct RealHamApi(pub &'static PluginApi);
// SAFETY: the server's table is static for the life of the game library and only read; since
// version 3 on it also holds the engine's table and globals pointers, which this module never uses.
// Everything runs on the engine's thread.
unsafe impl Sync for RealHamApi {}
// SAFETY: as above.
unsafe impl Send for RealHamApi {}

impl HamApi for RealHamApi {
    fn set_hook(&self, on: bool) {
        (self.0.ham_set_hook)(if on { Some(hook) } else { None });
    }
    fn watch(&self, h: i32, on: bool) -> i32 {
        (self.0.ham_watch)(h, on as c_int)
    }
    fn supported(&self, h: i32, class: i32) -> i32 {
        (self.0.ham_supported)(h, class)
    }
    fn class_id(&self, classname: &str) -> i32 {
        let c = CString::new(classname.replace('\0', "")).unwrap_or_default();
        // SAFETY: a C string valid for the call.
        unsafe { (self.0.class_id)(c.as_ptr()) }
    }
    fn entity_class_id(&self, ent: i32) -> i32 {
        (self.0.entity_class_id)(ent)
    }
    fn execute(&self, h: i32, this: i32, args: &[(Ty, Val)], ret: Ty, run_hooks: bool) -> Result<(Val, Vec<Val>), i32> {
        let class = (self.0.entity_class_id)(this);
        if class < 0 {
            return Err(class);
        }
        let mut keep = Vec::new();
        let mut call = HamCall { ham: h, this, class, argc: args.len().min(ham::MAX_ARGS) as c_int, ..Default::default() };
        for (i, (t, v)) in args.iter().take(ham::MAX_ARGS).enumerate() {
            call.args[i] = to_arg(*t, v, &mut keep);
        }
        call.ret = HamArg { kind: kind(ret), ..Default::default() };
        // SAFETY: a fully initialised call whose string arguments (`keep`) outlive it.
        let rc = unsafe { (self.0.ham_execute)(&mut call, run_hooks as c_int) };
        if rc < 0 {
            return Err(rc);
        }
        let out = args.iter().zip(&call.args).map(|((t, _), a)| from_arg(*t, a)).collect();
        Ok((from_arg(ret, &call.ret), out))
    }
    fn private_size(&self, ent: i32) -> i32 {
        (self.0.private_size)(ent)
    }
}
