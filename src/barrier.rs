use crate::{Condvar, Mutex};
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;

/// A barrier enables multiple threads to synchronize the beginning
/// of some computation.
pub struct Barrier {
    lock: Mutex<BarrierState>,
    cvar: Condvar,
    num_threads: usize,
}

// The inner state of a double barrier
struct BarrierState {
    count: usize,
    generation_id: usize,
}

/// A `BarrierWaitResult` is returned by [`Barrier::wait()`] when all threads
/// in the [`Barrier`] have rendezvoused.
pub struct BarrierWaitResult(bool);

impl fmt::Debug for Barrier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Barrier { .. }")
    }
}

impl Barrier {
    /// Creates an uninitialized barrier that can block a given number of threads.
    ///
    /// A barrier will block `n`-1 threads which call [`wait()`] and then wake
    /// up all threads at once when the `n`th thread calls [`wait()`].
    ///
    /// [`wait()`]: Barrier::wait
    pub const fn uninit(n: usize) -> Barrier {
        Barrier {
            lock: Mutex::uninit(BarrierState {
                count: 0,
                generation_id: 0,
            }),
            cvar: Condvar::uninit(),
            num_threads: n,
        }
    }

    /// Create a new, initialized `Barrier`.
    ///
    /// The resulting mutex is wrapped and ready for use.
    #[inline]
    pub fn boxed(n: usize) -> Pin<Box<Self>> {
        let this = Box::pin(Self::uninit(n));
        this.as_ref().init();
        this
    }
    
    /// Create a new, initialized `Barrier`.
    ///
    /// The resulting mutex is wrapped and ready for use.
    #[inline]
    pub fn arc(n: usize) -> Pin<Arc<Self>> {
        let this = Arc::pin(Self::uninit(n));
        this.as_ref().init();
        this
    }

    /// Initializes the barrier.
    #[inline]
    pub fn init(self: Pin<&Self>) {
        self.lock().init();
        self.cvar().init();
    }

    /// Blocks the current thread until all threads have rendezvoused here.
    ///
    /// Barriers are re-usable after all threads have rendezvoused once, and can
    /// be used continuously.
    ///
    /// A single (arbitrary) thread will receive a [`BarrierWaitResult`] that
    /// returns `true` from [`BarrierWaitResult::is_leader()`] when returning
    /// from this function, and all other threads will receive a result that
    /// will return `false` from [`BarrierWaitResult::is_leader()`].
    pub fn wait(self: Pin<&Self>) -> BarrierWaitResult {
        let mut lock = self.lock().lock().unwrap();
        let local_gen = lock.generation_id;
        lock.count += 1;
        if lock.count < self.num_threads {
            // We need a while loop to guard against spurious wakeups.
            // https://en.wikipedia.org/wiki/Spurious_wakeup
            while local_gen == lock.generation_id && lock.count < self.num_threads {
                lock = self.cvar().wait(lock).unwrap();
            }
            BarrierWaitResult(false)
        } else {
            lock.count = 0;
            lock.generation_id = lock.generation_id.wrapping_add(1);
            self.cvar().notify_all();
            BarrierWaitResult(true)
        }
    }

    #[inline]
    fn lock(self: Pin<&Self>) -> Pin<&Mutex<BarrierState>> {
        unsafe { self.map_unchecked(|this| &this.lock) }
    }

    #[inline]
    fn cvar(self: Pin<&Self>) -> Pin<&Condvar> {
        unsafe { self.map_unchecked(|this| &this.cvar) }
    }
}

impl fmt::Debug for BarrierWaitResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BarrierWaitResult")
            .field("is_leader", &self.is_leader())
            .finish()
    }
}

impl BarrierWaitResult {
    /// Returns `true` if this thread is the "leader thread" for the call to
    /// [`Barrier::wait()`].
    ///
    /// Only one thread will have `true` returned from their result, all other
    /// threads will have `false` returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Barrier;
    ///
    /// let barrier = Barrier::new(1);
    /// let barrier_wait_result = barrier.wait();
    /// println!("{:?}", barrier_wait_result.is_leader());
    /// ```
    pub fn is_leader(&self) -> bool {
        self.0
    }
}
