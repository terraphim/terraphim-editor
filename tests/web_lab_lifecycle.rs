//! Mechanical partition of original real Lab tests; same fixtures and assertions.
#![cfg(target_arch = "wasm32")]
mod support;
use support::*;
#[path = "support/lab_partition.rs"]
mod lab_partition;
use lab_partition::*;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_keyboard_navigation_mode_change_and_destroy_cleanup() {
    setup();
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = teLab.editor();
          const pill = teLab.pill();
          pill.focus();
          pill.click();
          const pop = teLab.popover();
          const items = Array.from(pop.querySelectorAll('[role="menuitemradio"]'));
          const active = () => items.indexOf(document.activeElement);
          if (pill.getAttribute('aria-expanded') !== 'true') out.push('aria-expanded not true');
          if (pill.getAttribute('aria-controls') !== pop.id) out.push('aria-controls');
          if (active() !== 0) out.push('first item not focused: ' + active());
          if (items.filter((b) => b.tabIndex === 0).length !== 1) out.push('roving tabindex');
          const key = (k) => teTest.key(document.activeElement, k);
          key('ArrowDown');
          if (active() !== 1) out.push('ArrowDown ' + active());
          key('End');
          if (active() !== 5) out.push('End ' + active());
          key('ArrowDown');
          if (active() !== 0) out.push('wrap down ' + active());
          key('ArrowUp');
          if (active() !== 5) out.push('wrap up ' + active());
          key('Home');
          if (active() !== 0) out.push('Home ' + active());
          if (items[0].tabIndex !== 0 || items[5].tabIndex !== -1) out.push('tabindex not moved');
          if (!pop.querySelector('.te-lab-hint').textContent) out.push('no hint for the focused action');

          // Escape closes and returns focus to the pill.
          key('Escape');
          if (!pop.hidden) out.push('Escape did not close');
          if (document.activeElement !== pill) out.push('focus not returned to the pill');
          if (pill.getAttribute('aria-expanded') !== 'false') out.push('aria-expanded not false');

          // Closed with marks: the chip says what is marked and clears.
          pill.click();
          // The cheap actions (long sentences, off-tone) keep this test light;
          // the first test covers every action against the engine.
          items[2].focus();
          items[2].click();
          if (document.activeElement !== items[2]) out.push('running an action moved focus out of the popover');
          key('Escape');
          const chip = ed.lab.chip;
          if (chip.hidden || !/2 long sentences/.test(chip.textContent)) out.push('chip ' + chip.hidden + ' ' + chip.textContent);
          chip.querySelector('button').focus();
          chip.querySelector('button').click();
          if (teLab.marks().length || !chip.hidden) out.push('chip clear');
          if (document.activeElement !== pill) out.push('chip clear did not hand focus to the pill');

          // A pointer press outside closes without moving focus.
          pill.click();
          ed.surface.root.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
          if (!pop.hidden) out.push('outside press did not close');

          // Leaving Write_On closes the popover and clears the marks.
          pill.click();
          items[4].click();
          if (!teLab.marks().length) out.push('no off-tone marks');
          ed.chrome.setMode('plain');
          if (!pop.hidden) out.push('plain mode left the popover open');
          if (teLab.marks().length) out.push('plain mode left marks');
          ed.chrome.setMode('write-on');

          // Destroy removes the DOM, the layer and every listener.
          pill.click();
          items[2].click();
          const lab = ed.lab;
          const surface = ed.surface;
          ed.destroy();
          if (document.querySelector('.te-lab, .te-lab-chip, .te-lab-descriptions')) out.push('DOM left after destroy');
          if (surface.getDecorations().some((d) => d.id.startsWith('lab-marks:'))) out.push('marks left after destroy');
          if (!lab.destroyed) out.push('not destroyed');
          if (pill.hasAttribute('aria-expanded')) out.push('pill attributes left');
          document.dispatchEvent(new CustomEvent('te:lab', { bubbles: true, detail: { editor: ed } }));
          if (lab.isOpen || lab.root.isConnected) out.push('reacted to te:lab after destroy');
          return out.join('\n');
        })()"##,
    );
    assert_eq!(result, "");
}

/// R-8.1, R-8.3 (issue #107), as in the video at 4:56 to 5:30: the five trim
/// buttons sit in one row, each with its target under its name and nothing
/// clipped; the actions are text only; the popover shows no description
/// line and no "TRIM" label, but both remain the menu's description and the
/// radio group's name.
#[wasm_bindgen_test]
fn test_popover_trim_row_and_hidden_texts_match_the_video() {
    setup();
    let result = js_string(
        r##"(() => {
          const out = [];
          teLab.pill().click();
          const pop = teLab.popover();
          if (pop.hidden) return 'popover did not open';
          const buttons = Array.from(pop.querySelectorAll('.te-trim-level'));
          if (buttons.length !== 5) return buttons.length + ' trim buttons';
          const rects = buttons.map((b) => b.getBoundingClientRect());
          if (new Set(rects.map((r) => Math.round(r.top))).size !== 1) out.push('trim buttons wrap: ' + JSON.stringify(rects.map((r) => r.top)));
          if (new Set(rects.map((r) => Math.round(r.width))).size !== 1) out.push('trim buttons unequal: ' + JSON.stringify(rects.map((r) => r.width)));
          for (const b of buttons) {
            for (const part of b.children) {
              if (part.scrollWidth > part.clientWidth + 0.5) out.push('clipped: ' + part.textContent);
            }
            const name = b.querySelector('.te-trim-level-name').getBoundingClientRect();
            const target = b.querySelector('.te-trim-level-target');
            if (target && !(target.getBoundingClientRect().top >= name.bottom - 1)) out.push('target not under the name: ' + b.textContent);
          }
          // The actions are text only, as in the video.
          if (pop.querySelector('.te-lab-action i')) out.push('action icons shown');
          const pr = pop.getBoundingClientRect();
          if (rects[0].left < pr.left || rects[4].right > pr.right) out.push('row overflows the popover');
          // Hidden, not removed: the menu's description and the group's name.
          const tiny = (el) => { const r = el.getBoundingClientRect(); return r.width <= 1 && r.height <= 1; };
          const hint = pop.querySelector('.te-lab-hint'), label = pop.querySelector('.te-trim-label');
          if (!tiny(hint)) out.push('description line visible');
          if (!tiny(label)) out.push('TRIM label visible');
          const menu = pop.querySelector('[role="menu"]');
          if (document.getElementById(menu.getAttribute('aria-describedby')) !== hint || !hint.textContent) out.push('menu description lost');
          const group = pop.querySelector('[role="radiogroup"]');
          if (document.getElementById(group.getAttribute('aria-labelledby')) !== label || label.textContent !== 'Trim') out.push('group name lost');
          teLab.pill().click();
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
