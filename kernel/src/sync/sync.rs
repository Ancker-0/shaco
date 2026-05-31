// HUMAN
#![no_std]
use core::sync::atomic::{AtomicBool, Ordering};
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};

pub type SpinLock<T> = SpinMutex<T>;
pub type SpinNoIrqLock<T> = SpinMutex<T>;
pub type SleepLock<T> = SpinMutex<T>;
pub type MutexGuard<'a, T> = SpinMutexGuard<'a, T>;

pub struct SpinMutex<T> {
    lock: AtomicBool,
    value: UnsafeCell<T>,
}

pub struct SpinMutexGuard<'a, T> {
    mutex: &'a SpinMutex<T>,
}

unsafe impl<T: Send> Send for SpinMutex<T> {}
unsafe impl<T: Send> Sync for SpinMutex<T> {}

impl<T> SpinMutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> SpinMutexGuard<'_, T> {
        while self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            // core::hint::spin_loop();
            core::sync::atomic::spin_loop_hint()
        }
        SpinMutexGuard { mutex: self }
    }

    fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }

    pub fn try_lock(&self) -> Option<SpinMutexGuard<'_, T>> {
        match (self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)) {
            Ok(_) => Some(SpinMutexGuard { mutex: self }),
            Err(_) => None,
        }
    }

    pub fn is_lock(&self) -> bool {
        self.lock.load(Ordering::Relaxed)
    }
}

impl<T> Drop for SpinMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

impl<T> Deref for SpinMutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for SpinMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.value.get() }
    }
}