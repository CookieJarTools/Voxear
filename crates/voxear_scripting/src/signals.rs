//! A signal stack large enough for .NET on each thread that calls C#.
//!
//! On Linux and macOS, .NET turns a null dereference in C# into a
//! `NullReferenceException` from inside its `SIGSEGV` handler, which runs on
//! the thread's alternate signal stack. Rust gives the main thread and each
//! thread it starts an alternate stack of its own, 8 KiB, and .NET uses the
//! one it finds there. That is too small for .NET's handler, which faults
//! again on the stack's guard page, and the process dies where the script
//! should have thrown. Before a thread first calls C#, a stack that small is
//! swapped for a larger one.

/// Gives this thread a signal stack .NET's fault handler fits in, once.
/// Called before every call into C#; after the first on a thread it only
/// reads a thread-local.
pub(crate) fn prepare_thread() {
    #[cfg(unix)]
    unix::prepare();
}

#[cfg(unix)]
mod unix {
    use std::cell::OnceCell;
    use std::ptr;

    /// Room for the kernel's signal frame, .NET's handler and its saved
    /// context, many times over.
    const SIZE: usize = 64 * 1024;

    thread_local! {
        static STACK: OnceCell<Option<SignalStack>> = const { OnceCell::new() };
    }

    pub(super) fn prepare() {
        STACK.with(|stack| {
            stack.get_or_init(SignalStack::install);
        });
    }

    /// A mapping with a guard page below the stack, as Rust's own has.
    struct SignalStack {
        mapping: *mut libc::c_void,
        length: usize,
        top: *mut libc::c_void,
    }

    impl SignalStack {
        /// Replaces this thread's signal stack when it has one smaller than
        /// [`SIZE`]. A thread with none is left alone: .NET puts its own on
        /// a thread without one when the thread first calls C#, and frees
        /// whichever stack is set when the thread leaves, which must not be
        /// this one.
        fn install() -> Option<Self> {
            unsafe {
                let mut current: libc::stack_t = std::mem::zeroed();
                if libc::sigaltstack(ptr::null(), &mut current) != 0 {
                    return None;
                }
                if current.ss_flags & libc::SS_DISABLE != 0 || current.ss_size >= SIZE {
                    return None;
                }
                let page = libc::sysconf(libc::_SC_PAGESIZE).max(4096) as usize;
                let length = page + SIZE;
                let flags = libc::MAP_PRIVATE | libc::MAP_ANON;
                let mapping = libc::mmap(ptr::null_mut(), length, libc::PROT_READ | libc::PROT_WRITE, flags, -1, 0);
                if mapping == libc::MAP_FAILED {
                    return None;
                }
                libc::mprotect(mapping, page, libc::PROT_NONE);
                let top = mapping.cast::<u8>().add(page).cast::<libc::c_void>();
                let mut stack: libc::stack_t = std::mem::zeroed();
                stack.ss_sp = top;
                stack.ss_size = SIZE;
                if libc::sigaltstack(&stack, ptr::null_mut()) != 0 {
                    libc::munmap(mapping, length);
                    return None;
                }
                Some(Self { mapping, length, top })
            }
        }
    }

    impl Drop for SignalStack {
        fn drop(&mut self) {
            unsafe {
                let mut current: libc::stack_t = std::mem::zeroed();
                libc::sigaltstack(ptr::null(), &mut current);
                if current.ss_flags & libc::SS_DISABLE == 0 && current.ss_sp == self.top {
                    let mut off: libc::stack_t = std::mem::zeroed();
                    off.ss_flags = libc::SS_DISABLE;
                    off.ss_size = SIZE;
                    libc::sigaltstack(&off, ptr::null_mut());
                }
                libc::munmap(self.mapping, self.length);
            }
        }
    }
}
