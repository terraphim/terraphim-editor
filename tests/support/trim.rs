//! Fixture, engine expectations and page helpers shared by the trim browser
//! binaries (`tests/web_trim.rs`, `tests/web_trim_cuts.rs` and
//! `tests/web_trim_done.rs`, issue #15).

use super::*;
pub use terraphim_lab::{make_cuts, trim_plan, Cut, CutId, LabConfig, TrimLevel, TrimPlan};

pub const FIXTURE: &str = include_str!("../fixtures/lab/lab.md");

pub fn plan() -> TrimPlan {
    trim_plan(
        FIXTURE,
        &LabConfig::with_defaults().expect("default Lab config"),
    )
}

pub fn level_id(level: TrimLevel) -> String {
    serde_json::to_value(level)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

pub fn pieces(cuts: &[Cut]) -> serde_json::Value {
    cuts.iter()
        .map(|c| serde_json::json!([c.id.0, c.start, c.end]))
        .collect::<Vec<_>>()
        .into()
}

/// `{ level: { card, label, pieces } }` from the engine, as JSON.
pub fn expected_levels() -> String {
    let plan = plan();
    let mut out = serde_json::Map::new();
    for level in &TrimLevel::ALL[1..] {
        out.insert(
            level_id(*level),
            serde_json::json!({
                "card": plan.status(*level, &[]).card_text(),
                "label": level.label(),
                "pieces": pieces(&plan.active(*level, &[])),
            }),
        );
    }
    serde_json::Value::Object(out).to_string()
}

/// The UTF-16 offset of the first `needle` in the fixture.
pub fn utf16_at(needle: &str) -> usize {
    FIXTURE[..FIXTURE.find(needle).expect("needle in fixture")]
        .encode_utf16()
        .count()
}

/// Fresh editor, fixture open, Write_On on, helpers on `window.teTrim`.
pub fn setup() {
    let _document = fresh_full_editor();
    let src = format!(
        r##"(() => {{
          window.teTrim = {{
            fixture: {fixture},
            trim() {{ return window.__teEditor.lab.trim; }},
            pill() {{ return teTest.control('lab'); }},
            open() {{ if (!window.__teEditor.lab.isOpen) teTrim.pill().click(); return window.__teEditor.lab.root; }},
            button(level) {{ return teTrim.trim().slot.querySelector('[data-level="' + level + '"]'); }},
            pick(level) {{ teTrim.open(); teTrim.button(level).click(); return teTrim.trim(); }},
            layer() {{ return window.__teEditor.decorations.get('lab-trim'); }},
            pieces() {{ return teTrim.layer().map((i) => [i.data.cut, i.start, i.end]); }},
            card() {{ return teTrim.trim().card; }},
            cardCount() {{ return teTrim.card().querySelector('.te-trim-card-count').textContent; }},
            ghosts() {{ return JSON.stringify(window.__teEditor.annotations().ghosts.map((g) => [g.anchor.start, g.anchor.end])); }},
            segment(cut, start) {{
              return window.__teEditor.surface.root.querySelector('[data-te-decoration~="lab-trim:' + cut + '-' + start + '"]');
            }},
            click(el) {{ el.dispatchEvent(new MouseEvent('click', {{ bubbles: true, cancelable: true }})); }},
          }};
          const ed = window.__teEditor;
          ed.openDocument(teTrim.fixture, 'trim-test-' + Math.random());
          ed.chrome.setMode('write-on');
          return ed.lab && ed.lab.trim ? 'ok' : 'no trim';
        }})()"##,
        fixture = js_string_literal(FIXTURE),
    );
    assert_eq!(js_string(&src), "ok");
}
