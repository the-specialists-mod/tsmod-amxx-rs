// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports the module setup of
// modules/hamsandwich (amxx_api.cpp, moduleconfig.h), Copyright (C) The AMX Mod X Development
// Team.

//! The FFI layer: exports, the server hook, and the real [`Host`](crate::host::Host).

mod api;
mod host;

use crate::natives as n;
use amxx::Cell;
use amxx::meta::MetaDef;
use amxx::module::{ModuleDef, nothing};
use amxx::native::Args;
use host::RealHost;

const VERSION: &core::ffi::CStr = match core::ffi::CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
    Ok(v) => v,
    Err(_) => c"0",
};

static MODULE: ModuleDef = ModuleDef {
    name: c"Ham Sandwich",
    author: c"AMX Mod X Dev Team; Rust port: tsmod-amxx-rs",
    version: VERSION,
    logtag: c"HAMSANDWICH",
    library: c"hamsandwich",
    libclass: c"",
    reload: false,
    check_game: None,
    attach: amxx_attach,
    detach: amxx_detach,
    plugins_loaded,
    plugins_unloading: nothing,
    plugins_unloaded,
};

/// A metamod plugin for the engine table and metamod's game-library path (the API lookup); it
/// hooks nothing.
static META: MetaDef = MetaDef {
    name: c"Ham Sandwich",
    version: VERSION,
    date: c"",
    author: c"AMX Mod X Dev Team; Rust port: tsmod-amxx-rs",
    url: c"https://github.com/the-specialists-mod/tsmod-amxx-rs",
    logtag: c"HAMSANDWICH",
    attach: nothing,
    dll: |_| {},
    dll_post: |_| {},
    engine: |_| {},
    engine_post: |_| {},
};

amxx::export_module!(MODULE);
amxx::export_metamod!(META);

/// `OnAmxxAttach`: all natives (stock registers them only when its gamedata loaded; here they
/// always exist and fail like stock's "not configured" without the server API).
fn amxx_attach() {
    amxx::core::add_natives(&NATIVES);
}

/// `OnPluginsLoaded`: finds the server's entity hooks and installs the hook.
fn plugins_loaded() {
    match api::find() {
        Ok(_) => {}
        Err(why) => amxx::core::log(&format!(
            "WARNING: no TS plugin API version 5 ({why}). Ham Sandwich needs a Rust TS server with entity hooks; until then \
             RegisterHam and ExecuteHam fail as for functions missing from the gamedata."
        )),
    }
}

/// `OnPluginsUnloaded`: the plugins' forwards are gone, so are their hooks.
fn plugins_unloaded() {
    let watched: Vec<usize> = crate::state::with(|s| {
        s.clear();
        let w = s.watched.iter().enumerate().filter(|(_, w)| **w).map(|(i, _)| i).collect();
        s.watched.fill(false);
        w
    })
    .unwrap_or_default();
    if let Some(a) = api::table() {
        for ham in watched {
            (a.ham_watch)(ham as i32, 0);
        }
    }
}

/// `OnAmxxDetach`: the server must not call into an unloaded library.
fn amxx_detach() {
    if let Some(a) = api::table() {
        (a.ham_set_hook)(None);
    }
    host::free_item_infos();
}

macro_rules! bind {
    ($($name:ident => $f:path),* $(,)?) => {
        $(fn $name(a: &mut dyn Args) -> Cell { $f(&RealHost, a) })*
    };
}

bind! {
    b_register_ham => n::register_ham,
    b_register_ham_from_entity => n::register_ham_from_entity,
    b_execute_ham => n::execute_ham,
    b_execute_ham_b => n::execute_ham_b,
    b_is_ham_valid => n::is_ham_valid,
    b_disable_ham_forward => n::disable_ham_forward,
    b_enable_ham_forward => n::enable_ham_forward,
    b_get_ham_return_integer => n::get_ham_return_integer,
    b_get_ham_return_float => n::get_ham_return_float,
    b_get_ham_return_vector => n::get_ham_return_vector,
    b_get_ham_return_entity => n::get_ham_return_entity,
    b_get_ham_return_string => n::get_ham_return_string,
    b_get_orig_ham_return_integer => n::get_orig_ham_return_integer,
    b_get_orig_ham_return_float => n::get_orig_ham_return_float,
    b_get_orig_ham_return_vector => n::get_orig_ham_return_vector,
    b_get_orig_ham_return_entity => n::get_orig_ham_return_entity,
    b_get_orig_ham_return_string => n::get_orig_ham_return_string,
    b_set_ham_return_integer => n::set_ham_return_integer,
    b_set_ham_return_float => n::set_ham_return_float,
    b_set_ham_return_vector => n::set_ham_return_vector,
    b_set_ham_return_entity => n::set_ham_return_entity,
    b_set_ham_return_string => n::set_ham_return_string,
    b_get_ham_return_status => n::get_ham_return_status,
    b_set_ham_param_integer => n::set_ham_param_integer,
    b_set_ham_param_float => n::set_ham_param_float,
    b_set_ham_param_vector => n::set_ham_param_vector,
    b_set_ham_param_entity => n::set_ham_param_entity,
    b_set_ham_param_entity2 => n::set_ham_param_entity2,
    b_set_ham_param_string => n::set_ham_param_string,
    b_set_ham_param_trace_result => n::set_ham_param_trace_result,
    b_set_ham_param_item_info => n::set_ham_param_item_info,
    b_get_ham_item_info => n::get_ham_item_info,
    b_set_ham_item_info => n::set_ham_item_info,
    b_create_ham_item_info => n::create_ham_item_info,
    b_free_ham_item_info => n::free_ham_item_info,
    b_get_pdata_cbase => n::get_pdata_cbase,
    b_set_pdata_cbase => n::set_pdata_cbase,
    b_get_pdata_cbase_safe => n::get_pdata_cbase_safe,
}

// `RegisterNatives`, `ReturnNatives`, `pdata_natives` and `pdata_natives_safe`, with stock's names.
amxx::natives! {
    pub(crate) static NATIVES = [
        c"RegisterHam" => b_register_ham,
        c"RegisterHamFromEntity" => b_register_ham_from_entity,
        c"ExecuteHam" => b_execute_ham,
        c"ExecuteHamB" => b_execute_ham_b,
        c"IsHamValid" => b_is_ham_valid,
        c"DisableHamForward" => b_disable_ham_forward,
        c"EnableHamForward" => b_enable_ham_forward,
        c"GetHamReturnInteger" => b_get_ham_return_integer,
        c"GetHamReturnFloat" => b_get_ham_return_float,
        c"GetHamReturnVector" => b_get_ham_return_vector,
        c"GetHamReturnEntity" => b_get_ham_return_entity,
        c"GetHamReturnString" => b_get_ham_return_string,
        c"GetOrigHamReturnInteger" => b_get_orig_ham_return_integer,
        c"GetOrigHamReturnFloat" => b_get_orig_ham_return_float,
        c"GetOrigHamReturnVector" => b_get_orig_ham_return_vector,
        c"GetOrigHamReturnEntity" => b_get_orig_ham_return_entity,
        c"GetOrigHamReturnString" => b_get_orig_ham_return_string,
        c"SetHamReturnInteger" => b_set_ham_return_integer,
        c"SetHamReturnFloat" => b_set_ham_return_float,
        c"SetHamReturnVector" => b_set_ham_return_vector,
        c"SetHamReturnEntity" => b_set_ham_return_entity,
        c"SetHamReturnString" => b_set_ham_return_string,
        c"GetHamReturnStatus" => b_get_ham_return_status,
        c"SetHamParamInteger" => b_set_ham_param_integer,
        c"SetHamParamFloat" => b_set_ham_param_float,
        c"SetHamParamVector" => b_set_ham_param_vector,
        c"SetHamParamEntity" => b_set_ham_param_entity,
        c"SetHamParamEntity2" => b_set_ham_param_entity2,
        c"SetHamParamString" => b_set_ham_param_string,
        c"SetHamParamTraceResult" => b_set_ham_param_trace_result,
        c"SetHamParamItemInfo" => b_set_ham_param_item_info,
        c"GetHamItemInfo" => b_get_ham_item_info,
        c"SetHamItemInfo" => b_set_ham_item_info,
        c"CreateHamItemInfo" => b_create_ham_item_info,
        c"FreeHamItemInfo" => b_free_ham_item_info,
        c"get_pdata_cbase" => b_get_pdata_cbase,
        c"set_pdata_cbase" => b_set_pdata_cbase,
        c"get_pdata_cbase_safe" => b_get_pdata_cbase_safe,
    ];
}

#[cfg(test)]
mod tests {
    use super::NATIVES;

    #[test]
    fn native_table_matches_stock() {
        let names: Vec<String> = NATIVES
            .iter()
            .take_while(|n| !n.name.is_null())
            // SAFETY: the table's names are `'static` C string literals.
            .map(|n| unsafe { core::ffi::CStr::from_ptr(n.name) }.to_string_lossy().into())
            .collect();
        // 7 + 28 + 2 + 1 in hook_native.cpp, DataHandler.cpp and pdata.cpp.
        assert_eq!(names.len(), 38);
        let mut d = names.clone();
        d.sort();
        d.dedup();
        assert_eq!(d.len(), 38);
    }
}
