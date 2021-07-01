use pinned_sync::{Condvar, Mutex};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;

struct Packet<T>(Pin<Arc<(Mutex<T>, Condvar)>>);
impl<T> Packet<T> {
    fn mutex(&self) -> Pin<&Mutex<T>> {
        unsafe { self.0.as_ref().map_unchecked(|pair| &pair.0) }
    }

    fn condvar(&self) -> Pin<&Condvar> {
        unsafe { self.0.as_ref().map_unchecked(|pair| &pair.1) }
    }
}

#[derive(Eq, PartialEq, Debug)]
struct NonCopy(i32);

#[test]
fn smoke() {
    let m = Mutex::boxed(());
    drop(m.as_ref().lock().unwrap());
    drop(m.as_ref().lock().unwrap());
}

#[test]
fn lots_and_lots() {
    const J: u32 = 1000;
    const K: u32 = 3;

    let m = Mutex::arc(0);

    fn inc(m: Pin<&Mutex<u32>>) {
        for _ in 0..J {
            *m.lock().unwrap() += 1;
        }
    }

    let (tx, rx) = channel();
    for _ in 0..K {
        let tx2 = tx.clone();
        let m2 = m.clone();
        thread::spawn(move || {
            inc(m2.as_ref());
            tx2.send(()).unwrap();
        });
        let tx2 = tx.clone();
        let m2 = m.clone();
        thread::spawn(move || {
            inc(m2.as_ref());
            tx2.send(()).unwrap();
        });
    }

    drop(tx);
    for _ in 0..2 * K {
        rx.recv().unwrap();
    }
    assert_eq!(*m.as_ref().lock().unwrap(), J * K * 2);
}

#[test]
fn try_lock() {
    let m = Mutex::boxed(());
    *m.as_ref().try_lock().unwrap() = ();
}

#[test]
fn test_into_inner() {
    let m = Mutex::boxed(NonCopy(10));
    assert_eq!(unsafe { Pin::into_inner_unchecked(m) }.into_inner().unwrap(), NonCopy(10));
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
    let m = Mutex::boxed(Foo(num_drops.clone()));
    assert_eq!(num_drops.load(Ordering::SeqCst), 0);
    {
        let _inner = unsafe { Pin::into_inner_unchecked(m) }.into_inner().unwrap();
        assert_eq!(num_drops.load(Ordering::SeqCst), 0);
    }
    assert_eq!(num_drops.load(Ordering::SeqCst), 1);
}

#[test]
fn test_into_inner_poison() {
    let m = Mutex::arc(NonCopy(10));
    let m2 = m.clone();
    let _ = thread::spawn(move || {
        let _lock = m2.as_ref().lock().unwrap();
        panic!("test panic in inner thread to poison mutex");
    })
    .join();

    assert!(m.as_ref().is_poisoned());
    match Arc::try_unwrap(unsafe { Pin::into_inner_unchecked(m) })
        .unwrap_or_else(|_| panic!())
        .into_inner()
    {
        Err(e) => assert_eq!(e.into_inner(), NonCopy(10)),
        Ok(x) => panic!("into_inner of poisoned Mutex is Ok: {:?}", x),
    }
}

#[test]
fn test_get_mut() {
    let mut m = Mutex::boxed(NonCopy(10));
    *unsafe { m.as_mut().get_unchecked_mut() }.get_mut().unwrap() = NonCopy(20);
    assert_eq!(unsafe { Pin::into_inner_unchecked(m) }.into_inner().unwrap(), NonCopy(20));
}

#[test]
fn test_get_mut_poison() {
    let m = Mutex::arc(NonCopy(10));
    let m2 = m.clone();
    let _ = thread::spawn(move || {
        let _lock = m2.as_ref().lock().unwrap();
        panic!("test panic in inner thread to poison mutex");
    })
    .join();

    assert!(m.as_ref().is_poisoned());
    match Arc::try_unwrap(unsafe { Pin::into_inner_unchecked(m) })
        .unwrap_or_else(|_| panic!())
        .get_mut()
    {
        Err(e) => assert_eq!(*e.into_inner(), NonCopy(10)),
        Ok(x) => panic!("get_mut of poisoned Mutex is Ok: {:?}", x),
    }
}

#[test]
fn test_mutex_arc_condvar() {
    let packet = Packet(Arc::pin((Mutex::uninit(false), Condvar::uninit())));
    packet.mutex().init();
    packet.condvar().init();
    let packet2 = Packet(packet.0.clone());
    let (tx, rx) = channel();
    let _t = thread::spawn(move || {
        // wait until parent gets in
        rx.recv().unwrap();
        let lock = packet2.mutex();
        let cvar = packet2.condvar();
        let mut lock = lock.lock().unwrap();
        *lock = true;
        cvar.notify_one();
    });

    let lock = packet.mutex();
    let cvar = packet.condvar();
    let mut lock = lock.lock().unwrap();
    tx.send(()).unwrap();
    assert!(!*lock);
    while !*lock {
        lock = cvar.wait(lock).unwrap();
    }
}

#[test]
fn test_arc_condvar_poison() {
    let packet = Packet(Arc::pin((Mutex::uninit(1), Condvar::uninit())));
    packet.mutex().init();
    packet.condvar().init();
    let packet2 = Packet(packet.0.clone());
    let (tx, rx) = channel();

    let _t = thread::spawn(move || {
        rx.recv().unwrap();
        let lock = packet2.mutex();
        let cvar = packet2.condvar();
        let _g = lock.lock().unwrap();
        cvar.notify_one();
        // Parent should fail when it wakes up.
        panic!();
    });

    let lock = packet.mutex();
    let cvar = packet.condvar();
    let mut lock = lock.lock().unwrap();
    tx.send(()).unwrap();
    while *lock == 1 {
        match cvar.wait(lock) {
            Ok(l) => {
                lock = l;
                assert_eq!(*lock, 1);
            }
            Err(..) => break,
        }
    }
}

#[test]
fn test_mutex_arc_poison() {
    let arc = Mutex::arc(1);
    assert!(!arc.as_ref().is_poisoned());
    let arc2 = arc.clone();
    let _ = thread::spawn(move || {
        let lock = arc2.as_ref().lock().unwrap();
        assert_eq!(*lock, 2);
    })
    .join();
    assert!(arc.as_ref().lock().is_err());
    assert!(arc.as_ref().is_poisoned());
}

#[test]
fn test_mutex_arc_nested() {
    // Tests nested mutexes and access
    // to underlying data.
    let arc = Mutex::arc(1);
    let arc2 = Mutex::arc(arc);
    let (tx, rx) = channel();
    let _t = thread::spawn(move || {
        let lock = arc2.as_ref().lock().unwrap();
        let lock2 = lock.as_ref().lock().unwrap();
        assert_eq!(*lock2, 1);
        tx.send(()).unwrap();
    });
    rx.recv().unwrap();
}

#[test]
fn test_mutex_arc_access_in_unwind() {
    let arc = Mutex::arc(1);
    let arc2 = arc.clone();
    let _ = thread::spawn(move || {
        struct Unwinder {
            i: Pin<Arc<Mutex<i32>>>,
        }
        impl Drop for Unwinder {
            fn drop(&mut self) {
                *self.i.as_ref().lock().unwrap() += 1;
            }
        }
        let _u = Unwinder { i: arc2 };
        panic!();
    })
    .join();
    let lock = arc.as_ref().lock().unwrap();
    assert_eq!(*lock, 2);
}

#[test]
fn test_mutex_unsized() {
    let mutex: Pin<Box<Mutex<[i32]>>> = Mutex::boxed([1, 2, 3]);
    {
        let b = &mut *mutex.as_ref().lock().unwrap();
        b[0] = 4;
        b[2] = 5;
    }
    let comp: &[i32] = &[4, 2, 5];
    assert_eq!(&*mutex.as_ref().lock().unwrap(), comp);
}
