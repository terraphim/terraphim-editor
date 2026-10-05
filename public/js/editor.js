/*
 * EditorSurface: a span-aware plain-text editing surface built on a
 * contenteditable element.
 *
 * Offset unit
 * -----------
 * Every offset accepted or returned by this API is a UTF-16 code unit index
 * into the plain-text document, i.e. exactly the index space of a JavaScript
 * string (`text.length`, `text.slice(start, end)`). A character outside the
 * Basic Multilingual Plane (for example most emoji) occupies two code units;
 * a combining mark (for example U+0301) is a separate code unit from its base
 * character. Setters snap an offset that would split a surrogate pair back to
 * the start of the pair, so a caret or decoration can never sit inside one
 * code point. Offsets between a base character and a combining mark are
 * permitted (they are valid code unit boundaries).
 *
 * DOM invariant
 * -------------
 * After every edit the root element is normalised to a canonical shape:
 * only Text nodes, decoration `<span class="te-decoration">` elements (each
 * holding a single Text node) and, when the text ends with a newline, one
 * trailing placeholder `<br class="te-placeholder">` that makes the final
 * empty line visible. Newlines are literal "\n" characters inside Text nodes
 * (the root uses `white-space: pre-wrap`). Consequently
 * `root.textContent === surface.getText()`, which is what the Rust preview
 * handler relies on. The surface's `input` listener runs in the capture phase
 * so it normalises the DOM before any bubbling/target-phase listener (such as
 * the Rust Markdown conversion) reads it.
 *
 * Undo/redo
 * ---------
 * The surface keeps its own history stack instead of the browser's native
 * one, because programmatic edits (shortcuts, the command palette, decoration
 * re-rendering) would otherwise corrupt or bypass the native stack. Bursts of
 * typing are coalesced into one undo step.
 */
class EditorSurface {
  constructor(root, options = {}) {
    this.root = root;
    this.decorations = [];
    this.changeListeners = new Set();
    this.composing = false;
    this.programmatic = false;
    this.pendingSource = null;
    this.history = [];
    this.historyIndex = -1;
    this.historyLimit = options.historyLimit || 500;
    this.coalesceMs = options.coalesceMs === undefined ? 1000 : options.coalesceMs;
    this.nextDecorationId = 1;
    // Pre-edit range reported by the last `beforeinput`, used to place the
    // edit when the content diff alone is ambiguous (see sync()).
    this.pendingHint = null;
    // One AbortController removes every listener this instance adds.
    this.abortController = new AbortController();
    this.destroyed = false;
    this.plaintextOnly = EditorSurface.enablePlaintextEditing(root);

    root.setAttribute('role', 'textbox');
    root.setAttribute('aria-multiline', 'true');
    root.spellcheck = false;

    this.text = this.serialise().text;
    this.render();
    this.lastSelection = { start: this.text.length, end: this.text.length, direction: 'none' };
    this.record('init', this.lastSelection);

    this.handlers = {
      beforeinput: (e) => this.onBeforeInput(e),
      input: (e) => this.onInput(e),
      keydown: (e) => this.onKeyDown(e),
      compositionstart: () => { this.composing = true; this.pendingHint = null; },
      compositionend: () => this.onCompositionEnd(),
      paste: (e) => this.onPaste(e),
      drop: (e) => this.onDrop(e),
      copy: (e) => this.onCopy(e, false),
      cut: (e) => this.onCopy(e, true),
    };
    const signal = this.abortController.signal;
    root.addEventListener('beforeinput', this.handlers.beforeinput, { signal });
    root.addEventListener('input', this.handlers.input, { capture: true, signal });
    root.addEventListener('keydown', this.handlers.keydown, { signal });
    root.addEventListener('compositionstart', this.handlers.compositionstart, { signal });
    root.addEventListener('compositionend', this.handlers.compositionend, { signal });
    root.addEventListener('paste', this.handlers.paste, { signal });
    root.addEventListener('drop', this.handlers.drop, { signal });
    root.addEventListener('copy', this.handlers.copy, { signal });
    root.addEventListener('cut', this.handlers.cut, { signal });
    // Remember the raw DOM selection points while they are inside the
    // surface; they are only converted to offsets when needed (converting
    // eagerly would force a layout on every keystroke).
    this.pendingSelection = null;
    this.onSelectionChange = () => {
      // A selection change means any recorded beforeinput hint is stale.
      this.pendingHint = null;
      const sel = document.getSelection();
      if (sel && sel.rangeCount > 0 && this.root.contains(sel.anchorNode) && this.root.contains(sel.focusNode)) {
        this.pendingSelection = [sel.anchorNode, sel.anchorOffset, sel.focusNode, sel.focusOffset];
      }
    };
    document.addEventListener('selectionchange', this.onSelectionChange, { signal });
  }

  /**
   * Detach the surface: remove every listener it added (including the
   * document-level `selectionchange` listener) and drop change subscribers.
   * The DOM content is left in place. Safe to call more than once.
   */
  destroy() {
    if (this.destroyed) return;
    this.destroyed = true;
    this.abortController.abort();
    this.changeListeners.clear();
    this.pendingSelection = null;
    this.pendingHint = null;
  }

  /** Use plaintext-only editing where supported, otherwise fall back to "true". */
  static enablePlaintextEditing(root) {
    try {
      root.contentEditable = 'plaintext-only';
    } catch (e) {
      // Browsers without plaintext-only throw a SyntaxError.
    }
    if (root.contentEditable !== 'plaintext-only') {
      root.contentEditable = 'true';
      return false;
    }
    return true;
  }

  // ---------------------------------------------------------------------
  // Public API
  // ---------------------------------------------------------------------

  /** The plain-text document. */
  getText() {
    return this.text;
  }

  /** Replace the whole document. The edit is recorded in the undo history. */
  setText(text, options = {}) {
    return this.replaceRange(0, this.text.length, text, { source: 'api', ...options });
  }

  /**
   * Replace [start, end) with `insert` (UTF-16 offsets). Line endings in
   * `insert` are normalised to "\n". Options: selectStart, selectEnd
   * (selection after the edit; defaults to a caret after the insertion) and
   * source (a label passed to change listeners and used for undo grouping).
   */
  replaceRange(start, end, insert, options = {}) {
    const source = options.source || 'api';
    const old = this.text;
    let s = this.snap(this.clamp(Math.min(start, end)), old);
    let e = this.snap(this.clamp(Math.max(start, end)), old);
    const inserted = EditorSurface.normaliseNewlines(String(insert));
    const edit = {
      start: s,
      deletedLength: e - s,
      deletedText: old.slice(s, e),
      insertedText: inserted,
    };
    this.text = old.slice(0, s) + inserted + old.slice(e);
    this.decorations = EditorSurface.mapRanges(this.decorations, edit);
    this.render();
    const selStart = options.selectStart === undefined ? s + inserted.length : options.selectStart;
    const selEnd = options.selectEnd === undefined ? selStart : options.selectEnd;
    this.setSelectionOffsets(selStart, selEnd);
    this.record(source, this.lastSelection, edit);
    this.emitChange(edit, source);
    this.dispatchInput(source === 'paste' ? 'insertFromPaste' : 'insertReplacementText', inserted);
    return edit;
  }

  /** Replace the current selection with `insert`. */
  replaceSelection(insert, source = 'api') {
    const { start, end } = this.getSelectionOffsets();
    return this.replaceRange(start, end, insert, { source });
  }

  /**
   * Current selection as { start, end, direction } in UTF-16 offsets with
   * start <= end. If the live DOM selection is outside the surface (for
   * example a toolbar button has focus) the last known selection is returned.
   */
  getSelectionOffsets() {
    const live = this.readSelection();
    if (live) {
      this.lastSelection = live;
    } else if (this.pendingSelection) {
      const [a, ao, f, fo] = this.pendingSelection;
      if (this.root.contains(a) && this.root.contains(f)) {
        const x = this.serialise(a, ao).offset;
        const y = this.serialise(f, fo).offset;
        this.lastSelection = {
          start: Math.min(x, y),
          end: Math.max(x, y),
          direction: x === y ? 'none' : x < y ? 'forward' : 'backward',
        };
      }
    }
    this.pendingSelection = null;
    return { ...this.lastSelection };
  }

  /** Select [start, end). direction is 'forward', 'backward' or 'none'. */
  setSelectionOffsets(start, end = start, direction) {
    let s = this.snap(this.clamp(start));
    let e = this.snap(this.clamp(end));
    let dir = direction;
    if (s > e) {
      [s, e] = [e, s];
      dir = dir || 'backward';
    }
    if (!dir) dir = s === e ? 'none' : 'forward';
    if (!this.isCanonical().ok) this.render();
    this.lastSelection = { start: s, end: e, direction: s === e ? 'none' : dir };
    this.pendingSelection = null;
    if (!this.root.isConnected) return { ...this.lastSelection };
    const anchor = this.offsetToPoint(dir === 'backward' ? e : s);
    const focus = this.offsetToPoint(dir === 'backward' ? s : e);
    const sel = document.getSelection();
    if (sel) sel.setBaseAndExtent(anchor.node, anchor.offset, focus.node, focus.offset);
    return { ...this.lastSelection };
  }

  /** Map a DOM point (node, offset) inside the surface to a UTF-16 offset. */
  pointToOffset(node, offset) {
    if (!this.root.contains(node)) return -1;
    return this.serialise(node, offset).offset;
  }

  /** Map a UTF-16 offset to a DOM point { node, offset } in the canonical DOM. */
  offsetToPoint(offset) {
    const target = this.snap(this.clamp(offset));
    const walker = document.createTreeWalker(this.root, NodeFilter.SHOW_TEXT);
    let acc = 0;
    let last = null;
    let node;
    while ((node = walker.nextNode())) {
      const len = node.data.length;
      if (target <= acc + len) return { node, offset: target - acc };
      acc += len;
      last = node;
    }
    if (last) return { node: last, offset: last.data.length };
    return { node: this.root, offset: 0 };
  }

  /** A DOM Range covering [start, end), useful for positioning popovers. */
  rangeForOffsets(start, end = start) {
    if (!this.isCanonical().ok) this.render();
    const a = this.offsetToPoint(Math.min(start, end));
    const b = this.offsetToPoint(Math.max(start, end));
    const range = document.createRange();
    range.setStart(a.node, a.offset);
    range.setEnd(b.node, b.offset);
    return range;
  }

  /** Viewport rectangle of the caret at `offset` (defaults to selection start). */
  getCaretRect(offset) {
    const at = offset === undefined ? this.getSelectionOffsets().start : offset;
    const rects = this.rangeForOffsets(at).getClientRects();
    if (rects.length > 0) {
      const r = rects[0];
      return { left: r.left, top: r.top, bottom: r.bottom, height: r.height };
    }
    const box = this.root.getBoundingClientRect();
    const style = getComputedStyle(this.root);
    const left = box.left + (parseFloat(style.paddingLeft) || 0);
    const top = box.top + (parseFloat(style.paddingTop) || 0);
    const lineHeight = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.2 || 16;
    return { left, top, bottom: top + lineHeight, height: lineHeight };
  }

  /**
   * Decorate ranges of the text. Each item is { start, end, className?, id?,
   * data?, attributes? } in UTF-16 offsets; empty or out-of-range items are
   * ignored. `attributes` is a map of extra attributes (for example
   * `aria-describedby`) set on every rendered span of the decoration; where
   * decorations overlap, the later one (by start, then end) wins. Only
   * `aria-*`, `data-*` and `role` are accepted (see sanitiseAttributes()).
   * Overlapping ranges are allowed: each rendered span lists every covering
   * decoration id in `data-te-decoration` (space separated) and carries the
   * union of their class names. Decorations move with edits made before them
   * and are dropped when an edit lands inside them.
   */
  setDecorations(list) {
    const len = this.text.length;
    const next = [];
    for (const item of list || []) {
      const s = this.snap(Math.max(0, Math.min(len, item.start | 0)));
      const e = this.snap(Math.max(0, Math.min(len, item.end | 0)));
      if (e <= s) continue;
      next.push({
        id: item.id === undefined ? String(this.nextDecorationId++) : String(item.id),
        start: s,
        end: e,
        className: item.className || '',
        data: item.data,
        attributes: EditorSurface.sanitiseAttributes(item.attributes),
      });
    }
    next.sort((a, b) => a.start - b.start || a.end - b.end);
    const sel = this.getSelectionOffsets();
    this.decorations = next;
    this.render();
    if (this.readSelection() !== null || document.activeElement === this.root) {
      this.setSelectionOffsets(sel.start, sel.end, sel.direction);
    }
    return this.getDecorations();
  }

  /**
   * The decoration attributes that may be set on rendered spans: `role`,
   * `aria-*` and `data-*` names in plain lowercase attribute-name syntax,
   * except `data-te-decoration`, which the surface owns. Anything else
   * (event handlers, `style`, `href`, namespaced names, ...) is dropped with
   * a console warning; this never throws. Returns a new map, or null when
   * nothing is left.
   */
  static sanitiseAttributes(attributes, warn = true) {
    if (!attributes || typeof attributes !== 'object') return null;
    const out = {};
    let any = false;
    for (const [name, value] of Object.entries(attributes)) {
      if (EditorSurface.isSafeAttributeName(name)) {
        out[name] = String(value);
        any = true;
      } else if (warn) {
        console.warn(`EditorSurface: decoration attribute "${name}" is not allowed and was ignored`);
      }
    }
    return any ? out : null;
  }

  static isSafeAttributeName(name) {
    if (typeof name !== 'string' || name === 'data-te-decoration') return false;
    return name === 'role' || /^aria-[a-z]+$/.test(name) || /^data-[a-z0-9_.-]*[a-z0-9_]$/.test(name);
  }

  /** Current decorations (already mapped through any edits). */
  getDecorations() {
    return this.decorations.map((d) => ({ ...d }));
  }

  /** Decorations covering `offset` (start <= offset < end). */
  decorationsAt(offset) {
    return this.decorations.filter((d) => d.start <= offset && offset < d.end).map((d) => ({ ...d }));
  }

  clearDecorations() {
    return this.setDecorations([]);
  }

  /**
   * Subscribe to text changes. The callback receives { text, edit, source }
   * where edit is { start, deletedLength, deletedText, insertedText }.
   * Returns an unsubscribe function.
   */
  onChange(callback) {
    this.changeListeners.add(callback);
    return () => this.changeListeners.delete(callback);
  }

  /**
   * Forget the undo history so the current text becomes the base state.
   * Used when a different document is opened: undo must not step back into
   * the previous file.
   */
  resetHistory() {
    this.history = [];
    this.historyIndex = -1;
    this.record('init', this.lastSelection);
  }

  canUndo() {
    return this.historyIndex > 0;
  }

  canRedo() {
    return this.historyIndex < this.history.length - 1;
  }

  undo() {
    if (!this.canUndo()) return false;
    const undone = this.history[this.historyIndex];
    this.historyIndex -= 1;
    const steps = undone.edits ? undone.edits.slice().reverse().map(EditorSurface.invertStep) : null;
    this.restore(this.history[this.historyIndex], 'undo', steps);
    return true;
  }

  redo() {
    if (!this.canRedo()) return false;
    this.historyIndex += 1;
    const entry = this.history[this.historyIndex];
    this.restore(entry, 'redo', entry.edits);
    return true;
  }

  /** Focus the surface, restoring the last known selection. */
  focus() {
    const had = this.readSelection();
    this.root.focus({ preventScroll: true });
    if (!had) {
      const { start, end, direction } = this.lastSelection;
      this.setSelectionOffsets(start, end, direction);
    }
  }

  // ---------------------------------------------------------------------
  // DOM <-> text mapping
  // ---------------------------------------------------------------------

  /**
   * Serialise the root's DOM to plain text. If stopNode is given, also
   * return the text offset of the DOM point (stopNode, stopOffset). Handles
   * the canonical shape plus the shapes browsers produce natively (<br>
   * line breaks, <div>/<p> blocks) so that any DOM can be read back.
   */
  serialise(stopNode = null, stopOffset = 0) {
    const parts = [];
    let length = 0;
    let found = -1;
    // True when the last emitted text was the line break closing a block, so
    // the next block must not add another one.
    let afterBlock = false;
    const emit = (s) => {
      if (s) {
        parts.push(s);
        length += s.length;
        afterBlock = false;
      }
    };
    const visit = (node) => {
      const children = node.childNodes;
      for (let i = 0; i < children.length; i++) {
        if (node === stopNode && i === stopOffset && found < 0) found = length;
        const child = children[i];
        if (child.nodeType === Node.TEXT_NODE) {
          if (child === stopNode && found < 0) found = length + Math.min(stopOffset, child.data.length);
          emit(child.data);
        } else if (child.nodeType === Node.ELEMENT_NODE) {
          if (child.tagName === 'BR') {
            if (!EditorSurface.isPlaceholderBr(child, this.root)) emit('\n');
          } else if (EditorSurface.isBlock(child)) {
            if (length > 0 && !afterBlock) emit('\n');
            visit(child);
            if (EditorSurface.hasFollowingContent(child)) {
              emit('\n');
              afterBlock = true;
            }
          } else {
            visit(child);
          }
        }
      }
      if (node === stopNode && stopOffset >= children.length && found < 0) found = length;
    };
    visit(this.root);
    return { text: parts.join(''), offset: found };
  }

  /** Is the root in the canonical shape described at the top of this file? */
  isCanonical() {
    const children = this.root.childNodes;
    let hasPlaceholder = false;
    for (let i = 0; i < children.length; i++) {
      const child = children[i];
      if (child.nodeType === Node.TEXT_NODE) continue;
      if (child.nodeType !== Node.ELEMENT_NODE) return { ok: false, hasPlaceholder };
      if (child.tagName === 'SPAN' && child.classList.contains('te-decoration')) {
        if (child.childNodes.length !== 1 || child.firstChild.nodeType !== Node.TEXT_NODE) {
          return { ok: false, hasPlaceholder };
        }
        continue;
      }
      if (child.tagName === 'BR' && child.classList.contains('te-placeholder') && i === children.length - 1) {
        hasPlaceholder = true;
        continue;
      }
      return { ok: false, hasPlaceholder };
    }
    return { ok: true, hasPlaceholder };
  }

  readSelection() {
    const sel = document.getSelection();
    if (!sel || sel.rangeCount === 0) return null;
    const a = sel.anchorNode;
    const f = sel.focusNode;
    if (!a || !f || !this.root.contains(a) || !this.root.contains(f)) return null;
    const ao = this.serialise(a, sel.anchorOffset).offset;
    const fo = a === f && sel.anchorOffset === sel.focusOffset ? ao : this.serialise(f, sel.focusOffset).offset;
    return {
      start: Math.min(ao, fo),
      end: Math.max(ao, fo),
      direction: ao === fo ? 'none' : ao < fo ? 'forward' : 'backward',
    };
  }

  // ---------------------------------------------------------------------
  // Rendering, history and events
  // ---------------------------------------------------------------------

  render() {
    // Number of full DOM rebuilds; plain native typing should never need one.
    this.renderCount = (this.renderCount || 0) + 1;
    const text = this.text;
    const root = this.root;
    if (this.decorations.length === 0) {
      root.textContent = text;
    } else {
      const frag = document.createDocumentFragment();
      const bounds = new Set([0, text.length]);
      for (const d of this.decorations) {
        bounds.add(d.start);
        bounds.add(d.end);
      }
      const points = Array.from(bounds).sort((x, y) => x - y);
      for (let i = 0; i + 1 < points.length; i++) {
        const s = points[i];
        const e = points[i + 1];
        if (e <= s) continue;
        const piece = text.slice(s, e);
        const covering = this.decorations.filter((d) => d.start <= s && d.end >= e);
        if (covering.length === 0) {
          frag.appendChild(document.createTextNode(piece));
        } else {
          const span = document.createElement('span');
          const classes = ['te-decoration'];
          for (const d of covering) {
            if (d.className) classes.push(...d.className.split(/\s+/).filter(Boolean));
          }
          span.className = Array.from(new Set(classes)).join(' ');
          span.setAttribute('data-te-decoration', covering.map((d) => d.id).join(' '));
          for (const d of covering) {
            if (!d.attributes) continue;
            for (const [name, value] of Object.entries(d.attributes)) span.setAttribute(name, String(value));
          }
          span.appendChild(document.createTextNode(piece));
          frag.appendChild(span);
        }
      }
      root.textContent = '';
      root.appendChild(frag);
    }
    if (text.endsWith('\n')) {
      const br = document.createElement('br');
      br.className = 'te-placeholder';
      root.appendChild(br);
    }
  }

  /**
   * Read native edits back into the model and normalise the DOM.
   *
   * The new text is known exactly, but where the edit happened is not: in
   * repeated text ("aaaa", "abab", repeated words) several alignments
   * explain the same change, and the plain minimal diff would always pick
   * the last one. The location is therefore taken from what the browser
   * knows, in this order:
   * 1. `hint.exact`: the `beforeinput` target range (getTargetRanges()).
   *    It is authoritative and always applied.
   * 2. Only when the content diff is ambiguous: the pre-edit selection,
   *    from `beforeinput` or else the last known selection provided no newer
   *    DOM selection is pending (native edits without a `beforeinput`, such
   *    as execCommand, take this path). A collapsed selection before a pure
   *    deletion is widened by the deletion length in the direction given by
   *    `inputType`. If the resulting edit is minimal it is used as is (no
   *    DOM read, so typing stays cheap); otherwise, as for a replacement,
   *    it is accepted only if it ends at the post-edit caret.
   * 3. The post-edit caret read from the DOM (the edit ends at the caret).
   * 4. The plain minimal diff, for example after IME composition or an
   *    external DOM mutation with the selection elsewhere.
   */
  sync(source, hint = null, inputType = '') {
    const shape = this.isCanonical();
    const text = shape.ok ? this.root.textContent : this.serialise().text;
    const old = this.text;
    let edit;
    let sel;
    if (hint && hint.exact) {
      edit = EditorSurface.diff(old, text, hint);
    } else {
      const ends = old === text ? null : EditorSurface.commonEnds(old, text);
      if (!ends || !EditorSurface.isAmbiguous(old, text, ends)) {
        edit = ends && EditorSurface.diff(old, text, null, ends);
      } else {
        // Ambiguous, hence a pure insertion or deletion of `growth` code units.
        const growth = text.length - old.length;
        const minimal = { deletedLength: Math.max(0, -growth), insertedLength: Math.max(0, growth) };
        let resolved = null;
        let pre = hint || (this.pendingSelection === null ? this.lastSelection : null);
        if (pre && pre.start === pre.end && growth < 0) {
          const k = pre.start;
          const d = minimal.deletedLength;
          if (/Backward$/.test(inputType) && k - d >= 0) pre = { start: k - d, end: k };
          else if (/Forward$/.test(inputType) && k + d <= old.length) pre = { start: k, end: k + d };
          else pre = null;
        }
        if (pre) {
          const candidate = EditorSurface.diff(old, text, pre, ends);
          if (
            candidate.deletedLength === minimal.deletedLength &&
            candidate.insertedText.length === minimal.insertedLength
          ) {
            resolved = candidate;
          } else {
            sel = this.readSelection();
            const caret = sel && sel.start === sel.end ? sel.start : null;
            const end = candidate.start + candidate.insertedText.length;
            if (caret === null ? hint !== null : end === caret) resolved = candidate;
          }
        }
        if (!resolved) {
          if (sel === undefined) sel = this.readSelection();
          if (sel && sel.start === sel.end) resolved = EditorSurface.diff(old, text, { caret: sel.start }, ends);
        }
        edit = resolved || EditorSurface.diff(old, text, null, ends);
      }
    }
    let needsRender = !shape.ok || shape.hasPlaceholder !== text.endsWith('\n');
    if (edit && this.decorations.length > 0) {
      const editEnd = edit.start + edit.deletedLength;
      if (this.decorations.some((d) => d.start <= editEnd && d.end >= edit.start)) needsRender = true;
    }
    // Reading the DOM selection forces a synchronous layout, so only do it
    // when the DOM is about to be rebuilt. For a plain native edit the caret
    // is derived from the edit itself (selectionchange refreshes it later).
    if (sel === undefined) sel = needsRender ? this.readSelection() : null;
    if (edit) this.decorations = EditorSurface.mapRanges(this.decorations, edit);
    this.text = text;
    if (needsRender) {
      this.render();
      if (sel) this.setSelectionOffsets(sel.start, sel.end, sel.direction);
    } else if (edit) {
      const caret = edit.start + edit.insertedText.length;
      this.lastSelection = { start: caret, end: caret, direction: 'none' };
      this.pendingSelection = null;
    }
    if (edit) {
      this.record(source, this.lastSelection, edit);
      this.emitChange(edit, source);
    }
  }

  /**
   * Record the current state in the undo history. `edit` is the exact edit
   * that produced it from the previous state. Each entry keeps the forward
   * edits (in order) that lead to it from the entry before, so undo and redo
   * replay exactly what happened instead of re-diffing the text, which is
   * ambiguous in repeated text and would mis-map decorations and the caret.
   */
  record(source, sel, edit = null) {
    const now = performance.now();
    const step = edit
      ? { start: edit.start, deletedText: edit.deletedText, insertedText: edit.insertedText }
      : null;
    const entry = {
      text: this.text,
      start: sel ? sel.start : this.text.length,
      end: sel ? sel.end : this.text.length,
      source,
      time: now,
      edits: step ? [step] : null,
    };
    const top = this.history[this.historyIndex];
    if (top && top.text === entry.text) {
      top.start = entry.start;
      top.end = entry.end;
      return;
    }
    this.history.length = this.historyIndex + 1;
    const coalesce =
      top &&
      this.historyIndex > 0 &&
      (source === 'typing' || source === 'delete') &&
      top.source === source &&
      now - top.time < this.coalesceMs;
    if (coalesce) {
      // The burst's entry keeps every step; contiguous steps are combined.
      entry.edits = top.edits && step ? EditorSurface.appendStep(top.edits, step, top.text) : null;
      this.history[this.historyIndex] = entry;
    } else {
      this.history.push(entry);
      if (this.history.length > this.historyLimit) this.history.shift();
      this.historyIndex = this.history.length - 1;
    }
  }

  /**
   * Move the model to history `entry`, replaying `steps` (exact edits from
   * the current text to entry.text). Decorations are mapped and change
   * listeners notified once per step, each with the text after that step.
   * After an undo the caret goes to the end of the restored text; after a
   * redo the recorded selection is restored. If the steps are missing or do
   * not reproduce entry.text, the minimal diff is the fallback.
   */
  restore(entry, source, steps) {
    const applied = steps ? EditorSurface.replaySteps(this.text, steps, entry.text) : null;
    let caret = null;
    if (applied) {
      for (let i = 0; i < applied.length; i++) {
        const edit = applied[i].edit;
        this.decorations = EditorSurface.mapRanges(this.decorations, edit);
        this.text = applied[i].text;
        caret = edit.start + edit.insertedText.length;
        this.emitChange(edit, source);
      }
      this.text = entry.text;
    } else {
      const edit = EditorSurface.diff(this.text, entry.text);
      if (edit) this.decorations = EditorSurface.mapRanges(this.decorations, edit);
      this.text = entry.text;
      if (edit) this.emitChange(edit, source);
    }
    this.render();
    if (source === 'undo' && caret !== null) this.setSelectionOffsets(caret);
    else this.setSelectionOffsets(entry.start, entry.end);
    this.dispatchInput(source === 'undo' ? 'historyUndo' : 'historyRedo', null);
  }

  emitChange(edit, source) {
    for (const listener of this.changeListeners) {
      try {
        listener({ text: this.text, edit, source });
      } catch (err) {
        console.error('EditorSurface change listener failed', err);
      }
    }
  }

  /** Fire a real `input` event so that listeners (e.g. the Rust preview) update. */
  dispatchInput(inputType, data) {
    this.programmatic = true;
    try {
      this.root.dispatchEvent(new InputEvent('input', { bubbles: true, inputType, data }));
    } finally {
      this.programmatic = false;
    }
  }

  onBeforeInput(e) {
    const type = e.inputType || '';
    if (type === 'historyUndo') {
      e.preventDefault();
      this.undo();
      return;
    }
    if (type === 'historyRedo') {
      e.preventDefault();
      this.redo();
      return;
    }
    if (type.startsWith('format')) {
      // Rich formatting (fallback contenteditable="true" mode) is never allowed.
      e.preventDefault();
      return;
    }
    if (e.isComposing || this.composing) return;
    if (type === 'insertParagraph' || type === 'insertLineBreak') {
      e.preventDefault();
      this.replaceSelection('\n', 'newline');
      return;
    }
    if (type === 'insertFromPaste' || type === 'insertFromPasteAsQuotation' || type === 'insertFromDrop') {
      // Normally handled (and cancelled) by the paste/drop listeners; this is
      // a safety net so rich content can never be inserted.
      e.preventDefault();
      const data = e.dataTransfer ? e.dataTransfer.getData('text/plain') : '';
      if (data) this.replaceSelection(data, 'paste');
      return;
    }
    this.pendingSource = type.startsWith('delete') ? 'delete' : 'typing';
    // The hint is single-shot: the native `input` follows synchronously in
    // the same task, so it is dropped when the next task runs, on any later
    // selectionchange, and ignored if a later listener cancelled the event.
    const hint = this.preEditRange(e);
    this.pendingHint = hint;
    if (hint) {
      setTimeout(() => {
        if (this.pendingHint === hint) this.pendingHint = null;
      }, 0);
    }
  }

  /**
   * The range a native edit is about to replace, as UTF-16 offsets into the
   * current text: the event's first target range where the browser provides
   * one (exact: true), otherwise the current selection (exact: false, since
   * for example a backward delete removes the character before a collapsed
   * selection). Returns null if neither maps into the surface.
   */
  preEditRange(e) {
    let start = -1;
    let end = -1;
    let exact = false;
    const ranges = typeof e.getTargetRanges === 'function' ? e.getTargetRanges() : [];
    if (ranges.length > 0) {
      const r = ranges[0];
      start = this.pointToOffset(r.startContainer, r.startOffset);
      end = this.pointToOffset(r.endContainer, r.endOffset);
      exact = start >= 0 && end >= 0;
    }
    if (start < 0 || end < 0) {
      const sel = this.readSelection();
      if (!sel) return null;
      start = sel.start;
      end = sel.end;
    }
    return {
      start: Math.min(start, end),
      end: Math.max(start, end),
      exact,
      inputType: e.inputType || '',
      text: this.text,
      event: e,
    };
  }

  onInput(e) {
    if (this.programmatic) return;
    if (e.isComposing || this.composing) return;
    // Prefer the event's own inputType: a beforeinput that was cancelled
    // must not label a later, unrelated input.
    const type = e.inputType || '';
    const source = type ? (type.startsWith('delete') ? 'delete' : 'typing') : this.pendingSource || 'typing';
    // Only trust a hint recorded for this very edit: its beforeinput was not
    // cancelled, same input type and no model change since it was taken.
    const h = this.pendingHint;
    const hint =
      h && !h.event.defaultPrevented && h.text === this.text && h.inputType === (e.inputType || '') ? h : null;
    this.pendingSource = null;
    this.pendingHint = null;
    this.sync(source, hint, e.inputType || '');
  }

  onCompositionEnd() {
    this.composing = false;
    this.pendingSource = null;
    this.pendingHint = null;
    this.sync('composition');
  }

  onKeyDown(e) {
    if (e.isComposing || this.composing) return;
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    const key = (e.key || '').toLowerCase();
    if (key === 'z' && !e.shiftKey) {
      e.preventDefault();
      this.undo();
    } else if ((key === 'z' && e.shiftKey) || (key === 'y' && !e.shiftKey)) {
      e.preventDefault();
      this.redo();
    }
  }

  onPaste(e) {
    e.preventDefault();
    const data = e.clipboardData ? e.clipboardData.getData('text/plain') : '';
    if (data) this.replaceSelection(data, 'paste');
  }

  onDrop(e) {
    e.preventDefault();
    const data = e.dataTransfer ? e.dataTransfer.getData('text/plain') : '';
    if (!data) return;
    let offset = null;
    if (document.caretRangeFromPoint) {
      const range = document.caretRangeFromPoint(e.clientX, e.clientY);
      if (range && this.root.contains(range.startContainer)) {
        offset = this.pointToOffset(range.startContainer, range.startOffset);
      }
    }
    if (offset === null || offset < 0) offset = this.getSelectionOffsets().start;
    this.replaceRange(offset, offset, data, { source: 'drop' });
  }

  onCopy(e, cut) {
    const { start, end } = this.getSelectionOffsets();
    if (start === end || !e.clipboardData) return;
    e.preventDefault();
    e.clipboardData.setData('text/plain', this.text.slice(start, end));
    if (cut) this.replaceRange(start, end, '', { source: 'cut' });
  }

  clamp(offset) {
    const n = Number.isFinite(offset) ? Math.trunc(offset) : 0;
    return Math.max(0, Math.min(this.text.length, n));
  }

  /** Move an offset that splits a surrogate pair back to the start of the pair. */
  snap(offset, text = this.text) {
    if (offset > 0 && offset < text.length) {
      const hi = text.charCodeAt(offset - 1);
      const lo = text.charCodeAt(offset);
      if (hi >= 0xd800 && hi <= 0xdbff && lo >= 0xdc00 && lo <= 0xdfff) return offset - 1;
    }
    return offset;
  }

  // ---------------------------------------------------------------------
  // Static helpers
  // ---------------------------------------------------------------------

  static normaliseNewlines(text) {
    return text.replace(/\r\n?/g, '\n');
  }

  static isBlock(node) {
    return /^(DIV|P|LI|UL|OL|H[1-6]|PRE|BLOCKQUOTE|SECTION|ARTICLE)$/.test(node.tagName);
  }

  /** A <br> that is the last node of the root or of a block only holds a line open. */
  static isPlaceholderBr(br, root) {
    const parent = br.parentNode;
    if (parent !== root && !EditorSurface.isBlock(parent)) return false;
    let next = br.nextSibling;
    while (next && next.nodeType === Node.TEXT_NODE && next.data === '') next = next.nextSibling;
    return next === null;
  }

  static hasFollowingContent(node) {
    let next = node.nextSibling;
    while (next) {
      if (next.nodeType === Node.ELEMENT_NODE) return true;
      if (next.nodeType === Node.TEXT_NODE && next.data !== '') return true;
      next = next.nextSibling;
    }
    return false;
  }

  /**
   * Lengths of the common prefix and the common suffix of `a` and `b`, each
   * at most min(a.length, b.length) (they may overlap). Scanning is the
   * expensive part of a diff on a long document, so it is done once per
   * edit and every candidate alignment is derived from the result.
   */
  static commonEnds(a, b) {
    const min = Math.min(a.length, b.length);
    let prefix = 0;
    while (prefix < min && a.charCodeAt(prefix) === b.charCodeAt(prefix)) prefix++;
    // For a single-region change the suffix scan stops at the change, so the
    // two scans together touch each code unit about once; they only overlap
    // across a run of repeated text, which is exactly the ambiguous case.
    let suffix = 0;
    while (suffix < min && a.charCodeAt(a.length - 1 - suffix) === b.charCodeAt(b.length - 1 - suffix)) suffix++;
    return { prefix, suffix };
  }

  /**
   * Single-region diff between two strings, or null if equal.
   *
   * Without a hint the result is minimal with the longest common prefix.
   * An optional hint chooses among equally valid alignments (in the manner
   * of ProseMirror's findDiff with a preferred position):
   * - { start, end }: the pre-edit range in `a`. The common prefix is capped
   *   at `start` and the common suffix at `a.length - end`, so the edit
   *   starts at `start` and covers the hinted range.
   * - { caret }: the post-edit caret in `b`, taken to sit at the end of the
   *   inserted text. The common suffix is capped at `b.length - caret` and
   *   taken first, so the edit ends at the caret.
   * The result always transforms `a` into `b`; a wrong hint can only make
   * it larger than necessary, never incorrect. `ends` is an optional
   * precomputed commonEnds(a, b).
   */
  static diff(a, b, hint = null, ends = null) {
    if (a === b) return null;
    const min = Math.min(a.length, b.length);
    const { prefix, suffix } = ends || EditorSurface.commonEnds(a, b);
    let pMax = min;
    let sMax = min;
    let suffixFirst = false;
    if (hint) {
      const { start, end, caret } = hint;
      if (Number.isInteger(start) && Number.isInteger(end) && start >= 0 && start <= end && end <= a.length) {
        pMax = Math.min(pMax, start);
        sMax = Math.min(sMax, a.length - end);
      } else if (Number.isInteger(caret) && caret >= 0 && caret <= b.length) {
        pMax = Math.min(pMax, caret);
        sMax = Math.min(sMax, b.length - caret);
        suffixFirst = true;
      }
    }
    // Never split a surrogate pair: back off a prefix ending on a high
    // surrogate or a suffix starting on a low surrogate.
    const fixPrefix = (n) => {
      if (n > 0) {
        const c = a.charCodeAt(n - 1);
        if (c >= 0xd800 && c <= 0xdbff) return n - 1;
      }
      return n;
    };
    const fixSuffix = (n) => {
      if (n > 0) {
        const c = a.charCodeAt(a.length - n);
        if (c >= 0xdc00 && c <= 0xdfff) return n - 1;
      }
      return n;
    };
    let p;
    let s;
    if (suffixFirst) {
      s = fixSuffix(Math.min(suffix, sMax));
      p = fixPrefix(Math.min(prefix, pMax, min - s));
    } else {
      p = fixPrefix(Math.min(prefix, pMax));
      s = fixSuffix(Math.min(suffix, sMax, min - p));
    }
    return {
      start: p,
      deletedLength: a.length - p - s,
      deletedText: a.slice(p, a.length - s),
      insertedText: b.slice(p, b.length - s),
    };
  }

  /** The step that undoes `step` ({ start, deletedText, insertedText }). */
  static invertStep(step) {
    return { start: step.start, deletedText: step.insertedText, insertedText: step.deletedText };
  }

  /**
   * Append `next` (an edit of the text `before`, which is the result of
   * `steps`) to a burst's steps. When it touches or overlaps the region
   * written by the last step, the two are combined into one step covering
   * both, which is exact; otherwise it is kept as a separate step.
   */
  static appendStep(steps, next, before) {
    const last = steps[steps.length - 1];
    if (!last) return [next];
    const lastEnd = last.start + last.insertedText.length;
    const nextEnd = next.start + next.deletedText.length;
    if (next.start > lastEnd || nextEnd < last.start) return [...steps, next];
    // Union of both regions, in the coordinates of `before`.
    const lo = Math.min(last.start, next.start);
    const hi = Math.max(lastEnd, nextEnd);
    const combined = {
      start: lo,
      // The region's content before `last` ...
      deletedText: before.slice(lo, last.start) + last.deletedText + before.slice(lastEnd, hi),
      // ... and after `next`.
      insertedText: before.slice(lo, next.start) + next.insertedText + before.slice(nextEnd, hi),
    };
    return [...steps.slice(0, -1), combined];
  }

  /**
   * Apply `steps` to `text`, checking each one against the text it edits.
   * Returns [{ edit, text }] (text after each step) if every step matches
   * and the result is `expected`, otherwise null.
   */
  static replaySteps(text, steps, expected) {
    const out = [];
    let t = text;
    for (const step of steps) {
      const end = step.start + step.deletedText.length;
      if (step.start < 0 || end > t.length || t.slice(step.start, end) !== step.deletedText) return null;
      t = t.slice(0, step.start) + step.insertedText + t.slice(end);
      out.push({
        edit: {
          start: step.start,
          deletedLength: step.deletedText.length,
          deletedText: step.deletedText,
          insertedText: step.insertedText,
        },
        text: t,
      });
    }
    return t === expected ? out : null;
  }

  /**
   * Does the change from `a` to `b` admit more than one minimal alignment?
   * That happens exactly when the common prefix and common suffix overlap,
   * as in typing "a" inside "aaaa"; the change is then a pure insertion or
   * deletion and only a hint can say where it happened. `ends` is an
   * optional precomputed commonEnds(a, b).
   */
  static isAmbiguous(a, b, ends = null) {
    if (a === b) return false;
    const { prefix, suffix } = ends || EditorSurface.commonEnds(a, b);
    return prefix + suffix > Math.min(a.length, b.length);
  }

  /**
   * Map ranges through an edit: ranges entirely before the edit are kept,
   * ranges entirely after it are shifted, ranges the edit lands inside are
   * dropped. An insertion exactly at a range boundary does not extend it.
   */
  static mapRanges(ranges, edit) {
    if (!edit || ranges.length === 0) return ranges;
    const editStart = edit.start;
    const editEnd = edit.start + edit.deletedLength;
    const delta = edit.insertedText.length - edit.deletedLength;
    const out = [];
    for (const r of ranges) {
      if (r.end <= editStart) out.push(r);
      else if (r.start >= editEnd) out.push({ ...r, start: r.start + delta, end: r.end + delta });
    }
    return out;
  }
}

class MarkdownEditor {
  constructor(config) {
    this.config = config;
    this.shortcuts = config.shortcuts;
    this.commands = config.commands.map(cmd => ({
      ...cmd,
      action: () => this.wrapSelectedText(cmd.prefix, cmd.suffix)
    }));
    // Every listener added by this instance is registered with this signal
    // so destroy() can remove them all; createdNodes are the DOM nodes it
    // inserted (toolbar buttons, help items, the command menu).
    this.abortController = new AbortController();
    this.createdNodes = [];
    this.destroyed = false;
    // Document model bridge (issue #6): warning subscribers, the notice
    // element and a guard that stops opening a file from being mirrored as
    // an edit.
    this.warningListeners = new Set();
    this.warningElement = null;
    this.warning = null;
    // What the current warning is about: 'malformed' (stays until dismissed,
    // the raw block is still preserved), 'set-aside' or 'detached' (both
    // clear once nothing is set aside).
    this.warningKind = null;
    // Set-aside spans and ghosts in the model, as last reported.
    this.setAsideCount = 0;
    this.suppressModelSync = false;
  }

  /**
   * Tear the editor down: remove every listener it added (on the surface,
   * the document and the window), remove the DOM it created and destroy the
   * editing surface. Safe to call more than once.
   */
  destroy() {
    if (this.destroyed) return;
    this.destroyed = true;
    this.abortController.abort();
    for (const node of this.createdNodes) {
      // Close an open dialog first so Shoelace releases its scroll lock.
      if (node.tagName === 'SL-DIALOG' && node.open) node.open = false;
      node.remove();
    }
    this.createdNodes = [];
    if (this.chrome) this.chrome.destroy();
    if (this.indicators) this.indicators.destroy();
    if (this.lab) this.lab.destroy();
    if (this.decorations) this.decorations.destroy();
    if (this.surface) this.surface.destroy();
    this.warningListeners.clear();
  }

  initialize() {
    // Get DOM elements after template is rendered
    this.input = document.querySelector('.markdown-input');
    this.toolbar = document.querySelector('#formatting-toolbar');
    this.shortcutsList = document.querySelector('#shortcuts-list');
    this.dialog = document.querySelector('.shortcuts-dialog');
    this.helpButton = document.querySelector('#show-help');

    // Check if elements exist
    if (!this.input || !this.toolbar || !this.shortcutsList || !this.dialog || !this.helpButton) {
      console.error('Required DOM elements not found');
      return;
    }

    if (this.shortcuts.length === 0) {
      console.error('No shortcuts available');
      return;
    }

    this.surface = new EditorSurface(this.input);
    this.connectDocumentModel();

    this.setupShortcuts();
    this.setupHelpDialog();
    this.setupCommandPalette();

    // Write_On mode toggle and corner chrome (public/js/chrome.js, issue #7).
    if (typeof window.WriteOnChrome === 'function') {
      this.chrome = new window.WriteOnChrome(this, { signal: this.abortController.signal });
    }

    // Decoration registry and inline indicators (public/js/indicators.js,
    // issue #8): later layers (ghosts, issue #11) register with
    // this.decorations.
    if (typeof window.TeDecorationRegistry === 'function') {
      this.decorations = new window.TeDecorationRegistry(this.surface);
      if (typeof window.TeIndicatorLayer === 'function') {
        this.indicators = new window.TeIndicatorLayer(this, { signal: this.abortController.signal });
      }
      // The Lab popover and its marks (public/js/lab.js, issue #14).
      if (typeof window.TeLabPopover === 'function') {
        this.lab = new window.TeLabPopover(this, { signal: this.abortController.signal });
      }
    }
  }

  wrapSelectedText(prefix, suffix) {
    const { start, end } = this.surface.getSelectionOffsets();
    const text = this.surface.getText();
    const selection = text.substring(start, end);
    const wrappedText = selection ? selection : 'text';

    this.surface.focus();
    this.surface.replaceRange(start, end, prefix + wrappedText + suffix, {
      source: 'format',
      selectStart: start + prefix.length,
      selectEnd: selection ? end + prefix.length : start + prefix.length + 4,
    });
  }

  setupShortcuts() {
    // Create toolbar buttons
    this.shortcuts.forEach(shortcut => {
      const button = document.createElement('sl-tooltip');
      button.setAttribute('content', shortcut.key);

      button.innerHTML = `
        <sl-button size="small" variant="default">
          <sl-icon name="${shortcut.name}"></sl-icon>
        </sl-button>
      `;

      button.querySelector('sl-button').addEventListener('click', () => {
        this.wrapSelectedText(shortcut.prefix, shortcut.suffix);
      }, { signal: this.abortController.signal });

      this.toolbar.appendChild(button);
      this.createdNodes.push(button);
    });

    // Setup keyboard shortcuts
    this.input.addEventListener('keydown', (e) => {
      if (e.isComposing) return;
      const key = `${e.ctrlKey ? 'ctrl+' : ''}${e.key.toLowerCase()}`;
      const shortcut = this.shortcuts.find(s => s.key === key);

      if (shortcut) {
        e.preventDefault();
        this.wrapSelectedText(shortcut.prefix, shortcut.suffix);
      }
    }, { signal: this.abortController.signal });
  }

  setupHelpDialog() {
    // Create shortcut list items
    this.shortcuts.forEach(shortcut => {
      const item = document.createElement('div');
      item.className = 'shortcut-item';
      item.innerHTML = `
        <sl-icon name="${shortcut.name}"></sl-icon>
        <span class="shortcut-desc">${shortcut.desc}</span>
        <sl-badge variant="neutral">${shortcut.key}</sl-badge>
      `;
      this.shortcutsList.appendChild(item);
      this.createdNodes.push(item);
    });

    this.helpButton.addEventListener('click', () => this.dialog.show(), { signal: this.abortController.signal });
  }

  setupCommandPalette() {
    const signal = this.abortController.signal;
    // Create inline command menu
    const commandMenu = document.createElement('div');
    commandMenu.classList.add('command-menu');
    commandMenu.style.display = 'none';
    commandMenu.setAttribute('tabindex', '0');
    this.commandMenu = commandMenu;

    const commandList = document.createElement('div');
    commandList.classList.add('command-list');

    commandMenu.appendChild(commandList);
    document.body.appendChild(commandMenu);
    this.createdNodes.push(commandMenu);

    let selectedIndex = -1;
    let visibleItems = [];
    let slashPosition = null;

    // Add commands to the list
    this.commands.forEach(cmd => {
      const item = document.createElement('div');
      item.classList.add('command-item');
      item.innerHTML = `
        <sl-icon name="${cmd.icon}"></sl-icon>
        <span>${cmd.name}</span>
      `;

      item.addEventListener('click', () => {
        if (slashPosition !== null && this.surface.getText().charAt(slashPosition) === '/') {
          this.surface.replaceRange(slashPosition, slashPosition + 1, '', { source: 'command' });
        }
        cmd.action();
        hideCommandMenu();
      }, { signal });

      commandList.appendChild(item);
    });

    const updateSelection = () => {
      visibleItems = Array.from(commandList.querySelectorAll('.command-item'));
      visibleItems.forEach((item, index) => {
        if (index === selectedIndex) {
          item.classList.add('selected');
          item.scrollIntoView({ block: 'nearest' });
        } else {
          item.classList.remove('selected');
        }
      });
    };

    const positionCommandMenu = () => {
      if (commandMenu.style.display === 'none') return;
      const caret = this.surface.getCaretRect();
      const menuRect = commandMenu.getBoundingClientRect();
      const viewportWidth = window.innerWidth;
      const viewportHeight = window.innerHeight;

      // Calculate initial position just below the caret
      let left = caret.left;
      let top = caret.bottom + 4;

      // Adjust horizontal position if menu would go outside viewport
      if (left + menuRect.width > viewportWidth) {
        left = viewportWidth - menuRect.width - 10; // 10px padding from right edge
      }
      if (left < 0) {
        left = 10; // 10px padding from left edge
      }

      // Adjust vertical position if menu would go outside viewport
      if (top + menuRect.height > viewportHeight) {
        // Show menu above the caret if there's not enough space below
        top = caret.top - menuRect.height - 10;
      }
      if (top < 0) {
        top = 10; // 10px padding from top edge
      }

      commandMenu.style.position = 'fixed';
      commandMenu.style.left = `${left}px`;
      commandMenu.style.top = `${top}px`;
    };

    const showCommandMenu = () => {
      commandMenu.style.display = 'block';
      selectedIndex = 0;
      updateSelection();
      positionCommandMenu();
      commandMenu.focus();
    };

    const hideCommandMenu = () => {
      commandMenu.style.display = 'none';
      selectedIndex = -1;
      slashPosition = null;
      this.surface.focus();
    };

    // Keyboard navigation
    commandMenu.addEventListener('keydown', (e) => {
      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          selectedIndex = Math.min(selectedIndex + 1, visibleItems.length - 1);
          if (selectedIndex === -1 && visibleItems.length > 0) selectedIndex = 0;
          updateSelection();
          break;

        case 'ArrowUp':
          e.preventDefault();
          selectedIndex = Math.max(selectedIndex - 1, 0);
          updateSelection();
          break;

        case 'Enter':
          e.preventDefault();
          if (selectedIndex >= 0 && selectedIndex < visibleItems.length) {
            visibleItems[selectedIndex].click();
          }
          break;

        case 'Escape':
          e.preventDefault();
          hideCommandMenu();
          break;
      }
    }, { signal });

    // Show command menu on forward slash
    this.input.addEventListener('keydown', (e) => {
      if (e.key === '/' && !e.ctrlKey && !e.metaKey && !e.altKey && !e.isComposing) {
        e.preventDefault();
        const { start } = this.surface.getSelectionOffsets();
        this.surface.replaceSelection('/', 'typing');
        slashPosition = start;
        showCommandMenu();
      }
    }, { signal });

    // Hide menu when clicking outside
    document.addEventListener('click', (e) => {
      if (commandMenu.style.display === 'none') return;
      if (!commandMenu.contains(e.target) && !this.input.contains(e.target)) {
        hideCommandMenu();
      }
    }, { signal });

    // Update menu position on scroll or resize
    window.addEventListener('scroll', positionCommandMenu, { signal });
    window.addEventListener('resize', positionCommandMenu, { signal });
    this.input.addEventListener('scroll', positionCommandMenu, { signal });
  }

  // ---------------------------------------------------------------------
  // Document model (issue #6)
  //
  // The Rust span model (crates/terraphim_alternatives, wrapped by
  // src/document.rs) holds the open document: the body shown on the surface
  // plus alternatives, ghosts and overflow, which never appear on the
  // surface or in the preview. Every surface edit is mirrored into it so
  // anchors follow typing. Offsets are UTF-16 code units on both sides.
  // ---------------------------------------------------------------------

  /**
   * The WASM document API: `window.wasmBindings` in the Trunk build. Returns
   * null when the module does not expose it, in which case the editor still
   * works as a plain Markdown editor.
   */
  documentApi() {
    const api = window.wasmBindings;
    return api && typeof api.open_document === 'function' && typeof api.apply_edit === 'function'
      ? api
      : null;
  }

  requireDocumentApi() {
    const api = this.documentApi();
    if (!api) throw new Error('The WASM document API is not available');
    return api;
  }

  /** Subscribe the model to surface edits and align it with the surface. */
  connectDocumentModel() {
    const api = this.documentApi();
    if (api) api.sync_document_body(this.surface.getText());
    this.surface.onChange((change) => this.mirrorEdit(change));
  }

  mirrorEdit(change) {
    if (this.suppressModelSync) return;
    const api = this.documentApi();
    if (!api) return;
    const { edit, text } = change;
    let outcome;
    try {
      outcome = api.apply_edit(edit.start, edit.deletedLength, edit.insertedText);
    } catch (err) {
      // The model refused the edit (for example a stale ghost): fall back to
      // replacing its body with the surface text and re-anchoring by text.
      this.reflectSync(api.sync_document_body(text));
      return;
    }
    if (outcome) this.reflectSetAside(outcome.setAside, outcome.notice, outcome.warning);
  }

  /** Make sure the model body is exactly the surface text before reading it. */
  alignDocumentModel(api) {
    this.reflectSync(api.sync_document_body(this.surface.getText()));
  }

  /** Surface the result of a full-body re-sync ({ changed, unresolved, notice }). */
  reflectSync(synced) {
    if (synced && synced.changed) this.reflectSetAside(synced.unresolved, synced.notice);
  }

  /**
   * Keep the annotation notice in step with the model's set-aside count.
   * A fresh detach shows its own warning; otherwise a changed count shows
   * the current notice, and a count of zero clears an annotation warning.
   * The malformed-block warning is never replaced or cleared here: it stays
   * until dismissed, because the raw block is still being preserved.
   */
  reflectSetAside(count, notice, detachWarning = null) {
    const previous = this.setAsideCount;
    this.setAsideCount = count;
    if (this.warningKind === 'malformed') return;
    if (detachWarning) {
      this.showWarning(detachWarning, 'detached');
    } else if (count === 0) {
      if (this.warningKind === 'set-aside' || this.warningKind === 'detached') this.showWarning(null);
    } else if (count !== previous && notice) {
      this.showWarning(notice, 'set-aside');
    }
  }

  /**
   * Open `.md` source: the body goes on the surface (the annotation block
   * never does), the undo history starts afresh and a malformed block (or
   * annotations that no longer match the text) shows one non-blocking
   * warning. Returns { body, warning, unresolved }.
   *
   * `name` (a string, or `{ name }`) becomes `editor.documentKey`; without
   * one, `documentKey` is left as it is, so a caller may set it beforehand.
   * Afterwards `editor.chrome.documentChanged()` is called when the chrome
   * (issue #7) is attached.
   */
  openDocument(source, name) {
    const api = this.requireDocumentApi();
    const key = typeof name === 'string' ? name : name && typeof name.name === 'string' ? name.name : null;
    if (key) this.documentKey = key;
    const opened = api.open_document(String(source));
    this.suppressModelSync = true;
    try {
      this.surface.setText(opened.body, { source: 'open', selectStart: 0, selectEnd: 0 });
    } finally {
      this.suppressModelSync = false;
    }
    this.surface.resetHistory();
    // The surface normalises line endings; re-anchor if that changed the text.
    const synced = api.sync_document_body(this.surface.getText());
    let warning = opened.warning || null;
    let kind = opened.kind || null;
    if (kind !== 'malformed' && synced.changed) {
      if (synced.unresolved === 0) {
        warning = null;
        kind = null;
      } else if (synced.unresolved !== opened.unresolved) {
        warning = synced.notice;
        kind = 'set-aside';
      }
    }
    this.setAsideCount = synced.changed ? synced.unresolved : opened.unresolved;
    this.showWarning(warning, kind);
    if (this.chrome && typeof this.chrome.documentChanged === 'function') {
      try {
        this.chrome.documentChanged();
      } catch (err) {
        console.error('chrome.documentChanged failed', err);
      }
    }
    if (this.indicators) this.indicators.flush();
    return opened;
  }

  /**
   * The document as `.md` text: body plus the trailing annotation block.
   * Returns the text and also dispatches a bubbling `te:saved` event from the
   * surface with `detail: { editor, text, documentKey }`, so whoever owns
   * storage (a file, localStorage, a server) can write it. Choosing where to
   * write is out of scope here.
   */
  saveDocument() {
    const api = this.requireDocumentApi();
    this.alignDocumentModel(api);
    const text = api.save_document();
    const target = this.input || document;
    target.dispatchEvent(new CustomEvent('te:saved', {
      bubbles: true,
      detail: { editor: this, text, documentKey: this.documentKey },
    }));
    return text;
  }

  /** Clean Markdown: active alternatives, ghosted text dropped, no block. */
  exportDocument() {
    const api = this.requireDocumentApi();
    this.alignDocumentModel(api);
    return api.export_document();
  }

  /** { words, chars } of the body, ghosted text included. */
  counts() {
    const api = this.requireDocumentApi();
    this.alignDocumentModel(api);
    return api.document_counts();
  }

  /**
   * { spans, ghosts, overflow, setAside: { spans, ghosts }, preservedBlock }
   * in UTF-16 offsets. Set-aside items are out of the live model but saved.
   */
  annotations() {
    const api = this.requireDocumentApi();
    this.alignDocumentModel(api);
    return api.document_annotations();
  }

  /**
   * Subscribe to document warnings. The callback receives the message, or
   * null when the warning is cleared. Returns an unsubscribe function.
   */
  onWarning(callback) {
    this.warningListeners.add(callback);
    return () => this.warningListeners.delete(callback);
  }

  /**
   * Show (or, with null, clear) the single non-blocking document warning:
   * a `.te-warning` status element above the surface, which never takes
   * focus, plus every onWarning subscriber. `kind` records what the warning
   * is about ('malformed', 'set-aside' or 'detached'); it is exposed as
   * `data-kind`.
   */
  showWarning(message, kind = null) {
    this.warning = message || null;
    this.warningKind = this.warning ? kind : null;
    if (this.warning && !this.warningElement && this.input && this.input.parentNode) {
      const el = document.createElement('div');
      el.className = 'te-warning';
      el.setAttribute('role', 'status');
      el.setAttribute('aria-live', 'polite');
      const text = document.createElement('span');
      text.className = 'te-warning-text';
      const dismiss = document.createElement('button');
      dismiss.type = 'button';
      dismiss.className = 'te-warning-dismiss';
      dismiss.setAttribute('aria-label', 'Dismiss warning');
      dismiss.textContent = '×';
      dismiss.addEventListener('click', () => this.showWarning(null), {
        signal: this.abortController.signal,
      });
      el.append(text, dismiss);
      this.input.parentNode.insertBefore(el, this.input);
      this.createdNodes.push(el);
      this.warningElement = el;
    }
    if (this.warningElement) {
      this.warningElement.hidden = !this.warning;
      if (this.warningKind) this.warningElement.dataset.kind = this.warningKind;
      else delete this.warningElement.dataset.kind;
      this.warningElement.querySelector('.te-warning-text').textContent = this.warning || '';
    }
    for (const listener of this.warningListeners) {
      try {
        listener(this.warning);
      } catch (err) {
        console.error('Document warning listener failed', err);
      }
    }
  }

  showCustomDialog() {
    const dialog = document.createElement('sl-dialog');
    dialog.label = 'Custom Formatting';

    dialog.innerHTML = `
      <sl-input label="Prefix" id="prefix-input"></sl-input>
      <sl-input label="Suffix" id="suffix-input"></sl-input>
      <sl-button slot="footer" variant="primary">Apply</sl-button>
      <sl-button slot="footer" variant="default">Cancel</sl-button>
    `;

    document.body.appendChild(dialog);
    // Tracked so destroy() removes it even while it is open.
    this.createdNodes.push(dialog);
    const signal = this.abortController.signal;

    const [applyBtn, cancelBtn] = dialog.querySelectorAll('sl-button');
    const prefixInput = dialog.querySelector('#prefix-input');
    const suffixInput = dialog.querySelector('#suffix-input');

    const dispose = () => {
      dialog.remove();
      this.createdNodes = this.createdNodes.filter((node) => node !== dialog);
    };
    // Without the Shoelace element defined there is no hide animation (and
    // so no sl-after-hide); remove the dialog directly.
    const close = () => {
      if (typeof dialog.hide === 'function') dialog.hide();
      else dispose();
    };

    applyBtn.addEventListener('click', () => {
      this.wrapSelectedText(prefixInput.value || '', suffixInput.value || '');
      close();
    }, { signal });

    cancelBtn.addEventListener('click', close, { signal });

    dialog.addEventListener('sl-after-hide', dispose, { signal });

    if (typeof dialog.show === 'function') dialog.show();
    return dialog;
  }
}

window.EditorSurface = EditorSurface;
window.MarkdownEditor = MarkdownEditor;

// Update the initEditor function
const initEditor = () => {
  const checkElements = () => {
    const required = [
      '.markdown-input',
      '#formatting-toolbar',
      '#shortcuts-list',
      '.shortcuts-dialog',
      '#show-help'
    ];

    if (required.every(selector => document.querySelector(selector))) {
      // Tear down any previous editor so its listeners and DOM do not leak.
      if (window.terraphimEditor && typeof window.terraphimEditor.destroy === 'function') {
        window.terraphimEditor.destroy();
      }
      // Pass the EditorConfig when initializing
      const editor = new MarkdownEditor(window.EditorConfig || {
        shortcuts: [],
        commands: [],
        styles: {}
      });
      editor.initialize();
      window.terraphimEditor = editor;
    } else {
      // Check again in 100ms
      setTimeout(checkElements, 100);
    }
  };

  checkElements();
};

// Make sure config is loaded before initializing
document.addEventListener('DOMContentLoaded', () => {
  if (window.EditorConfig) {
    initEditor();
  } else {
    console.error('Editor configuration not found. Make sure config.js is loaded before editor.js');
  }
});
