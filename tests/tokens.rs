//! Browser tests for the Write_On design tokens (issue #5).
//!
//! Loads the real stylesheet into the live document and checks, via
//! `getComputedStyle`, that the custom properties resolve and that the
//! opt-in scope applies the theme. The scope is the editor root (`.te-app`)
//! carrying `data-mode="write-on"` or the `write-on` class; the same
//! attribute on any other element is not styled (issue #77). No mocks.

use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use web_sys::{Document, Element};

wasm_bindgen_test_configure!(run_in_browser);

const TOKENS_CSS: &str = include_str!("../public/css/tokens.css");

// Local bindings so the test does not depend on extra web-sys features.
#[wasm_bindgen]
extern "C" {
    type ComputedStyle;

    #[wasm_bindgen(js_namespace = window, js_name = getComputedStyle)]
    fn get_computed_style(el: &Element) -> ComputedStyle;

    #[wasm_bindgen(method, js_name = getPropertyValue)]
    fn get_property_value(this: &ComputedStyle, name: &str) -> String;
}

fn document() -> Document {
    web_sys::window()
        .expect("no window")
        .document()
        .expect("no document")
}

/// Injects the stylesheet into the document; returns the `<style>` node.
fn inject_stylesheet(doc: &Document) -> Element {
    let style = doc.create_element("style").unwrap();
    style.set_attribute("data-test", "tokens").unwrap();
    style.set_text_content(Some(TOKENS_CSS));
    doc.document_element()
        .unwrap()
        .append_child(&style)
        .unwrap();
    style
}

fn computed(el: &Element, prop: &str) -> String {
    get_computed_style(el)
        .get_property_value(prop)
        .trim()
        .to_string()
}

fn px(value: &str) -> f64 {
    value
        .trim_end_matches("px")
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("not a px value: {value}"))
}

#[wasm_bindgen_test]
fn tokens_resolve_on_root() {
    let doc = document();
    let style = inject_stylesheet(&doc);
    let root = doc.document_element().unwrap();

    let expected = [
        ("--te-color-bg", "#0a0d1c"),
        ("--te-color-text", "#e8d9c4"),
        ("--te-color-text-dim", "#8f8781"),
        ("--te-color-accent", "#8c86e6"),
        ("--te-color-accent-soft", "rgba(140, 134, 230, 0.4)"),
        ("--te-color-panel", "#11142a"),
        ("--te-color-panel-border", "#262a4a"),
        ("--te-color-popover", "#171a2e"),
        ("--te-color-popover-hover", "#1f2340"),
        ("--te-radius-popover", "8px"),
        ("--te-font-size-body", "1.0625rem"),
        ("--te-line-height", "1.75"),
        ("--te-ghost-opacity", "0.1"),
        ("--te-measure", "70ch"),
        ("--te-page-margin-top", "clamp(3rem, 12vh, 8rem)"),
    ];
    let mut failures = Vec::new();
    for (name, want) in expected {
        let got = computed(&root, name);
        if got != want {
            failures.push(format!("{name}: expected {want:?}, got {got:?}"));
        }
    }
    let mono = computed(&root, "--te-font-mono");
    if !mono.ends_with("monospace") {
        failures.push(format!("--te-font-mono should end in monospace: {mono:?}"));
    }
    let display = computed(&root, "--te-font-display");
    if !display.ends_with("cursive") {
        failures.push(format!(
            "--te-font-display should end in cursive: {display:?}"
        ));
    }

    style.remove();
    assert!(
        failures.is_empty(),
        "token mismatches:\n{}",
        failures.join("\n")
    );
}

fn assert_scope_applies(scope_attr: (&str, &str)) {
    let doc = document();
    let body = doc.body().unwrap();
    let style = inject_stylesheet(&doc);

    let scoped = doc.create_element("div").unwrap();
    scoped.set_attribute("class", "te-app").unwrap();
    if scope_attr.0 == "class" {
        scoped
            .set_attribute("class", &format!("te-app {}", scope_attr.1))
            .unwrap();
    } else {
        scoped.set_attribute(scope_attr.0, scope_attr.1).unwrap();
    }
    scoped.set_attribute("style", "width: 2000px").unwrap();
    let surface = doc.create_element("div").unwrap();
    surface.set_attribute("class", "te-surface").unwrap();
    scoped.append_child(&surface).unwrap();
    let preview = doc.create_element("div").unwrap();
    preview.set_attribute("class", "markdown-preview").unwrap();
    scoped.append_child(&preview).unwrap();
    body.append_child(&scoped).unwrap();

    let plain = doc.create_element("div").unwrap();
    plain.set_attribute("class", "te-surface").unwrap();
    body.append_child(&plain).unwrap();

    let bg = computed(&scoped, "background-color");
    let fg = computed(&scoped, "color");
    let font_size = px(&computed(&scoped, "font-size"));
    let line_height = px(&computed(&scoped, "line-height"));
    let family = computed(&scoped, "font-family");

    let surface_max = computed(&surface, "max-width");
    let surface_ml = computed(&surface, "margin-left");
    let surface_mr = computed(&surface, "margin-right");
    let surface_pt = px(&computed(&surface, "padding-top"));
    let preview_max = computed(&preview, "max-width");

    let plain_bg = computed(&plain, "background-color");
    let plain_max = computed(&plain, "max-width");

    scoped.remove();
    plain.remove();
    style.remove();

    let label = format!("{}={}", scope_attr.0, scope_attr.1);
    assert_eq!(bg, "rgb(10, 13, 28)", "{label}: background");
    assert_eq!(fg, "rgb(232, 217, 196)", "{label}: text colour");
    assert!(
        (line_height / font_size - 1.75).abs() < 0.01,
        "{label}: line-height ratio {line_height}/{font_size}"
    );
    assert!(
        family.contains("monospace"),
        "{label}: font-family {family:?}"
    );

    assert_ne!(surface_max, "none", "{label}: surface max-width");
    assert!(
        px(&surface_max) > 0.0,
        "{label}: surface max-width {surface_max}"
    );
    assert_eq!(surface_ml, surface_mr, "{label}: surface centred");
    assert!(
        px(&surface_ml) > 0.0,
        "{label}: surface margin {surface_ml}"
    );
    assert!(
        surface_pt >= 48.0,
        "{label}: generous top margin {surface_pt}px"
    );
    assert_eq!(
        preview_max, surface_max,
        "{label}: preview shares the measure"
    );

    // Outside the scope the plain editor is untouched.
    assert_eq!(plain_bg, "rgba(0, 0, 0, 0)", "unscoped background");
    assert_eq!(plain_max, "none", "unscoped max-width");
}

#[wasm_bindgen_test]
fn data_mode_attribute_applies_write_on_theme() {
    assert_scope_applies(("data-mode", "write-on"));
}

#[wasm_bindgen_test]
fn write_on_class_applies_write_on_theme() {
    assert_scope_applies(("class", "write-on"));
}

/// A host element that is not the editor root is never themed, whatever it
/// carries (issue #77: an embedded editor must not restyle the host page).
#[wasm_bindgen_test]
fn write_on_scope_outside_the_editor_root_is_not_styled() {
    let doc = document();
    let body = doc.body().unwrap();
    let style = inject_stylesheet(&doc);
    let mut failures = Vec::new();
    for (name, value) in [("data-mode", "write-on"), ("class", "write-on")] {
        let host = doc.create_element("div").unwrap();
        host.set_attribute(name, value).unwrap();
        let surface = doc.create_element("div").unwrap();
        surface.set_attribute("class", "te-surface").unwrap();
        host.append_child(&surface).unwrap();
        body.append_child(&host).unwrap();
        let bg = computed(&host, "background-color");
        let max = computed(&surface, "max-width");
        host.remove();
        if bg != "rgba(0, 0, 0, 0)" {
            failures.push(format!("{name}={value}: host background {bg}"));
        }
        if max != "none" {
            failures.push(format!("{name}={value}: host surface max-width {max}"));
        }
    }
    style.remove();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
