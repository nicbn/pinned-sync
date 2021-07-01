use crate::sys;
use crate::sys_common::init_assert::InitAssert;
use std::pin::Pin;
use std::sync;
use std::time::Duration;

use super::ignore_poison;

pub struct Condvar {
    inner: InitAssert<sync::Condvar>,
}

unsafe impl Send for Condvar {}
unsafe impl Sync for Condvar {}

impl Condvar {
    #[inline]
    pub const fn uninit() -> Self {
        Self {
            inner: InitAssert::new(),
        }
    }

    #[inline]
    pub fn init(self: Pin<&Self>) {
        self.inner.init(sync::Condvar::new);
    }

    #[inline]
    pub fn notify_one(self: Pin<&Self>) {
        self.inner.get_ref().notify_one()
    }

    #[inline]
    pub fn notify_all(self: Pin<&Self>) {
        self.inner.get_ref().notify_all()
    }

    #[inline]
    pub unsafe fn wait<'a>(
        self: Pin<&Self>,
        lock: sys::mutex::MutexGuard<'a>,
    ) -> sys::mutex::MutexGuard<'a> {
        ignore_poison(self.inner.get_ref().wait(lock))
    }

    #[inline]
    pub unsafe fn wait_timeout<'a>(
        &self,
        lock: sys::mutex::MutexGuard<'a>,
        dur: Duration,
    ) -> (bool, sys::mutex::MutexGuard<'a>) {
        let (lock, r) = ignore_poison(self.inner.get_ref().wait_timeout(lock, dur));
        (!r.timed_out(), lock)
    }
}
