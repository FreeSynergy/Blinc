//! A component built under a scope, opened and disposed, leaves no live signal, computed or
//! effect behind: the count does not grow from the tenth cycle to the fiftieth.

mod common;

use blinc_core::owner::Owner;
use blinc_core::reactive::{global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::{ElementBuilder, div};
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::RenderTree;
use common::*;
use std::time::Duration;

fn find(tree: &RenderTree, class: &str) -> Option<LayoutNodeId> {
    let registry = tree.element_registry();
    let mut stack = vec![tree.root()?];
    while let Some(node) = stack.pop() {
        if registry.has_class(node, class) {
            return Some(node);
        }
        stack.extend(tree.layout_tree.children(node));
    }
    None
}

fn frame(tree: &mut RenderTree) {
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(400.0, 400.0);
}

fn click(tree: &mut RenderTree, router: &mut EventRouter, node: LayoutNodeId) {
    let b = tree.get_absolute_bounds(node).unwrap();
    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    for (n, e) in router.on_mouse_move(tree, x, y) {
        tree.dispatch_event(n, e, x, y);
    }
    frame(tree);
    for (n, e) in router.on_mouse_down(tree, x, y, MouseButton::Left) {
        tree.dispatch_event(n, e, x, y);
    }
    frame(tree);
    for (n, e) in router.on_mouse_up(tree, x, y, MouseButton::Left) {
        tree.dispatch_event(n, e, x, y);
    }
    frame(tree);
}

fn live() -> (usize, usize, usize) {
    let s = global_graph().lock().unwrap().stats();
    (s.signal_count, s.derived_count, s.effect_count)
}

/// Build `make` 50 times under a scope, open it via `open` if given, close, dispose; no hang, no growth.
fn cycle(name: &'static str, open: Option<&'static str>, make: fn() -> Box<dyn ElementBuilder>) {
    guarded(|| {
        init();
        let (done, wait) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut counts = Vec::new();
            for _ in 0..50 {
                // Instance keys count calls per frame; a new frame starts each cycle.
                blinc_layout::reset_call_counters();
                let build = Owner::new();
                let mut router = EventRouter::new();
                let mut tree = build.run(|| {
                    let host = div().w(400.0).h(400.0).child_box(make());
                    let mut tree = RenderTree::from_element(&host);
                    tree.compute_layout(400.0, 400.0);
                    tree
                });
                {
                    let _in = build.enter();
                    if let Some(class) = open
                        && let Some(node) = find(&tree, class)
                    {
                        click(&mut tree, &mut router, node);
                    }
                    blinc_layout::click_outside::fire_click_outside(&[]);
                    frame(&mut tree);
                }
                build.dispose();
                drop(tree);
                blinc_layout::widgets::text_input::blur_all_text_inputs();
                counts.push(live());
            }
            let _ = done.send(counts);
        });
        let counts = wait
            .recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|_| panic!("{name}: hung"));
        assert_eq!(
            counts[49], counts[9],
            "{name}: (signals, computeds, effects) grew between cycle 10 and 50"
        );
    })
}

#[test]
fn time_picker_cycle() {
    cycle("time_picker", Some("cn-time-picker-segment"), || {
        Box::new(blinc_cn::time_picker(
            signal(blinc_cn::TimeOfDay::default()),
        ))
    });
}

#[test]
fn color_picker_cycle() {
    cycle("color_picker", Some("cn-color-picker-trigger"), || {
        Box::new(blinc_cn::color_picker(signal(blinc_core::Color::WHITE)))
    });
}

/// The panel itself, with its hex field, built without a popover.
#[test]
fn color_picker_panel_cycle() {
    cycle("color_picker_panel", None, || {
        Box::new(blinc_cn::color_picker(signal(blinc_core::Color::WHITE)).inline())
    });
}

#[test]
fn number_input_cycle() {
    cycle("number_input", None, || {
        Box::new(blinc_cn::number_input(signal(1.0)))
    });
}

#[test]
fn input_otp_cycle() {
    cycle("input_otp", None, || {
        Box::new(blinc_cn::input_otp(signal(String::new()), 4))
    });
}

#[test]
fn select_cycle() {
    cycle("select", Some("cn-select-trigger"), || {
        Box::new(
            blinc_cn::select(signal(String::new()))
                .option("a", "A")
                .option("b", "B")
                .w(200.0),
        )
    });
}

#[test]
fn bare_text_input_cycle() {
    cycle("bare_text_input", None, || {
        let data = blinc_layout::widgets::text_input::text_input_data();
        Box::new(blinc_layout::widgets::text_input::text_input(&data))
    });
}

#[test]
fn data_only_cycle() {
    cycle("data_only", None, || {
        let _data = blinc_layout::widgets::text_input::text_input_data();
        Box::new(div())
    });
}

#[test]
fn builder_without_tree() {
    guarded(|| {
        init();
        let (done, wait) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let run = || {
                let build = Owner::new();
                build.run(|| {
                    for _ in 0..10 {
                        let data = blinc_layout::widgets::text_input::text_input_data();
                        let t = blinc_layout::widgets::text_input::text_input(&data);
                        drop(t);
                        drop(data);
                    }
                });
                build.dispose();
            };
            run();
            let before = live();
            for _ in 0..50 {
                run();
            }
            let _ = done.send((before, live()));
        });
        let (before, after) = wait
            .recv_timeout(Duration::from_secs(60))
            .expect("builder_without_tree: hung");
        assert_eq!(
            after, before,
            "a builder without a tree left signals behind"
        );
    })
}

#[test]
fn select_closed_cycle() {
    cycle("select_closed", None, || {
        Box::new(
            blinc_cn::select(signal(String::new()))
                .option("a", "A")
                .option("b", "B")
                .w(200.0),
        )
    });
}

#[test]
fn input_cycle() {
    cycle("input", None, || {
        let data = blinc_layout::widgets::text_input::text_input_data();
        Box::new(blinc_cn::input(&data))
    });
}

#[test]
fn textarea_cycle() {
    cycle("textarea", None, || {
        let state = blinc_layout::widgets::text_area::text_area_state();
        Box::new(blinc_cn::textarea(&state))
    });
}

#[test]
fn combobox_cycle() {
    cycle("combobox", None, || {
        Box::new(
            blinc_cn::combobox(signal(String::new()))
                .option("a", "A")
                .option("b", "B")
                .w(200.0),
        )
    });
}

#[test]
fn date_picker_cycle() {
    cycle("date_picker", Some("cn-date-picker-trigger"), || {
        Box::new(blinc_cn::date_picker(signal(None)))
    });
}
