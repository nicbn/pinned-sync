use crate::sys_common::init_assert::InitAssert;

use super::{ignore_poison, try_ignore_poison};
use std::pin::Pin;
use std::sync;

pub struct RwLock {
    rw_lock: InitAssert<sync::RwLock<()>>,
}

impl RwLock {
    #[inline]
    pub const fn uninit() -> Self {
        Self {
            rw_lock: InitAssert::new(),
        }
    }

    pub fn init(self: Pin<&Self>) {
        self.rw_lock.init(|| sync::RwLock::new(()))
    }

    #[inline]
    pub fn try_read(self: Pin<&Self>) -> Option<ReadGuard> {
        try_ignore_poison(self.get_ref().rw_lock.get_ref().try_read())
    }

    #[inline]
    pub fn read(self: Pin<&Self>) -> ReadGuard {
        ignore_poison(self.get_ref().rw_lock.get_ref().read())
    }

    #[inline]
    pub fn try_write(self: Pin<&Self>) -> Option<WriteGuard> {
        try_ignore_poison(self.get_ref().rw_lock.get_ref().try_write())
    }

    #[inline]
    pub fn write(self: Pin<&Self>) -> WriteGuard {
        ignore_poison(self.get_ref().rw_lock.get_ref().write())
    }
}

pub type ReadGuard<'a> = sync::RwLockReadGuard<'a, ()>;
pub type WriteGuard<'a> = sync::RwLockWriteGuard<'a, ()>;
