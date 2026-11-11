use super::*;
use crate::div::ElementBuilder;
use crate::event_handler::EventContext;
use crate::tree::LayoutNodeId;
use blinc_core::events::event_types;
use std::sync::{Arc, Mutex, Once};
use taffy::Dimension;

static THEME_INIT: Once = Once::new();

fn ensure_theme_initialized() {
    THEME_INIT.call_once(ThemeState::init_default);
}

#[test]
fn test_text_input_data_insert() {
    let mut data = TextInputData::new();

    data.insert("hello");
    assert_eq!(data.value, "hello");
    assert_eq!(data.cursor, 5);

    data.cursor = 0;
    data.insert("world ");
    assert_eq!(data.value, "world hello");
}

#[test]
fn test_text_input_data_delete() {
    let mut data = TextInputData::with_value("hello");

    data.cursor = 5;
    data.delete_backward();
    assert_eq!(data.value, "hell");

    data.cursor = 0;
    data.delete_forward();
    assert_eq!(data.value, "ell");
}

#[test]
fn test_input_type_filtering() {
    let mut data = TextInputData::new();
    data.input_type = InputType::Number;

    data.insert("123.45");
    assert_eq!(data.value, "123.45");

    data.value.clear();
    data.cursor = 0;
    data.insert("abc123");
    assert_eq!(data.value, "123");
}

/// Regression test: keyboard stepping while focused used to leave
/// the displayed value stale — the focus guard meant for typing
/// swallowed step changes too.
#[test]
fn force_sync_once_overrides_focus_guard_for_step_changes() {
    let mut data = TextInputData::with_value("5");
    data.visual = TextFieldState::Focused;

    // Typing case: no flag, stays guarded.
    let is_focused = data.visual.is_focused();
    let force = std::mem::take(&mut data.force_sync_once);
    assert!(!force && is_focused);

    // Step case: flag forces the sync through despite focus, then
    // resets so it doesn't leak into the next one.
    data.force_sync_once = true;
    let is_focused = data.visual.is_focused();
    let force = std::mem::take(&mut data.force_sync_once);
    assert!(force || !is_focused);
    assert!(!data.force_sync_once);
}

#[test]
fn the_shown_text_is_the_value_on_change_normalized() {
    ensure_theme_initialized();

    let data = text_input_data();
    {
        let mut data = data.lock().unwrap();
        data.visual = TextFieldState::Focused;
    }

    let data_for_change = Arc::clone(&data);
    let input = text_input(&data)
        .input_type(InputType::Integer)
        .on_change(move |_| {
            let mut data = data_for_change.lock().unwrap();
            data.value.clear();
            data.cursor = 0;
            data.selection_start = None;
        });
    let mut tree = crate::renderer::RenderTree::from_element(&input);
    tree.compute_layout(300.0, 100.0);

    let ctx =
        EventContext::new(event_types::TEXT_INPUT, LayoutNodeId::default()).with_key_char('-');
    ElementBuilder::event_handlers(&input)
        .unwrap()
        .dispatch(&ctx);
    let updates = crate::stateful::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);

    assert_eq!(data.lock().unwrap().value, "");
    let mut shown = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if let Some(crate::renderer::ElementType::Text(t)) =
            tree.get_render_node(node).map(|r| &r.element_type)
        {
            shown.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(node));
    }
    assert!(
        shown.iter().all(|t| t.is_empty()),
        "the field shows {shown:?}, not the value on_change left"
    );
}

#[test]
fn state_callback_preserves_explicit_width() {
    ensure_theme_initialized();

    let data = text_input_data();
    let input = text_input(&data).w(42.0);

    let mut tree = LayoutTree::new();
    let root = input.build(&mut tree);
    let style = tree.get_style(root).unwrap();

    assert_eq!(style.size.width, Dimension::length(42.0));
}

#[test]
fn blur_cancels_a_pending_deferred_focus() {
    let _guard = FOCUS_TEST_LOCK.lock().unwrap();
    ensure_theme_initialized();
    blur_all_text_inputs();

    let data = text_input_data();
    let mut tree = LayoutTree::new();
    let _root = text_input(&data).build(&mut tree);

    // Auto-advance queues a re-focus (e.g. OTP typing the last
    // digit), then the user clicks outside before it drains.
    focus_text_input_deferred(&data);
    blur_all_text_inputs();
    process_pending_input_focus();

    assert!(!data.lock().unwrap().visual.is_focused());
}

#[test]
fn deferred_focus_applies_without_an_intervening_blur() {
    let _guard = FOCUS_TEST_LOCK.lock().unwrap();
    ensure_theme_initialized();
    blur_all_text_inputs();

    let data = text_input_data();
    let mut tree = LayoutTree::new();
    let _root = text_input(&data).build(&mut tree);

    focus_text_input_deferred(&data);
    process_pending_input_focus();

    assert!(data.lock().unwrap().visual.is_focused());
}

#[test]
fn focusing_another_input_blurs_previous_stateful_visual_state() {
    let _guard = FOCUS_TEST_LOCK.lock().unwrap();
    ensure_theme_initialized();
    blur_all_text_inputs();

    let first = text_input_data();
    let second = text_input_data();
    let mut tree = LayoutTree::new();
    let _first_root = text_input(&first).build(&mut tree);
    let _second_root = text_input(&second).build(&mut tree);

    focus_text_input(&first);
    focus_text_input(&second);

    assert!(!first.lock().unwrap().visual.is_focused());
}

fn css_config(css: &str, id: Option<&str>, classes: &[&str]) -> TextInputConfig {
    ensure_theme_initialized();
    let sheet = Stylesheet::parse(css).expect("css");
    let classes: Vec<std::sync::Arc<str>> = classes
        .iter()
        .map(|c| blinc_core::intern::intern(c))
        .collect();
    let mut cfg = TextInputConfig::default();
    css::apply_css_overrides(&mut cfg, &sheet, id, &classes, &TextFieldState::Idle);
    cfg
}

#[test]
fn class_rules_colour_the_placeholder_the_selection_and_the_caret() {
    let css = r#"
        .field { caret-color: #00ff00; }
        .field::placeholder { color: #ff0000; }
        .field::selection { background: #0000ff; }
    "#;
    let cfg = css_config(css, None, &["field"]);
    assert_eq!(cfg.placeholder_color, Color::rgb(1.0, 0.0, 0.0));
    assert_eq!(cfg.selection_color, Color::rgb(0.0, 0.0, 1.0));
    assert_eq!(cfg.cursor_color, Color::rgb(0.0, 1.0, 0.0));

    let other = css_config(css, None, &["other"]);
    let default = TextInputConfig::default();
    assert_eq!(other.placeholder_color, default.placeholder_color);
    assert_eq!(other.selection_color, default.selection_color);
}

#[test]
fn an_id_placeholder_rule_beats_a_class_rule() {
    let css = r#"
        .field::placeholder { color: #ff0000; }
        #email::placeholder { color: #00ff00; }
    "#;
    let cfg = css_config(css, Some("email"), &["field"]);
    assert_eq!(cfg.placeholder_color, Color::rgb(0.0, 1.0, 0.0));
}

#[test]
fn a_placeholder_rule_does_not_colour_the_field_itself() {
    let css = ".field::placeholder { color: #ff0000; }";
    let cfg = css_config(css, None, &["field"]);
    assert_eq!(cfg.text_color, TextInputConfig::default().text_color);
}

/// Focus is process-wide and `escape_in` blurs every field, so the tests that read focus back
/// take the shared focus lock for their whole run.
fn focus_lock() -> std::sync::MutexGuard<'static, ()> {
    FOCUS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn escape_in(input_type: InputType, value: &str) -> SharedTextInputData {
    ensure_theme_initialized();
    blur_all_text_inputs();
    let data = text_input_data();
    {
        let mut d = data.lock().unwrap();
        d.input_type = input_type;
        d.value = value.to_string();
        d.cursor = d.value.chars().count();
    }
    let input = text_input(&data);
    let mut tree = crate::renderer::RenderTree::from_element(&input);
    tree.compute_layout(300.0, 60.0);
    let root = tree.root().unwrap();
    // Focused through a click, as a user does.
    let click = EventContext::new(event_types::POINTER_DOWN, root)
        .with_bounds(200.0, 36.0)
        .with_mouse_pos(10.0, 10.0);
    let handlers = ElementBuilder::event_handlers(&input).unwrap();
    handlers.dispatch(&click);
    let esc = EventContext::new(event_types::KEY_DOWN, root).with_key_code(27);
    handlers.dispatch(&esc);
    data
}

#[test]
fn escape_clears_a_search_field() {
    let _focus = focus_lock();
    let data = escape_in(InputType::Search, "rust");
    let d = data.lock().unwrap();
    assert_eq!(d.value, "");
    assert!(d.visual.is_focused(), "clearing a search also blurred it");
}

#[test]
fn escape_blurs_an_empty_search_and_other_fields() {
    let _focus = focus_lock();
    let empty = escape_in(InputType::Search, "");
    assert!(!empty.lock().unwrap().visual.is_focused());
    let text = escape_in(InputType::Text, "rust");
    let d = text.lock().unwrap();
    assert_eq!(d.value, "rust", "Escape cleared a plain text field");
    assert!(!d.visual.is_focused());
}

/// A bound placeholder and invalid flag land in the field's data when their
/// signals move.
#[test]
fn a_bound_placeholder_and_invalid_follow_their_signals() {
    ensure_theme_initialized();
    let data = text_input_data();
    let placeholder = blinc_core::reactive::signal("Name".to_string());
    let invalid = blinc_core::reactive::signal(false);
    let _input = text_input(&data).placeholder(placeholder).invalid(invalid);
    assert_eq!(data.lock().unwrap().placeholder, "Name");
    assert!(!data.lock().unwrap().invalid);

    placeholder.set("Full name".to_string());
    invalid.set(true);
    let d = data.lock().unwrap();
    assert_eq!(d.placeholder, "Full name");
    assert!(d.invalid);
}

/// A field's signals belong to its data: the build that created the data can be disposed and
/// the field still follows its value; they go when the data does.
#[test]
fn the_signals_of_the_data_outlive_the_build_that_made_them() {
    let mut gone = Vec::new();
    for _ in 0..1000 {
        let build = blinc_core::owner::Owner::new();
        let data = build.run(|| {
            let data = Arc::new(Mutex::new(TextInputData::new()));
            let _ = text_input_focused(&data);
            data
        });
        build.dispose();
        let (revision, focus) = {
            let d = data.lock().unwrap();
            (d.revision.unwrap(), d.focus_signal.unwrap())
        };
        assert!(revision.try_get().is_some(), "revision went with the build");
        assert!(focus.try_get().is_some(), "focus went with the build");
        drop(data);
        gone.push((revision, focus));
    }
    blinc_core::owner::dispose_pending();
    assert!(
        gone.iter()
            .all(|(r, f)| r.try_get().is_none() && f.try_get().is_none()),
        "a signal outlived its data"
    );
}

/// An effect or computed that holds the last handle of the data is dropped when the build is
/// disposed, with the graph locked; releasing the data's signals then must not lock it again.
#[test]
fn disposing_a_build_whose_effect_holds_the_data_does_not_hang() {
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let build = blinc_core::owner::Owner::new();
        let revision = build.run(|| {
            let data = Arc::new(Mutex::new(TextInputData::new()));
            let revision = data.lock().unwrap().revision.unwrap();
            let held = Arc::clone(&data);
            let _ = blinc_core::reactive::effect(move |_| {
                let _ = held.lock().map(|d| d.value.len());
            });
            let held = Arc::clone(&data);
            let _ = blinc_core::reactive::computed(move |_| {
                held.lock().map(|d| d.value.len()).unwrap_or(0)
            });
            revision
        });
        build.dispose();
        assert!(revision.try_get().is_none());
        let _ = done.send(());
    });
    wait.recv_timeout(std::time::Duration::from_secs(10))
        .expect("disposing a build hung");
}

/// The last handle of the data falls while this thread holds the binding registry: telling the
/// registry about the disposed signals must not wait for it.
#[test]
fn dropping_the_data_while_the_registry_is_held_does_not_hang() {
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let data = Arc::new(Mutex::new(TextInputData::new()));
        let revision = data.lock().unwrap().revision.unwrap();
        crate::binding::with_registry(|_| drop(data));
        blinc_core::owner::dispose_pending();
        assert!(revision.try_get().is_none());
        let _ = done.send(());
    });
    wait.recv_timeout(std::time::Duration::from_secs(10))
        .expect("dropping the data hung");
}

/// The caret and the click-to-caret mapping use the letter spacing the text is drawn
/// with, taken from the field's CSS like the font size is.
#[test]
fn caret_and_hit_test_follow_css_letter_spacing() {
    use crate::text_measure::{TextLayoutOptions, measure_text_with_options};
    ensure_theme_initialized();
    let sheet = Stylesheet::parse(".tracked { letter-spacing: -3px; }").unwrap();
    let mut cfg = TextInputConfig::default();
    apply_css_overrides(
        &mut cfg,
        &sheet,
        None,
        &[Arc::from("tracked")],
        &TextFieldState::Idle,
    );
    let drawn = |text: &str| {
        measure_text_with_options(
            text,
            cfg.font_size,
            &TextLayoutOptions::new().with_letter_spacing(-3.0),
        )
        .width
    };

    let mut data = TextInputData::with_value("hello world");
    data.cursor = 5;
    // The caret is a difference of two widths, so it matches up to float rounding.
    assert!((caret_x(&mut data, &cfg) - drawn("hello")).abs() < 0.01);

    // A click on the drawn edge after three characters lands after three.
    let x = drawn("hel");
    assert_eq!(
        data.cursor_position_from_x_with(x, cfg.font_size, &cfg.face),
        3
    );
    assert_ne!(data.cursor_position_from_x(x, cfg.font_size), 3);
}
