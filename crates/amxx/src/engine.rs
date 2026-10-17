// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! The engine functions metamod hands the plugin (`GiveFnptrsToDll`), with safe wrappers for the
//! calls and entity fields the modules use: the TS modules' few, and what a bot's world model
//! needs (traces, contents, the PVS, entity searches, client info keys and the entvars fields a
//! bot reads), for modules in other repositories too.
//!
//! Entities are passed around as [`Edict`] handles; reading a field is a short unsafe access to
//! the engine's edict array, which lives for the whole map. The accessors never hand out
//! references, so a handle can be held across engine calls that re-enter the game.

use crate::core::{c_string, cstr_lossy};
use core::ffi::{CStr, c_char};
use goldsrc_sys::{TraceResult, edict_t, enginefuncs_t, globalvars_t};
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

/// `pfnTraceLine` / `pfnTraceHull` flags (the SDK's `ignore_monsters`, `missile`, `ignore_glass`).
pub mod trace_flags {
    /// Hit only the world and brush entities (`ignore_monsters`).
    pub const IGNORE_MONSTERS: i32 = 1;
    /// Monsters with a larger hull (`missile`).
    pub const MISSILE: i32 = 2;
    /// Pass through glass (`ignore_glass`: entities rendered translucent).
    pub const IGNORE_GLASS: i32 = 0x100;
}

/// The hulls `pfnTraceHull` takes.
pub mod hull {
    pub const POINT: i32 = 0;
    /// A standing player (32 x 32 x 72).
    pub const HUMAN: i32 = 1;
    pub const LARGE: i32 = 2;
    /// A ducked player (32 x 32 x 36).
    pub const HEAD: i32 = 3;
}

/// `pfnPointContents` values (`CONTENTS_*`).
pub mod contents {
    pub const EMPTY: i32 = -1;
    pub const SOLID: i32 = -2;
    pub const WATER: i32 = -3;
    pub const SLIME: i32 = -4;
    pub const LAVA: i32 = -5;
    pub const SKY: i32 = -6;
    pub const LADDER: i32 = -16;
}

/// A trace's result (`TraceResult`), copied out of the engine's struct.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Trace {
    /// The whole trace was in a solid (`fAllSolid`), it started in one (`fStartSolid`), it ran
    /// through empty space (`fInOpen`) or water (`fInWater`).
    pub all_solid: bool,
    pub start_solid: bool,
    pub in_open: bool,
    pub in_water: bool,
    /// How far it got, 0..1 (`flFraction`), and where (`vecEndPos`).
    pub fraction: f32,
    pub end_pos: [f32; 3],
    /// The plane it hit (`vecPlaneNormal`, `flPlaneDist`).
    pub plane_normal: [f32; 3],
    pub plane_dist: f32,
    /// What it hit (`pHit`: the world's edict for the world, NULL for nothing).
    pub hit: Edict,
    /// The hitgroup of a studio model it hit (`iHitgroup`).
    pub hitgroup: i32,
}

impl Default for Edict {
    fn default() -> Self {
        Edict::NULL
    }
}

impl From<&TraceResult> for Trace {
    fn from(t: &TraceResult) -> Self {
        Trace {
            all_solid: t.fAllSolid != 0,
            start_solid: t.fStartSolid != 0,
            in_open: t.fInOpen != 0,
            in_water: t.fInWater != 0,
            fraction: t.flFraction,
            end_pos: t.vecEndPos,
            plane_normal: t.vecPlaneNormal,
            plane_dist: t.flPlaneDist,
            hit: Edict(t.pHit),
            hitgroup: t.iHitgroup,
        }
    }
}

/// The most results one entity search collects (FWGS's `MAX_EDICTS`).
pub const MAX_SEARCH: usize = 4096;

/// Runs an engine search (`pfnFindEntityByString`, `pfnFindEntityInSphere`) to its end: `next`
/// makes the call after the previous result and returns its index (`None` for NULL or a free
/// edict). The engines don't end a search with NULL: FWGS and ReHLDS return the world (index 0),
/// and a search continued from the world starts over at 1, so stopping on NULL alone repeats every
/// match until the edict limit. This stops at the world, at NULL, and at an index it already gave.
pub fn collect_search(mut next: impl FnMut() -> Option<i32>) -> Vec<i32> {
    let mut out = Vec::new();
    let mut seen = vec![0u64; MAX_SEARCH / 64 + 1];
    while out.len() < MAX_SEARCH {
        let Some(i) = next().filter(|&i| i > 0) else { break };
        let (word, bit) = (i as usize / 64, 1u64 << (i as usize % 64));
        if word >= seen.len() {
            seen.resize(word + 1, 0);
        }
        if seen[word] & bit != 0 {
            break;
        }
        seen[word] |= bit;
        out.push(i);
    }
    out
}

impl Engine {
    /// `pfnTraceLine(start, end, flags, ignore, &tr)`: `flags` are [`trace_flags`] bits, `ignore`
    /// an entity the trace passes through (a player's own body). Without the engine function the
    /// trace reaches `end`.
    pub fn trace_line(&self, start: [f32; 3], end: [f32; 3], flags: i32, ignore: Edict) -> Trace {
        let mut tr = TraceResult::default();
        match self.f.pfnTraceLine {
            // SAFETY: both vectors and the result are valid for the call; `ignore` is NULL or an
            // edict of the engine's array.
            Some(f) => unsafe { f(start.as_ptr(), end.as_ptr(), flags, ignore.0, &mut tr) },
            None => return Trace { fraction: 1.0, end_pos: end, in_open: true, ..Default::default() },
        }
        Trace::from(&tr)
    }

    /// `pfnTraceHull(start, end, flags, hull, ignore, &tr)`: a box of [`hull`] `hull` swept along
    /// the line ([`trace_flags`] as for [`trace_line`](Self::trace_line)).
    pub fn trace_hull(&self, start: [f32; 3], end: [f32; 3], flags: i32, hull: i32, ignore: Edict) -> Trace {
        let mut tr = TraceResult::default();
        match self.f.pfnTraceHull {
            // SAFETY: as in `trace_line`; the engine checks the hull number.
            Some(f) => unsafe { f(start.as_ptr(), end.as_ptr(), flags, hull, ignore.0, &mut tr) },
            None => return Trace { fraction: 1.0, end_pos: end, in_open: true, ..Default::default() },
        }
        Trace::from(&tr)
    }

    /// `POINT_CONTENTS(at)`: a [`contents`] value ([`contents::EMPTY`] without the engine).
    pub fn point_contents(&self, at: [f32; 3]) -> i32 {
        call!(self.pfnPointContents(at.as_ptr())).unwrap_or(contents::EMPTY)
    }

    /// `ENGINE_CHECK_VISIBILITY(e, ENGINE_SET_PVS(from))`: whether entity `e` is in the potentially
    /// visible set of the point `from` (a bot's eyes). True when the engine can't tell, so a
    /// caller's own trace decides.
    pub fn in_pvs(&self, e: Edict, from: [f32; 3]) -> bool {
        let (Some(set), Some(check)) = (self.f.pfnSetFatPVS, self.f.pfnCheckVisibility) else { return true };
        if e.is_free() {
            return false;
        }
        // SAFETY: the origin is valid for the call; the engine returns its own static PVS buffer,
        // which `pfnCheckVisibility` reads at once (nothing runs in between); `e` is a live edict.
        unsafe { check(e.0, set(from.as_ptr())) != 0 }
    }

    /// One `pfnFindEntityInSphere(after, origin, radius)` step: the next entity whose bounding box
    /// reaches into the sphere, after `after` (NULL starts the search). The world (or NULL) means
    /// no more; prefer [`entities_in_sphere`](Self::entities_in_sphere).
    pub fn find_entity_in_sphere(&self, after: Edict, origin: [f32; 3], radius: f32) -> Edict {
        Edict(call!(self.pfnFindEntityInSphere(after.0, origin.as_ptr(), radius)).unwrap_or(core::ptr::null_mut()))
    }

    /// Every entity in the sphere, by index, once each ([`collect_search`]).
    pub fn entities_in_sphere(&self, origin: [f32; 3], radius: f32) -> Vec<i32> {
        let mut e = Edict::NULL;
        collect_search(|| {
            e = self.find_entity_in_sphere(e, origin, radius);
            (!e.is_free()).then(|| self.ent_index(e))
        })
    }

    /// One `pfnFindEntityByString(after, field, value)` step: the next entity whose string field
    /// `field` (`classname`, `targetname`, `target`, `netname`, `model`, ...) is `value`.
    pub fn find_entity_by_string(&self, after: Edict, field: &CStr, value: &CStr) -> Edict {
        Edict(call!(self.pfnFindEntityByString(after.0, field.as_ptr(), value.as_ptr())).unwrap_or(core::ptr::null_mut()))
    }

    /// Every entity whose string field `field` is `value`, by index, once each.
    pub fn find_entities_by_string(&self, field: &str, value: &str) -> Vec<i32> {
        let (f, v) = (c_string(field), c_string(value));
        let mut e = Edict::NULL;
        collect_search(|| {
            e = self.find_entity_by_string(e, &f, &v);
            (!e.is_free()).then(|| self.ent_index(e))
        })
    }

    /// `CREATE_FAKE_CLIENT(netname)`: a client slot for a bot, NULL when the server is full. The
    /// TS server makes its bots itself (the plugin API's `create_bot`); this is for engines and
    /// games without one.
    pub fn create_fake_client(&self, netname: &CStr) -> Edict {
        Edict(call!(self.pfnCreateFakeClient(netname.as_ptr())).unwrap_or(core::ptr::null_mut()))
    }

    /// `INFOKEY_VALUE(GET_INFOKEYBUFFER(e), key)`: a client's user info key ("" when unset; `e`
    /// NULL reads the server's localinfo, as [`localinfo`](Self::localinfo)).
    pub fn client_info_key(&self, e: Edict, key: &CStr) -> String {
        let Some(buf) = call!(self.pfnGetInfoKeyBuffer(e.0)) else { return String::new() };
        if buf.is_null() {
            return String::new();
        }
        // SAFETY: an info buffer the engine just returned.
        unsafe { self.info_key_value(buf, key) }
    }

    /// `pfnGetPlayerUserId(e)`: the client's user id, -1 for a free slot or a non-client.
    pub fn user_id(&self, e: Edict) -> i32 {
        if e.is_free() {
            return -1;
        }
        call!(self.pfnGetPlayerUserId(e.0)).unwrap_or(-1)
    }

    /// `gpGlobals->frametime`.
    pub fn frametime(&self) -> f32 {
        self.globals().frametime
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

    /// The address of the private data (what a C++ pointer to the entity's object holds), 0 for
    /// none.
    pub fn private_address(self) -> usize {
        // SAFETY: as in `field!`.
        if self.is_null() { 0 } else { unsafe { (*self.0).pvPrivateData as usize } }
    }

    /// The 32-bit word at byte `offset` of the private data, when `offset + 4 <= size`: `size` is
    /// the block's size as the game reports it (the Rust server's `PluginApi::private_size`).
    /// `None` without private data or outside `size`.
    pub fn private_word(self, offset: usize, size: usize) -> Option<u32> {
        if !self.has_private_data() || offset.checked_add(4).is_none_or(|end| end > size) {
            return None;
        }
        // SAFETY: the game allocated at least `size` bytes at `pvPrivateData` (its API's
        // contract) and `offset + 4 <= size`; an unaligned copy, on the engine's thread.
        Some(unsafe { (*self.0).pvPrivateData.cast::<u8>().add(offset).cast::<u32>().read_unaligned() })
    }

    /// Writes the 32-bit word at byte `offset` of the private data, as [`private_word`](Self::private_word)
    /// reads it: false (nothing written) without private data or outside `size`.
    pub fn set_private_word(self, offset: usize, size: usize, value: u32) -> bool {
        if !self.has_private_data() || offset.checked_add(4).is_none_or(|end| end > size) {
            return false;
        }
        // SAFETY: as in `private_word`; a plain store into the game's block.
        unsafe { (*self.0).pvPrivateData.cast::<u8>().add(offset).cast::<u32>().write_unaligned(value) };
        true
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
    field!(/// `pev->velocity`.
        velocity: [f32; 3] = velocity);
    field!(/// `pev->friction`.
        friction: f32 = friction);
    field!(/// `pev->classname`.
        classname_id: goldsrc_sys::string_t = classname);
    field!(/// `pev->netname`.
        netname_id: goldsrc_sys::string_t = netname);
    field!(/// `pev->weaponmodel`.
        weapon_model_id: goldsrc_sys::string_t = weaponmodel);
    field!(/// `pev->angles`.
        angles: [f32; 3] = angles);
    field!(/// `pev->v_angle` (a player's view).
        v_angle: [f32; 3] = v_angle);
    field!(/// `pev->punchangle`.
        punchangle: [f32; 3] = punchangle);
    field!(/// `pev->view_ofs` (eyes above the origin).
        view_ofs: [f32; 3] = view_ofs);
    field!(/// `pev->mins`.
        mins: [f32; 3] = mins);
    field!(/// `pev->maxs`.
        maxs: [f32; 3] = maxs);
    field!(/// `pev->size`.
        size: [f32; 3] = size);
    field!(/// `pev->absmin`.
        absmin: [f32; 3] = absmin);
    field!(/// `pev->absmax`.
        absmax: [f32; 3] = absmax);
    field!(/// `pev->movedir`.
        movedir: [f32; 3] = movedir);
    field!(/// `pev->movetype` (`MOVETYPE_*`).
        movetype: i32 = movetype);
    field!(/// `pev->solid` (`SOLID_*`).
        solid: i32 = solid);
    field!(/// `pev->takedamage` (`DAMAGE_*`).
        takedamage: f32 = takedamage);
    field!(/// `pev->effects` (`EF_*`; `EF_NODRAW` hides it).
        effects: i32 = effects);
    field!(/// `pev->spawnflags`.
        spawnflags: i32 = spawnflags);
    field!(/// `pev->button` (`IN_*` of the last command).
        button: i32 = button);
    field!(/// `pev->impulse`.
        impulse: i32 = impulse);
    field!(/// `pev->waterlevel` (0 dry .. 3 under).
        waterlevel: i32 = waterlevel);
    field!(/// `pev->watertype` (a `CONTENTS_*` value).
        watertype: i32 = watertype);
    field!(/// `pev->armorvalue`.
        armorvalue: f32 = armorvalue);
    field!(/// `pev->max_health`.
        max_health: f32 = max_health);
    field!(/// `pev->rendermode` (`kRender*`).
        rendermode: i32 = rendermode);
    field!(/// `pev->renderamt`.
        renderamt: f32 = renderamt);
    field!(/// `pev->gravity`.
        gravity: f32 = gravity);
    field!(/// `pev->maxspeed`.
        maxspeed: f32 = maxspeed);
    field!(/// `pev->speed`.
        speed: f32 = speed);
    field!(/// `pev->nextthink`.
        nextthink: f32 = nextthink);
    field!(/// `pev->ltime` (a brush mover's local time).
        ltime: f32 = ltime);
    field!(/// `pev->frame`.
        frame: f32 = frame);
    field!(/// `pev->sequence`.
        sequence: i32 = sequence);
    field!(/// `pev->iuser1` (the observer mode on players).
        iuser1: i32 = iuser1);
    field!(/// `pev->iuser2` (the observer's target).
        iuser2: i32 = iuser2);
    field!(/// `pev->fuser1` (TS: the time scale).
        fuser1: f32 = fuser1);
    field!(/// `pev->model`.
        model_id: goldsrc_sys::string_t = model);
    field!(/// `pev->targetname`.
        targetname_id: goldsrc_sys::string_t = targetname);
    field!(/// `pev->target`.
        target_id: goldsrc_sys::string_t = target);
    field!(/// `pev->globalname`.
        globalname_id: goldsrc_sys::string_t = globalname);

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

    /// An edict-valued entvars field, NULL for none.
    fn edict_field(self, f: impl FnOnce(&goldsrc_sys::entvars_t) -> *mut edict_t) -> Edict {
        if self.is_null() {
            return Edict::NULL;
        }
        // SAFETY: as in `field!`; the reference lives only for the read.
        Edict(f(unsafe { &(*self.0).v }))
    }

    /// `pev->groundentity` (what it stands on).
    pub fn groundentity(self) -> Edict {
        self.edict_field(|v| v.groundentity)
    }

    /// `pev->enemy`.
    pub fn enemy(self) -> Edict {
        self.edict_field(|v| v.enemy)
    }

    /// `pev->aiment` (what it is attached to).
    pub fn aiment(self) -> Edict {
        self.edict_field(|v| v.aiment)
    }

    /// `pev->pContainingEntity` (the edict itself, for a live one).
    pub fn containing_entity(self) -> Edict {
        self.edict_field(|v| v.pContainingEntity)
    }

    /// The middle of the bounding box, `(absmin + absmax) / 2` (`Center()` for brush entities,
    /// whose origin is often 0).
    pub fn center(self) -> [f32; 3] {
        let (a, b) = (self.absmin(), self.absmax());
        [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]
    }

    /// The eyes: `origin + view_ofs`.
    pub fn eye_position(self) -> [f32; 3] {
        let (o, v) = (self.origin(), self.view_ofs());
        [o[0] + v[0], o[1] + v[1], o[2] + v[2]]
    }

    /// `STRING(pev->model)`.
    pub fn model(self) -> String {
        engine().map(|e| e.string(self.model_id())).unwrap_or_default()
    }

    /// `STRING(pev->targetname)`.
    pub fn targetname(self) -> String {
        engine().map(|e| e.string(self.targetname_id())).unwrap_or_default()
    }

    /// `STRING(pev->target)`.
    pub fn target(self) -> String {
        engine().map(|e| e.string(self.target_id())).unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches_stop_at_the_world_and_at_a_repeat() {
        // An engine that ends with the world and then starts over.
        let seq = [3, 7, 9, 0, 3, 7];
        let mut i = 0;
        let got = collect_search(|| {
            let r = seq.get(i).copied();
            i += 1;
            r
        });
        assert_eq!(got, [3, 7, 9]);
        // One that never returns the world but loops.
        let seq = [5, 6, 5, 6];
        let mut i = 0;
        assert_eq!(
            collect_search(|| {
                i += 1;
                seq.get(i - 1).copied()
            }),
            [5, 6]
        );
        // NULL at once.
        assert!(collect_search(|| None).is_empty());
    }

    #[test]
    fn traces_copy_every_field() {
        let tr = TraceResult {
            fAllSolid: 0,
            fStartSolid: 1,
            fInOpen: 1,
            fInWater: 0,
            flFraction: 0.25,
            vecEndPos: [1.0, 2.0, 3.0],
            flPlaneDist: 4.0,
            vecPlaneNormal: [0.0, 0.0, 1.0],
            pHit: core::ptr::null_mut(),
            iHitgroup: 2,
        };
        let t = Trace::from(&tr);
        assert!(t.start_solid && t.in_open && !t.all_solid && !t.in_water);
        assert_eq!((t.fraction, t.end_pos, t.plane_normal, t.plane_dist, t.hitgroup), (0.25, [1.0, 2.0, 3.0], [0.0, 0.0, 1.0], 4.0, 2));
        assert!(t.hit.is_null());
    }

    #[test]
    fn edict_fields_of_null_are_zero() {
        let e = Edict::NULL;
        assert_eq!((e.v_angle(), e.solid(), e.groundentity(), e.center()), ([0.0; 3], 0, Edict::NULL, [0.0; 3]));
    }
}
