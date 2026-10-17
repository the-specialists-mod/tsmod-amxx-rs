// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! The Rust TS server's plugin API (`ts-plugin-api`), for metamod plugins and AMX Mod X modules.
//!
//! * [`find`]: the game library exports tier1's `CreateInterface`, which returns the table for
//!   `ts_plugin_api::INTERFACE_NAME` (`TSRS_PluginApi007`). The name carries the version, and
//!   versions aren't compatible: a server with another version answers NULL, and the module says
//!   so.
//! * [`Api`]: safe wrappers over the table's functions (buffers, out-pointers and strings handled
//!   here).
//! * [`Identity`] (version 7): a plugin identity of the module's own (`attach_plugin`), which owns
//!   a data directory, a log prefix, commands, listeners and a bot backend.
//! * [`backend`]: bot backends a module registers on behalf of something else (tsx for a Pawn
//!   plugin): the server's `BotBackend` callbacks carry no context, so they are trampolines per
//!   slot that call one dispatcher with the slot number.

use core::ffi::{CStr, c_char, c_int, c_void};
use std::ffi::CString;
use ts_plugin_api::{
    BotBackend, BotCmd, BotCmdResult, EntityInfo, FrameInfo, GameState, ItemInfo, ObjectiveInfo, PlayerInfo, PlayerSnapshot, Plugin,
    PluginApi, PluginId, WeaponEntry,
};

pub use ts_plugin_api as api;

type CreateInterfaceFn = unsafe extern "C" fn(name: *const c_char, rc: *mut c_int) -> *mut c_void;

/// The table this crate was built against (`ts_plugin_api::VERSION`).
pub const SIZE: usize = size_of::<PluginApi>();

/// Looks the table up in the loaded game library (metamod's `GINFO_REALDLL_FULLPATH`, then the
/// already loaded library's `CreateInterface`) and checks that it is exactly the version this
/// crate was built against, at least [`SIZE`] bytes. `Err` says why, for the log.
pub fn find() -> Result<&'static PluginApi, String> {
    let path = crate::meta::game_dll_path().ok_or("metamod didn't name the game library")?;
    let sym = crate::dl::loaded_symbol(&path, c"CreateInterface").ok_or_else(|| format!("{path} exports no CreateInterface"))?;
    // SAFETY: tier1's `CreateInterface(const char*, int*)`, the signature the plugin API's
    // documentation fixes.
    let create = unsafe { core::mem::transmute::<*mut c_void, CreateInterfaceFn>(sym) };
    let mut rc = 1;
    // SAFETY: a C string and an out-pointer for the return code.
    let p = unsafe { create(ts_plugin_api::INTERFACE_NAME.as_ptr(), &mut rc) } as *const PluginApi;
    if p.is_null() {
        return Err(format!(
            "{path} has no {} (a server without the plugin API, or with another version of it)",
            ts_plugin_api::INTERFACE_NAME.to_string_lossy()
        ));
    }
    // SAFETY: a non-NULL table from the server; `size` and `version` are its first fields in
    // every version.
    let (size, version) = unsafe { ((*p).size, (*p).version) };
    if (size as usize) < SIZE || version != ts_plugin_api::VERSION {
        return Err(format!(
            "the server's plugin API is version {version} ({size} bytes); this module needs version {} ({SIZE} bytes)",
            ts_plugin_api::VERSION
        ));
    }
    // SAFETY: the server's static table of this version, at least `SIZE` bytes (checked), living
    // as long as the game library.
    Ok(unsafe { &*p })
}

/// Whether `name` is a valid plugin name for [`Api::attach_plugin`]: 1 to 32 ASCII letters,
/// digits, `_` or `-` (the server's rule for `tsmod_<name>`).
pub fn valid_plugin_name(name: &str) -> bool {
    (1..=32).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A C string from bytes, dropping interior NULs rather than failing (the API's strings are
/// NUL-terminated byte strings, not necessarily UTF-8).
pub fn c_bytes(b: &[u8]) -> CString {
    CString::new(b.iter().copied().filter(|&c| c != 0).collect::<Vec<u8>>()).unwrap_or_default()
}

/// Reads a buffer the API filled (NUL-terminated, cut to fit) as text.
fn buf_text(buf: &[c_char]) -> String {
    // SAFETY: `c_char` and `u8` have the same size and alignment, and every bit pattern is valid
    // for both.
    let bytes = unsafe { core::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len()) };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Calls a "write a string to `buf`, return its full length" API function, growing the buffer
/// when the first one was too small. `Err` is the API's status.
fn read_string(mut f: impl FnMut(*mut c_char, c_int) -> c_int) -> Result<String, i32> {
    let mut buf = vec![0 as c_char; 256];
    loop {
        let n = f(buf.as_mut_ptr(), buf.len() as c_int);
        if n < 0 {
            return Err(n);
        }
        if (n as usize) < buf.len() || buf.len() >= 1 << 16 {
            return Ok(buf_text(&buf));
        }
        buf = vec![0 as c_char; n as usize + 1];
    }
}

/// Fills a list through a "fill `out[..max]`, return the total" API function: asks for the
/// count first, then for the list (again when it grew in between).
fn read_list<T: Default + Clone>(mut f: impl FnMut(*mut T, c_int) -> c_int) -> Result<Vec<T>, i32> {
    let n = f(core::ptr::null_mut(), 0);
    if n < 0 {
        return Err(n);
    }
    let mut v = vec![T::default(); n as usize];
    for _ in 0..4 {
        let total = f(v.as_mut_ptr(), v.len() as c_int);
        if total < 0 {
            return Err(total);
        }
        if total as usize <= v.len() {
            v.truncate(total as usize);
            return Ok(v);
        }
        v = vec![T::default(); total as usize];
    }
    Ok(v)
}

/// Safe access to the plugin API table. Copy it freely; every call follows the API's rules (the
/// engine's thread, inside an engine or game callback). Functions that may run plugin code
/// (`run_cmd`, `bot_command`, `create_bot`, the setters, which may send messages) must not be
/// called with module state borrowed that a native might take again.
#[derive(Clone, Copy)]
pub struct Api(&'static PluginApi);

impl Api {
    /// Wraps the table [`find`] returned.
    pub fn new(table: &'static PluginApi) -> Self {
        Api(table)
    }

    /// [`find`], wrapped.
    pub fn find() -> Result<Self, String> {
        find().map(Api)
    }

    /// The raw table, for the functions without a wrapper here (the plain `extern "C" fn` setters
    /// are safe to call through it).
    pub fn table(self) -> &'static PluginApi {
        self.0
    }

    /// `log(plugin, level, msg)`: a line on the server console with the plugin's prefix.
    pub fn log(self, plugin: PluginId, level: c_int, msg: &str) {
        let m = crate::core::c_string(msg);
        // SAFETY: a C string valid for the call.
        unsafe { (self.0.log)(plugin, level, m.as_ptr()) }
    }

    /// `data_dir(plugin)`: `<gamedir>/tsmod/<name>/`, created on first use; `Err(INVALID)` for
    /// `NO_PLUGIN` or a dead id.
    pub fn data_dir(self, plugin: PluginId) -> Result<String, i32> {
        // SAFETY: a buffer of `len` bytes valid for writes.
        read_string(|b, n| unsafe { (self.0.data_dir)(plugin, b, n) })
    }

    /// `map_name()`.
    pub fn map_name(self) -> String {
        // SAFETY: as in `data_dir`.
        read_string(|b, n| unsafe { (self.0.map_name)(b, n) }).unwrap_or_default()
    }

    /// `register_command(plugin, name, f)`: `OK` or `EXISTS`.
    pub fn register_command(self, plugin: PluginId, name: &CStr, f: ts_plugin_api::CommandFn) -> c_int {
        // SAFETY: a C string valid for the call; `f` is a function the server may call any time.
        unsafe { (self.0.register_command)(plugin, name.as_ptr(), f) }
    }

    /// `register_cvar(plugin, name, value, flags)`: the engine's `cvar_t *`, NULL for a bad name.
    pub fn register_cvar(self, plugin: PluginId, name: &CStr, value: &CStr, flags: c_int) -> *mut c_void {
        // SAFETY: C strings valid for the call (the server copies them).
        unsafe { (self.0.register_cvar)(plugin, name.as_ptr(), value.as_ptr(), flags) }
    }

    /// `register_bot_backend(plugin, name, backend)`: `OK`, `EXISTS` or `INVALID`. The server
    /// copies the descriptor; its callbacks must stay callable until the backend is unregistered
    /// (a detach), which they do as functions of this library while it is loaded.
    pub fn register_bot_backend(self, plugin: PluginId, name: &CStr, backend: &BotBackend) -> c_int {
        // SAFETY: a C string and a descriptor valid for the call; the server copies both.
        unsafe { (self.0.register_bot_backend)(plugin, name.as_ptr(), backend) }
    }

    /// `player_info(player)`.
    pub fn player_info(self, player: c_int) -> Result<PlayerInfo, i32> {
        let mut out = PlayerInfo::default();
        // SAFETY: `out` is valid for writes.
        let r = unsafe { (self.0.player_info)(player, &mut out) };
        if r >= 0 { Ok(out) } else { Err(r) }
    }

    /// `player_inventory(player)`: the weapons it owns (kung fu not included).
    pub fn player_inventory(self, player: c_int) -> Result<Vec<WeaponEntry>, i32> {
        // SAFETY: `out` is NULL with `max` 0, or valid for `max` entries.
        read_list(|out, max| unsafe { (self.0.player_inventory)(player, out, max) })
    }

    /// `entity_info(entity)`.
    pub fn entity_info(self, entity: c_int) -> Result<EntityInfo, i32> {
        let mut out = EntityInfo::default();
        // SAFETY: `out` is valid for writes.
        let r = unsafe { (self.0.entity_info)(entity, &mut out) };
        if r >= 0 { Ok(out) } else { Err(r) }
    }

    /// `master_triggered(master, activator)`.
    pub fn master_triggered(self, master: &CStr, activator: c_int) -> bool {
        // SAFETY: a C string valid for the call.
        unsafe { (self.0.master_triggered)(master.as_ptr(), activator) == 1 }
    }

    /// `game_state()`.
    pub fn game_state(self) -> GameState {
        let mut out = GameState::default();
        // SAFETY: `out` is valid for writes.
        unsafe { (self.0.game_state)(&mut out) };
        out
    }

    /// `objectives(player)`: the HUD's objectives, with `player`'s team's names (0 for none).
    pub fn objectives(self, player: c_int) -> Result<Vec<ObjectiveInfo>, i32> {
        // SAFETY: as in `player_inventory`.
        read_list(|out, max| unsafe { (self.0.objectives)(player, out, max) })
    }

    /// `create_entity(classname)`: its index or `INVALID`.
    pub fn create_entity(self, classname: &CStr) -> c_int {
        // SAFETY: a C string valid for the call.
        unsafe { (self.0.create_entity)(classname.as_ptr()) }
    }

    /// `entity_key_value(entity, key, value)`: 1 handled, 0 not, or a status.
    pub fn entity_key_value(self, entity: c_int, key: &CStr, value: &CStr) -> c_int {
        // SAFETY: C strings valid for the call.
        unsafe { (self.0.entity_key_value)(entity, key.as_ptr(), value.as_ptr()) }
    }

    /// `create_bot(plugin, name, model, skill)`: the bot's index or `FULL`/`INVALID`. The names
    /// are the bytes as given (not UTF-8). The server joins and spawns the bot inside.
    pub fn create_bot(self, plugin: PluginId, name: &[u8], model: &[u8], skill: f32) -> c_int {
        let (n, m) = (c_bytes(name), c_bytes(model));
        // SAFETY: C strings valid for the call.
        unsafe { (self.0.create_bot)(plugin, n.as_ptr(), m.as_ptr(), skill) }
    }

    /// `run_cmd(player, cmd)`: what the command did, or the status.
    pub fn run_cmd(self, player: c_int, cmd: &BotCmd) -> Result<BotCmdResult, i32> {
        let mut out = BotCmdResult::default();
        // SAFETY: `cmd` is valid for reads and `out` for writes during the call.
        let r = unsafe { (self.0.run_cmd)(player, cmd, &mut out) };
        if r == ts_plugin_api::OK { Ok(out) } else { Err(r) }
    }

    /// `bot_command(player, line)`: the line's bytes as typed.
    pub fn bot_command(self, player: c_int, line: &[u8]) -> c_int {
        let l = c_bytes(line);
        // SAFETY: a C string valid for the call.
        unsafe { (self.0.bot_command)(player, l.as_ptr()) }
    }

    /// `add_listener(plugin, listener)`: a handle (> 0) or `INVALID`.
    ///
    /// # Safety
    /// The server copies the descriptor but not what its pointers point to: `name` and `version`
    /// must be `'static` strings (or NULL), and the callbacks must stay callable until
    /// [`remove_listener`](Self::remove_listener) (or the owner's detach): call that before the
    /// library can be unloaded.
    pub unsafe fn add_listener(self, plugin: PluginId, listener: &Plugin) -> c_int {
        // SAFETY: the caller's contract; the descriptor is valid for the call.
        unsafe { (self.0.add_listener)(plugin, listener) }
    }

    /// `remove_listener(handle)`.
    pub fn remove_listener(self, handle: c_int) -> c_int {
        (self.0.remove_listener)(handle)
    }

    /// `players()`: every slot's snapshot of this frame, `[0]` slot 1, `maxClients` long.
    pub fn players(self) -> Vec<PlayerSnapshot> {
        // SAFETY: NULL only counts.
        let n = unsafe { (self.0.players)(core::ptr::null_mut(), 0) }.max(0);
        let mut v = vec![PlayerSnapshot::default(); n as usize];
        // SAFETY: `v` is valid for `n` entries.
        unsafe { (self.0.players)(v.as_mut_ptr(), n) };
        v
    }

    /// One slot of `players()`: `None` for an index outside `1..=maxClients` or an empty slot.
    pub fn player_snapshot(self, player: c_int) -> Option<PlayerSnapshot> {
        if !(1..=64).contains(&player) {
            return None;
        }
        let mut v = vec![PlayerSnapshot::default(); player as usize];
        // SAFETY: `v` is valid for `player` entries; the server fills slots 1..=min(player,
        // maxClients).
        let max = unsafe { (self.0.players)(v.as_mut_ptr(), player) };
        if player > max {
            return None;
        }
        let s = v[player as usize - 1];
        (s.user_id != 0).then_some(s)
    }

    /// `items()`: the map's items (taken ones too, with their `respawn_at`), in index order.
    pub fn items(self) -> Vec<ItemInfo> {
        // SAFETY: as in `player_inventory`.
        read_list(|out, max| unsafe { (self.0.items)(out, max) }).unwrap_or_default()
    }

    /// `frame_info()`.
    pub fn frame_info(self) -> FrameInfo {
        let mut out = FrameInfo::default();
        // SAFETY: `out` is valid for writes.
        unsafe { (self.0.frame_info)(&mut out) };
        out
    }

    /// `entity_key(entity, key)`: a map key the port keeps outside `entvars_t` (`master`); `Err`
    /// is `NOT_PLAYER`, `INVALID` or `UNSUPPORTED` (the entity wasn't given the key).
    pub fn entity_key(self, entity: c_int, key: &CStr) -> Result<String, i32> {
        // SAFETY: a C string and a buffer of `len` bytes, valid for the call.
        read_string(|b, n| unsafe { (self.0.entity_key)(entity, key.as_ptr(), b, n) })
    }

    /// `attach_plugin(name)` (version 7): a new [`PluginId`], or `EXISTS` / `INVALID`.
    pub fn attach_plugin(self, name: &str) -> Result<PluginId, i32> {
        if !valid_plugin_name(name) {
            return Err(ts_plugin_api::INVALID);
        }
        let n = crate::core::c_string(name);
        // SAFETY: a C string valid for the call.
        let id = unsafe { (self.0.attach_plugin)(n.as_ptr()) };
        if id > 0 { Ok(id) } else { Err(id) }
    }

    /// `detach_plugin(id)` (version 7): `OK` or `INVALID`. Its backend (if active: deactivated,
    /// its bots kicked) and listeners go; the server may call the backend's `deactivate` inside.
    pub fn detach_plugin(self, plugin: PluginId) -> c_int {
        (self.0.detach_plugin)(plugin)
    }
}

/// A plugin identity of the module's own ([`Api::attach_plugin`], version 7): the server treats
/// the module like a `tsmod_*` library for the data directory, log prefix, commands, cvars,
/// listeners and bot backend. It isn't given up on drop (identities usually live as long as the
/// module); call [`detach`](Self::detach) before the library is unloaded if it registered
/// callbacks.
#[derive(Clone, Copy)]
pub struct Identity {
    api: Api,
    id: PluginId,
}

impl Identity {
    /// Attaches under `name` ([`valid_plugin_name`]); `Err` is `EXISTS` or `INVALID`.
    pub fn attach(api: Api, name: &str) -> Result<Self, i32> {
        api.attach_plugin(name).map(|id| Identity { api, id })
    }

    /// The id, for the table's functions.
    pub fn id(self) -> PluginId {
        self.id
    }

    pub fn api(self) -> Api {
        self.api
    }

    /// Its data directory, `<gamedir>/tsmod/<name>/`.
    pub fn data_dir(self) -> Result<String, i32> {
        self.api.data_dir(self.id)
    }

    /// A console line prefixed `[<name>]`.
    pub fn log(self, level: c_int, msg: &str) {
        self.api.log(self.id, level, msg);
    }

    /// Registers its bot backend (see [`Api::register_bot_backend`]).
    pub fn register_bot_backend(self, name: &CStr, backend: &BotBackend) -> c_int {
        self.api.register_bot_backend(self.id, name, backend)
    }

    /// Makes a bot of its (active) backend.
    pub fn create_bot(self, name: &[u8], model: &[u8], skill: f32) -> c_int {
        self.api.create_bot(self.id, name, model, skill)
    }

    /// Gives the identity up ([`Api::detach_plugin`]).
    pub fn detach(self) -> c_int {
        self.api.detach_plugin(self.id)
    }
}

/// Bot backends registered on behalf of something without callbacks of its own (tsx registers
/// one for a Pawn plugin). The server's [`BotBackend`] has no context pointer, so each of the
/// [`SLOTS`](backend::SLOTS) slots has its own trampolines, which call the dispatcher set with
/// [`set_dispatch`](backend::set_dispatch) with the slot's number.
pub mod backend {
    use super::*;
    use std::sync::OnceLock;

    /// How many backends can be registered this way at once.
    pub const SLOTS: usize = 4;

    /// A server callback of a slot's backend.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum Call<'a> {
        /// `add_bot(name, model, skill)`: make the bot and return its index (≤ 0 for none). The
        /// names are the command's bytes (not UTF-8), empty for "the backend picks".
        Add { name: &'a [u8], model: &'a [u8], skill: f32 },
        /// The backend became the active one.
        Activate,
        /// It no longer is (its bots are being kicked).
        Deactivate,
        /// A bot of this backend kept across a level change: pick it up.
        Adopt(c_int),
    }

    /// The dispatcher: what a slot's callback returns (only `Add`'s result is used).
    pub type Dispatch = fn(slot: usize, call: Call) -> c_int;

    static DISPATCH: OnceLock<Dispatch> = OnceLock::new();

    /// Sets the dispatcher (once; later calls are ignored). Callbacks before it return 0.
    pub fn set_dispatch(d: Dispatch) {
        let _ = DISPATCH.set(d);
    }

    fn dispatch(slot: usize, call: Call) -> c_int {
        DISPATCH.get().map_or(0, |d| d(slot, call))
    }

    /// Reads a C string the server passed as bytes.
    ///
    /// # Safety
    /// `p` is NULL or NUL-terminated and valid for the call.
    unsafe fn bytes<'a>(p: *const c_char) -> &'a [u8] {
        if p.is_null() {
            return &[];
        }
        // SAFETY: the caller's contract.
        unsafe { CStr::from_ptr(p) }.to_bytes()
    }

    unsafe extern "C" fn add<const N: usize>(name: *const c_char, model: *const c_char, skill: f32) -> c_int {
        // SAFETY: the server passes NUL-terminated strings valid for the call (the API's contract).
        let (name, model) = unsafe { (bytes(name), bytes(model)) };
        dispatch(N, Call::Add { name, model, skill })
    }

    extern "C" fn activate<const N: usize>() {
        dispatch(N, Call::Activate);
    }

    extern "C" fn deactivate<const N: usize>() {
        dispatch(N, Call::Deactivate);
    }

    extern "C" fn adopt<const N: usize>(player: c_int) {
        dispatch(N, Call::Adopt(player));
    }

    const fn table_of<const N: usize>() -> BotBackend {
        BotBackend {
            size: size_of::<BotBackend>() as u32,
            add_bot: Some(add::<N>),
            activate: Some(activate::<N>),
            deactivate: Some(deactivate::<N>),
            adopt_bot: Some(adopt::<N>),
        }
    }

    /// The descriptor of slot `slot`'s trampolines, for [`Api::register_bot_backend`]; `None` past
    /// [`SLOTS`].
    pub fn table(slot: usize) -> Option<BotBackend> {
        const TABLES: [BotBackend; SLOTS] = [table_of::<0>(), table_of::<1>(), table_of::<2>(), table_of::<3>()];
        TABLES.get(slot).copied()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::Mutex;

        static SEEN: Mutex<Vec<(usize, String)>> = Mutex::new(Vec::new());

        fn record(slot: usize, call: Call) -> c_int {
            let s = match call {
                Call::Add { name, model, skill } => {
                    format!("add {} {} {skill}", String::from_utf8_lossy(name), String::from_utf8_lossy(model))
                }
                other => format!("{other:?}"),
            };
            SEEN.lock().unwrap().push((slot, s));
            7
        }

        #[test]
        fn slots_dispatch_with_their_number() {
            set_dispatch(record);
            assert!(table(SLOTS).is_none());
            let t2 = table(2).unwrap();
            assert_eq!(t2.size as usize, size_of::<BotBackend>());
            // SAFETY: C string literals.
            let r = unsafe { (t2.add_bot.unwrap())(c"Neo".as_ptr(), core::ptr::null(), 0.5) };
            assert_eq!(r, 7);
            (table(0).unwrap().activate.unwrap())();
            (table(3).unwrap().adopt_bot.unwrap())(5);
            (table(1).unwrap().deactivate.unwrap())();
            let seen = SEEN.lock().unwrap().clone();
            assert_eq!(seen, [(2, "add Neo  0.5".to_string()), (0, "Activate".into()), (3, "Adopt(5)".into()), (1, "Deactivate".into())]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_names_follow_the_servers_rule() {
        assert!(valid_plugin_name("rcbot_amxx"));
        assert!(valid_plugin_name("tsbot-e2e9"));
        assert!(valid_plugin_name(&"a".repeat(32)));
        assert!(!valid_plugin_name(""));
        assert!(!valid_plugin_name(&"a".repeat(33)));
        assert!(!valid_plugin_name("rc bot"));
        assert!(!valid_plugin_name("../x"));
        assert!(!valid_plugin_name("bötz"));
    }

    #[test]
    fn byte_strings_drop_nuls() {
        assert_eq!(c_bytes(b"N\0eo\xe9").as_bytes(), b"Neo\xe9");
    }

    #[test]
    fn strings_grow_their_buffer() {
        let text = "x".repeat(300);
        let mut calls = 0;
        let s = read_string(|b, n| {
            calls += 1;
            let src = text.as_bytes();
            let k = src.len().min(n as usize - 1);
            for (i, &c) in src[..k].iter().enumerate() {
                // SAFETY: `b` holds `n` bytes and `i < k < n`.
                unsafe { *b.add(i) = c as c_char };
            }
            // SAFETY: as above, `k < n`.
            unsafe { *b.add(k) = 0 };
            src.len() as c_int
        });
        assert_eq!((s.unwrap(), calls), (text, 2));
        assert_eq!(read_string(|_, _| -2), Err(-2));
    }

    #[test]
    fn lists_ask_for_the_count_first() {
        let data = [1, 2, 3];
        let got = read_list::<i32>(|out, max| {
            if !out.is_null() {
                for (i, &d) in data.iter().take(max as usize).enumerate() {
                    // SAFETY: `out` holds `max` entries.
                    unsafe { *out.add(i) = d };
                }
            }
            data.len() as c_int
        });
        assert_eq!(got.unwrap(), data);
        assert_eq!(read_list::<i32>(|_, _| -1), Err(-1));
    }
}
