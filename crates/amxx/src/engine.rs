// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! The engine functions metamod hands the plugin (`GiveFnptrsToDll`), with safe wrappers for the
//! few calls and entity fields the TS modules use.
//!
//! Entities are passed around as [`Edict`] handles; reading a field is a short unsafe access to
//! the engine's edict array, which lives for the whole map. The accessors never hand out
//! references, so a handle can be held across engine calls that re-enter the game.

use crate::core::{c_string, cstr_lossy};
use core::ffi::{CStr, c_char};
use goldsrc_sys::{edict_t, enginefuncs_t, globalvars_t};
use std::sync::OnceLock;

/// The engine table and globals.
pub struct Engine {
    f: enginefuncs_t,
    g: *mut globalvars_t,
}

// SAFETY: the engine and every plugin callback run on the server's main thread; the struct is
// only shared so it can sit in a `static`.
unsafe impl Send for Engine {}
// SAFETY: as above.
unsafe impl Sync for Engine {}

static ENGINE: OnceLock<Engine> = OnceLock::new();

/// The engine, once metamod called `GiveFnptrsToDll`.
pub fn engine() -> Option<&'static Engine> {
    ENGINE.get()
}

/// Number of `enginefuncs_t` entries every engine and metamod build has: up to
/// `pfnCheckParm`. FWGS's header appends more, which metamod's (smaller) table doesn't hold.
pub(crate) fn common_engine_entries() -> usize {
    core::mem::offset_of!(enginefuncs_t, pfnCheckParm) / size_of::<usize>() + 1
}

/// Stores the engine table.
///
/// # Safety
/// `funcs` points to a table of at least [`common_engine_entries`] function pointers and
/// `globals` to the engine's globals, both valid for the life of the library.
pub(crate) unsafe fn set(funcs: *const enginefuncs_t, globals: *mut globalvars_t) {
    let mut f = enginefuncs_t::default();
    // SAFETY: both are arrays of pointer-sized function pointers; only the entries every engine
    // has are copied (caller's contract), the rest stay NULL.
    unsafe {
        core::ptr::copy_nonoverlapping(funcs as *const usize, &mut f as *mut enginefuncs_t as *mut usize, common_engine_entries());
    }
    let _ = ENGINE.set(Engine { f, g: globals });
}

/// A registered cvar.
#[derive(Clone, Copy, Debug)]
pub struct Cvar(*mut goldsrc_sys::cvar_t);

// SAFETY: the pointer is to a leaked, engine-linked cvar read on the main thread only.
unsafe impl Send for Cvar {}
// SAFETY: as above.
unsafe impl Sync for Cvar {}

impl Cvar {
    /// The current value.
    pub fn value(self) -> f32 {
        // SAFETY: a registered cvar lives until the process exits (it is leaked and the engine
        // links it); the engine updates `value` in place.
        unsafe { core::ptr::read_volatile(&raw const (*self.0).value) }
    }
}

macro_rules! call {
    ($self:ident.$f:ident($($a:expr),*)) => {
        match $self.f.$f {
            // SAFETY: an engine function from metamod's table, called with the argument types its
            // C declaration has.
            Some(f) => Some(unsafe { f($($a),*) }),
            None => None,
        }
    };
}

impl Engine {
    fn globals(&self) -> &globalvars_t {
        // SAFETY: the engine's globals outlive the library; fields are only read through this.
        unsafe { &*self.g }
    }

    /// `gpGlobals->time`.
    pub fn time(&self) -> f32 {
        self.globals().time
    }

    /// `gpGlobals->maxEntities`.
    pub fn max_entities(&self) -> i32 {
        self.globals().maxEntities
    }

    /// `gpGlobals->maxClients`.
    pub fn max_clients(&self) -> i32 {
        self.globals().maxClients
    }

    /// `STRING(s)`.
    pub fn string(&self, s: goldsrc_sys::string_t) -> String {
        if s == 0 {
            return String::new();
        }
        // SAFETY: string_t is an offset from pStringBase on 32-bit engines (the only ones AMX
        // Mod X supports); the result is a NUL-terminated engine string.
        unsafe { cstr_lossy(self.globals().pStringBase.wrapping_offset(s as isize)) }
    }

    /// `MAKE_STRING(s)` for a string with static lifetime (offset from `pStringBase`, as in the
    /// SDK; doesn't allocate, unlike `ALLOC_STRING`).
    pub fn make_string(&self, s: &'static CStr) -> goldsrc_sys::string_t {
        (s.as_ptr() as isize).wrapping_sub(self.globals().pStringBase as isize) as goldsrc_sys::string_t
    }

    /// `ENTINDEX(e)`.
    pub fn ent_index(&self, e: Edict) -> i32 {
        if e.is_null() {
            return 0;
        }
        call!(self.pfnIndexOfEdict(e.0)).unwrap_or(0)
    }

    /// `INDEXENT(i)`; a null handle when there is no such edict.
    pub fn edict(&self, index: i32) -> Edict {
        Edict(call!(self.pfnPEntityOfEntIndex(index)).unwrap_or(core::ptr::null_mut()))
    }

    /// `CREATE_NAMED_ENTITY(MAKE_STRING(class))`.
    pub fn create_named_entity(&self, class: &'static CStr) -> Edict {
        let s = self.make_string(class);
        Edict(call!(self.pfnCreateNamedEntity(s)).unwrap_or(core::ptr::null_mut()))
    }

    /// `REMOVE_ENTITY(e)`.
    pub fn remove_entity(&self, e: Edict) {
        if !e.is_null() {
            call!(self.pfnRemoveEntity(e.0));
        }
    }

    /// `CVAR_GET_FLOAT`.
    pub fn cvar_float(&self, name: &CStr) -> f32 {
        call!(self.pfnCVarGetFloat(name.as_ptr())).unwrap_or(0.0)
    }

    /// `CVAR_SET_FLOAT`.
    pub fn set_cvar_float(&self, name: &CStr, value: f32) {
        call!(self.pfnCVarSetFloat(name.as_ptr(), value));
    }

    /// `CVAR_REGISTER` of a new cvar, then `CVAR_GET_POINTER` (as tsx does): the engine keeps
    /// the struct, so it is leaked.
    pub fn register_cvar(&self, name: &'static CStr, value: &'static CStr, flags: u32) -> Option<Cvar> {
        let c = Box::leak(Box::new(goldsrc_sys::cvar_t {
            name: name.as_ptr() as *mut c_char,
            string: value.as_ptr() as *mut c_char,
            flags,
            value: value.to_str().ok().and_then(|v| v.parse().ok()).unwrap_or(0.0),
            next: core::ptr::null_mut(),
        }));
        call!(self.pfnCVarRegister(c));
        let p = call!(self.pfnCVarGetPointer(name.as_ptr())).unwrap_or(core::ptr::null_mut());
        (!p.is_null()).then_some(Cvar(p))
    }

    /// `LOCALINFO(key)`: the server's localinfo (`-localinfo` / `localinfo` command).
    pub fn localinfo(&self, key: &str) -> String {
        let k = c_string(key);
        let Some(buf) = call!(self.pfnGetInfoKeyBuffer(core::ptr::null_mut())) else { return String::new() };
        let v = call!(self.pfnInfoKeyValue(buf, k.as_ptr())).unwrap_or(core::ptr::null());
        // SAFETY: the engine returns NULL or a string in its info buffer, copied out at once.
        unsafe { cstr_lossy(v) }
    }

    /// `SET_LOCALINFO(key, value)`.
    pub fn set_localinfo(&self, key: &str, value: &str) {
        let (k, v) = (c_string(key), c_string(value));
        let Some(buf) = call!(self.pfnGetInfoKeyBuffer(core::ptr::null_mut())) else { return };
        call!(self.pfnSetKeyValue(buf, k.as_ptr() as *mut c_char, v.as_ptr() as *mut c_char));
    }

    /// `GETPLAYERAUTHID(e)`.
    pub fn auth_id(&self, e: Edict) -> Option<String> {
        if e.is_null() {
            return None;
        }
        let p = call!(self.pfnGetPlayerAuthId(e.0))?;
        // SAFETY: NULL or an engine string, copied at once.
        (!p.is_null()).then(|| unsafe { cstr_lossy(p) })
    }

    /// `INFOKEY_VALUE(infobuffer, key)`.
    ///
    /// # Safety
    /// `infobuffer` is an engine info buffer (e.g. the one `ClientUserInfoChanged` passes).
    pub unsafe fn info_key_value(&self, infobuffer: *mut c_char, key: &CStr) -> String {
        let v = call!(self.pfnInfoKeyValue(infobuffer, key.as_ptr())).unwrap_or(core::ptr::null());
        // SAFETY: NULL or a string inside the buffer, copied at once.
        unsafe { cstr_lossy(v) }
    }

    /// `CLIENT_COMMAND(e, "%s\n", cmd)`, as AMX Mod X's `client_cmd` sends it.
    pub fn client_command(&self, e: Edict, cmd: &str) {
        if e.is_null() {
            return;
        }
        let c = c_string(&format!("{cmd}\n"));
        if let Some(f) = self.f.pfnClientCommand {
            // SAFETY: the engine's variadic printf-style function with "%s" and one C string.
            unsafe { f(e.0, c"%s".as_ptr() as *mut c_char, c.as_ptr()) };
        }
    }

    /// `SERVER_PRINT`.
    pub fn server_print(&self, msg: &str) {
        let m = c_string(msg);
        call!(self.pfnServerPrint(m.as_ptr()));
    }
}

/// An edict handle. Holding one is always fine; the accessors check for NULL and read or write
/// one field at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edict(pub *mut edict_t);

macro_rules! field {
    ($(#[$m:meta])* $get:ident $(/ $set:ident)? : $t:ty = $f:ident) => {
        $(#[$m])*
        pub fn $get(self) -> $t {
            if self.is_null() {
                return Default::default();
            }
            // SAFETY: a non-NULL edict from the engine's edict array, which stays allocated for
            // the map; the field is copied out, no reference escapes.
            unsafe { (*self.0).v.$f }
        }
        $(
            #[doc = concat!("Sets `pev->", stringify!($f), "`.")]
            pub fn $set(self, value: $t) {
                if !self.is_null() {
                    // SAFETY: as for the getter; a plain store into the engine's entvars.
                    unsafe { (*self.0).v.$f = value };
                }
            }
        )?
    };
}

impl Edict {
    pub const NULL: Edict = Edict(core::ptr::null_mut());

    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    /// `FNullEnt`-style check: NULL or a free slot.
    pub fn is_free(self) -> bool {
        // SAFETY: as in `field!`.
        self.is_null() || unsafe { (*self.0).free } != 0
    }

    /// Whether the game attached private data (`pvPrivateData != NULL`).
    pub fn has_private_data(self) -> bool {
        // SAFETY: as in `field!`.
        !self.is_null() && !unsafe { (*self.0).pvPrivateData }.is_null()
    }

    field!(/// `pev->flags`.
        flags: i32 = flags);
    field!(/// `pev->frags`.
        frags: f32 = frags);
    field!(/// `pev->health`.
        health: f32 = health);
    field!(/// `pev->deadflag`.
        deadflag: i32 = deadflag);
    field!(/// `pev->iuser4` (TS keeps the stunt flags there).
        iuser4: i32 = iuser4);
    field!(/// `pev->team`.
        team: i32 = team);
    field!(/// `pev->dmg_take`.
        dmg_take / set_dmg_take: f32 = dmg_take);
    field!(/// `pev->origin`.
        origin / set_origin_field: [f32; 3] = origin);
    field!(/// `pev->classname`.
        classname_id: goldsrc_sys::string_t = classname);
    field!(/// `pev->netname`.
        netname_id: goldsrc_sys::string_t = netname);
    field!(/// `pev->weaponmodel`.
        weapon_model_id: goldsrc_sys::string_t = weaponmodel);

    /// `pev->dmg_inflictor`.
    pub fn dmg_inflictor(self) -> Edict {
        if self.is_null() {
            return Edict::NULL;
        }
        // SAFETY: as in `field!`.
        Edict(unsafe { (*self.0).v.dmg_inflictor })
    }

    /// Sets `pev->dmg_inflictor`.
    pub fn set_dmg_inflictor(self, e: Edict) {
        if !self.is_null() {
            // SAFETY: as in `field!`.
            unsafe { (*self.0).v.dmg_inflictor = e.0 };
        }
    }

    /// `pev->owner`.
    pub fn owner(self) -> Edict {
        if self.is_null() {
            return Edict::NULL;
        }
        // SAFETY: as in `field!`.
        Edict(unsafe { (*self.0).v.owner })
    }

    /// `STRING(pev->classname)`.
    pub fn classname(self) -> String {
        engine().map(|e| e.string(self.classname_id())).unwrap_or_default()
    }

    /// `STRING(pev->netname)`.
    pub fn netname(self) -> String {
        engine().map(|e| e.string(self.netname_id())).unwrap_or_default()
    }

    /// `STRING(pev->weaponmodel)`.
    pub fn weapon_model(self) -> String {
        engine().map(|e| e.string(self.weapon_model_id())).unwrap_or_default()
    }

    /// The edict at `index` in the array starting at `self` (`pEdictList + i` in `ServerActivate`).
    pub fn offset(self, index: i32) -> Edict {
        Edict(self.0.wrapping_offset(index as isize))
    }
}
