// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. The ItemInfo natives' memory
// follows DataHandler.cpp (`CreateHamItemInfo`, `FreeHamItemInfo`, Get/SetHamItemInfo),
// Copyright (C) The AMX Mod X Development Team.

//! The real [`Host`]: AMX Mod X forwards, the engine's entities, ItemInfo memory.

use super::api::{RealHamApi, table};
use crate::hooks::{FwdArg, FwdType};
use crate::host::{EntState, HamApi, Host, ItemVal};
use amxx::Cell;
use amxx::core::FwdValue;
use amxx::engine::engine;
use amxx::ffi::{FP_ARRAY, FP_CELL, FP_STRING};
use core::ffi::{CStr, c_char};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;

/// HLSDK's `ItemInfo` (DataHandler.h), as the server passes it and `CreateHamItemInfo` makes it.
#[repr(C)]
#[derive(Clone, Copy)]
struct ItemInfo {
    slot: i32,
    position: i32,
    ammo1: *const c_char,
    max_ammo1: i32,
    ammo2: *const c_char,
    max_ammo2: i32,
    name: *const c_char,
    max_clip: i32,
    id: i32,
    flags: i32,
    weight: i32,
}

/// One the module made, with the strings its pointers point into.
struct Owned {
    info: ItemInfo,
    strings: [Option<CString>; 3],
}

thread_local! {
    static ITEM_INFOS: RefCell<HashMap<Cell, Box<Owned>>> = RefCell::new(HashMap::new());
    /// Strings written into a server-owned ItemInfo during a call; kept for the module's life
    /// (rare, and the server may keep the pointer).
    static LEAKED: RefCell<Vec<CString>> = const { RefCell::new(Vec::new()) };
}

/// Frees the module's ItemInfos (`OnAmxxDetach`).
pub fn free_item_infos() {
    ITEM_INFOS.with(|m| m.borrow_mut().clear());
}

fn read_str(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: ItemInfo strings are NULL or NUL-terminated (the game's constants or ours).
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

fn get(i: &ItemInfo, field: i32) -> Option<ItemVal> {
    Some(match field {
        0 => ItemVal::Int(i.slot),
        1 => ItemVal::Int(i.position),
        2 => ItemVal::Str(read_str(i.ammo1)),
        3 => ItemVal::Int(i.max_ammo1),
        4 => ItemVal::Str(read_str(i.ammo2)),
        5 => ItemVal::Int(i.max_ammo2),
        6 => ItemVal::Str(read_str(i.name)),
        7 => ItemVal::Int(i.max_clip),
        8 => ItemVal::Int(i.id),
        9 => ItemVal::Int(i.flags),
        10 => ItemVal::Int(i.weight),
        _ => return None,
    })
}

/// Writes an int field.
fn set_int(i: &mut ItemInfo, field: i32, x: i32) -> bool {
    match field {
        0 => i.slot = x,
        1 => i.position = x,
        3 => i.max_ammo1 = x,
        5 => i.max_ammo2 = x,
        7 => i.max_clip = x,
        8 => i.id = x,
        9 => i.flags = x,
        10 => i.weight = x,
        _ => return false,
    }
    true
}

/// The `Owned::strings` slot of a string field.
fn string_slot(field: i32) -> Option<usize> {
    match field {
        2 => Some(0),
        4 => Some(1),
        6 => Some(2),
        _ => None,
    }
}

/// Points a string field at `p`.
fn set_str(i: &mut ItemInfo, field: i32, p: *const c_char) -> bool {
    match field {
        2 => i.ammo1 = p,
        4 => i.ammo2 = p,
        6 => i.name = p,
        _ => return false,
    }
    true
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

/// A server-owned ItemInfo of a call in progress (the natives checked the handle against the
/// call stack first).
fn server_item(h: Cell) -> *mut ItemInfo {
    h as usize as *mut ItemInfo
}

pub struct RealHost;

static API: std::sync::OnceLock<RealHamApi> = std::sync::OnceLock::new();

impl Host for RealHost {
    fn api(&self) -> Option<&dyn HamApi> {
        let t = table()?;
        Some(API.get_or_init(|| RealHamApi(t)) as &dyn HamApi)
    }

    fn register_forward(&self, amx: usize, name: &str, params: &[FwdType]) -> i32 {
        let types: Vec<core::ffi::c_int> = params
            .iter()
            .map(|p| match p {
                FwdType::Cell => FP_CELL,
                FwdType::Str => FP_STRING,
                FwdType::Array => FP_ARRAY,
            })
            .collect();
        // SAFETY: `amx` is the AMX the core passed to the native that asked (`Args::amx`).
        unsafe { amxx::core::register_sp_forward(amx, name, &types) }
    }

    fn execute_forward(&self, id: i32, args: &[FwdArg]) -> Cell {
        let strings: Vec<CString> = args
            .iter()
            .filter_map(|a| match a {
                FwdArg::Str(s) => Some(CString::new(s.replace('\0', "")).unwrap_or_default()),
                _ => None,
            })
            .collect();
        let mut arrays: Vec<[Cell; 3]> = args
            .iter()
            .filter_map(|a| match a {
                FwdArg::Array(v) => Some(*v),
                _ => None,
            })
            .collect();
        let (mut si, mut ai) = (strings.iter(), arrays.iter_mut());
        let mut values: Vec<FwdValue> = args
            .iter()
            .map(|a| match a {
                FwdArg::Cell(c) => FwdValue::Cell(*c),
                FwdArg::Str(_) => FwdValue::Str(si.next().map_or(c"", |s| s.as_c_str())),
                FwdArg::Array(_) => FwdValue::Array(ai.next().map_or(&mut [][..], |a| &mut a[..])),
            })
            .collect();
        amxx::core::execute_sp_forward(id, &mut values)
    }

    fn fail_plugin(&self, amx: usize, ham: i32, err: i32, reason: &str) {
        // SAFETY: as in `register_forward`.
        let id = unsafe { amxx::core::register_sp_forward(amx, "__fatal_ham_error", &[FP_CELL, FP_CELL, FP_STRING]) };
        let r = CString::new(reason.replace('\0', "")).unwrap_or_default();
        amxx::core::execute_sp_forward(id, &mut [FwdValue::Cell(ham), FwdValue::Cell(err), FwdValue::Str(&r)]);
        amxx::core::unregister_sp_forward(id);
    }

    fn max_entities(&self) -> i32 {
        engine().map_or(0, |e| e.max_entities())
    }

    fn entity_state(&self, ent: i32) -> EntState {
        let Some(e) = engine() else { return EntState::Free };
        let ed = e.edict(ent);
        if ed.is_free() {
            EntState::Free
        } else if !ed.has_private_data() {
            EntState::NoPrivateData
        } else {
            EntState::Ok
        }
    }

    fn log(&self, msg: &str) {
        amxx::core::log(msg);
    }

    fn item_info_create(&self) -> Cell {
        let b = Box::new(Owned {
            info: ItemInfo {
                slot: 0,
                position: 0,
                ammo1: core::ptr::null(),
                max_ammo1: 0,
                ammo2: core::ptr::null(),
                max_ammo2: 0,
                name: core::ptr::null(),
                max_clip: 0,
                id: 0,
                flags: 0,
                weight: 0,
            },
            strings: [None, None, None],
        });
        // The handle is the struct's address, so it can also be passed to the server.
        let h = &b.info as *const ItemInfo as usize as Cell;
        ITEM_INFOS.with(|m| m.borrow_mut().insert(h, b));
        h
    }

    fn item_info_free(&self, h: Cell) -> bool {
        ITEM_INFOS.with(|m| m.borrow_mut().remove(&h).is_some())
    }

    fn item_info_owned(&self, h: Cell) -> bool {
        ITEM_INFOS.with(|m| m.borrow().contains_key(&h))
    }

    fn item_info_get(&self, h: Cell, field: i32) -> Option<ItemVal> {
        if let Some(v) = ITEM_INFOS.with(|m| m.borrow().get(&h).map(|o| get(&o.info, field))) {
            return v;
        }
        // SAFETY: not ours, so a server ItemInfo of a call in progress (checked by the native),
        // valid for that call.
        get(unsafe { &*server_item(h) }, field)
    }

    fn item_info_set(&self, h: Cell, field: i32, v: ItemVal) -> bool {
        let owned = ITEM_INFOS.with(|m| {
            let mut m = m.borrow_mut();
            let o = m.get_mut(&h)?;
            Some(match (string_slot(field), &v) {
                (Some(k), ItemVal::Str(s)) => {
                    // The CString's buffer stays put while the box lives.
                    let p = o.strings[k].insert(cstring(s)).as_ptr();
                    set_str(&mut o.info, field, p)
                }
                (None, ItemVal::Int(x)) => set_int(&mut o.info, field, *x),
                _ => false,
            })
        });
        if let Some(r) = owned {
            return r;
        }
        // SAFETY: not ours, so a server ItemInfo of a call in progress (checked by the native),
        // valid and writable for that call.
        let info = unsafe { &mut *server_item(h) };
        match (string_slot(field), &v) {
            (Some(_), ItemVal::Str(s)) => {
                let p = LEAKED.with(|l| {
                    let mut l = l.borrow_mut();
                    l.push(cstring(s));
                    l.last().map_or(core::ptr::null(), |c| c.as_ptr())
                });
                set_str(info, field, p)
            }
            (None, ItemVal::Int(x)) => set_int(info, field, *x),
            _ => false,
        }
    }

    fn pdata_word(&self, ent: i32, offset: usize, size: usize) -> Option<u32> {
        engine()?.edict(ent).private_word(offset, size)
    }

    fn set_pdata_word(&self, ent: i32, offset: usize, size: usize, value: u32) -> bool {
        engine().is_some_and(|e| e.edict(ent).set_private_word(offset, size, value))
    }

    fn private_address(&self, ent: i32) -> usize {
        engine().map_or(0, |e| e.edict(ent).private_address())
    }

    fn entity_of_private(&self, addr: usize) -> Option<i32> {
        let e = engine()?;
        (0..e.max_entities()).find(|&i| e.edict(i).private_address() == addr)
    }
}
