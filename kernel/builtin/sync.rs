//! Synchronisation primitives: a spin-lock and a single-init [`Once`] cell.
//!
//! Both are correct by hand review, not Kani — their purpose is cross-CPU
//! concurrency, which single-threaded model checking cannot exercise (and on an
//! aarch64 host `spin_loop()` lowers to an intrinsic Kani rejects).

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ops::Drop;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// A spinning mutual-exclusion lock. [`lock`](SpinLock::lock) busy-waits with
/// acquire/release ordering; the returned guard releases on drop.
pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: core::marker::Send> core::marker::Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        while self.locked.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        SpinLockGuard { lock: self }
    }
}

/// RAII guard for a held [`SpinLock`]. Derefs to the protected data and
/// releases the lock when dropped.
pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<'a, T> core::ops::Deref for SpinLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> core::ops::DerefMut for SpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for SpinLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

const UNINIT: u8 = 0;
const RUNNING: u8 = 1;
const READY: u8 = 2;

/// Single-initialisation cell. `call_once` runs the initialiser exactly once
/// across all CPUs; everyone else observes the stored value.
pub struct Once<T> {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<T>>,
}

// A shared `&T` crosses CPUs → T: Sync; the value is moved in → T: Send.
unsafe impl<T: Send + Sync> Sync for Once<T> {}
unsafe impl<T: Send> Send for Once<T> {}

impl<T> Once<T> {
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(UNINIT),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    pub fn call_once(&self, f: impl FnOnce() -> T) -> &T {
        if self
            .state
            .compare_exchange(UNINIT, RUNNING, Ordering::Acquire, Ordering::Acquire)
            .is_ok()
        {
            let v = f();
            unsafe { (*self.value.get()).write(v) };
            self.state.store(READY, Ordering::Release); // publishes the write
        } else {
            while self.state.load(Ordering::Acquire) != READY {
                core::hint::spin_loop();
            }
        }
        // Safe: state is READY and the value-write happened-before (Acquire/Release).
        unsafe { (*self.value.get()).assume_init_ref() }
    }

    pub fn get(&self) -> Option<&T> {
        if self.state.load(Ordering::Acquire) == READY {
            Some(unsafe { (*self.value.get()).assume_init_ref() })
        } else {
            None
        }
    }
}
