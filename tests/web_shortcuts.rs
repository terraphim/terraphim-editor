//! Browser tests for R-7.4 (issue #85): the configured editor shortcuts and
//! the Write_On shortcuts never collide, with Shift taken into account. Run
//! with `wasm-pack test --headless --chrome --test web_shortcuts`. Real
//! scripts, real DOM and real configuration; nothing is mocked. See
//! `tests/web.rs` for why the browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

/// `editor_config.toml` is named by R-7.4 but not read by the code; its keys
/// are checked as well so the two configuration files cannot drift into a
/// collision.
const EDITOR_CONFIG_TOML: &str = include_str!("../editor_config.toml");

fn toml_shortcut_keys() -> Vec<String> {
    EDITOR_CONFIG_TOML
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == "key").then(|| value.trim().trim_matches('"').to_string())
        })
        .collect()
}

/// Rebuild the editor (as `fresh_full_editor` does) with `extra` appended to
/// the configured shortcuts.
fn editor_with_extra_shortcuts(extra: &str) {
    let _document = fresh_full_editor();
    let script = format!(
        r##"(() => {{
          window.__teEditor.destroy();
          teTest.resetWriteOn();
          teTest.resetBlocks();
          teTest.resetDrafts();
          const config = Object.assign({{}}, window.EditorConfig, {{
            shortcuts: window.EditorConfig.shortcuts.concat({extra}),
          }});
          const ed = new MarkdownEditor(config);
          ed.initialize();
          window.__teEditor = ed;
          return ed.surface ? 'ok' : 'no surface';
        }})()"##
    );
    assert_eq!(js_string(&script), "ok");
}

#[wasm_bindgen_test]
fn test_configured_and_write_on_shortcuts_are_disjoint() {
    let _document = fresh_full_editor();
    let toml_keys = serde_json::to_string(&toml_shortcut_keys()).unwrap();
    let script = format!(
        r##"(() => {{
          const ed = window.__teEditor;
          ed.chrome.setMode('write-on');
          const out = [];
          // One spelling per chord: modifiers in a fixed order, key
          // lower-cased ('Ctrl+Shift+A' -> 'ctrl+shift+a'). Shift counts for
          // letters and digits only, as in teConfigShortcutMatches, so a
          // config 'ctrl+shift+/' collides with Ctrl+/ as it would at runtime.
          const order = ['ctrl', 'alt', 'shift', 'meta'];
          const norm = (key) => {{
            const parts = String(key).toLowerCase().split('+').filter(Boolean);
            const k = parts.pop();
            const shiftCounts = /^[a-z0-9]$/.test(k);
            return order.filter((m) => parts.includes(m) && (m !== 'shift' || shiftCounts)).concat(k).join('+');
          }};
          if (norm('Shift+Ctrl+/') !== 'ctrl+/' || norm('ctrl+shift+X') !== 'ctrl+shift+x') out.push('normaliser');
          const tomlKeys = {toml_keys};
          if (tomlKeys.length < 5) out.push('expected the five editor_config.toml keys, got ' + JSON.stringify(tomlKeys));
          const configured = window.EditorConfig.shortcuts.map((s) => s.key).concat(tomlKeys).map(norm);
          if (window.EditorConfig.shortcuts.length < 5) out.push('expected the five configured shortcuts');
          // The same chord may appear in both files, but only for one action.
          const byKey = new Map();
          for (const s of window.EditorConfig.shortcuts) {{
            const k = norm(s.key);
            if (byKey.has(k)) out.push('config.js binds ' + k + ' twice');
            byKey.set(k, s);
          }}
          // Write_On: the selection-menu items as registered on the live
          // editor, the chrome's shortcut reference, and the keys the
          // panels, cycling and pull-back handle directly.
          const menuKeys = ed.selectionMenu.items.map((it) => it.key).filter(Boolean).map(norm).sort();
          const wantMenu = ['ctrl+/', 'ctrl+shift+a', 'ctrl+shift+g', 'ctrl+shift+x'];
          if (JSON.stringify(menuKeys) !== JSON.stringify(wantMenu)) out.push('menu keys ' + JSON.stringify(menuKeys));
          const reference = WRITE_ON_SELECTION_SHORTCUTS.map((s) => norm(s.key)).sort();
          if (JSON.stringify(reference) !== JSON.stringify(menuKeys)) out.push('shortcut reference ' + JSON.stringify(reference));
          const direct = [
            // In-place cycling: hover plus arrows, Alt+arrows at the caret.
            'arrowup', 'arrowdown', 'alt+arrowup', 'alt+arrowdown',
            // Overflow pull-back and close.
            'ctrl+enter', 'meta+enter', 'escape',
            // Alternatives panel keys.
            'enter', 'arrowleft', 'arrowright', 'home', 'end',
            'ctrl+z', 'ctrl+shift+z', 'ctrl+y', 'meta+z', 'meta+shift+z', 'meta+y',
            // Opening the selection menu from the keyboard.
            'contextmenu', 'shift+f10',
            // The plain-mode tools toggle (issue #86).
            'ctrl+shift+l',
            // The Overflow panel toggle (R-6.1, the XYZ tooltip at 3:52).
            'ctrl+shift+d',
          ].map(norm);
          if (TOOLS_TOGGLE_KEY !== 'ctrl+shift+l') out.push('tools toggle key ' + TOOLS_TOGGLE_KEY);
          if (OVERFLOW_TOGGLE_KEY !== 'ctrl+shift+d') out.push('overflow toggle key ' + OVERFLOW_TOGGLE_KEY);
          // The panel toggles are distinct from each other and from the menu.
          const toggles = [TOOLS_TOGGLE_KEY, OVERFLOW_TOGGLE_KEY].map(norm);
          for (const k of toggles) {{
            if (menuKeys.includes(k)) out.push('toggle collides with the menu: ' + k);
          }}
          if (new Set(toggles).size !== toggles.length) out.push('toggles collide');
          const writeOn = new Set(menuKeys.concat(reference, direct));
          for (const k of configured) {{
            if (writeOn.has(k)) out.push('collision: ' + k);
          }}
          return out.join('; ');
        }})()"##
    );
    assert_eq!(js_string(&script), "");
}

#[wasm_bindgen_test]
fn test_config_shortcut_does_not_fire_on_write_on_shift_chord() {
    // A config entry on the unshifted letter of each R-7.3 Ctrl+Shift item:
    // it must fire on Ctrl+<letter> only, never on Ctrl+Shift+<letter>.
    editor_with_extra_shortcuts(
        r#"[
          { name: 'type-strikethrough', key: 'ctrl+x', prefix: '~~', suffix: '~~', desc: 'Strike' },
          { name: 'type-strikethrough', key: 'ctrl+a', prefix: '<a>', suffix: '</a>', desc: 'A' },
          { name: 'type-strikethrough', key: 'ctrl+g', prefix: '<g>', suffix: '</g>', desc: 'G' }
        ]"#,
    );
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const s = ed.surface;
          const out = [];
          for (const mode of ['plain', 'write-on']) {
            ed.chrome.setMode(mode);
            for (const [letter, prefix] of [['x', '~~'], ['a', '<a>'], ['g', '<g>']]) {
              // The browser reports the shifted letter in upper case.
              s.setText('hello world');
              s.focus();
              s.setSelectionOffsets(6, 11);
              teTest.key(s.root, letter.toUpperCase(), { ctrlKey: true, shiftKey: true, code: 'Key' + letter.toUpperCase() });
              if (s.getText().includes(prefix)) out.push(mode + ' ctrl+shift+' + letter + ' fired the config entry: ' + JSON.stringify(s.getText()));
              if (ed.altPanel && ed.altPanel.close) ed.altPanel.close();
              if (ed.selectionMenu.close) ed.selectionMenu.close();
              // The unshifted chord still runs the config entry.
              s.setText('hello world');
              s.focus();
              s.setSelectionOffsets(6, 11);
              const prevented = teTest.key(s.root, letter, { ctrlKey: true, code: 'Key' + letter.toUpperCase() });
              const suffix = prefix === '~~' ? '~~' : prefix.replace('<', '</');
              const want = 'hello ' + prefix + 'world' + suffix;
              if (!prevented) out.push(mode + ' ctrl+' + letter + ': default not prevented');
              if (s.getText() !== want) out.push(mode + ' ctrl+' + letter + ': got ' + JSON.stringify(s.getText()));
            }
          }
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_config_shortcuts_need_their_exact_modifiers() {
    editor_with_extra_shortcuts(
        r#"[
          { name: 'type-underline', key: 'Ctrl+Shift+U', prefix: '<u>', suffix: '</u>', desc: 'Underline' },
          { name: 'quote', key: 'ctrl+]', prefix: '[', suffix: ']', desc: 'Bracket' }
        ]"#,
    );
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          const attempt = (label, key, opts, want) => {
            s.setText('hello world');
            s.setSelectionOffsets(6, 11);
            const prevented = teTest.key(s.root, key, opts);
            if (s.getText() !== want) out.push(label + ': got ' + JSON.stringify(s.getText()));
            if (prevented !== (want !== 'hello world')) out.push(label + ': prevented ' + prevented);
          };
          // A configured Shift chord (written in any case) fires with Shift only.
          attempt('ctrl+shift+u', 'U', { ctrlKey: true, shiftKey: true }, 'hello <u>world</u>');
          attempt('ctrl+u', 'u', { ctrlKey: true }, 'hello world');
          // ctrl+b is Ctrl+B exactly: not with Shift, Alt or Cmd added.
          attempt('ctrl+b', 'b', { ctrlKey: true }, 'hello **world**');
          attempt('ctrl+shift+b', 'B', { ctrlKey: true, shiftKey: true }, 'hello world');
          attempt('ctrl+alt+b', 'b', { ctrlKey: true, altKey: true }, 'hello world');
          attempt('ctrl+meta+b', 'b', { ctrlKey: true, metaKey: true }, 'hello world');
          attempt('plain b', 'b', {}, 'hello world');
          // Punctuation ignores Shift, which some layouts need to type it.
          attempt('ctrl+]', ']', { ctrlKey: true }, 'hello [world]');
          attempt('ctrl+shift+]', ']', { ctrlKey: true, shiftKey: true }, 'hello [world]');
          attempt('ctrl+alt+]', ']', { ctrlKey: true, altKey: true }, 'hello world');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_tools_toggle_needs_exactly_ctrl_shift_l() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const s = ed.surface;
          const chrome = ed.chrome;
          const out = [];
          for (const mode of ['plain', 'write-on']) {
            chrome.setMode(mode);
            chrome.setTools(false);
            // Ctrl+Shift+L, in either case, toggles the tools and leaves the text.
            for (const key of ['L', 'l']) {
              s.setText('hello world');
              s.focus();
              s.setSelectionOffsets(6, 11);
              const prevented = teTest.key(s.root, key, { ctrlKey: true, shiftKey: true, code: 'KeyL' });
              if (!prevented) out.push(mode + ' ctrl+shift+' + key + ': default not prevented');
              if (!chrome.toolsVisible()) out.push(mode + ' ctrl+shift+' + key + ': tools not on');
              if (s.getText() !== 'hello world') out.push(mode + ' ctrl+shift+' + key + ': text ' + JSON.stringify(s.getText()));
              chrome.setTools(false);
            }
            // Any other modifier set leaves the tools alone.
            for (const [label, key, opts] of [
              ['ctrl+alt+shift+l', 'L', { ctrlKey: true, altKey: true, shiftKey: true }],
              ['ctrl+meta+shift+l', 'L', { ctrlKey: true, metaKey: true, shiftKey: true }],
              ['meta+shift+l', 'L', { metaKey: true, shiftKey: true }],
              ['shift+l', 'L', { shiftKey: true }],
              ['ctrl+l', 'l', { ctrlKey: true }],
            ]) {
              s.setText('hello world');
              s.focus();
              s.setSelectionOffsets(6, 11);
              teTest.key(s.root, key, Object.assign({ code: 'KeyL' }, opts));
              if (chrome.toolsVisible()) out.push(mode + ' ' + label + ' toggled the tools');
              chrome.setTools(false);
            }
          }
          chrome.setMode('plain');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
