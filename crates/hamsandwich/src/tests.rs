// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Registration, dispatch with stock's aggregation, the data natives, ExecuteHam(B), ItemInfo
//! and pdata, against [`FakeHost`]. Each test runs on its own thread, with its own state.

use crate::hooks::{FwdArg, Val, error, result::*};
use crate::host::{HamApi, Host};
use crate::natives as n;
use crate::testing::*;
use amxx::Cell;
use amxx::native::FakeArgs;

fn f(x: f32) -> Cell {
    x.to_bits() as Cell
}

/// `RegisterHam(func, class, callback, post)`.
fn register(h: &FakeHost, func: Cell, class: &str, callback: &str, post: bool) -> (Cell, Vec<String>) {
    let mut a = FakeArgs::new(&[]);
    let (c, cb) = (a.alloc_str(class), a.alloc_str(callback));
    a.params = vec![func, c, cb, post as Cell];
    (n::register_ham(h, &mut a), a.errors)
}

/// Calls a native from inside a forward.
fn call(h: &FakeHost, native: fn(&dyn crate::host::Host, &mut dyn amxx::native::Args) -> Cell, params: &[Cell]) -> Cell {
    n_call(h, native, &mut FakeArgs::new(params))
}

fn n_call(h: &FakeHost, native: fn(&dyn crate::host::Host, &mut dyn amxx::native::Args) -> Cell, a: &mut FakeArgs) -> Cell {
    native(h, a)
}

fn takedamage(h: &FakeHost, victim: i32, dmg: f32) -> Val {
    h.game_call(TAKEDAMAGE, victim, vec![Val::Ent(2), Val::Ent(2), Val::Float(dmg), Val::Int(0)])
}

#[test]
fn register_errors_follow_stock() {
    let h = FakeHost::new();
    h.public("cb", |_, _| IGNORED);
    assert_eq!(register(&h, 9999, "player", "cb", false).0, 0);
    let removed = crate::hooklist::HOOKS.iter().position(|e| e.removed).unwrap() as Cell;
    assert_eq!(register(&h, removed, "player", "cb", false).0, 0, "a removed Sven Co-op function");
    assert_eq!(register(&h, 1, "player", "cb", false).0, 0, "Ham_Precache: the server has no such hook");
    let fails: Vec<i32> = h.failed.borrow().iter().map(|f| f.1).collect();
    assert_eq!(fails, [error::INVALID_FUNC, error::FUNC_NOT_AVAILABLE, error::FUNC_NOT_CONFIGURED]);
    assert!(h.failed.borrow()[0].2.starts_with("Function out of bounds.  Got: 9999  Max: 480"));

    let (r, e) = register(&h, TAKEDAMAGE, "no_such_class", "cb", false);
    assert_eq!((r, e), (0, vec!["Failed to retrieve classtype for \"no_such_class\", hook for \"cb\" not active.".to_owned()]));
    let (r, e) = register(&h, TAKEDAMAGE, "player", "missing", false);
    assert_eq!((r, e), (0, vec!["Function missing not found.".to_owned()]));
    let (r, e) = register(&h, THINK, "player", "cb", false);
    assert_eq!((r, e.len()), (0, 1), "the class lacks the function");
    assert!(register(&h, TAKEDAMAGE, "player", "cb", false).0 > 0);

    for (func, valid) in [(TAKEDAMAGE, 1), (1, 0), (removed, 0), (-1, 0), (481, 0)] {
        assert_eq!(call(&h, n::is_ham_valid, &[func]), valid, "IsHamValid({func})");
    }
}

#[test]
fn without_the_api_nothing_is_configured() {
    let h = FakeHost { has_api: false, ..FakeHost::new() };
    h.public("cb", |_, _| IGNORED);
    assert_eq!(register(&h, TAKEDAMAGE, "player", "cb", false).0, 0);
    assert_eq!(h.failed.borrow()[0].1, error::FUNC_NOT_CONFIGURED);
    assert_eq!(call(&h, n::is_ham_valid, &[TAKEDAMAGE]), 0);
    assert_eq!(h.logs.borrow().len(), 1);
}

#[test]
fn pre_changes_damage_and_post_sees_it() {
    let h = FakeHost::new();
    h.public("halve", |h, args| {
        assert_eq!(cell(&args[3]), f(40.0));
        call(h, n::set_ham_param_float, &[4, f(20.0)]);
        HANDLED
    });
    h.public("check", |h, args| {
        // A later pre forward sees the change; so does the post one.
        assert_eq!(cell(&args[3]), f(20.0));
        assert_eq!(call(h, n::get_ham_return_status, &[]), HANDLED);
        IGNORED
    });
    h.public("after", |h, args| {
        assert_eq!(cell(&args[3]), f(20.0));
        let mut a = FakeArgs::new(&[]);
        let r = a.alloc(vec![0]);
        a.params = vec![r];
        assert_eq!(n::get_orig_ham_return_integer(h, &mut a), 0);
        assert_eq!(a.memory[r as usize][0], 1, "TakeDamage returned 1");
        IGNORED
    });
    register(&h, TAKEDAMAGE, "player", "halve", false);
    register(&h, TAKEDAMAGE, "player", "check", false);
    register(&h, TAKEDAMAGE, "player", "after", true);
    assert_eq!(takedamage(&h, 1, 40.0), Val::Int(1));
    assert_eq!(h.health(1), 80.0);
    // Other classes aren't hooked.
    takedamage(&h, 50, 40.0);
    assert_eq!(h.health(50), 60.0);
}

#[test]
fn supercede_skips_the_function_and_the_last_result_decides_the_return() {
    let h = FakeHost::new();
    h.public("block", |h, _| {
        call(h, n::set_ham_return_integer, &[7]);
        SUPERCEDE
    });
    let (block, _) = register(&h, TAKEDAMAGE, "player", "block", false);
    assert_eq!(takedamage(&h, 1, 40.0), Val::Int(7), "superceded with no post forward: the plugin's value");
    assert_eq!(h.health(1), 100.0, "the function didn't run");

    // A post forward that ignores makes stock return the original value, 0 after a supercede.
    h.public("post_ignore", |h, _| {
        let st = call(h, n::get_ham_return_status, &[]);
        h.logs.borrow_mut().push(format!("status {st}"));
        IGNORED
    });
    register(&h, TAKEDAMAGE, "player", "post_ignore", true);
    assert_eq!(takedamage(&h, 1, 40.0), Val::Int(0));
    assert_eq!(h.logs.borrow().last().map(String::as_str), Some("status 4"), "post forwards see the pre supercede");

    // Disabled, the hook stops superceding.
    assert_eq!(call(&h, n::disable_ham_forward, &[block]), 0);
    assert_eq!(takedamage(&h, 1, 40.0), Val::Int(1));
    assert_eq!(h.health(1), 60.0);
    assert_eq!(call(&h, n::enable_ham_forward, &[block]), 0);
    takedamage(&h, 1, 40.0);
    assert_eq!(h.health(1), 60.0);
    let mut bad = FakeArgs::new(&[0]);
    assert_eq!(n::disable_ham_forward(&h, &mut bad), -1);
    assert_eq!(bad.errors, ["Invalid HamHook handle."]);
}

#[test]
fn override_in_post_replaces_the_return_value() {
    let h = FakeHost::new();
    h.public("over", |h, _| {
        call(h, n::set_ham_return_integer, &[42]);
        OVERRIDE
    });
    register(&h, TAKEDAMAGE, "player", "over", true);
    assert_eq!(takedamage(&h, 1, 10.0), Val::Int(42));
    assert_eq!(h.health(1), 90.0, "the function ran");
}

#[test]
fn entity_params_and_the_index_forwards_see() {
    let h = FakeHost::new();
    h.public("swap", |h, args| {
        assert_eq!(cell(&args[1]), 2);
        call(h, n::set_ham_param_entity, &[2, 3]);
        IGNORED
    });
    h.public("look", |_, args| {
        // SetHamParamEntity keeps the index later forwards see (stock).
        assert_eq!(cell(&args[1]), 2);
        IGNORED
    });
    h.public("swap2", |h, _| {
        call(h, n::set_ham_param_entity2, &[3, 3]);
        IGNORED
    });
    h.public("look2", |_, args| {
        assert_eq!(cell(&args[2]), 3, "SetHamParamEntity2 updates it");
        IGNORED
    });
    register(&h, TAKEDAMAGE, "player", "swap2", false);
    register(&h, TAKEDAMAGE, "player", "look2", false);
    register(&h, TAKEDAMAGE, "player", "swap", false);
    register(&h, TAKEDAMAGE, "player", "look", false);
    takedamage(&h, 1, 1.0);
    let errors = {
        let mut a = FakeArgs::new(&[9, 1]);
        crate::state::with(|s| assert!(s.frames.is_empty()));
        n::set_ham_param_integer(&h, &mut a);
        a.errors
    };
    assert_eq!(errors, ["ParamStack is empty!"]);
}

#[test]
fn data_natives_check_types_and_bounds() {
    let h = FakeHost::new();
    h.public("probe", |h, _| {
        let mut a = FakeArgs::new(&[9, 5]);
        assert_eq!(n::set_ham_param_integer(h, &mut a), 0);
        assert_eq!(a.errors, ["Invalid parameter number, got 9, expected 5"]);
        let mut b = FakeArgs::new(&[4, 5]);
        assert_eq!(n::set_ham_param_integer(h, &mut b), -1, "param 4 is the float damage");
        assert_eq!(b.errors, ["Wrong data type (data is of type float)"]);
        let mut c = FakeArgs::new(&[f(1.0)]);
        assert_eq!(n::set_ham_return_float(h, &mut c), -1);
        IGNORED
    });
    h.public("void", |h, _| {
        let mut a = FakeArgs::new(&[1]);
        assert_eq!(n::set_ham_return_integer(h, &mut a), -2);
        assert_eq!(a.errors, ["Data pointer is NULL!"]);
        IGNORED
    });
    register(&h, TAKEDAMAGE, "player", "probe", false);
    register(&h, SPAWN, "player", "void", false);
    takedamage(&h, 1, 1.0);
    h.game_call(SPAWN, 1, vec![]);
    let mut a = FakeArgs::new(&[]);
    assert_eq!(n::get_ham_return_status(&h, &mut a), 0);
    assert_eq!(a.errors, ["ReturnStatus is empty!"]);
}

#[test]
fn execute_ham_and_execute_ham_b() {
    let h = FakeHost::new();
    h.public("count", |h, _| {
        h.logs.borrow_mut().push("hooked".into());
        IGNORED
    });
    register(&h, TAKEDAMAGE, "player", "count", false);
    let mut a = FakeArgs::new(&[]);
    let refs: Vec<Cell> = [2, 2, f(25.0), 0].iter().map(|&v| a.alloc(vec![v])).collect();
    a.params = [&[TAKEDAMAGE, 1][..], &refs[..]].concat();
    assert_eq!(n::execute_ham(&h, &mut a), 1);
    assert_eq!((h.health(1), h.logs.borrow().len()), (75.0, 0), "ExecuteHam skips the hooks");
    assert_eq!(n::execute_ham_b(&h, &mut a), 1);
    assert_eq!((h.health(1), h.logs.borrow().len()), (50.0, 1), "ExecuteHamB runs them");

    // A vector result goes to the by-reference parameter after the arguments.
    let mut v = FakeArgs::new(&[]);
    let out = v.alloc(vec![0; 3]);
    v.params = vec![CENTER, 1, out];
    h.watch(CENTER, false);
    assert_eq!(n::execute_ham(&h, &mut v), 1);
    assert_eq!(v.memory[out as usize], [f(1.0), f(2.0), f(3.0)]);

    // The port's `Ham_TS_GiveSlowMul` returns its float: into an out parameter when the plugin
    // passes one, and a stock-compiled call without one still runs.
    let mut slow = FakeArgs::new(&[]);
    let out = slow.alloc(vec![0]);
    slow.params = vec![GIVESLOWMUL, 1, out];
    assert_eq!(n::execute_ham(&h, &mut slow), 1);
    assert_eq!(slow.memory[out as usize], [f(0.5)]);
    let mut stock = FakeArgs::new(&[GIVESLOWMUL, 1]);
    assert_eq!(n::execute_ham(&h, &mut stock), 1);
    assert!(stock.errors.is_empty(), "{:?}", stock.errors);

    // Arguments and entities are checked as stock.
    let mut short = FakeArgs::new(&[TAKEDAMAGE, 1]);
    assert_eq!(n::execute_ham(&h, &mut short), 0);
    assert_eq!(short.errors, ["Bad arg count.  Expected 6, got 2."]);
    let mut gone = FakeArgs::new(&[]);
    let refs: Vec<Cell> = [2, 2, f(1.0), 0].iter().map(|&v| gone.alloc(vec![v])).collect();
    gone.params = [&[TAKEDAMAGE, 77][..], &refs[..]].concat();
    assert_eq!(n::execute_ham(&h, &mut gone), 0);
    assert_eq!(gone.errors, ["Invalid entity (77)"]);
}

#[test]
fn nested_calls_keep_their_own_frames() {
    let h = FakeHost::new();
    // Player 1's damage hurts the breakable too, through ExecuteHamB, inside the forward.
    h.public("chain", |h, args| {
        if cell(&args[0]) == 1 {
            let mut a = FakeArgs::new(&[]);
            let refs: Vec<Cell> = [1, 1, f(10.0), 0].iter().map(|&v| a.alloc(vec![v])).collect();
            a.params = [&[TAKEDAMAGE, 50][..], &refs[..]].concat();
            n::execute_ham_b(h, &mut a);
            // Back in the outer call: its own parameters.
            let mut b = FakeArgs::new(&[4, f(5.0)]);
            assert_eq!(n::set_ham_param_float(h, &mut b), 0);
        }
        IGNORED
    });
    register(&h, TAKEDAMAGE, "player", "chain", false);
    register(&h, TAKEDAMAGE, "func_breakable", "chain", false);
    takedamage(&h, 1, 40.0);
    assert_eq!((h.health(1), h.health(50)), (95.0, 90.0));
    crate::state::with(|s| assert!(s.frames.is_empty()));
}

#[test]
fn item_infos() {
    let h = FakeHost::new();
    let ii = call(&h, n::create_ham_item_info, &[]);
    let mut a = FakeArgs::new(&[]);
    let (v, s) = (a.alloc(vec![30]), a.alloc_str("9mm"));
    a.params = vec![ii, 7, v];
    assert_eq!(n::set_ham_item_info(&h, &mut a), 1);
    a.params = vec![ii, 2, s];
    assert_eq!(n::set_ham_item_info(&h, &mut a), 3);
    assert_eq!(call(&h, n::get_ham_item_info, &[ii, 7]), 30);
    let buf = a.alloc(vec![0; 16]);
    a.params = vec![ii, 2, buf, 15];
    n::get_ham_item_info(&h, &mut a);
    assert_eq!(a.read_str(buf), "9mm");
    let mut bad = FakeArgs::new(&[ii, 2]);
    assert_eq!(n::get_ham_item_info(&h, &mut bad), 0);
    assert_eq!(bad.errors, ["Bad arg count.  Expected 4, got 2."]);
    let mut unknown = FakeArgs::new(&[0x55, 7]);
    assert_eq!(n::get_ham_item_info(&h, &mut unknown), 0, "never dereferences an unknown handle");
    assert_eq!(call(&h, n::free_ham_item_info, &[ii]), 1);
    assert_eq!(call(&h, n::free_ham_item_info, &[ii]), 0);
}

/// Without the server's private-data size (no plugin API version 5) the natives read nothing.
#[test]
fn pdata_natives_without_the_server_read_nothing() {
    let h = FakeHost { has_api: false, ..FakeHost::new() };
    assert_eq!(call(&h, n::get_pdata_cbase, &[1, 100, 5, 5]), -1);
    assert_eq!(call(&h, n::get_pdata_cbase_safe, &[1, 100, 5, 5]), -1);
    assert_eq!(call(&h, n::set_pdata_cbase, &[1, 100, 2, 5, 5]), 1);
    assert_eq!(h.logs.borrow().len(), 1, "logged once");
    let mut a = FakeArgs::new(&[77, 100, 5, 5]);
    assert_eq!(n::get_pdata_cbase(&h, &mut a), 0);
    assert_eq!(a.errors, ["Invalid entity 77"]);
    let mut b = FakeArgs::new(&[1, -100, 5, 5]);
    assert_eq!(n::get_pdata_cbase(&h, &mut b), 0);
    assert_eq!(b.errors.len(), 1);
}

/// With the server's mirror (plugin API version 5): pointers come back as entities, writes land
/// in the block, and the block's end and the server's link are guarded.
#[test]
fn pdata_cbase_reads_and_writes_the_mirror() {
    let h = FakeHost::new();
    h.pdata.borrow_mut().insert(1, vec![0; 0x3000]);
    h.pdata.borrow_mut().insert(50, vec![0; 0x1000]);
    // `m_pActiveItem` (int 639 on Windows, +5 on Linux) as the server mirrors it.
    let linux = if cfg!(target_os = "linux") { 5 } else { 0 };
    let active = (639 + linux) as usize * 4;
    assert!(h.set_pdata_word(1, active, 0x3000, h.private_address(50) as u32));
    assert_eq!(call(&h, n::get_pdata_cbase, &[1, 639, 5, 5]), 50);
    assert_eq!(call(&h, n::get_pdata_cbase_safe, &[1, 639, 5, 5]), 50);
    // NULL, and a pointer to no entity.
    assert_eq!(call(&h, n::get_pdata_cbase, &[1, 600, 5, 5]), -1);
    assert!(h.set_pdata_word(1, 600 * 4 + linux as usize * 4, 0x3000, 0xdead));
    assert_eq!(call(&h, n::get_pdata_cbase, &[1, 600, 5, 5]), -1);
    assert_eq!(call(&h, n::get_pdata_cbase_safe, &[1, 600, 5, 5]), -2);
    // set_pdata_cbase writes the target's pointer, or NULL.
    assert_eq!(call(&h, n::set_pdata_cbase, &[50, 28, 1, 4, 4]), 1);
    assert_eq!(call(&h, n::get_pdata_cbase, &[50, 28, 4, 4]), 1);
    assert_eq!(call(&h, n::set_pdata_cbase, &[50, 28, -1, 4, 4]), 1);
    assert_eq!(call(&h, n::get_pdata_cbase, &[50, 28, 4, 4]), -1);
    assert!(h.logs.borrow().is_empty(), "no note with the server's size");
    // Past the end, and over the server's link: native errors, nothing written.
    let mut a = FakeArgs::new(&[50, 0x1000, 0, 0]);
    assert_eq!(n::get_pdata_cbase(&h, &mut a), 0);
    assert_eq!(a.errors, ["Offset 4096 is past the end of entity 50's private data (4096 bytes)"]);
    let mut b = FakeArgs::new(&[50, 1023, 1, 0, 0]);
    assert_eq!(n::set_pdata_cbase(&h, &mut b), 1, "the last int");
    let mut c = FakeArgs::new(&[50, 2, 1, 0, 0]);
    assert_eq!(n::set_pdata_cbase(&h, &mut c), 0);
    assert_eq!(c.errors, ["Offset 2 would overwrite the server's link to entity 50"]);
    assert_eq!(h.pdata_word(50, 8, 0x1000), Some(0));
}

#[test]
fn string_forward_args() {
    assert_eq!(Val::Str("x".into()).forward_arg(), FwdArg::Str("x".into()));
}
