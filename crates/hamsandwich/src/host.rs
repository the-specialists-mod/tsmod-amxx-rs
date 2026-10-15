// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! What the module's logic needs from outside: AMX Mod X (forwards, logging), the engine (entity
//! checks) and the server's entity hooks. `ffi` implements both traits; tests fake them.

use crate::hooks::{FwdArg, FwdType, Ty, Val};
use amxx::Cell;

/// The server's entity hooks (`ts-plugin-api`'s entity hooks, version 5) and private-data size. Results are the API's codes
/// (`OK` 0, negative errors).
pub trait HamApi {
    /// `ham_set_hook`: installs (true) or removes the module's single hook.
    fn set_hook(&self, on: bool);
    /// `ham_watch`: OK, or UNSUPPORTED when the server has the function for no class.
    fn watch(&self, ham: i32, on: bool) -> i32;
    /// `ham_supported`: 1, 0 or UNSUPPORTED.
    fn supported(&self, ham: i32, class: i32) -> i32;
    /// `class_id`: > 0, or INVALID for an unknown classname.
    fn class_id(&self, classname: &str) -> i32;
    /// `entity_class_id`: > 0, or NOT_PLAYER.
    fn entity_class_id(&self, ent: i32) -> i32;
    /// `ham_execute` of `ham` on `this`: the return value and the arguments after the call
    /// (out parameters updated), or the API's error.
    fn execute(&self, ham: i32, this: i32, args: &[(Ty, Val)], ret: Ty, run_hooks: bool) -> Result<(Val, Vec<Val>), i32>;
    /// `private_size` (version 5): the bytes of `ent`'s private data, or NOT_PLAYER.
    fn private_size(&self, ent: i32) -> i32;
}

/// An entity as stock's `CHECK_ENTITY` sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntState {
    Free,
    NoPrivateData,
    Ok,
}

/// An `ItemInfo` field value.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemVal {
    Int(i32),
    Str(String),
}

/// AMX Mod X and the engine.
pub trait Host {
    /// The server's entity hooks, if it has them.
    fn api(&self) -> Option<&dyn HamApi>;
    /// `MF_RegisterSPForwardByName(amx, name, params...)`: the forward id, or -1 when the plugin
    /// has no such public.
    fn register_forward(&self, amx: usize, name: &str, params: &[FwdType]) -> i32;
    /// `MF_ExecuteForward`: runs the plugin's public and returns what it returned.
    fn execute_forward(&self, id: i32, args: &[FwdArg]) -> Cell;
    /// `FailPlugin`: runs the plugin's `__fatal_ham_error` stock (which fails the plugin unless
    /// its `HamFilter` handles the error).
    fn fail_plugin(&self, amx: usize, ham: i32, err: i32, reason: &str);
    /// `gpGlobals->maxEntities`.
    fn max_entities(&self) -> i32;
    /// The entity at `ent` (0..=maxEntities).
    fn entity_state(&self, ent: i32) -> EntState;
    /// A line in the AMX Mod X log.
    fn log(&self, msg: &str);
    /// `CreateHamItemInfo`: a zeroed ItemInfo the module owns.
    fn item_info_create(&self) -> Cell;
    /// `FreeHamItemInfo`: false when `h` isn't one of the module's.
    fn item_info_free(&self, h: Cell) -> bool;
    /// Whether `h` is an ItemInfo the module created.
    fn item_info_owned(&self, h: Cell) -> bool;
    /// Reads / writes field `field` (`HamItemInfo`) of a valid handle.
    fn item_info_get(&self, h: Cell, field: i32) -> Option<ItemVal>;
    fn item_info_set(&self, h: Cell, field: i32, v: ItemVal) -> bool;
    /// The 32-bit word at byte `offset` of `ent`'s private data, `size` bytes long (the server's
    /// `private_size`); `None` outside it.
    fn pdata_word(&self, ent: i32, offset: usize, size: usize) -> Option<u32>;
    /// Writes it: false outside the block.
    fn set_pdata_word(&self, ent: i32, offset: usize, size: usize, value: u32) -> bool;
    /// The address of `ent`'s private data (a C++ pointer to its object), 0 for none.
    fn private_address(&self, ent: i32) -> usize;
    /// The entity whose private data is at `addr` (stock's `TypeConversion.cbase_to_id`, by
    /// `get_pdata_cbase_safe`'s scan of the edicts), if any.
    fn entity_of_private(&self, addr: usize) -> Option<i32>;
}
