/*
 * Write_On mode toggle and corner chrome (issue #7, spec R-2.1, R-7.1-R-7.3)
 * ==========================================================================
 *
 * The plain editor is the default. The only addition in plain mode is a dim
 * `N words M chars` counter in the top-left corner. Clicking the counter
 * toggles Write_On mode, which sets `data-mode="write-on"` on <body> (the
 * scope used by public/css/tokens.css and public/css/write-on.css) and
 * reveals the corner controls:
 *
 *   top-left       N words M chars   toggles Write_On mode (always visible)
 *   top-centre     ●●●               alternatives panel   -> te:open-panel {panel: 'alternatives'}
 *   top-centre     M↓                Markdown view/export -> te:markdown
 *   top-right      keyboard glyph    shortcut reference   -> opens the reference, then te:shortcuts
 *   bottom-left    floppy            save                 -> te:save, then editor.saveDocument() if present
 *   bottom-left    folder            open                 -> te:open
 *   bottom-centre  LAB               Lab                  -> te:lab
 *   bottom-right   XYZ               Overflow panel       -> te:overflow
 *
 * Events are CustomEvents dispatched from the chrome root inside #app; they
 * bubble, so listen on `document`. `detail.editor` is the MarkdownEditor.
 * te:save is cancelable: call preventDefault() to stop the fallback call to
 * editor.saveDocument(), which dispatches te:saved {text} with the saved
 * document (save() also returns it). te:open carries no payload because
 * editor.openDocument(text) needs the text; the owner of the open flow
 * (issue #6) reads the file and then calls openDocument() followed by
 * chrome.documentChanged(). Every mode change dispatches
 * te:mode-change {mode: 'plain' | 'write-on'}.
 *
 * Toggle persistence (per document)
 * ---------------------------------
 * The mode is stored in localStorage under
 *   terraphim-editor:write-on:<document key>
 * as "1" (Write_On) or "0" (plain); absence means plain. The document key is
 * the first of these that is a non-empty string:
 *   1. editor.documentKey     (the save key or file name, set by persistence)
 *   2. editor.config.documentKey
 *   3. "h:" + FNV-1a 32-bit hash (hex) of the surface text when the chrome is
 *      created, or when documentChanged() is called.
 * localStorage access is wrapped in try/catch, so private browsing or a full
 * quota degrades to an unpersisted toggle. Storing the mode inside the
 * document (the annotation block) is out of scope.
 *
 * Lifecycle: MarkdownEditor creates the chrome in initialize() and passes its
 * AbortController signal; every listener is registered with that signal, and
 * MarkdownEditor.destroy() calls chrome.destroy(), which removes the DOM,
 * the change subscription and the <body> mode attribute.
 */

const WRITE_ON_STORAGE_PREFIX = 'terraphim-editor:write-on:';

/** Selection shortcuts from R-7.3; the editor shortcuts come from config.js. */
const WRITE_ON_SELECTION_SHORTCUTS = [
  { key: 'ctrl+shift+a', desc: 'Alternatives for selection' },
  { key: 'ctrl+shift+g', desc: 'AI alternatives for selection' },
  { key: 'ctrl+/', desc: 'Ghost it / Revive' },
  { key: 'ctrl+shift+x', desc: 'Stash this in Overflow' },
];

/**
 * Whether the editor's Rust document model (issue #6) is usable. Without
 * `window.wasmBindings` the editor still works as a plain Markdown editor
 * (see MarkdownEditor.documentApi()), and the chrome degrades with it.
 */
function documentModelAvailable(editor) {
  try {
    return !!(editor && typeof editor.documentApi === 'function' && editor.documentApi());
  } catch (e) {
    return false;
  }
}

/**
 * Word and character counts for an editor. Uses editor.counts() (issue #6,
 * which includes ghosted text) when the document model is available and
 * falls back to counting the surface text otherwise, or if counts() fails.
 * Chars are Unicode code points. Words follow the editor's one word
 * definition (terraphim_alternatives::words, issue #59), which WORD_RE mirrors
 * for the fallback: runs of letters, digits or `_`, joined by an apostrophe,
 * hyphen, full stop or slash only between word characters. Markdown syntax,
 * punctuation and emoji are not words.
 */
const WORD_RE = /[\p{Alphabetic}\p{N}_]+(?:['\u2019\-./][\p{Alphabetic}\p{N}_]+)*/gu;

function countsFor(editor) {
  if (documentModelAvailable(editor) && typeof editor.counts === 'function') {
    try {
      const c = editor.counts();
      if (c && Number.isFinite(c.words) && Number.isFinite(c.chars)) {
        return { words: c.words, chars: c.chars };
      }
    } catch (e) {
      // Fall through to the surface text.
    }
  }
  const text = editor && editor.surface ? editor.surface.getText() : '';
  const words = (text.match(WORD_RE) || []).length;
  return { words, chars: Array.from(text).length };
}

/** FNV-1a 32-bit hash of a string, as eight hex digits. */
function writeOnHash(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, '0');
}

/** Render `ctrl+shift+a` as `Ctrl+Shift+A`. */
function formatShortcutKey(key) {
  return key
    .split('+')
    .map((part) => (part.length === 1 ? part.toUpperCase() : part.charAt(0).toUpperCase() + part.slice(1)))
    .join('+');
}

class WriteOnChrome {
  constructor(editor, options = {}) {
    this.editor = editor;
    this.signal = options.signal;
    this.mount = options.mount || (editor.input && editor.input.closest('.editor-container')) || document.body;
    this.destroyed = false;
    this.mode = 'plain';
    this.ownsBodyMode = false;

    this.build();
    this.offChange = editor.surface ? editor.surface.onChange(() => this.refresh()) : () => {};
    this.documentChanged();
  }

  // ---------------------------------------------------------------------
  // DOM
  // ---------------------------------------------------------------------

  build() {
    const listen = (el, type, fn) => el.addEventListener(type, fn, { signal: this.signal });

    const root = document.createElement('div');
    root.className = 'te-chrome';
    this.root = root;

    const counter = document.createElement('button');
    counter.type = 'button';
    counter.className = 'te-chrome-counter';
    counter.setAttribute('aria-pressed', 'false');
    counter.title = 'Toggle Write_On mode';
    const counterText = document.createElement('span');
    counterText.className = 'te-chrome-counter-text';
    const arrow = document.createElement('span');
    arrow.className = 'te-chrome-counter-arrow';
    arrow.setAttribute('aria-hidden', 'true');
    arrow.textContent = '←'; // left arrow, lit in Write_On mode
    counter.append(counterText, arrow);
    listen(counter, 'click', () => this.toggle());
    this.counter = counter;
    this.counterText = counterText;

    const corners = document.createElement('div');
    corners.className = 'te-chrome-corners';
    corners.hidden = true;
    this.corners = corners;

    const group = (where) => {
      const g = document.createElement('div');
      g.className = `te-chrome-group te-chrome-${where}`;
      corners.appendChild(g);
      return g;
    };
    const control = (parent, opts) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = `te-chrome-control ${opts.className}`;
      b.dataset.control = opts.name;
      b.setAttribute('aria-label', opts.label);
      b.title = opts.label;
      if (opts.icon) {
        const i = document.createElement('i');
        i.className = opts.icon;
        i.setAttribute('aria-hidden', 'true');
        b.appendChild(i);
      } else {
        const span = document.createElement('span');
        span.setAttribute('aria-hidden', 'true');
        span.textContent = opts.text;
        b.appendChild(span);
      }
      // Corner controls act only in Write_On mode; when plain they are
      // hidden, and a programmatic click must not dispatch te:* events.
      listen(b, 'click', (e) => {
        if (!this.isWriteOn()) return;
        opts.onClick(e);
      });
      parent.appendChild(b);
      return b;
    };

    const top = group('top-centre');
    control(top, {
      name: 'alternatives', className: 'te-chrome-dots', text: '●●●',
      label: 'Alternatives panel',
      onClick: () => this.emit('te:open-panel', { panel: 'alternatives' }),
    });
    control(top, {
      name: 'markdown', className: 'te-chrome-markdown', text: 'M↓',
      label: 'Markdown view and export',
      onClick: () => this.emit('te:markdown'),
    });

    const topRight = group('top-right');
    control(topRight, {
      name: 'shortcuts', className: 'te-chrome-keyboard', icon: 'fa-regular fa-keyboard',
      label: 'Keyboard shortcuts',
      onClick: () => this.openShortcuts(),
    });

    const bottomLeft = group('bottom-left');
    control(bottomLeft, {
      name: 'save', className: 'te-chrome-save', icon: 'fa-solid fa-floppy-disk',
      label: 'Save document',
      onClick: () => this.save(),
    });
    control(bottomLeft, {
      name: 'open', className: 'te-chrome-open', icon: 'fa-regular fa-folder-open',
      label: 'Open document',
      onClick: () => this.emit('te:open'),
    });

    const bottomCentre = group('bottom-centre');
    control(bottomCentre, {
      name: 'lab', className: 'te-chrome-lab', text: 'LAB',
      label: 'Lab',
      onClick: () => this.emit('te:lab'),
    });

    const bottomRight = group('bottom-right');
    control(bottomRight, {
      name: 'overflow', className: 'te-chrome-overflow', text: 'XYZ',
      label: 'Overflow panel',
      onClick: () => this.emit('te:overflow'),
    });

    root.append(counter, corners, this.buildShortcutReference(listen));
    this.mount.appendChild(root);
  }

  /** The shortcut reference: every R-7.3 shortcut and every config.js shortcut. */
  shortcutEntries() {
    const config = (this.editor && this.editor.config) || {};
    const editorShortcuts = (config.shortcuts || []).map((s) => ({ key: s.key, desc: s.desc }));
    return [
      { title: 'Formatting', items: editorShortcuts },
      { title: 'Selection', items: config.writeOnShortcuts || WRITE_ON_SELECTION_SHORTCUTS },
      { title: 'Editor', items: [{ key: '/', desc: 'Command palette' }] },
    ];
  }

  buildShortcutReference(listen) {
    const dialog = document.createElement('dialog');
    dialog.className = 'te-chrome-shortcuts';
    dialog.setAttribute('aria-labelledby', 'te-chrome-shortcuts-title');

    const header = document.createElement('div');
    header.className = 'te-chrome-shortcuts-header';
    const title = document.createElement('h2');
    title.id = 'te-chrome-shortcuts-title';
    title.textContent = 'Keyboard shortcuts';
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'te-chrome-shortcuts-close';
    close.setAttribute('aria-label', 'Close keyboard shortcuts');
    close.textContent = '×';
    listen(close, 'click', () => dialog.close());
    header.append(title, close);
    dialog.appendChild(header);

    for (const section of this.shortcutEntries()) {
      const h = document.createElement('h3');
      h.textContent = section.title;
      const list = document.createElement('dl');
      for (const item of section.items) {
        const row = document.createElement('div');
        row.className = 'te-chrome-shortcut';
        row.dataset.key = item.key;
        const dt = document.createElement('dt');
        dt.textContent = item.desc;
        const dd = document.createElement('dd');
        const kbd = document.createElement('kbd');
        kbd.textContent = formatShortcutKey(item.key);
        dd.appendChild(kbd);
        row.append(dt, dd);
        list.appendChild(row);
      }
      dialog.append(h, list);
    }

    // A click on the backdrop (outside the dialog box) closes it.
    listen(dialog, 'click', (e) => {
      if (e.target === dialog) dialog.close();
    });
    this.shortcutsDialog = dialog;
    return dialog;
  }

  // ---------------------------------------------------------------------
  // Behaviour
  // ---------------------------------------------------------------------

  emit(type, detail = {}, cancelable = false) {
    const ev = new CustomEvent(type, {
      bubbles: true,
      cancelable,
      detail: Object.assign({ editor: this.editor }, detail),
    });
    this.root.dispatchEvent(ev);
    return ev;
  }

  /**
   * Dispatch te:save; unless it is cancelled, call editor.saveDocument() and
   * return the saved text. Returns null, without throwing, when te:save was
   * cancelled or the document model is unavailable (no `window.wasmBindings`:
   * there is nothing to serialise the annotations with, so te:save is still
   * dispatched for listeners but no save happens and no te:saved follows).
   * The editor dispatches te:saved with the text after a real save.
   */
  save() {
    const ev = this.emit('te:save', { available: documentModelAvailable(this.editor) }, true);
    if (ev.defaultPrevented || !documentModelAvailable(this.editor)) return null;
    if (typeof this.editor.saveDocument !== 'function') return null;
    try {
      return this.editor.saveDocument();
    } catch (err) {
      console.error('saveDocument failed', err);
      return null;
    }
  }

  openShortcuts() {
    const dialog = this.shortcutsDialog;
    if (!dialog.open) {
      if (typeof dialog.showModal === 'function') dialog.showModal();
      else dialog.setAttribute('open', '');
    }
    this.emit('te:shortcuts');
  }

  /** Update the counter text from countsFor(). */
  refresh() {
    if (this.destroyed) return;
    const { words, chars } = countsFor(this.editor);
    this.counterText.textContent = `${words} ${words === 1 ? 'word' : 'words'} ${chars} ${chars === 1 ? 'char' : 'chars'}`;
  }

  /** The document identity used for the persisted toggle (see header). */
  documentKey() {
    const ed = this.editor || {};
    if (typeof ed.documentKey === 'string' && ed.documentKey) return ed.documentKey;
    const configKey = ed.config && ed.config.documentKey;
    if (typeof configKey === 'string' && configKey) return configKey;
    return `h:${writeOnHash(this.initialText)}`;
  }

  storageKey() {
    return WRITE_ON_STORAGE_PREFIX + this.key;
  }

  /**
   * Re-key after a different document is loaded (e.g. after
   * editor.openDocument()), then apply that document's stored mode.
   */
  documentChanged() {
    if (this.destroyed) return;
    this.initialText = this.editor.surface ? this.editor.surface.getText() : '';
    this.key = this.documentKey();
    let stored = null;
    try {
      stored = window.localStorage.getItem(this.storageKey());
    } catch (e) {
      stored = null;
    }
    this.setMode(stored === '1' ? 'write-on' : 'plain', { persist: false });
    this.refresh();
  }

  isWriteOn() {
    return this.mode === 'write-on';
  }

  toggle() {
    this.setMode(this.isWriteOn() ? 'plain' : 'write-on');
  }

  setMode(mode, { persist = true } = {}) {
    if (this.destroyed) return;
    const next = mode === 'write-on' ? 'write-on' : 'plain';
    const changed = next !== this.mode;
    this.mode = next;
    const on = next === 'write-on';
    if (on) {
      document.body.dataset.mode = 'write-on';
      this.ownsBodyMode = true;
    } else if (this.ownsBodyMode) {
      delete document.body.dataset.mode;
      this.ownsBodyMode = false;
    }
    this.root.classList.toggle('te-chrome-on', on);
    this.corners.hidden = !on;
    this.counter.setAttribute('aria-pressed', on ? 'true' : 'false');
    if (!on && this.shortcutsDialog.open) this.shortcutsDialog.close();
    if (persist) {
      try {
        window.localStorage.setItem(this.storageKey(), on ? '1' : '0');
      } catch (e) {
        // Storage unavailable: the toggle still works for this session.
      }
    }
    if (changed) this.emit('te:mode-change', { mode: next });
  }

  /** Remove the chrome DOM, the change subscription and the body mode. */
  destroy() {
    if (this.destroyed) return;
    if (this.shortcutsDialog && this.shortcutsDialog.open) this.shortcutsDialog.close();
    if (this.ownsBodyMode) {
      delete document.body.dataset.mode;
      this.ownsBodyMode = false;
    }
    this.destroyed = true;
    this.offChange();
    this.root.remove();
  }
}

window.WriteOnChrome = WriteOnChrome;
window.countsFor = countsFor;
window.documentModelAvailable = documentModelAvailable;
window.WRITE_ON_STORAGE_PREFIX = WRITE_ON_STORAGE_PREFIX;
