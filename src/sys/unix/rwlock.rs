use std::cell::UnsafeCell;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering::*};

use crate::sys_common::init_assert::InitAssert;

pub struct RwLock {
    lock: UnsafeCell<libc::pthread_rwlock_t>,
    write_locked: UnsafeCell<bool>,
    num_readers: AtomicUsize,
    #[cfg(debug_assertions)]
    initialized: InitAssert,
    _p: PhantomPinned,
}

unsafe impl Send for RwLock {}
unsafe impl Sync for RwLock {}

impl RwLock {
    #[inline]
    pub const fn uninit() -> Self {
        Self {
            lock: UnsafeCell::new(libc::PTHREAD_RWLOCK_INITIALIZER),
            write_locked: UnsafeCell::new(false),
            num_readers: AtomicUsize::new(0),
            #[cfg(debug_assertions)]
            initialized: InitAssert::new(),
            _p: PhantomPinned,
        }
    }

    #[inline]
    pub fn init(self: Pin<&Self>) {
        #[cfg(debug_assertions)]
        self.initialized.init(|| {});
    }

    #[inline]
    pub fn try_read(self: Pin<&Self>) -> Option<ReadGuard> {
        #[cfg(debug_assertions)]
        {
            self.initialized.get();
        }

        unsafe {
            let r = libc::pthread_rwlock_tryrdlock(self.lock.get());
            if r == 0 {
                if *self.write_locked.get() {
                    self.unlock();
                    None
                } else {
                    self.num_readers.fetch_add(1, Relaxed);
                    Some(ReadGuard { lock: self })
                }
            } else {
                None
            }
        }
    }

    #[inline]
    pub fn read(self: Pin<&Self>) -> ReadGuard {
        #[cfg(debug_assertions)]
        {
            self.initialized.get();
        }

        unsafe {
            let r = libc::pthread_rwlock_rdlock(self.lock.get());

            // According to POSIX, when a thread tries to acquire this read lock
            // while it already holds the write lock
            // (or vice versa, or tries to acquire the write lock twice),
            // "the call shall either deadlock or return [EDEADLK]"
            // (https://pubs.opengroup.org/onlinepubs/9699919799/functions/pthread_rwlock_wrlock.html,
            // https://pubs.opengroup.org/onlinepubs/9699919799/functions/pthread_rwlock_rdlock.html).
            // So, in principle, all we have to do here is check `r == 0` to be sure we properly
            // got the lock.
            //
            // However, (at least) glibc before version 2.25 does not conform to this spec,
            // and can return `r == 0` even when this thread already holds the write lock.
            // We thus check for this situation ourselves and panic when detecting that a thread
            // got the write lock more than once, or got a read and a write lock.
            if r == libc::EAGAIN {
                panic!("rwlock maximum reader count exceeded");
            } else if r == libc::EDEADLK || (r == 0 && *self.write_locked.get()) {
                // Above, we make sure to only access `write_locked` when `r == 0` to avoid
                // data races.
                if r == 0 {
                    // `pthread_rwlock_rdlock` succeeded when it should not have.
                    self.unlock();
                }
                panic!("rwlock read lock would result in deadlock");
            } else {
                // According to POSIX, for a properly initialized rwlock this can only
                // return EAGAIN or EDEADLK or 0. We rely on that.
                debug_assert_eq!(r, 0);
                self.num_readers.fetch_add(1, Relaxed);
                ReadGuard { lock: self }
            }
        }
    }

    #[inline]
    pub fn try_write(self: Pin<&Self>) -> Option<WriteGuard> {
        #[cfg(debug_assertions)]
        {
            self.initialized.get();
        }

        unsafe {
            let r = libc::pthread_rwlock_trywrlock(self.lock.get());
            if r == 0 {
                if *self.write_locked.get() || self.num_readers.load(Relaxed) != 0 {
                    // `pthread_rwlock_trywrlock` succeeded when it should not have.
                    self.unlock();
                    None
                } else {
                    *self.write_locked.get() = true;
                    Some(WriteGuard { lock: self })
                }
            } else {
                None
            }
        }
    }

    #[inline]
    pub fn write(self: Pin<&Self>) -> WriteGuard {
        #[cfg(debug_assertions)]
        {
            self.initialized.get();
        }

        unsafe {
            let r = libc::pthread_rwlock_wrlock(self.lock.get());
            // See comments above for why we check for EDEADLK and write_locked. For the same reason,
            // we also need to check that there are no readers (tracked in `num_readers`).
            if r == libc::EDEADLK
                || (r == 0 && *self.write_locked.get())
                || self.num_readers.load(Relaxed) != 0
            {
                // Above, we make sure to only access `write_locked` when `r == 0` to avoid
                // data races.
                if r == 0 {
                    // `pthread_rwlock_wrlock` succeeded when it should not have.
                    self.unlock();
                }
                panic!("rwlock write lock would result in deadlock");
            } else {
                // According to POSIX, for a properly initialized rwlock this can only
                // return EDEADLK or 0. We rely on that.
                debug_assert_eq!(r, 0);
            }
            *self.write_locked.get() = true;

            WriteGuard { lock: self }
        }
    }

    #[inline]
    unsafe fn unlock(self: Pin<&Self>) {
        let result = libc::pthread_rwlock_unlock(self.lock.get());
        debug_assert_eq!(result, 0);
    }
}

pub struct ReadGuard<'a> {
    lock: Pin<&'a RwLock>,
}
impl Drop for ReadGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            debug_assert!(!*self.lock.write_locked.get());
            self.lock.num_readers.fetch_sub(1, Relaxed);
            self.lock.unlock();
        }
    }
}

pub struct WriteGuard<'a> {
    lock: Pin<&'a RwLock>,
}
impl Drop for WriteGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            debug_assert_eq!(self.lock.num_readers.load(Relaxed), 0);
            debug_assert!(*self.lock.write_locked.get());
            *self.lock.write_locked.get() = false;
            self.lock.unlock();
        }
    }
}
