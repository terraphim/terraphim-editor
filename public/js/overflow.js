/*
 * Overflow panel: stash, pull back, drag into the page (issue #12; spec
 * R-6.1 to R-6.5, R-7.3)
 * =====================================================================
 *
 * TeOverflowPanel (`editor.overflow`)
 * -----------------------------------
 * A right-hand panel (`<aside role="complementary">`, labelled by its
 * "Overflow" title in the cursive display face, lavender) holding one
 * free-form monospace text area (R-6.1, R-6.2). It is toggled by the XYZ
 * corner control (the chrome dispatches `te:overflow`), which gets
 * `aria-expanded` and `aria-controls`, and by Ctrl+Shift+D (exactly those
 * modifiers; the chrome's OVERFLOW_TOGGLE_KEY, shown in the XYZ tooltip as in
 * the demo at 3:52) from anywhere in the editor or the panel, in Write_On
 * mode only. As in the demo, the panel runs full height against the right
 * edge and the corner controls stay on top of it: the keyboard glyph opens
 * the shortcut reference and the XYZ tag, lit while the panel is open,
 * closes it.
 *
 *   editor.overflow.open({ focus })    // -> true when shown (Write_On only)
 *   editor.overflow.close({ restoreFocus })
 *   editor.overflow.toggle()
 *   editor.overflow.isOpen()
 *   editor.overflow.text()             // the overflow text in the model
 *   editor.overflow.stash(start, end)  // R-6.3, a move (see below)
 *   editor.overflow.pullBack()         // R-6.4, Ctrl+Enter (see below)
 *   editor.overflow.flush()            // write waiting panel input now
 *   editor.overflow.refresh()          // re-read the model into the panel
 *
 * Data. The overflow is document state, kept by the WASM model
 * (`Annotations::overflow`), saved in the annotation block and never
 * exported (R-6.5, R-9.3). What the author types is written to the model
 * after a short debounce (`delay`, default 250 ms), on blur, and before any
 * model read (MarkdownEditor.alignDocumentModel calls flush(), which costs
 * nothing when no input is waiting), so a save always includes it. Typing in
 * the panel is not part of the document's undo history; the text area keeps
 * its own native undo.
 *
 * Modes. The panel UI exists only in Write_On mode: the XYZ control is a
 * Write_On control, open() refuses in plain mode, and switching to plain
 * closes the panel. The stash menu item and Ctrl+Shift+X are offered only in
 * Write_On mode too, so text never moves somewhere the author cannot see.
 * The data itself is the same in both modes: a document opened or saved in
 * plain mode keeps its overflow.
 *
 * Stash (R-6.3, R-7.3). "Stash this in Overflow" (Ctrl+Shift+X) is
 * registered in the selection menu (slot `stash`). It calls
 * MarkdownEditor.stashRange: the text is removed from the page and appended
 * to the end of the overflow (after a blank line when the overflow is not
 * empty) as ONE undo step; undo puts the text back and takes it out of the
 * overflow, redo does both again. The stashed text is identified by its
 * recorded position in the overflow (the end of the overflow before the
 * stash), never by searching for it: text typed after it, even an identical
 * copy, is kept; if text was changed before it, or it was edited itself,
 * undo leaves the overflow unchanged and shows a notice (`warningKind`
 * 'overflow') instead of removing some other copy.
 * Annotations inside the stashed text follow the normal edit rules: spans
 * whose text is removed are detached and set aside (saved, with the usual
 * notice), ghosts inside it are set aside, and both re-attach when undo
 * brings the text back. The overflow itself is plain text and carries no
 * annotations. The panel opens (without taking focus) so the stash is seen.
 * Refused while a malformed annotation block is preserved: its save writes
 * the raw block back, so the overflow could not be saved; the panel is then
 * read-only and says why.
 *
 * Pull back (R-6.4) is a COPY: the text goes into the page and stays in the
 * overflow, so nothing can be lost (the spec calls it "use"; the author
 * deletes it from the panel when done; terraphim-lsp copies too). Ctrl+Enter
 * in the text area inserts the selected panel text, or the current line when
 * nothing is selected, without its own line breaks at either end, at the
 * document caret (at the end of a document selection, which is never
 * replaced), as one undo step through the normal edit path. It is separated
 * exactly as terraphim-lsp separates a pulled paragraph (the model's
 * `overflow_pull_back_text`): as it is in the middle of a line; at the start
 * or end of a line, or on a blank line, a paragraph of its own with one
 * blank line on each side, in the document's line ending. Focus and the
 * panel selection stay in the panel.
 *
 * Drag. Dragging selected panel text into the page pulls it back the same
 * way at the drop point (one undo step); the drag allows only `copy`, so the
 * text area never deletes its text. Text dropped from anywhere else is
 * inserted by EditorSurface as it is.
 * Dragging a page selection onto the panel stashes it (a move, the same
 * single undo step as Ctrl+Shift+X); the drop is accepted as `copy` so the
 * browser never deletes the page text behind the model's back.
 *
 * Keyboard and ARIA. Escape anywhere in the panel closes it and returns
 * focus to where it was before the panel was opened (the XYZ control), or
 * to the page. The text area is labelled and described by the footer hint.
 *
 * Lifecycle. Listeners use the editor's AbortController signal plus the
 * panel's own; destroy() removes the DOM, the menu item, the body attribute
 * and every listener. Load order: after blocks.js, before editor.js, which
 * creates the panel in MarkdownEditor.initialize().
 */

class TeOverflowPanel {
  constructor(editor, options = {}) {
    this.editor = editor;
    this.surface = editor.surface;
    this.delay = options.delay === undefined ? 250 : options.delay;
    this.destroyed = false;
    this.timer = null;
    this.returnFocus = null;
    this.pageDrag = null;
    this.panelDrag = null;
    this.readOnly = false;
    this.unregisterItem = () => {};

    this.abortController = new AbortController();
    if (options.signal) {
      if (options.signal.aborted) this.abortController.abort();
      else options.signal.addEventListener('abort', () => this.destroy(), { signal: this.abortController.signal });
    }
    this.mount = options.mount || (editor.input && editor.input.closest('.editor-container')) || document.body;
    this.build();
    this.listen();
    this.offChange = this.surface ? this.surface.onChange((change) => this.onSurfaceChange(change)) : () => {};
    if (editor.selectionMenu && typeof editor.selectionMenu.register === 'function') {
      this.unregisterItem = editor.selectionMenu.register(this.stashItem());
    }
    this.refresh();
  }

  static get HINT() {
    return 'ctrl+⏎ or drag into the page to use';
  }

  // ---------------------------------------------------------------------
  // DOM
  // ---------------------------------------------------------------------

  build() {
    const n = TeOverflowPanel.nextId = (TeOverflowPanel.nextId || 0) + 1;
    const root = document.createElement('aside');
    root.className = 'te-overflow';
    root.id = `te-overflow-${n}`;
    root.setAttribute('role', 'complementary');
    root.setAttribute('aria-labelledby', `te-overflow-title-${n}`);
    root.hidden = true;

    const header = document.createElement('div');
    header.className = 'te-overflow-header';
    const title = document.createElement('h2');
    title.className = 'te-overflow-title';
    title.id = `te-overflow-title-${n}`;
    title.textContent = 'Overflow';
    const tools = document.createElement('div');
    tools.className = 'te-overflow-tools';
    const button = (cls, icon, label) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = `te-overflow-button ${cls}`;
      b.setAttribute('aria-label', label);
      b.title = label;
      const i = document.createElement('i');
      i.className = icon;
      i.setAttribute('aria-hidden', 'true');
      b.appendChild(i);
      tools.appendChild(b);
      return b;
    };
    this.closeButton = button('te-overflow-close', 'fa-solid fa-xmark', 'Close Overflow');
    header.append(title, tools);

    const note = document.createElement('p');
    note.className = 'te-overflow-note';
    note.setAttribute('role', 'status');
    note.hidden = true;

    const area = document.createElement('textarea');
    area.className = 'te-overflow-text';
    area.spellcheck = false;
    area.setAttribute('aria-label', 'Overflow text');
    area.setAttribute('aria-describedby', `te-overflow-hint-${n}`);
    area.placeholder = 'Select text on the page and press Ctrl+Shift+X to stash it here, or just write.';

    const hint = document.createElement('p');
    hint.className = 'te-overflow-hint';
    hint.id = `te-overflow-hint-${n}`;
    hint.textContent = TeOverflowPanel.HINT;

    root.append(header, note, area, hint);
    this.mount.appendChild(root);
    this.root = root;
    this.area = area;
    this.note = note;
  }

  listen() {
    const signal = this.abortController.signal;
    const on = (el, type, fn, opts = {}) => el.addEventListener(type, fn, { signal, ...opts });

    on(document, 'te:overflow', (e) => {
      if (e.detail && e.detail.editor && e.detail.editor !== this.editor) return;
      this.toggle();
    });
    on(document, 'te:mode-change', (e) => {
      if (e.detail && e.detail.editor && e.detail.editor !== this.editor) return;
      if (e.detail && e.detail.mode !== 'write-on') this.close();
    });
    on(this.closeButton, 'click', () => this.close());
    // Ctrl+Shift+D, from the editor or the panel (which may be mounted
    // outside it). Capture phase, so a focused control cannot swallow it; the
    // browser's own binding is overridden only when the panel acts.
    const toggleKey = typeof OVERFLOW_TOGGLE_KEY === 'string' ? OVERFLOW_TOGGLE_KEY : 'ctrl+shift+d';
    const onToggleKey = (e) => {
      if (e.defaultPrevented || typeof teShortcutMatches !== 'function' || !teShortcutMatches(e, toggleKey)) return;
      if (!this.isOpen() && !this.usable()) return;
      e.preventDefault();
      e.stopPropagation();
      this.toggle();
    };
    const keyScope = this.editor.root || this.mount;
    on(keyScope, 'keydown', onToggleKey, { capture: true });
    if (!keyScope.contains(this.root)) on(this.root, 'keydown', onToggleKey, { capture: true });
    on(this.root, 'keydown', (e) => {
      if (e.key === 'Escape' && !e.isComposing) {
        e.preventDefault();
        e.stopPropagation();
        this.close();
      }
    });
    on(this.area, 'input', () => this.schedule());
    on(this.area, 'blur', () => this.flush());
    on(this.area, 'keydown', (e) => {
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && !e.isComposing) {
        e.preventDefault();
        this.pullBack();
      }
    });
    // Panel text dragged out is copied, never moved out of the panel; in
    // the page it is pulled back (separated) at the drop point.
    on(this.area, 'dragstart', (e) => {
      if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy';
      const start = Math.min(this.area.selectionStart, this.area.selectionEnd);
      const end = Math.max(this.area.selectionStart, this.area.selectionEnd);
      this.panelDrag = start === end ? null : { start, end, text: this.area.value.slice(start, end) };
    });
    on(this.area, 'dragend', () => { this.panelDrag = null; });
    // A page selection dragged onto the panel is stashed.
    if (this.surface) {
      on(this.surface.root, 'dragstart', () => {
        const sel = this.surface.getSelectionOffsets();
        this.pageDrag = sel.start === sel.end
          ? null
          : { start: sel.start, end: sel.end, text: this.surface.getText().slice(sel.start, sel.end) };
      });
      on(this.surface.root, 'dragend', () => { this.pageDrag = null; });
      // Captured before EditorSurface's own drop handling, which inserts
      // the dropped text as it is.
      on(this.surface.root, 'drop', (e) => this.onPageDrop(e), { capture: true });
    }
    on(this.root, 'dragover', (e) => {
      if (!this.pageDrag || !this.canStash()) return;
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
    });
    on(this.root, 'drop', (e) => this.onPanelDrop(e));
  }

  // ---------------------------------------------------------------------
  // Model
  // ---------------------------------------------------------------------

  /** The WASM document API, if it offers the overflow calls. */
  api() {
    const editor = this.editor;
    const api = typeof editor.documentApi === 'function' ? editor.documentApi() : null;
    return api && typeof api.document_overflow === 'function' && typeof api.set_document_overflow === 'function'
      ? api
      : null;
  }

  /** The overflow text in the model (panel input flushed first). */
  text() {
    this.flush();
    const api = this.api();
    return api ? api.document_overflow() : '';
  }

  /** Is a malformed annotation block being preserved (overflow unsavable)? */
  preservedBlock() {
    const api = this.api();
    if (!api || typeof api.document_annotations !== 'function') return false;
    try {
      const ann = api.document_annotations();
      return !!(ann && ann.preservedBlock);
    } catch (err) {
      return false;
    }
  }

  /**
   * Mark the panel open (`data-te-overflow="open"`) on the editor's root,
   * which the Write_On layout uses to keep the column clear of it, and on
   * <body> only where the chrome owns the page (#77: an embedded editor
   * never touches <body>).
   */
  setPageFlag(open) {
    const chrome = this.editor.chrome;
    const targets = [this.editor.root];
    if (!chrome || chrome.pageMode !== false) targets.push(document.body);
    for (const el of targets) {
      if (!el) continue;
      if (open) el.dataset.teOverflow = 'open';
      else delete el.dataset.teOverflow;
    }
  }

  /** Can the panel be shown: Write_On mode (or no chrome) and a model? */
  usable() {
    if (this.destroyed || !this.api()) return false;
    const chrome = this.editor.chrome;
    return !chrome || typeof chrome.isWriteOn !== 'function' || chrome.isWriteOn();
  }

  canStash() {
    if (!this.usable()) return false;
    const api = this.api();
    return typeof api.stash_document_range === 'function' && typeof this.editor.stashRange === 'function' &&
      !this.preservedBlock();
  }

  schedule() {
    if (this.destroyed || this.readOnly) return;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      this.write();
    }, this.delay);
  }

  /** Is panel input waiting for its debounce? */
  pending() {
    return this.timer !== null;
  }

  /** Write waiting panel input to the model now (no-op when none waits). */
  flush() {
    if (this.timer === null) return;
    clearTimeout(this.timer);
    this.timer = null;
    this.write();
  }

  write() {
    const api = this.api();
    if (!api || this.readOnly) return;
    const result = api.set_document_overflow(this.area.value);
    if (result && result.ok) this.changed('edit');
  }

  /** Re-read the model into the panel (dropping waiting input). */
  refresh() {
    if (this.destroyed) return;
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    const api = this.api();
    const value = api ? api.document_overflow() : '';
    if (this.area.value !== value) this.area.value = value;
    this.readOnly = this.preservedBlock();
    this.area.readOnly = this.readOnly;
    this.note.hidden = !this.readOnly;
    this.note.textContent = this.readOnly
      ? 'Overflow is read-only: the annotation block at the end of this file could not be read. Repair it to stash again.'
      : '';
  }

  onSurfaceChange(change) {
    if (this.destroyed || !change) return;
    // A different document was opened, or a stash was done, undone or
    // redone: the model holds the overflow to show.
    if (change.source === 'open' || (change.edit && change.edit.overflow)) {
      this.refresh();
      if (change.edit && change.edit.overflow && change.source !== 'stash') this.changed(change.source);
    }
  }

  /** Tell storage owners the overflow changed. */
  changed(action) {
    if (!this.surface || this.surface.destroyed) return;
    this.surface.root.dispatchEvent(new CustomEvent('te:overflow-change', {
      bubbles: true,
      detail: { editor: this.editor, action, overflow: this.area.value },
    }));
  }

  // ---------------------------------------------------------------------
  // Opening and closing
  // ---------------------------------------------------------------------

  control() {
    const chrome = this.editor.chrome;
    return chrome && chrome.root ? chrome.root.querySelector('[data-control="overflow"]') : null;
  }

  isOpen() {
    return !this.destroyed && !this.root.hidden;
  }

  /** Show the panel (Write_On mode only). Returns whether it is open. */
  open({ focus = true } = {}) {
    if (!this.usable()) return false;
    if (!this.isOpen()) {
      const active = document.activeElement;
      this.returnFocus = active && active !== document.body && !this.root.contains(active) ? active : null;
      this.refresh();
      this.root.hidden = false;
      this.setPageFlag(true);
      this.syncControl();
    }
    if (focus) this.area.focus({ preventScroll: true });
    return true;
  }

  /** Hide the panel; focus inside it returns to where it came from. */
  close({ restoreFocus = true } = {}) {
    if (this.destroyed || this.root.hidden) return;
    this.flush();
    const hadFocus = this.root.contains(document.activeElement);
    this.root.hidden = true;
    this.setPageFlag(false);
    this.syncControl();
    if (restoreFocus && hadFocus) {
      const back = this.returnFocus;
      if (back && back.isConnected && back.getClientRects().length > 0) back.focus({ preventScroll: true });
      else if (this.surface) this.surface.focus();
    }
    this.returnFocus = null;
  }

  toggle() {
    if (this.isOpen()) this.close();
    else this.open();
  }

  syncControl() {
    const control = this.control();
    if (!control) return;
    control.setAttribute('aria-controls', this.root.id);
    control.setAttribute('aria-expanded', this.isOpen() ? 'true' : 'false');
  }

  // ---------------------------------------------------------------------
  // Stash and pull back
  // ---------------------------------------------------------------------

  /** The selection menu item (slot `stash`, R-7.3). */
  stashItem() {
    return {
      id: 'stash',
      label: 'Stash this in Overflow',
      key: 'ctrl+shift+x',
      available: (ctx) => !ctx.collapsed && this.canStash(),
      run: (ctx) => this.stash(ctx.start, ctx.end),
    };
  }

  /**
   * Stash [start, end) (a move, one undo step; see the header) and show the
   * panel without taking focus. Returns the model outcome, or
   * { ok: false, error } when refused (nothing changes).
   */
  stash(start, end) {
    if (!this.canStash()) return { ok: false, error: 'Stashing is not available here' };
    let outcome;
    try {
      outcome = this.editor.stashRange(start, end);
    } catch (err) {
      console.warn('Could not stash:', err);
      return { ok: false, error: String(err && err.message ? err.message : err) };
    }
    this.open({ focus: false });
    this.area.scrollTop = this.area.scrollHeight;
    this.changed('stash');
    return { ok: true, ...outcome };
  }

  /**
   * The panel text pull back uses: the selection, or the current line when
   * nothing is selected. Returns { text, start, end } or null when empty.
   */
  pullBackText() {
    const value = this.area.value;
    let start = Math.min(this.area.selectionStart, this.area.selectionEnd);
    let end = Math.max(this.area.selectionStart, this.area.selectionEnd);
    if (start === end) {
      start = value.lastIndexOf('\n', start - 1) + 1;
      const nl = value.indexOf('\n', end);
      end = nl === -1 ? value.length : nl;
    }
    const text = value.slice(start, end);
    return text ? { text, start, end } : null;
  }

  /**
   * Ctrl+Enter (R-6.4): copy the panel selection (or current line) into the
   * page at the document caret, separated as terraphim-lsp separates it, as
   * one undo step. Focus and the panel selection stay in the panel. Returns
   * { start, end } of the inserted text in the page, or null when there is
   * nothing to insert.
   */
  pullBack() {
    if (this.destroyed || !this.surface) return null;
    const picked = this.pullBackText();
    if (!picked) return null;
    return this.pullBackAt(this.surface.getSelectionOffsets().end, picked);
  }

  /** Copy `picked` ({ text, start, end } of the panel) into the page at `at`. */
  pullBackAt(at, picked) {
    const { selectionStart, selectionEnd, selectionDirection } = this.area;
    const focused = document.activeElement === this.area;
    let start = at;
    let text = picked.text;
    const api = this.api();
    if (api && typeof api.overflow_pull_back_text === 'function') {
      // The model reads the overflow: panel input still waiting goes first.
      this.flush();
      try {
        ({ start, text } = api.overflow_pull_back_text(at, picked.start, picked.end));
      } catch (err) {
        console.warn('Could not pull back:', err);
        return null;
      }
    }
    const edit = this.surface.replaceRange(start, start, text, { source: 'overflow' });
    if (focused) {
      this.area.focus({ preventScroll: true });
      this.area.setSelectionRange(selectionStart, selectionEnd, selectionDirection);
    }
    return { start: edit.start, end: edit.start + edit.insertedText.length };
  }

  /** Selected panel text dropped in the page: pulled back at the drop point. */
  onPageDrop(e) {
    const drag = this.panelDrag;
    this.panelDrag = null;
    if (!drag) return;
    const data = e.dataTransfer ? e.dataTransfer.getData('text/plain') : '';
    if (data !== drag.text || this.area.value.slice(drag.start, drag.end) !== drag.text) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    this.pullBackAt(this.surface.dropOffset(e), drag);
  }

  onPanelDrop(e) {
    const drag = this.pageDrag;
    this.pageDrag = null;
    if (!drag) return;
    const data = e.dataTransfer ? e.dataTransfer.getData('text/plain') : '';
    if (data && data !== drag.text) return;
    if (this.surface.getText().slice(drag.start, drag.end) !== drag.text) return;
    e.preventDefault();
    this.stash(drag.start, drag.end);
  }

  // ---------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------

  /** Write waiting input, remove the DOM, the menu item and every listener. */
  destroy() {
    if (this.destroyed) return;
    this.flush();
    this.destroyed = true;
    this.unregisterItem();
    this.offChange();
    this.abortController.abort();
    if (!this.root.hidden) this.setPageFlag(false);
    const control = this.control();
    if (control) {
      control.removeAttribute('aria-expanded');
      control.removeAttribute('aria-controls');
    }
    this.root.remove();
  }
}

window.TeOverflowPanel = TeOverflowPanel;
