//! Signals owned by a value: they survive the build that made the value, go with the value, and
//! disposing a scope that holds the last handle of the value does not deadlock the graph.
//!
//! One process, one graph: these tests run one after the other under a lock, so the live-signal
//! count is not disturbed by another test.

use blinc_core::owner::{DataSignals, Owner};
use blinc_core::reactive::{computed, effect, global_graph, signal};
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

fn live_signals() -> usize {
    global_graph().lock().unwrap().stats().signal_count
}

/// Run `f` on its own thread (scopes are per thread) and fail instead of hanging.
fn no_hang(f: impl FnOnce() + Send + 'static) {
    let (done, wait) = mpsc::channel();
    std::thread::spawn(move || {
        f();
        let _ = done.send(());
    });
    wait.recv_timeout(Duration::from_secs(5))
        .expect("disposing hung: the graph lock was taken twice");
}

#[test]
fn rebuilds_leave_no_live_signal_behind() {
    let _g = serial();
    let before = live_signals();
    for _ in 0..1000 {
        let build = Owner::new();
        let data = build.run(|| {
            let data = DataSignals::default();
            data.signal(0_u64);
            data.signal(false);
            data
        });
        build.dispose();
        drop(data);
    }
    assert_eq!(live_signals(), before);
}

#[test]
fn a_value_held_by_many_builds_keeps_its_signals_until_the_last_is_gone() {
    let _g = serial();
    let before = live_signals();
    let data = DataSignals::default();
    let held = data.signal(1_u64);
    for _ in 0..1000 {
        let build = Owner::new();
        let copy = data.clone();
        build.run(|| {
            let _ = signal(0_u8);
            let c = computed(move |_| {
                let _keep = &copy;
                1_u8
            });
            let _ = c.try_get();
        });
        build.dispose();
    }
    assert_eq!(held.try_get(), Some(1));
    drop(data);
    assert_eq!(held.try_get(), None);
    assert_eq!(live_signals(), before);
}

#[test]
fn a_computed_that_holds_the_last_clone_can_be_disposed() {
    let _g = serial();
    no_hang(|| {
        let build = Owner::new();
        let data = DataSignals::default();
        let held = data.signal(1_u64);
        let copy = data.clone();
        build.run(|| {
            let c = computed(move |_| {
                let _keep = &copy;
                0_u8
            });
            let _ = c.try_get();
        });
        drop(data);
        build.dispose();
        assert_eq!(held.try_get(), None);
    });
}

#[test]
fn an_effect_that_holds_the_last_clone_can_be_disposed() {
    let _g = serial();
    no_hang(|| {
        let build = Owner::new();
        let data = DataSignals::default();
        let held = data.signal(1_u64);
        let copy = data.clone();
        build.run(|| {
            let _ = effect(move |_| {
                let _keep = &copy;
            });
        });
        drop(data);
        build.dispose();
        assert_eq!(held.try_get(), None);
    });
}

#[test]
fn a_write_that_replaces_the_last_clone_does_not_hang() {
    let _g = serial();
    no_hang(|| {
        let data = DataSignals::default();
        let held = data.signal(1_u64);
        let s = signal(Some(data));
        s.set(None);
        blinc_core::owner::dispose_pending();
        assert_eq!(held.try_get(), None);

        let data = DataSignals::default();
        let held = data.signal(1_u64);
        let s = signal(Some(data));
        s.update(|_| None);
        blinc_core::owner::dispose_pending();
        assert_eq!(held.try_get(), None);
    });
}

#[test]
fn a_state_write_that_replaces_the_last_clone_does_not_hang() {
    let _g = serial();
    no_hang(|| {
        let data = DataSignals::default();
        let held = data.signal(1_u64);
        let s = global_graph().lock().unwrap().create_signal(Some(data));
        let state = blinc_core::reactive::State::new(
            s,
            global_graph(),
            blinc_core::reactive::global_dirty_flag(),
        );
        state.set(None);
        blinc_core::owner::dispose_pending();
        assert_eq!(held.try_get(), None);
    });
}

#[test]
fn a_recompute_that_replaces_the_last_clone_does_not_hang() {
    let _g = serial();
    no_hang(|| {
        let dep = signal(0_u32);
        let c = computed(move |g| {
            let _ = g.get(dep);
            DataSignals::default()
        });
        let first = c.try_get().unwrap();
        let held = first.signal(1_u64);
        drop(first);
        dep.set(1);
        let _ = c.try_get();
        blinc_core::owner::dispose_pending();
        assert_eq!(held.try_get(), None);
    });
}

#[test]
fn a_write_that_keeps_another_clone_disposes_nothing() {
    let _g = serial();
    no_hang(|| {
        let data = DataSignals::default();
        let held = data.signal(1_u64);
        let s = signal(Some(data.clone()));
        s.set(None);
        assert_eq!(held.try_get(), Some(1));
        drop(data);
        assert_eq!(held.try_get(), None);
    });
}
