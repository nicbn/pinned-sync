use crate::sys::rwlock as sys;
use crate::sys_common::poison;
use std::cell::UnsafeCell;
use std::marker::PhantomPinned;
use std::ops::Deref;
use std::ops::DerefMut;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::LockResult;
use std::sync::TryLockError;
use std::sync::TryLockResult;

/// A reader-writer lock
///
/// This type of lock allows a number of readers or at most one writer at any
/// point in time. The write portion of this lock typically allows modification
/// of the underlying data (exclusive access) and the read portion of this lock
/// typically allows for read-only access (shared access).
///
/// In comparison, a [`Mutex`] does not distinguish between readers or writers
/// that acquire the lock, therefore blocking any threads waiting for the lock to
/// become available. An `RwLock` will allow any number of readers to acquire the
/// lock as long as a writer is not holding the lock.
///
/// The priority policy of the lock is dependent on the underlying operating
/// system's implementation, and this type does not guarantee that any
/// particular policy will be used.
///
/// The type parameter `T` represents the data that this lock protects. It is
/// required that `T` satisfies [`Send`] to be shared across threads and
/// [`Sync`] to allow concurrent access through readers. The RAII guards
/// returned from the locking methods implement [`Deref`] (and [`DerefMut`]
/// for the `write` methods) to allow access to the content of the lock.
///
/// # Poisoning
///
/// An `RwLock`, like [`Mutex`], will become poisoned on a panic. Note, however,
/// that an `RwLock` may only be poisoned if a panic occurs while it is locked
/// exclusively (write mode). If a panic occurs in any reader, then the lock
/// will not be poisoned.
pub struct RwLock<T: ?Sized> {
    inner: sys::RwLock,
    poison: poison::Flag,
    _p: PhantomPinned,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}

unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    /// Create a new, uninitialized read-write lock.
    ///
    /// This is *NOT* equivalent to `MaybeUninit::uninit().assume_init()`, which will cause
    /// undefined behaviour if used to create a new read-write lock.
    #[inline]
    pub const fn uninit(value: T) -> Self {
        Self {
            inner: sys::RwLock::uninit(),
            _p: PhantomPinned,
            poison: poison::Flag::new(),
            data: UnsafeCell::new(value),
        }
    }

    /// Create a new, initialized read-write lock.
    ///
    /// The resulting read-write lock is wrapped and ready for use.
    pub fn boxed(value: T) -> Pin<Box<Self>> {
        let this = Box::pin(Self::uninit(value));
        this.as_ref().init();
        this
    }

    /// Create a new, initialized read-write lock.
    ///
    /// The resulting read-write lock is wrapped and ready for use.
    pub fn arc(value: T) -> Pin<Arc<Self>> {
        let this = Arc::pin(Self::uninit(value));
        this.as_ref().init();
        this
    }
}

impl<T: ?Sized> RwLock<T> {
    /// Initialize a read-write lock, making it ready for use.
    ///
    /// # Panics
    ///
    /// This function may panic if the read-write lock was already initialized.
    #[inline]
    pub fn init(self: Pin<&Self>) {
        self.inner().init()
    }

    /// Locks this rwlock with shared read access, blocking the current thread
    /// until it can be acquired.
    ///
    /// The calling thread will be blocked until there are no more writers which
    /// hold the lock. There may be other readers currently inside the lock when
    /// this method returns. This method does not provide any guarantees with
    /// respect to the ordering of whether contentious readers or writers will
    /// acquire the lock first.
    ///
    /// Returns an RAII guard which will release this thread's shared access
    /// once it is dropped.
    ///
    /// # Errors
    ///
    /// This function will return an error if the RwLock is poisoned. An RwLock
    /// is poisoned whenever a writer panics while holding an exclusive lock.
    /// The failure will occur immediately after the lock has been acquired.
    ///
    /// # Panics
    ///
    /// This function might panic when called if the lock is already held by the current thread.
    ///
    /// This function may panic if the lock is not initialized.
    #[inline]
    pub fn read(self: Pin<&Self>) -> LockResult<RwLockReadGuard<T>> {
        let guard = self.inner().read();
        poison::map_result(self.poison.borrow(), |_| RwLockReadGuard {
            _guard: guard,
            lock: self,
        })
    }

    /// Attempts to acquire this rwlock with shared read access.
    ///
    /// If the access could not be granted at this time, then `Err` is returned.
    /// Otherwise, an RAII guard is returned which will release the shared access
    /// when it is dropped.
    ///
    /// This function does not block.
    ///
    /// This function does not provide any guarantees with respect to the ordering
    /// of whether contentious readers or writers will acquire the lock first.
    ///
    /// # Errors
    ///
    /// This function will return an error if the RwLock is poisoned. An RwLock
    /// is poisoned whenever a writer panics while holding an exclusive lock. An
    /// error will only be returned if the lock would have otherwise been
    /// acquired.
    ///
    /// # Panics
    ///
    /// This function may panic if the lock is not initialized.
    #[inline]
    pub fn try_read(self: Pin<&Self>) -> TryLockResult<RwLockReadGuard<T>> {
        let guard = self.inner().try_read().ok_or(TryLockError::WouldBlock)?;
        Ok(poison::map_result(self.poison.borrow(), |_| {
            RwLockReadGuard {
                _guard: guard,
                lock: self,
            }
        })?)
    }

    /// Locks this rwlock with exclusive write access, blocking the current
    /// thread until it can be acquired.
    ///
    /// This function will not return while other writers or other readers
    /// currently have access to the lock.
    ///
    /// Returns an RAII guard which will drop the write access of this rwlock
    /// when dropped.
    ///
    /// # Errors
    ///
    /// This function will return an error if the RwLock is poisoned. An RwLock
    /// is poisoned whenever a writer panics while holding an exclusive lock.
    /// An error will be returned when the lock is acquired.
    ///
    /// # Panics
    ///
    /// This function might panic when called if the lock is already held by the current thread.
    ///
    /// This function may panic if the lock is not initialized.
    #[inline]
    pub fn write(self: Pin<&Self>) -> LockResult<RwLockWriteGuard<T>> {
        let guard = self.inner().write();
        poison::map_result(self.poison.borrow(), |poison| RwLockWriteGuard {
            _guard: guard,
            lock: self,
            poison,
        })
    }

    /// Attempts to lock this rwlock with exclusive write access.
    ///
    /// If the lock could not be acquired at this time, then `Err` is returned.
    /// Otherwise, an RAII guard is returned which will release the lock when
    /// it is dropped.
    ///
    /// This function does not block.
    ///
    /// This function does not provide any guarantees with respect to the ordering
    /// of whether contentious readers or writers will acquire the lock first.
    ///
    /// # Errors
    ///
    /// This function will return an error if the RwLock is poisoned. An RwLock
    /// is poisoned whenever a writer panics while holding an exclusive lock. An
    /// error will only be returned if the lock would have otherwise been
    /// acquired.
    ///
    /// # Panics
    ///
    /// This function may panic if the lock is not initialized.
    #[inline]
    pub fn try_write(self: Pin<&Self>) -> TryLockResult<RwLockWriteGuard<T>> {
        let guard = self.inner().try_write().ok_or(TryLockError::WouldBlock)?;
        Ok(poison::map_result(self.poison.borrow(), |poison| {
            RwLockWriteGuard {
                _guard: guard,
                lock: self,
                poison,
            }
        })?)
    }

    /// Determines whether the read-write lock is poisoned.
    ///
    /// If another thread is active, the read-write lock can still become poisoned at any
    /// time. You should not trust a `false` value for program correctness
    /// without additional synchronization.
    #[inline]
    pub fn is_poisoned(self: Pin<&Self>) -> bool {
        self.poison.get()
    }

    /// Consumes this read-write lock, returning the underlying data.
    ///
    /// # Errors
    ///
    /// If another user of this read-write lock panicked while holding the
    /// read-write lock, then this call will return an error instead.
    pub fn into_inner(self) -> LockResult<T>
    where
        T: Sized,
    {
        let Self { data, poison, .. } = self;
        poison::map_result(poison.borrow(), |_| data.into_inner())
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the `RwLock` mutably, no actual locking needs to
    /// take place -- the mutable borrow statically guarantees no locks exist.
    ///
    /// # Errors
    ///
    /// If another user of this read-write lock panicked while holding the read-write lock, then
    /// this call will return an error instead.
    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        let data = self.data.get_mut();
        poison::map_result(self.poison.borrow(), |_| data)
    }

    #[inline]
    fn inner(self: Pin<&Self>) -> Pin<&sys::RwLock> {
        unsafe { self.map_unchecked(|this| &this.inner) }
    }
}

pub struct RwLockReadGuard<'a, T: ?Sized> {
    // This is suboptimal but necessary for `fallback` as `sync::Mutex` does not provide raw
    // unlocking.
    _guard: sys::ReadGuard<'a>,
    lock: Pin<&'a RwLock<T>>,
}

unsafe impl<T: ?Sized + Sync> Sync for RwLockReadGuard<'_, T> {}

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

pub struct RwLockWriteGuard<'a, T: ?Sized> {
    // This is suboptimal but necessary for `fallback` as `sync::Mutex` does not provide raw
    // unlocking.
    _guard: sys::WriteGuard<'a>,
    lock: Pin<&'a RwLock<T>>,
    poison: poison::Guard,
}

unsafe impl<T: ?Sized + Sync> Sync for RwLockWriteGuard<'_, T> {}

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        self.lock.poison.done(&self.poison);
    }
}
