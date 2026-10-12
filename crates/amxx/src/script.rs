// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions. Ports `RankSystem::loadCalc`,
// `unloadCalc` and the script call in `updatePos` (modules/ts/tsx/CRank.cpp), Copyright (C)
// The AMX Mod X Development Team, Copyright (C) 2004 Lukasz Wlasinski.

//! A plugin the module loads and runs itself: the stats modules' score calculator
//! (`data/tsstats.amxx`, `public get_score(stats[8], body[8])`).

use crate::core::{c_string, core};
use crate::ffi::{AMX_ERR_NONE, Amx, Cell};
use core::ffi::{c_char, c_int, c_void};

/// The `get_score` public of a loaded score plugin, with its two 8-cell argument arrays.
pub struct ScoreScript {
    amx: Box<Amx>,
    code: *mut c_void,
    func: c_int,
    addr: [Cell; 2],
    phys: [*mut Cell; 2],
}

// SAFETY: used on the main thread only; it sits in the module's state.
unsafe impl Send for ScoreScript {}

impl ScoreScript {
    /// `loadCalc`: loads the plugin, allocates the two arrays in its heap and finds `get_score`.
    pub fn load(path: &str) -> Result<ScoreScript, String> {
        let c = core().ok_or("AMX Mod X isn't attached")?;
        let p = c_string(path);
        // SAFETY: every field of `Amx` is an integer or a raw pointer, so all-zero is valid; it
        // is how the SDK passes a fresh one (`calc.amx` starts zeroed).
        let fresh: Amx = unsafe { core::mem::zeroed() };
        let mut s =
            ScoreScript { amx: Box::new(fresh), code: core::ptr::null_mut(), func: 0, addr: [0; 2], phys: [core::ptr::null_mut(); 2] };
        let mut err = [0 as c_char; 128];
        // SAFETY: the core's loader with a zeroed AMX, an out-pointer for the code and an error
        // buffer of the size each variant expects (64 for the old one).
        let rc = unsafe {
            match c.load_amx_script_ex {
                Some(f) => f(&mut *s.amx, &mut s.code, p.as_ptr(), err.as_mut_ptr(), err.len(), 0),
                None => (c.load_amx_script)(&mut *s.amx, &mut s.code, p.as_ptr(), err.as_mut_ptr(), 0),
            }
        };
        if rc != AMX_ERR_NONE {
            // SAFETY: the loader wrote a NUL-terminated message into the zeroed buffer.
            let e = unsafe { crate::core::cstr_lossy(err.as_ptr()) };
            s.unload();
            return Err(e);
        }
        for i in 0..2 {
            // SAFETY: the loaded AMX; Allot reserves 8 cells in its heap and returns both views.
            if unsafe { (c.amx_allot)(&mut *s.amx, 8, &mut s.addr[i], &mut s.phys[i]) } != AMX_ERR_NONE {
                s.unload();
                return Err("amx_Allot failed".into());
            }
        }
        // SAFETY: the loaded AMX and a C string.
        if unsafe { (c.amx_find_public)(&mut *s.amx, c"get_score".as_ptr(), &mut s.func) } != AMX_ERR_NONE {
            s.unload();
            return Err("no public get_score".into());
        }
        Ok(s)
    }

    /// `get_score(stats, body)`: `stats` are the 7 cells kills, deaths, headshots, team kills,
    /// shots, hits, damage, and `body` the body hits (index 0 unused).
    pub fn score(&mut self, stats: &[Cell; 7], body: &[Cell; 8]) -> Result<Cell, i32> {
        let Some(c) = core() else { return Err(-1) };
        // SAFETY: `phys` point at the 8-cell blocks allotted in `load`, live while the AMX is.
        unsafe {
            core::ptr::copy_nonoverlapping(stats.as_ptr(), self.phys[0], 7);
            core::ptr::copy_nonoverlapping(body.as_ptr().add(1), self.phys[1].add(1), 7);
        }
        let mut result = 0;
        // SAFETY: the loaded AMX; arguments are pushed last to first, as the SDK does.
        let err = unsafe {
            (c.amx_push)(&mut *self.amx, self.addr[1]);
            (c.amx_push)(&mut *self.amx, self.addr[0]);
            (c.amx_exec)(&mut *self.amx, &mut result, self.func)
        };
        if err == AMX_ERR_NONE { Ok(result) } else { Err(err) }
    }

    fn unload(&mut self) {
        if let Some(c) = core() {
            // SAFETY: the AMX and code the loader filled (UnloadAmxScript tolerates NULL code).
            unsafe { (c.unload_amx_script)(&mut *self.amx, &mut self.code) };
        }
        self.code = core::ptr::null_mut();
    }
}

impl Drop for ScoreScript {
    fn drop(&mut self) {
        if !self.code.is_null() {
            self.unload();
        }
    }
}
