// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports the metamod hooks of
// modules/ts/tsx/moduleconfig.cpp, Copyright (C) The AMX Mod X Development Team, Copyright (C)
// 2004 Lukasz Wlasinski.

//! The metamod hooks. Each converts the engine's pointers to entity indices and strings, runs
//! the state's handler, then fires any forward with the state released, and always tells
//! metamod it ignored the call (`RETURN_META(MRES_IGNORED)`).

use super::game::RealGame;
use crate::game::Game;
use crate::msgs::Followup;
use crate::state::{State, with};
use amxx::engine::{Edict, engine};
use amxx::meta;
use core::ffi::{c_char, c_int, c_ushort};
use goldsrc_sys::{DLL_FUNCTIONS, TraceResult, edict_t, enginefuncs_t, qboolean};

fn index(e: *const edict_t) -> Option<i32> {
    if e.is_null() {
        return None;
    }
    engine().map(|g| g.ent_index(Edict(e as *mut edict_t)))
}

/// Copies a C string the engine or game passed (NULL reads as empty).
fn text(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: hooks only pass the engine's or game's NUL-terminated strings, valid during the call.
    unsafe { core::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// Runs a handler on the state with the real game.
fn on_state<R>(f: impl FnOnce(&mut State, &RealGame) -> R) -> Option<R> {
    let g = RealGame::new();
    with(|s| f(s, &g))
}

pub fn dll(t: &mut DLL_FUNCTIONS) {
    t.pfnClientDisconnect = Some(client_disconnect);
    t.pfnServerDeactivate = Some(server_deactivate);
}

pub fn dll_post(t: &mut DLL_FUNCTIONS) {
    t.pfnClientConnect = Some(client_connect_post);
    t.pfnClientPutInServer = Some(client_put_in_server_post);
    t.pfnClientUserInfoChanged = Some(client_userinfo_changed_post);
    t.pfnServerActivate = Some(server_activate_post);
    t.pfnPlayerPreThink = Some(player_pre_think_post);
}

pub fn engine_pre(_t: &mut enginefuncs_t) {}

pub fn engine_post(t: &mut enginefuncs_t) {
    t.pfnTraceLine = Some(trace_line_post);
    t.pfnMessageBegin = Some(message_begin_post);
    t.pfnMessageEnd = Some(message_end_post);
    t.pfnWriteByte = Some(write_int_post);
    t.pfnWriteChar = Some(write_int_post);
    t.pfnWriteShort = Some(write_int_post);
    t.pfnWriteLong = Some(write_int_post);
    t.pfnWriteAngle = Some(write_float_post);
    t.pfnWriteCoord = Some(write_float_post);
    t.pfnWriteString = Some(write_string_post);
    t.pfnWriteEntity = Some(write_int_post);
    t.pfnRegUserMsg = Some(reg_user_msg_post);
    t.pfnPrecacheEvent = Some(precache_event_post);
    t.pfnPlaybackEvent = Some(playback_event_post);
}

extern "C" fn client_disconnect(e: *mut edict_t) {
    if let Some(i) = index(e) {
        on_state(|s, g| s.client_disconnect(g, i));
    }
    meta::ignored();
}

extern "C" fn server_deactivate() {
    on_state(|s, g| s.server_deactivate(g));
    super::save_rank();
    meta::ignored();
}

extern "C" fn client_connect_post(e: *mut edict_t, _name: *const c_char, address: *const c_char, _reject: *mut c_char) -> qboolean {
    if let Some(i) = index(e) {
        // SAFETY: the engine passes the client's address as a C string (or NULL).
        let addr = text(address);
        with(|s| s.client_connect(i, &addr));
    }
    meta::ignored();
    1
}

extern "C" fn client_put_in_server_post(e: *mut edict_t) {
    if let Some(i) = index(e) {
        on_state(|s, g| s.put_in_server(g, i));
    }
    meta::ignored();
}

extern "C" fn client_userinfo_changed_post(e: *mut edict_t, infobuffer: *mut c_char) {
    if let (Some(i), Some(eng)) = (index(e), engine())
        && !infobuffer.is_null()
    {
        // SAFETY: the engine's info buffer for this client, valid during the call.
        let name = unsafe { eng.info_key_value(infobuffer, c"name") };
        on_state(|s, g| s.userinfo_changed(g, i, &name));
    }
    meta::ignored();
}

extern "C" fn server_activate_post(_list: *mut edict_t, _count: c_int, _max: c_int) {
    // A module loaded after the game registered its messages missed `RegUserMsg`: ask metamod.
    with(|s| {
        for (k, name) in crate::msgs::Kind::ALL {
            if !s.msgs.has(k)
                && let Ok(c) = std::ffi::CString::new(name)
                && let Some(id) = meta::user_msg_id(&c)
            {
                s.msgs.register(name, id);
            }
        }
    });
    super::check_api();
    on_state(|s, g| s.server_activate(g));
    meta::ignored();
}

extern "C" fn player_pre_think_post(e: *mut edict_t) {
    if let Some(i) = index(e) {
        on_state(|s, g| s.pre_think(g, i));
    }
    meta::ignored();
}

extern "C" fn trace_line_post(_v1: *const f32, _v2: *const f32, _nomonsters: c_int, skip: *mut edict_t, tr: *mut TraceResult) {
    if !tr.is_null() && !skip.is_null() {
        // SAFETY: the engine passes the trace it just filled.
        let (hit, group) = unsafe { ((*tr).pHit, (*tr).iHitgroup) };
        if let (Some(shooter), Some(hit)) = (index(skip), index(hit)) {
            on_state(|s, g| s.trace_line(shooter, hit, group, g));
        }
    }
    meta::ignored();
}

extern "C" fn message_begin_post(_dest: c_int, msg_type: c_int, _origin: *const f32, ed: *mut edict_t) {
    let p = index(ed);
    with(|s| s.message_begin(msg_type, p));
    meta::ignored();
}

extern "C" fn message_end_post() {
    let g = RealGame::new();
    if let Some(Followup::Damage(hit)) = with(|s| s.message_end(&g)) {
        g.fire(hit.event());
        if let Some(Some(death)) = with(|s| s.after_damage(&g, hit)) {
            g.fire(death);
        }
    }
    meta::ignored();
}

extern "C" fn write_int_post(v: c_int) {
    with(|s| s.message_int(v));
    meta::ignored();
}

extern "C" fn write_float_post(v: f32) {
    with(|s| s.message_float(v));
    meta::ignored();
}

extern "C" fn write_string_post(v: *const c_char) {
    with(|s| {
        // Only copied for a message tsx reads: `WRITE_STRING` runs for every message.
        let v = if s.reading_message() { text(v) } else { String::new() };
        s.message_string(&v);
    });
    meta::ignored();
}

extern "C" fn reg_user_msg_post(name: *const c_char, _size: c_int) -> c_int {
    if !name.is_null()
        && let Some(id) = meta::orig_ret_int()
    {
        let n = text(name);
        with(|s| s.msgs.register(&n, id));
    }
    meta::ignored();
    0
}

extern "C" fn precache_event_post(_type: c_int, name: *const c_char) -> c_ushort {
    if !name.is_null()
        && let Some(idx) = meta::orig_ret_u16()
    {
        let n = text(name);
        with(|s| s.precache_event(&n, idx));
    }
    meta::ignored();
    0
}

#[allow(clippy::too_many_arguments)] // the engine's signature
extern "C" fn playback_event_post(
    _flags: c_int,
    invoker: *const edict_t,
    index_: c_ushort,
    _delay: f32,
    _origin: *mut f32,
    _angles: *mut f32,
    _f1: f32,
    _f2: f32,
    _i1: c_int,
    _i2: c_int,
    _b1: c_int,
    _b2: c_int,
) {
    if let Some(i) = index(invoker) {
        on_state(|s, g| s.playback_event(g, i, index_));
    }
    meta::ignored();
}
