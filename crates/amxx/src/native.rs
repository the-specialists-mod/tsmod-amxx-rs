// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Native arguments. Natives are written against the [`Args`] trait so their logic can be tested
//! with a fake (an in-memory cell buffer); [`natives!`](crate::natives) builds the core's table
//! with trampolines that wrap the real AMX.

use crate::core::{c_string, core};
use crate::ffi::{Amx, Cell};

/// The arguments of one native call. Indices are 1-based, like `params[i]` in C natives; a
/// missing argument reads as 0 (the C natives would read past the end instead).
pub trait Args {
    /// Number of arguments passed (`params[0] / sizeof(cell)`).
    fn count(&self) -> usize;
    /// `params[i]`.
    fn cell(&self, i: usize) -> Cell;
    /// `amx_ctof(params[i])`.
    fn float(&self, i: usize) -> f32 {
        f32::from_bits(self.cell(i) as u32)
    }
    /// `MF_GetAmxString(amx, params[i], ...)`.
    fn string(&mut self, i: usize) -> String;
    /// `MF_SetAmxString(amx, params[i], s, max)`: copies at most `max` characters and returns
    /// how many it copied.
    fn set_string(&mut self, i: usize, s: &str, max: Cell) -> Cell;
    /// Reads `n` cells of the array at `params[i]`.
    fn read_array(&mut self, i: usize, n: usize) -> Vec<Cell>;
    /// Writes `values` into the array (or by-reference cell) at `params[i]`, from element
    /// `start` on.
    fn write_array(&mut self, i: usize, start: usize, values: &[Cell]);
    /// `*MF_GetAmxAddr(amx, params[i]) = v`.
    fn set_ref(&mut self, i: usize, v: Cell) {
        self.write_array(i, 0, &[v]);
    }
    /// `MF_LogError(amx, AMX_ERR_NATIVE, ...)`.
    fn error(&mut self, msg: &str);
    /// The calling plugin's AMX, as an opaque handle (for per-plugin forwards); 0 in fakes.
    fn amx(&self) -> usize {
        0
    }
}

/// The real arguments: the core's AMX and `params`.
pub struct AmxArgs<'a> {
    amx: *mut Amx,
    params: &'a [Cell],
}

impl AmxArgs<'_> {
    fn addr(&self, i: usize) -> *mut Cell {
        let Some(c) = core() else { return core::ptr::null_mut() };
        // SAFETY: the AMX is the one the core passed to the running native; GetAmxAddr checks
        // the address against the plugin's data section.
        unsafe { (c.get_amx_addr)(self.amx, self.cell(i)) }
    }
}

impl Args for AmxArgs<'_> {
    fn count(&self) -> usize {
        self.params.len()
    }

    fn cell(&self, i: usize) -> Cell {
        if i == 0 { 0 } else { self.params.get(i - 1).copied().unwrap_or(0) }
    }

    fn string(&mut self, i: usize) -> String {
        let Some(c) = core() else { return String::new() };
        let mut len = 0;
        // SAFETY: as in `addr`; buffer 0 is the core's scratch buffer, copied out at once.
        unsafe { crate::core::cstr_lossy((c.get_amx_string)(self.amx, self.cell(i), 0, &mut len)) }
    }

    fn set_string(&mut self, i: usize, s: &str, max: Cell) -> Cell {
        let Some(c) = core() else { return 0 };
        let s = c_string(s);
        // SAFETY: as in `addr`; the core bounds the copy by `max`.
        unsafe { (c.set_amx_string)(self.amx, self.cell(i), s.as_ptr(), max) }
    }

    fn read_array(&mut self, i: usize, n: usize) -> Vec<Cell> {
        let p = self.addr(i);
        if p.is_null() {
            return vec![0; n];
        }
        // SAFETY: the plugin passed an array of at least `n` cells (the native's declared
        // signature, e.g. `Float:origin[3]`).
        unsafe { core::slice::from_raw_parts(p, n) }.to_vec()
    }

    fn write_array(&mut self, i: usize, start: usize, values: &[Cell]) {
        let p = self.addr(i);
        if p.is_null() {
            return;
        }
        // SAFETY: as in `read_array`: the declared array sizes (stats[8], bodyhits[8]) cover
        // `start + values.len()`.
        unsafe { core::ptr::copy_nonoverlapping(values.as_ptr(), p.add(start), values.len()) };
    }

    fn error(&mut self, msg: &str) {
        // SAFETY: the AMX of the running native.
        unsafe { crate::core::log_error(self.amx, msg) }
    }

    fn amx(&self) -> usize {
        self.amx as usize
    }
}

/// Runs a native's logic, for the trampolines [`natives!`](crate::natives) generates.
///
/// # Safety
/// `amx` and `params` are what the core passes a native: its AMX and an array whose first cell
/// is the size of the arguments that follow, in bytes.
#[doc(hidden)]
pub unsafe fn dispatch(amx: *mut Amx, params: *const Cell, f: fn(&mut dyn Args) -> Cell) -> Cell {
    if params.is_null() {
        return 0;
    }
    // SAFETY: the core's contract for natives: `params[0]` is the byte count of the arguments
    // that follow it.
    let params = unsafe {
        let n = (*params).max(0) as usize / size_of::<Cell>();
        core::slice::from_raw_parts(params.add(1), n)
    };
    f(&mut AmxArgs { amx, params })
}

/// Builds a `'static` native table (NULL-terminated) from `"name" => function` pairs, where each
/// function is `fn(&mut dyn Args) -> Cell`.
#[macro_export]
macro_rules! natives {
    ($vis:vis static $table:ident = [$($name:literal => $f:path),* $(,)?];) => {
        $vis static $table: [$crate::ffi::AmxNativeInfo; [$($name),*].len() + 1] = [
            $(
                $crate::ffi::AmxNativeInfo {
                    name: $name.as_ptr(),
                    func: Some({
                        unsafe extern "C" fn trampoline(amx: *mut $crate::ffi::Amx, params: *const $crate::ffi::Cell) -> $crate::ffi::Cell {
                            // SAFETY: the core calls natives with its AMX and `params`.
                            unsafe { $crate::native::dispatch(amx, params, $f) }
                        }
                        trampoline
                    }),
                },
            )*
            $crate::ffi::AmxNativeInfo { name: ::core::ptr::null(), func: None },
        ];
    };
}

/// A fake [`Args`] for tests: `params` are the cells, and each pointer argument is an index into
/// `memory`, a list of cell arrays (strings are one character per cell, NUL-terminated).
#[derive(Default, Debug)]
pub struct FakeArgs {
    pub params: Vec<Cell>,
    pub memory: Vec<Vec<Cell>>,
    pub errors: Vec<String>,
}

impl FakeArgs {
    pub fn new(params: &[Cell]) -> Self {
        FakeArgs { params: params.to_vec(), ..Default::default() }
    }

    /// Adds an array and returns its "address" for `params`.
    pub fn alloc(&mut self, cells: Vec<Cell>) -> Cell {
        self.memory.push(cells);
        (self.memory.len() - 1) as Cell
    }

    /// Adds a string (plus NUL) and returns its address.
    pub fn alloc_str(&mut self, s: &str) -> Cell {
        self.alloc(s.bytes().map(Cell::from).chain([0]).collect())
    }

    /// Reads back the string stored at address `a`.
    pub fn read_str(&self, a: Cell) -> String {
        self.memory[a as usize].iter().take_while(|&&c| c != 0).map(|&c| c as u8 as char).collect()
    }

    fn mem(&mut self, i: usize) -> Option<&mut Vec<Cell>> {
        let a = self.cell(i);
        self.memory.get_mut(usize::try_from(a).ok()?)
    }
}

impl Args for FakeArgs {
    fn count(&self) -> usize {
        self.params.len()
    }

    fn cell(&self, i: usize) -> Cell {
        if i == 0 { 0 } else { self.params.get(i - 1).copied().unwrap_or(0) }
    }

    fn string(&mut self, i: usize) -> String {
        let a = self.cell(i);
        if (a as usize) < self.memory.len() { self.read_str(a) } else { String::new() }
    }

    fn set_string(&mut self, i: usize, s: &str, max: Cell) -> Cell {
        let Some(m) = self.mem(i) else { return 0 };
        // Like the core's set_amxstring: at most `max` characters, then the terminator.
        let n = s.len().min(max.max(0) as usize).min(m.len().saturating_sub(1));
        for (d, b) in m.iter_mut().zip(s.bytes().take(n)) {
            *d = Cell::from(b);
        }
        m[n] = 0;
        n as Cell
    }

    fn read_array(&mut self, i: usize, n: usize) -> Vec<Cell> {
        let mut v = self.mem(i).cloned().unwrap_or_default();
        v.resize(n, 0);
        v
    }

    fn write_array(&mut self, i: usize, start: usize, values: &[Cell]) {
        if let Some(m) = self.mem(i) {
            if m.len() < start + values.len() {
                m.resize(start + values.len(), 0);
            }
            m[start..start + values.len()].copy_from_slice(values);
        }
    }

    fn error(&mut self, msg: &str) {
        self.errors.push(msg.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(a: &mut dyn Args) -> Cell {
        a.cell(1) + a.cell(2)
    }

    natives! {
        static TABLE = [c"add" => add];
    }

    #[test]
    fn table_is_null_terminated_and_dispatches() {
        assert_eq!(TABLE.len(), 2);
        assert!(TABLE[1].name.is_null() && TABLE[1].func.is_none());
        let params = [8, 2, 3]; // 8 bytes of arguments: two cells
        // SAFETY: a valid params array; `add` doesn't touch the AMX.
        let r = unsafe { (TABLE[0].func.unwrap())(core::ptr::null_mut(), params.as_ptr()) };
        assert_eq!(r, 5);
    }

    #[test]
    fn fake_strings_are_truncated_like_the_core() {
        let mut a = FakeArgs::new(&[]);
        let buf = a.alloc(vec![0; 8]);
        a.params = vec![buf, 3];
        assert_eq!(a.set_string(1, "Glock-18", 3), 3);
        assert_eq!(a.read_str(buf), "Glo");
    }
}
