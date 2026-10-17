//! Actual-DOM witness for the shown heading-marker / manual ghost cascade.
//! Compile-only preparation; browser qualification is a separate serial gate.
#![cfg(target_arch = "wasm32")]

mod support;
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn shown_manual_heading_ghost_preserves_fade_and_marker_lifecycle() {
    let _document = fresh_full_editor();
    let result = run_steps(&[
        r##"T.errors = [];
        T.body = '# heading\n';
        ed.openDocument(T.body, 'heading-ghost-control');
        ed.chrome.setMode('write-on');
        // Production selection and heading layer determine shown classes.
        T.select = at => {
          s.focus(); s.setSelectionOffsets(at, at);
          ed.headings.flush();
        };
        T.marker = () => s.root.querySelector('[data-te-decoration~="headings:m0"]');
        T.alpha = color => {
          const slash = color.match(/\/\s*([\d.]+)\s*\)$/);
          const rgba = color.match(/^rgba\(([^)]+)\)$/);
          return slash ? Number(slash[1]) : rgba ? Number(rgba[1].split(',')[3]) : 1;
        };
        T.check = (phase, ghost, shown) => {
          const el = T.marker(), errors = T.errors;
          if (!el) { errors.push(phase + ': marker missing'); return; }
          const style = getComputedStyle(el), rect = el.getBoundingClientRect();
          // Retain only class/geometry/color metadata, not document text.
          console.log('heading-ghost ' + JSON.stringify({phase,
            classes: Array.from(el.classList), connected: el.isConnected,
            rects: el.getClientRects().length, width: rect.width, height: rect.height,
            color: style.color, alpha: T.alpha(style.color), fontSize: style.fontSize}));
          if (!el.classList.contains('te-heading') || !el.classList.contains('te-heading-marker') ||
              el.classList.contains('te-heading-marker--shown') !== shown ||
              el.classList.contains('te-ghost') !== ghost) errors.push(phase + ': same-span classes');
          if (ghost && !(el.getAttribute('data-te-decoration') || '').split(/\s+/).includes('ghosts:' + T.id)) errors.push(phase + ': native ghost token');
          if (!el.isConnected || !el.getClientRects().length) errors.push(phase + ': detached marker');
          if (shown) {
            if (rect.width <= 0 || rect.height <= 0 || style.visibility !== 'visible' || parseFloat(style.fontSize) <= 0) errors.push(phase + ': visible geometry');
            if (ghost && Math.abs(T.alpha(style.color) - 0.1) > 0.02) errors.push(phase + ': expected alpha 0.1 +/- 0.02, got ' + style.color);
            if (!ghost) {
              // Resolve the real dim token through CSSOM, without writing classes.
              const probe = document.createElement('span');
              probe.style.color = 'var(--te-color-text-dim)'; s.root.parentElement.appendChild(probe);
              const dim = getComputedStyle(probe).color; probe.remove();
              if (style.color !== dim || T.alpha(style.color) !== 1) errors.push(phase + ': ordinary marker not opaque dim');
            }
          } else if (parseFloat(style.fontSize) !== 0 || rect.width !== 0) errors.push(phase + ': hidden marker occupies width');
        };
        T.select(3);
        T.check('ordinary-shown', false, true);
        // Real native ghost API, not synthetic decoration/class injection.
        ed.ghosts.ghost(0, 1); ed.ghosts.flush();
        const ghosts = ed.annotations().ghosts;
        if (ghosts.length !== 1 || ghosts[0].anchor.start !== 0 || ghosts[0].anchor.end !== 1 || ghosts[0].anchor.text !== '#' || typeof ghosts[0].id !== 'string') return 'native manual ghost setup';
        T.id = ghosts[0].id;
        T.annotations = JSON.stringify(ghosts);
        T.select(3);"##,
        r##"T.check('ghost-shown', true, true);
        T.saved = ed.saveDocument();
        // The trailing empty line is beyond the heading's inclusive end.
        T.select(T.body.length);"##,
        r##"T.check('ghost-hidden-after-move', true, false);
        if (s.getText() !== T.body || JSON.stringify(ed.annotations().ghosts) !== T.annotations) T.errors.push('selection changed native body/ghost');
        ed.openDocument(T.saved, 'heading-ghost-reopen');
        ed.chrome.setMode('write-on');
        ed.ghosts.flush();
        if (s.getText() !== T.body || ed.documentApi().document_body() !== T.body || JSON.stringify(ed.annotations().ghosts) !== T.annotations || ed.saveDocument() !== T.saved) T.errors.push('save/reopen changed body/ghost identity');
        T.select(3);"##,
        r##"T.check('reopen-ghost-shown', true, true);
        return T.errors.join('; ');"##,
    ])
    .await;
    assert_eq!(result, "");
}
