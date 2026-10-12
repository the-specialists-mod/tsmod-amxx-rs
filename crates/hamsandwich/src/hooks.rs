// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports the types of
// modules/hamsandwich (hook_native.cpp's `hook_t`, DataHandler.h's `Data` kinds, hook_create.cpp's
// forward parameter lists), Copyright (C) The AMX Mod X Development Team.

//! Function signatures and values.

use amxx::Cell;

/// A type in a hamsandwich signature (`Int_Entvar_Entvar_Float_Int` ...).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Void,
    Int,
    Bool,
    Short,
    Float,
    Vector,
    /// `Vector *`: the function may write it; ExecuteHam writes it back.
    PVector,
    /// `float *`, likewise.
    PFloat,
    Str,
    Cbase,
    Entvar,
    Edict,
    /// `TraceResult *`, a handle for fakemeta's `get_tr2`/`set_tr2`.
    Trace,
    /// `ItemInfo *`, a handle for `GetHamItemInfo`/`SetHamItemInfo`.
    ItemInfo,
}

impl Ty {
    /// Entity types: plugins see indices.
    pub fn is_entity(self) -> bool {
        matches!(self, Ty::Cbase | Ty::Entvar | Ty::Edict)
    }

    /// `Data`'s kind name for "Wrong data type (data is of type %s)".
    pub fn data_name(self) -> &'static str {
        match self {
            Ty::Void => "void",
            Ty::Int | Ty::Bool | Ty::Short => "integer",
            Ty::Float | Ty::PFloat => "float",
            Ty::Vector | Ty::PVector => "vector",
            Ty::Str => "string",
            Ty::Cbase | Ty::Entvar | Ty::Edict => "entity",
            Ty::Trace => "traceresult",
            Ty::ItemInfo => "iteminfo",
        }
    }

    /// The zero value of the type (stock starts `ret` and `origret` at 0).
    pub fn zero(self) -> Val {
        match self {
            Ty::Void => Val::Void,
            Ty::Int | Ty::Bool | Ty::Short => Val::Int(0),
            Ty::Float | Ty::PFloat => Val::Float(0.0),
            Ty::Vector | Ty::PVector => Val::Vector([0.0; 3]),
            Ty::Str => Val::Str(String::new()),
            Ty::Cbase | Ty::Entvar | Ty::Edict => Val::Ent(-1),
            Ty::Trace | Ty::ItemInfo => Val::Handle(0),
        }
    }

    /// How a forward receives a value of this type (`hook_create.cpp`): arrays for vectors,
    /// strings for strings, cells for the rest (floats as their bits: the plugin's `Float:`
    /// parameter is the same cell).
    pub fn forward_param(self) -> FwdType {
        match self {
            Ty::Vector | Ty::PVector => FwdType::Array,
            Ty::Str => FwdType::Str,
            _ => FwdType::Cell,
        }
    }

    /// By-reference parameters ExecuteHam adds after the arguments for the return value
    /// (`call_funcs.cpp`): a float or vector out, or a string buffer and its length.
    pub fn execute_ret_params(self) -> usize {
        match self {
            Ty::Float | Ty::Vector => 1,
            Ty::Str => 2,
            _ => 0,
        }
    }
}

/// A forward parameter type (AMX Mod X's `ForwardParam`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwdType {
    Cell,
    Str,
    Array,
}

/// A value passed to a forward.
#[derive(Clone, Debug, PartialEq)]
pub enum FwdArg {
    Cell(Cell),
    Str(String),
    Array([Cell; 3]),
}

/// An argument or return value.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Void,
    Int(i32),
    Float(f32),
    Vector([f32; 3]),
    Str(String),
    /// An entity index, -1 for none.
    Ent(i32),
    /// A `TraceResult *` or `ItemInfo *` as a cell (32-bit pointers).
    Handle(Cell),
}

impl Val {
    /// The value as a forward argument; `ent` overrides an entity's index (the index stock
    /// keeps for forwards after `SetHamParamEntity`).
    pub fn forward_arg(&self) -> FwdArg {
        match *self {
            Val::Void => FwdArg::Cell(0),
            Val::Int(i) | Val::Ent(i) | Val::Handle(i) => FwdArg::Cell(i),
            Val::Float(f) => FwdArg::Cell(f.to_bits() as Cell),
            Val::Vector(v) => FwdArg::Array(v.map(|x| x.to_bits() as Cell)),
            Val::Str(ref s) => FwdArg::Str(s.clone()),
        }
    }
}

/// One `Ham` function.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    /// The plugin's name (`Ham_TakeDamage`).
    pub ham_name: &'static str,
    /// Stock's hook name (`takedamage`), used in its messages.
    pub name: &'static str,
    pub ret: Ty,
    pub args: &'static [Ty],
    /// `V_REMOVED`: gone from the mod (HAM_FUNC_NOT_AVAILABLE).
    pub removed: bool,
    /// `Deprecated` signature: never configured (HAM_FUNC_NOT_CONFIGURED).
    pub deprecated: bool,
}

impl Entry {
    pub const fn new(ham_name: &'static str, name: &'static str, ret: Ty, args: &'static [Ty]) -> Entry {
        Entry { ham_name, name, ret, args, removed: false, deprecated: false }
    }
    pub const fn removed(ham_name: &'static str, name: &'static str) -> Entry {
        Entry { ham_name, name, ret: Ty::Void, args: &[], removed: true, deprecated: false }
    }
    pub const fn deprecated(ham_name: &'static str, name: &'static str) -> Entry {
        Entry { ham_name, name, ret: Ty::Void, args: &[], removed: false, deprecated: true }
    }

    /// The forward's parameter types: `this`, then the arguments.
    pub fn forward_params(&self) -> Vec<FwdType> {
        std::iter::once(FwdType::Cell).chain(self.args.iter().map(|t| t.forward_param())).collect()
    }
}

/// `HAM_*` results.
pub mod result {
    pub const UNSET: i32 = 0;
    pub const IGNORED: i32 = 1;
    pub const HANDLED: i32 = 2;
    pub const OVERRIDE: i32 = 3;
    pub const SUPERCEDE: i32 = 4;
}

/// `HamError` codes passed to `__fatal_ham_error`.
pub mod error {
    pub const INVALID_FUNC: i32 = 1;
    pub const FUNC_NOT_CONFIGURED: i32 = 2;
    pub const FUNC_NOT_AVAILABLE: i32 = 3;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooklist::HOOKS;

    #[test]
    fn table_follows_ham_const() {
        assert_eq!(HOOKS.len(), 481);
        // ham_const.inc's numbers, and amxx_api.cpp's alignment asserts.
        for (i, name, hook) in [
            (0, "Ham_Spawn", "spawn"),
            (9, "Ham_TakeDamage", "takedamage"),
            (11, "Ham_Killed", "killed"),
            (41, "Ham_Think", "think"),
            (42, "Ham_Touch", "touch"),
            (43, "Ham_Use", "use"),
        ] {
            assert_eq!((HOOKS[i].ham_name, HOOKS[i].name), (name, hook));
        }
        for hook in
            ["fvecvisible", "player_updateclientdata", "item_addtoplayer", "ts_breakablerespawn", "ts_shouldcollide", "item_getiteminfo"]
        {
            assert!(HOOKS.iter().any(|e| e.name == hook), "{hook}");
        }
        let td = &HOOKS[9];
        assert_eq!((td.ret, td.args), (Ty::Int, &[Ty::Entvar, Ty::Entvar, Ty::Float, Ty::Int][..]));
        assert_eq!(td.forward_params(), [FwdType::Cell; 5]);
        let ta = HOOKS.iter().find(|e| e.name == "traceattack").unwrap();
        assert_eq!(ta.forward_params()[3], FwdType::Array);
        assert!(HOOKS.iter().find(|e| e.name == "ts_respawnwait").unwrap().deprecated);
        assert!(HOOKS.iter().all(|e| e.args.len() <= ts_plugin_api::ham::MAX_ARGS));
    }

    #[test]
    fn values_reach_forwards_as_cells() {
        assert_eq!(Val::Float(1.5).forward_arg(), FwdArg::Cell(1.5f32.to_bits() as Cell));
        assert_eq!(Val::Ent(-1).forward_arg(), FwdArg::Cell(-1));
        assert_eq!(Ty::Str.execute_ret_params(), 2);
    }
}
