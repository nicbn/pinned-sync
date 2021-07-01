use crate::sys::cvt_nz;
use crate::sys_common::init_assert::InitAssert;
use std::marker::PhantomPinned;
use std::mem::MaybeUninit;
use std::pin::Pin;

pub struct Mutex {
    lock: InitAssert<libc::pthread_mutex_t>,
    _p: PhantomPinned,
}

unsafe impl Send for Mutex {}
unsafe impl Sync for Mutex {}

impl Mutex {
    #[inline]
    pub const fn uninit() -> Self {
        Self {
            lock: InitAssert::new(),
            _p: PhantomPinned,
        }
    }

    pub fn init(self: Pin<&Self>) {
        unsafe {
            self.lock.init_with(|p| {
                let mut attr = MaybeUninit::<libc::pthread_mutexattr_t>::uninit();

                cvt_nz(libc::pthread_mutexattr_init(attr.as_mut_ptr())).unwrap();
                let attr = PthreadMutexAttr(&mut attr);
                cvt_nz(libc::pthread_mutexattr_settype(
                    attr.0.as_mut_ptr(),
                    libc::PTHREAD_MUTEX_NORMAL,
                ))
                .unwrap();
                cvt_nz(libc::pthread_mutex_init(p, attr.0.as_ptr())).unwrap();
            });
        }
    }

    #[inline]
    pub fn lock(self: Pin<&Self>) -> MutexGuard {
        Self::lock_inner(self.lock.get());
        MutexGuard { mutex: self }
    }

    #[inline]
    pub fn try_lock(self: Pin<&Self>) -> Option<MutexGuard> {
        unsafe {
            let result = libc::pthread_mutex_lock(self.lock.get());
            if result == 0 {
                Some(MutexGuard { mutex: self })
            } else {
                None
            }
        }
    }

    fn lock_inner(x: *mut libc::pthread_mutex_t) {
        unsafe {
            let result = libc::pthread_mutex_lock(x);
            debug_assert_eq!(result, 0);
        }
    }
}

pub struct MutexGuard<'a> {
    mutex: Pin<&'a Mutex>,
}
impl MutexGuard<'_> {
    #[inline]
    pub fn as_raw(&self) -> *mut libc::pthread_mutex_t {
        self.mutex.lock.get()
    }
}
impl Drop for MutexGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            let result = libc::pthread_mutex_unlock(self.as_raw());
            debug_assert_eq!(result, 0);
        }
    }
}

struct PthreadMutexAttr<'a>(&'a mut MaybeUninit<libc::pthread_mutexattr_t>);

impl Drop for PthreadMutexAttr<'_> {
    fn drop(&mut self) {
        unsafe {
            let result = libc::pthread_mutexattr_destroy(self.0.as_mut_ptr());
            debug_assert_eq!(result, 0);
        }
    }
}
