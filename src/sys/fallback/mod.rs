//! This provides a thin wrapper around the current primitives.
//!
//! For platforms such as Windows which do not need boxing, this will be
//! close to the final result, though once in std code it will be easier
//! to make this fit in a more appropriate way.
//!
//! One problem, however, is that we are including the extra poison flags
//! here, which will be ignored for now, as we re-implement poisoning in a
//! higher level.
//!
//! Extra optimizations which can be made for these platforms are
//! removing the panic on usage of non-initialized primitives in
//! release mode, if the primitives can be constructed in `uninit`.

use std::sync::{LockResult, TryLockError, TryLockResult};

pub mod condvar;
pub mod mutex;
pub mod rwlock;

#[inline]
fn try_ignore_poison<T>(result: TryLockResult<T>) -> Option<T> {
    match result {
        Ok(lock) => Some(lock),
        Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

#[inline]
fn ignore_poison<T>(result: LockResult<T>) -> T {
    match result {
        Ok(lock) => lock,
        Err(error) => error.into_inner(),
    }
}
