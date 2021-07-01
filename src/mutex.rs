use crate::sys::mutex as sys;
use crate::sys_common::poison;
use std::cell::UnsafeCell;
use std::marker::PhantomPinned;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::ptr;
use std::sync::Arc;
use std::sync::LockResult;
use std::sync::PoisonError;
use std::sync::TryLockError;
use std::sync::TryLockResult;

/// A mutual exclusion primitive useful for protecting shared data
///
/// This mutex will block threads waiting for the lock to become available. The
/// mutex can also be statically initialized or created via a [`new`]
/// constructor. Each mutex has a type parameter which represents the data that
/// it is protecting. The data can only be accessed through the RAII guards
/// returned from [`lock`] and [`try_lock`], which guarantees that the data is only
/// ever accessed when the mutex is locked.
///
/// # Poisoning
///
/// The mutexes in this module implement a strategy called "poisoning" where a
/// mutex is considered poisoned whenever a thread panics while holding the
/// mutex. Once a mutex is poisoned, all other threads are unable to access the
/// data by default as it is likely tainted (some invariant is not being
/// upheld).
///
/// For a mutex, this means that the [`lock`] and [`try_lock`] methods return a
/// [`Result`] which indicates whether a mutex has been poisoned or not. Most
/// usage of a mutex will simply [`unwrap()`] these results, propagating panics
/// among threads to ensure that a possibly invalid invariant is not witnessed.
///
/// A poisoned mutex, however, does not prevent all access to the underlying
/// data. The [`PoisonError`] type has an [`into_inner`] method which will return
/// the guard that would have otherwise been returned on a successful lock. This
/// allows access to the data, despite the lock being poisoned.
///
/// [`new`]: Self::new
/// [`lock`]: Self::lock
/// [`try_lock`]: Self::try_lock
/// [`unwrap()`]: Result::unwrap
/// [`PoisonError`]: super::PoisonError
/// [`into_inner`]: super::PoisonError::into_inner
pub struct Mutex<T: ?Sized> {
    inner: sys::Mutex,
    poison: poison::Flag,
    _p: PhantomPinned,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}

unsafe impl<T: ?Sized + Send + Sync> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    /// Create a new, uninitialized mutex.
    ///
    /// This is *NOT* equivalent to `MaybeUninit::uninit().assume_init()`, which will cause
    /// undefined behaviour if used to create a new mutex.
    #[inline]
    pub const fn uninit(value: T) -> Self {
        Self {
            inner: sys::Mutex::uninit(),
            _p: PhantomPinned,
            poison: poison::Flag::new(),
            data: UnsafeCell::new(value),
        }
    }

    /// Create a new, initialized mutex.
    ///
    /// The resulting mutex is wrapped and ready for use.
    #[inline]
    pub fn boxed(value: T) -> Pin<Box<Self>> {
        let this = Box::pin(Self::uninit(value));
        this.as_ref().init();
        this
    }
    
    /// Create a new, initialized mutex.
    ///
    /// The resulting mutex is wrapped and ready for use.
    #[inline]
    pub fn arc(value: T) -> Pin<Arc<Self>> {
        let this = Arc::pin(Self::uninit(value));
        this.as_ref().init();
        this
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Initialize a mutex, making it ready for use.
    ///
    /// # Panics
    ///
    /// This function may panic if the mutex was already initialized.
    #[inline]
    pub fn init(self: Pin<&Self>) {
        self.inner().init()
    }

    /// Acquires a mutex, blocking the current thread until it is able to do so.
    ///
    /// This function will block the local thread until it is available to acquire
    /// the mutex. Upon returning, the thread is the only thread with the lock
    /// held. An RAII guard is returned to allow scoped unlock of the lock. When
    /// the guard goes out of scope, the mutex will be unlocked.
    ///
    /// The exact behavior on locking a mutex in the thread which already holds
    /// the lock is left unspecified. However, this function will not return on
    /// the second call (it might panic or deadlock, for example).
    ///
    /// # Errors
    ///
    /// If another user of this mutex panicked while holding the mutex, then
    /// this call will return an error once the mutex is acquired.
    ///
    /// # Panics
    ///
    /// This function might panic when called if the lock is already held by the
    /// current thread.
    ///
    /// This function may panic if the mutex is not initialized.
    #[inline]
    pub fn lock(self: Pin<&Self>) -> LockResult<MutexGuard<T>> {
        let guard = self.inner().lock();
        poison::map_result(self.poison.borrow(), |poison| MutexGuard {
            guard,
            mutex: self,
            poison,
        })
    }

    /// Attempts to acquire this lock.
    ///
    /// If the lock could not be acquired at this time, then [`Err`] is returned.
    /// Otherwise, an RAII guard is returned. The lock will be unlocked when the
    /// guard is dropped.
    ///
    /// This function does not block.
    ///
    /// # Errors
    ///
    /// If another user of this mutex panicked while holding the mutex, then
    /// this call will return an error if the mutex would otherwise be
    /// acquired.
    ///
    /// # Panics
    ///
    /// This function may panic if the mutex is not initialized.
    #[inline]
    pub fn try_lock(self: Pin<&Self>) -> TryLockResult<MutexGuard<T>> {
        let guard = self.inner().try_lock().ok_or(TryLockError::WouldBlock)?;
        Ok(poison::map_result(self.poison.borrow(), |poison| {
            MutexGuard {
                guard,
                mutex: self,
                poison,
            }
        })?)
    }

    /// Determines whether the mutex is poisoned.
    ///
    /// If another thread is active, the mutex can still become poisoned at any
    /// time. You should not trust a `false` value for program correctness
    /// without additional synchronization.
    #[inline]
    pub fn is_poisoned(self: Pin<&Self>) -> bool {
        self.poison.get()
    }

    /// Consumes this mutex, returning the underlying data.
    ///
    /// # Errors
    ///
    /// If another user of this mutex panicked while holding the mutex, then
    /// this call will return an error instead.
    pub fn into_inner(self) -> LockResult<T>
    where
        T: Sized,
    {
        let Self { data, poison, .. } = self;
        poison::map_result(poison.borrow(), |_| data.into_inner())
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the `Mutex` mutably, no actual locking needs to
    /// take place -- the mutable borrow statically guarantees no locks exist.
    ///
    /// # Errors
    ///
    /// If another user of this mutex panicked while holding the mutex, then
    /// this call will return an error instead.
    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        let data = self.data.get_mut();
        poison::map_result(self.poison.borrow(), |_| data)
    }

    #[inline]
    fn inner(self: Pin<&Self>) -> Pin<&sys::Mutex> {
        unsafe { self.map_unchecked(|this| &this.inner) }
    }
}

pub struct MutexGuard<'a, T: ?Sized> {
    // This is suboptimal but necessary for `fallback` as `sync::Mutex` does not provide raw
    // unlocking.
    guard: sys::MutexGuard<'a>,
    mutex: Pin<&'a Mutex<T>>,
    poison: poison::Guard,
}

unsafe impl<T: ?Sized + Sync> Sync for MutexGuard<'_, T> {}

impl<'a, T: ?Sized> MutexGuard<'a, T> {
    #[inline]
    pub(crate) fn map(self, f: impl FnOnce(sys::MutexGuard<'a>) -> sys::MutexGuard<'a>) -> LockResult<Self> {
        let (guard, mutex, poison) = unsafe {
            let guard = ptr::read(&self.guard);
            let mutex = ptr::read(&self.mutex);
            let poison = ptr::read(&self.poison);
            mem::forget(self);
            (guard, mutex, poison)
        };

        let guard = f(guard);

        Self {
            guard,
            mutex,
            poison,
        }.repoison()
    }

    #[inline]
    fn repoison(self) -> LockResult<Self> {
        if self.mutex.is_poisoned() {
            Err(PoisonError::new(self))
        } else {
            Ok(self)
        }
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        self.mutex.poison.done(&self.poison);
    }
}
