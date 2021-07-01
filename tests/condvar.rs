use pinned_sync::{Condvar, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn smoke() {
    let c = Condvar::boxed();
    c.as_ref().notify_one();
    c.as_ref().notify_all();
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore)]
fn notify_one() {
    let m = Mutex::arc(());
    let m2 = m.clone();
    let c = Condvar::arc();
    let c2 = c.clone();

    let g = m.as_ref().lock().unwrap();
    let _t = thread::spawn(move || {
        let _g = m2.as_ref().lock().unwrap();
        c2.as_ref().notify_one();
    });
    let g = c.as_ref().wait(g).unwrap();
    drop(g);
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore)]
fn notify_all() {
    const N: usize = 10;

    let m = Mutex::arc(0);
    let c = Condvar::arc();
    let (tx, rx) = channel();
    for _ in 0..N {
        let m = m.clone();
        let c = c.clone();
        let tx = tx.clone();
        thread::spawn(move || {
            let mut cnt = m.as_ref().lock().unwrap();
            *cnt += 1;
            if *cnt == N {
                tx.send(()).unwrap();
            }
            while *cnt != 0 {
                cnt = c.as_ref().wait(cnt).unwrap();
            }
            tx.send(()).unwrap();
        });
    }
    drop(tx);

    rx.recv().unwrap();
    let mut cnt = m.as_ref().lock().unwrap();
    *cnt = 0;
    c.as_ref().notify_all();
    drop(cnt);

    for _ in 0..N {
        rx.recv().unwrap();
    }
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore)]
fn wait_while() {
    let m = Mutex::arc(false);
    let m2 = m.clone();
    let c = Condvar::arc();
    let c2 = c.clone();

    // Inside of our lock, spawn a new thread, and then wait for it to start.
    thread::spawn(move || {
        let mut started = m2.as_ref().lock().unwrap();
        *started = true;
        // We notify the condvar that the value has changed.
        c2.as_ref().notify_one();
    });

    // Wait for the thread to start up.
    let guard = c
        .as_ref()
        .wait_while(m.as_ref().lock().unwrap(), |started| !*started);
    assert!(*guard.unwrap());
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore)]
fn wait_timeout_wait() {
    let m = Mutex::arc(());
    let c = Condvar::arc();

    loop {
        let g = m.as_ref().lock().unwrap();
        let (_g, no_timeout) = c
            .as_ref()
            .wait_timeout(g, Duration::from_millis(1))
            .unwrap();
        // spurious wakeups mean this isn't necessarily true
        // so execute test again, if not timeout
        if !no_timeout.timed_out() {
            continue;
        }

        break;
    }
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore)]
fn wait_timeout_while_wait() {
    let m = Mutex::arc(());
    let c = Condvar::arc();

    let g = m.as_ref().lock().unwrap();
    let (_g, wait) = c
        .as_ref()
        .wait_timeout_while(g, Duration::from_millis(1), |_| true)
        .unwrap();
    // no spurious wakeups. ensure it timed-out
    assert!(wait.timed_out());
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore)]
fn wait_timeout_while_instant_satisfy() {
    let m = Mutex::arc(());
    let c = Condvar::arc();

    let g = m.as_ref().lock().unwrap();
    let (_g, wait) = c
        .as_ref()
        .wait_timeout_while(g, Duration::from_millis(0), |_| false)
        .unwrap();
    // ensure it didn't time-out even if we were not given any time.
    assert!(!wait.timed_out());
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore)]
fn wait_timeout_while_wake() {
    let m = Mutex::arc(false);
    let m2 = m.clone();
    let c = Condvar::arc();
    let c2 = c.clone();

    let g = m.as_ref().lock().unwrap();
    let _t = thread::spawn(move || {
        let mut started = m2.as_ref().lock().unwrap();
        thread::sleep(Duration::from_millis(1));
        *started = true;
        c2.as_ref().notify_one();
    });
    let (g2, wait) = c
        .as_ref()
        .wait_timeout_while(g, Duration::from_millis(u64::MAX), |&mut notified| {
            !notified
        })
        .unwrap();
    // ensure it didn't time-out even if we were not given any time.
    assert!(!wait.timed_out());
    assert!(*g2);
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore)]
fn wait_timeout_wake() {
    let m = Mutex::arc(());
    let c = Condvar::arc();

    loop {
        let g = m.as_ref().lock().unwrap();

        let c2 = c.clone();
        let m2 = m.clone();

        let notified = Arc::new(AtomicBool::new(false));
        let notified_copy = notified.clone();

        let t = thread::spawn(move || {
            let _g = m2.as_ref().lock().unwrap();
            thread::sleep(Duration::from_millis(1));
            notified_copy.store(true, Ordering::SeqCst);
            c2.as_ref().notify_one();
        });
        let (g, timeout_res) = c.as_ref().wait_timeout(g, Duration::from_millis(u64::MAX)).unwrap();
        assert!(!timeout_res.timed_out());
        // spurious wakeups mean this isn't necessarily true
        // so execute test again, if not notified
        if !notified.load(Ordering::SeqCst) {
            t.join().unwrap();
            continue;
        }
        drop(g);

        t.join().unwrap();

        break;
    }
}

#[test]
#[should_panic]
#[cfg_attr(not(unix), ignore)]
fn two_mutexes() {
    let m = Mutex::arc(());
    let m2 = m.clone();
    let c = Condvar::arc();
    let c2 = c.clone();

    let mut g = m.as_ref().lock().unwrap();
    let _t = thread::spawn(move || {
        let _g = m2.as_ref().lock().unwrap();
        c2.as_ref().notify_one();
    });
    g = c.as_ref().wait(g).unwrap();
    drop(g);

    let m = Mutex::boxed(());
    let _ = c.as_ref().wait(m.as_ref().lock().unwrap()).unwrap();
}
