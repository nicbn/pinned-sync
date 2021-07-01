#![allow(dead_code)]

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::{AtomicIsize, Ordering::*};

const UNINIT: isize = 0;
const INIT_IN_PROGRESS: isize = -1;
const INIT: isize = 1;

pub struct InitAssert<T = ()> {
    state: AtomicIsize,
    data: UnsafeCell<MaybeUninit<T>>,
}
impl<T> InitAssert<T> {
    pub const fn new() -> Self {
        Self {
            state: AtomicIsize::new(UNINIT),
            data: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    #[inline]
    pub fn init<F>(&self, f: F)
    where
        F: FnOnce() -> T,
    {
        unsafe { self.init_with(|p| p.write(f())) }
    }

    #[inline]
    pub unsafe fn init_with<F>(&self, f: F)
    where
        F: FnOnce(*mut T),
    {
        assert_eq!(self.state.swap(INIT_IN_PROGRESS, Acquire), UNINIT);
        f((*self.data.get()).as_mut_ptr());
        self.state.store(INIT, Release);
    }

    #[inline]
    pub fn get_ref(&self) -> &T {
        assert_eq!(self.state.load(Acquire), INIT);
        unsafe { &*self.get() }
    }
    
    #[inline]
    pub fn get(&self) -> *mut T {
        assert_eq!(self.state.load(Acquire), INIT);
        self.data.get() as *mut T
    }
}
impl<T> Drop for InitAssert<T> {
    #[inline]
    fn drop(&mut self) {
        if self.state.load(Acquire) == INIT {
            unsafe { ptr::drop_in_place((*self.data.get()).as_mut_ptr()) };
        }
    }
}
