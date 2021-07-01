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

use super::{ignore_poison, try_ignore_poison};
use crate::sys_common::init_assert::InitAssert;
use std::pin::Pin;
use std::sync;

pub struct Mutex {
    mutex: InitAssert<sync::Mutex<()>>,
}

impl Mutex {
    #[inline]
    pub const fn uninit() -> Self {
        Self {
            mutex: InitAssert::new(),
        }
    }

    pub fn init(self: Pin<&Self>) {
        self.mutex.init(|| sync::Mutex::new(()))
    }

    #[inline]
    pub fn try_lock(self: Pin<&Self>) -> Option<MutexGuard> {
        try_ignore_poison(self.get_ref().mutex.get_ref().try_lock())
    }

    #[inline]
    pub fn lock(self: Pin<&Self>) -> MutexGuard {
        ignore_poison(self.get_ref().mutex.get_ref().lock())
    }
}

pub type MutexGuard<'a> = sync::MutexGuard<'a, ()>;
