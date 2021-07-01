use pinned_sync::RwLock;
use rand::{self, Rng};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, TryLockError};
use std::thread;

#[derive(Eq, PartialEq, Debug)]
struct NonCopy(i32);

#[test]
fn smoke() {
    let l = RwLock::boxed(());
    drop(l.as_ref().read().unwrap());
    drop(l.as_ref().write().unwrap());
    drop((l.as_ref().read().unwrap(), l.as_ref().read().unwrap()));
    drop(l.as_ref().write().unwrap());
}

#[test]
fn frob() {
    const N: u32 = 10;
    const M: usize = 1000;

    let r = RwLock::arc(());

    let (tx, rx) = channel::<()>();
    for _ in 0..N {
        let tx = tx.clone();
        let r = r.clone();
        thread::spawn(move || {
            let mut rng = rand::thread_rng();
            for _ in 0..M {
                if rng.gen_bool(1.0 / (N as f64)) {
                    drop(r.as_ref().write().unwrap());
                } else {
                    drop(r.as_ref().read().unwrap());
                }
            }
            drop(tx);
        });
    }
    drop(tx);
    let _ = rx.recv();
}

#[test]
fn test_rw_arc_poison_wr() {
    let arc = RwLock::arc(1);
    let arc2 = arc.clone();
    let _: Result<(), _> = thread::spawn(move || {
        let _lock = arc2.as_ref().write().unwrap();
        panic!();
    })
    .join();
    assert!(arc.as_ref().read().is_err());
}

#[test]
fn test_rw_arc_poison_ww() {
    let arc = RwLock::arc(1);
    assert!(!arc.as_ref().is_poisoned());
    let arc2 = arc.clone();
    let _: Result<(), _> = thread::spawn(move || {
        let _lock = arc2.as_ref().write().unwrap();
        panic!();
    })
    .join();
    assert!(arc.as_ref().write().is_err());
    assert!(arc.as_ref().is_poisoned());
}

#[test]
fn test_rw_arc_no_poison_rr() {
    let arc = RwLock::arc(1);
    let arc2 = arc.clone();
    let _: Result<(), _> = thread::spawn(move || {
        let _lock = arc2.as_ref().read().unwrap();
        panic!();
    })
    .join();
    let lock = arc.as_ref().read().unwrap();
    assert_eq!(*lock, 1);
}
#[test]
fn test_rw_arc_no_poison_rw() {
    let arc = RwLock::arc(1);
    let arc2 = arc.clone();
    let _: Result<(), _> = thread::spawn(move || {
        let _lock = arc2.as_ref().read().unwrap();
        panic!()
    })
    .join();
    let lock = arc.as_ref().write().unwrap();
    assert_eq!(*lock, 1);
}

#[test]
fn test_rw_arc() {
    let arc = RwLock::arc(0);
    let arc2 = arc.clone();
    let (tx, rx) = channel();

    thread::spawn(move || {
        let mut lock = arc2.as_ref().write().unwrap();
        for _ in 0..10 {
            let tmp = *lock;
            *lock = -1;
            thread::yield_now();
            *lock = tmp + 1;
        }
        tx.send(()).unwrap();
    });

    // Readers try to catch the writer in the act
    let mut children = Vec::new();
    for _ in 0..5 {
        let arc3 = arc.clone();
        children.push(thread::spawn(move || {
            let lock = arc3.as_ref().read().unwrap();
            assert!(*lock >= 0);
        }));
    }

    // Wait for children to pass their asserts
    for r in children {
        assert!(r.join().is_ok());
    }

    // Wait for writer to finish
    rx.recv().unwrap();
    let lock = arc.as_ref().read().unwrap();
    assert_eq!(*lock, 10);
}

#[test]
fn test_rw_arc_access_in_unwind() {
    let arc = RwLock::arc(1);
    let arc2 = arc.clone();
    let _ = thread::spawn(move || {
        struct Unwinder {
            i: Pin<Arc<RwLock<isize>>>,
        }
        impl Drop for Unwinder {
            fn drop(&mut self) {
                let mut lock = self.i.as_ref().write().unwrap();
                *lock += 1;
            }
        }
        let _u = Unwinder { i: arc2 };
        panic!();
    })
    .join();
    let lock = arc.as_ref().read().unwrap();
    assert_eq!(*lock, 2);
}

#[test]
fn test_rwlock_unsized() {
    let rw: Pin<Box<RwLock<[i32]>>> = RwLock::boxed([1, 2, 3]);
    {
        let b = &mut *rw.as_ref().write().unwrap();
        b[0] = 4;
        b[2] = 5;
    }
    let comp: &[i32] = &[4, 2, 5];
    assert_eq!(&*rw.as_ref().read().unwrap(), comp);
}

#[test]
fn test_rwlock_try_write() {
    let lock = RwLock::boxed(0isize);
    let read_guard = lock.as_ref().read().unwrap();

    let write_result = lock.as_ref().try_write();
    match write_result {
        Err(TryLockError::WouldBlock) => (),
        Ok(_) => panic!("try_write should not succeed while read_guard is in scope"),
        Err(_) => panic!("unexpected error"),
    }

    drop(read_guard);
}

#[test]
fn test_into_inner() {
    let m = RwLock::boxed(NonCopy(10));
    assert_eq!(
        unsafe { Pin::into_inner_unchecked(m) }
            .into_inner()
            .unwrap(),
        NonCopy(10)
    );
}

#[test]
fn test_into_inner_drop() {
    struct Foo(Arc<AtomicUsize>);
    impl Drop for Foo {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let num_drops = Arc::new(AtomicUsize::new(0));
    let m = RwLock::boxed(Foo(num_drops.clone()));
    assert_eq!(num_drops.load(Ordering::SeqCst), 0);
    {
        let _inner = unsafe { Pin::into_inner_unchecked(m) }
            .into_inner()
            .unwrap();
        assert_eq!(num_drops.load(Ordering::SeqCst), 0);
    }
    assert_eq!(num_drops.load(Ordering::SeqCst), 1);
}

#[test]
fn test_into_inner_poison() {
    let m = RwLock::arc(NonCopy(10));
    let m2 = m.clone();
    let _ = thread::spawn(move || {
        let _lock = m2.as_ref().write().unwrap();
        panic!("test panic in inner thread to poison RwLock");
    })
    .join();

    assert!(m.as_ref().is_poisoned());
    match Arc::try_unwrap(unsafe { Pin::into_inner_unchecked(m) })
        .unwrap_or_else(|_| panic!())
        .into_inner()
    {
        Err(e) => assert_eq!(e.into_inner(), NonCopy(10)),
        Ok(x) => panic!("into_inner of poisoned RwLock is Ok: {:?}", x),
    }
}

#[test]
fn test_get_mut() {
    let mut m = RwLock::boxed(NonCopy(10));
    *unsafe { m.as_mut().get_unchecked_mut() }.get_mut().unwrap() = NonCopy(20);
    assert_eq!(
        unsafe { Pin::into_inner_unchecked(m) }
            .into_inner()
            .unwrap_or_else(|_| panic!()),
        NonCopy(20)
    );
}

#[test]
fn test_get_mut_poison() {
    let m = RwLock::arc(NonCopy(10));
    let m2 = m.clone();
    let _ = thread::spawn(move || {
        let _lock = m2.as_ref().write().unwrap();
        panic!("test panic in inner thread to poison RwLock");
    })
    .join();

    assert!(m.as_ref().is_poisoned());
    match Arc::try_unwrap(unsafe { Pin::into_inner_unchecked(m) })
        .unwrap_or_else(|_| panic!())
        .get_mut()
    {
        Err(e) => assert_eq!(*e.into_inner(), NonCopy(10)),
        Ok(x) => panic!("get_mut of poisoned RwLock is Ok: {:?}", x),
    }
}
