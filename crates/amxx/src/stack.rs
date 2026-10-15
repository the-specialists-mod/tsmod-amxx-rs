// tsmod-amxx-rs, GPL-3.0-or-later with the AMX Mod X exceptions.

//! Stack realignment for calls from AMX Mod X into the modules (#17).
//!
//! The i386 System V ABI only promises a 4-byte aligned stack, and AMX Mod X 1.8.2's Linux core
//! calls natives with `esp` = 8 mod 16. Rust's `i686-unknown-linux-gnu` code (ours and the Rust
//! TS server's, which the natives call through the plugin API) assumes 16 bytes and spills SSE
//! registers with `movaps`, which faults on such a stack. rustc has no `-mstackrealign`, so the
//! entry points run their body through [`aligned`], which realigns `esp` first. Windows needs
//! nothing: the i686 Windows target assumes 4 bytes and realigns where it must.

/// Runs `f` on a 16-byte aligned stack (on i686 Linux; elsewhere just calls it).
#[inline]
pub fn aligned<R>(f: impl FnOnce() -> R) -> R {
    #[cfg(all(target_arch = "x86", not(windows)))]
    {
        imp::aligned(f)
    }
    #[cfg(not(all(target_arch = "x86", not(windows))))]
    {
        f()
    }
}

#[cfg(all(target_arch = "x86", not(windows)))]
mod imp {
    use core::ffi::c_void;

    /// Calls `f(ctx)` with `esp` realigned to 16 bytes at the call (cdecl; `ebp` and `esp`
    /// restored, `ebx`/`esi`/`edi` untouched).
    #[unsafe(naked)]
    unsafe extern "C" fn call_aligned(f: extern "C" fn(*mut c_void), ctx: *mut c_void) {
        core::arch::naked_asm!(
            "push ebp",
            "mov ebp, esp",
            "and esp, -16",
            // 12 bytes of padding and the 4-byte argument keep `esp` 16-aligned at the call.
            "sub esp, 12",
            "push dword ptr [ebp + 12]",
            "call dword ptr [ebp + 8]",
            "mov esp, ebp",
            "pop ebp",
            "ret",
        )
    }

    struct Ctx<F, R> {
        f: Option<F>,
        out: Option<R>,
    }

    extern "C" fn run<F: FnOnce() -> R, R>(p: *mut c_void) {
        // SAFETY: `p` is the `Ctx<F, R>` `aligned` passes, live and unaliased for this call.
        let ctx = unsafe { &mut *p.cast::<Ctx<F, R>>() };
        if let Some(f) = ctx.f.take() {
            ctx.out = Some(f());
        }
    }

    pub fn aligned<F: FnOnce() -> R, R>(f: F) -> R {
        let mut ctx = Ctx { f: Some(f), out: None };
        // SAFETY: `run::<F, R>` matches `call_aligned`'s callback type and takes the context it
        // gets; `ctx` outlives the call.
        unsafe { call_aligned(run::<F, R>, (&raw mut ctx).cast()) };
        // `run` always fills it (a panic aborts: the modules are built with panic = abort, and
        // unwinding out of an `extern "C"` function aborts anyway).
        ctx.out.expect("aligned: the callback didn't run")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn runs_the_closure_and_returns_its_value() {
        let mut seen = 0;
        let r = super::aligned(|| {
            seen += 1;
            41 + seen
        });
        assert_eq!((r, seen), (42, 1));
        // A stack-allocated 16-byte aligned value works inside (the point of the realignment).
        #[repr(align(16))]
        struct A([u8; 16]);
        let a = super::aligned(|| A([7; 16]));
        assert_eq!(a.0[15], 7);
    }
}
