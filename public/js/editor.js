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
    // A text move ({ start, end, to }, see MarkdownEditor.moveRange) rides
    // on the edit and its history step, so undo and redo can replay it as a
    // move in the document model rather than a deletion plus an insertion.
    if (options.move) edit.move = { ...options.move };
    // A swap of alternatives ({ span, from, to }, see
    // MarkdownEditor.swapAlternative, issue #9) rides the same way: undo and
    // redo replay it as set_active in the model.
    if (options.swap) edit.swap = { ...options.swap };
    // An overflow change ({ before, after }, see MarkdownEditor.stashRange,
    // issue #12) rides the same way: undo and redo replay it in the model.
    if (options.overflow) edit.overflow = { ...options.overflow };
    // A knowledge-graph swap ({ span, from, to }, see
    // MarkdownEditor.swapAlternative, issue #13) is tagged for listeners
    // only: its history step is a plain text step, replayed through
    // apply_edit, because KG spans are derived from the text.
    if (options.kg) edit.kg = { ...options.kg };
    this.text = old.slice(0, s) + inserted + old.slice(e);
    this.decorations = EditorSurface.mapRanges(this.decorations, edit);
    // #10: an alternatives-panel step ({ span, before, after }, see
    // MarkdownEditor.applyAltChange) rides on the edit and its history step;
    // undo and redo replay it as alt_restore in the model.
    if (options.alt) edit.alt = { ...options.alt };
    this.render();
    const selStart = options.selectStart === undefined ? s + inserted.length : options.selectStart;
    const selEnd = options.selectEnd === undefined ? selStart : options.selectEnd;
    this.setSelectionOffsets(selStart, selEnd);
    this.record(source, this.lastSelection, edit);
    this.emitChange(edit, source);
    this.dispatchInput(source === 'paste' ? 'insertFromPaste' : 'insertReplacementText', inserted);
    return edit;
  }

  // ---- Several edits as one undo step (issue #15, trim "Make the cuts") ----
  /**
   * Apply `edits` ([{ start, end, insert }] on the CURRENT text's UTF-16
   * offsets, non-overlapping) as ONE undo step. Each edit goes through the
   * normal edit path: decorations are mapped and change listeners notified
   * once per edit (so the document model mirrors each one with apply_edit
   * and spans and ghosts follow the usual rules), applied from the last to
   * the first so earlier offsets stay valid. The surface renders once and
   * one history entry records every step in application order, so undo
   * replays their inverses (and the model re-attaches what they detached)
   * and redo replays them again. Options: source (default 'api'), and
   * selectStart/selectEnd on the final text (default: a caret at the first
   * edit). Returns the applied edits, or [] when nothing changed.
   */
  replaceRanges(edits, options = {}) {
    const source = options.source || 'api';
    const sorted = (edits || [])
      .map((e) => ({ start: Math.min(e.start, e.end), end: Math.max(e.start, e.end), insert: String(e.insert || '') }))
      .sort((a, b) => a.start - b.start || a.end - b.end);
    for (let i = 1; i < sorted.length; i++) {
      if (sorted[i].start < sorted[i - 1].end) throw new Error('replaceRanges: edits overlap');
    }
    const applied = [];
    for (let i = sorted.length - 1; i >= 0; i--) {
      const old = this.text;
      const s = this.snap(this.clamp(sorted[i].start), old);
      const e = this.snap(this.clamp(sorted[i].end), old);
      const inserted = EditorSurface.normaliseNewlines(sorted[i].insert);
      if (s === e && inserted === '') continue;
      if (old.slice(s, e) === inserted) continue;
      const edit = { start: s, deletedLength: e - s, deletedText: old.slice(s, e), insertedText: inserted };
      this.text = old.slice(0, s) + inserted + old.slice(e);
      this.decorations = EditorSurface.mapRanges(this.decorations, edit);
      applied.push(edit);
      this.emitChange(edit, source);
    }
    if (applied.length === 0) return [];
    this.render();
    const first = applied[applied.length - 1];
    const selStart = options.selectStart === undefined ? first.start + first.insertedText.length : options.selectStart;
    const selEnd = options.selectEnd === undefined ? selStart : options.selectEnd;
    this.setSelectionOffsets(selStart, selEnd);
    this.record(source, this.lastSelection, applied[0]);
    const entry = this.history[this.historyIndex];
    if (entry && entry.text === this.text) {
      entry.edits = applied.map((edit) => ({
        start: edit.start, deletedText: edit.deletedText, insertedText: edit.insertedText,
      }));
    }
    this.dispatchInput('insertReplacementText', null);
    return applied;
  }
  // ---- end several edits as one undo step (issue #15) ----

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

  /**
   * Fold the last `count` history entries into one undo step (the Blocks
   * view uses it to keep a block move and its separator fix-up together).
   * The merged entry keeps every step in order, so undo replays them in
   * reverse (a recorded text move as a move) and redo replays them forward.
   * Returns false, changing nothing, when there are fewer than `count`
   * entries after the base state or one of them has no exact steps.
   */
  squashHistory(count, source = null) {
    const first = this.historyIndex - count + 1;
    if (count < 2 || first < 1) return false;
    const group = this.history.slice(first, this.historyIndex + 1);
    if (group.some((entry) => !entry.edits)) return false;
    const last = group[group.length - 1];
    const merged = {
      ...last,
      source: source || last.source,
      edits: group.flatMap((entry) => entry.edits),
    };
    if (group.some((entry) => entry.ghostHistory || entry.edits.some((step) => step.ghost))) merged.ghostHistory = true;
    this.history.splice(first, count, merged);
    this.historyIndex = first;
    return true;
  }

  /**
   * Enforce historyLimit now (dropping the oldest steps), after it was
   * lifted for a while (the alternatives panel lifts it during a live
   * addition, issue #106, so the steps it folds cannot be shifted out).
   */
  trimHistory() {
    const extra = this.history.length - this.historyLimit;
    if (extra <= 0) return;
    const drop = Math.min(extra, this.historyIndex);
    if (drop <= 0) return;
    this.history.splice(0, drop);
    this.historyIndex -= drop;
  }

  /**
   * Drop the steps redo would replay (the alternatives panel uses it after
   * undoing a cancelled addition, issue #106, so it cannot be redone).
   */
  forgetRedo() {
    this.history.length = this.historyIndex + 1;
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
    if (undone.ghostHistory || (undone.edits && undone.edits.some((step) => step.ghost))) {
      return this.replayGhostHistory(this.historyIndex - 1, 'undo', undone.edits);
    }
    this.historyIndex -= 1;
    const steps = undone.edits ? undone.edits.slice().reverse().map(EditorSurface.invertStep) : null;
    this.restore(this.history[this.historyIndex], 'undo', steps);
    return true;
  }

  redo() {
    if (!this.canRedo()) return false;
    const next = this.history[this.historyIndex + 1];
    if (next.ghostHistory || (next.edits && next.edits.some((step) => step.ghost))) {
      return this.replayGhostHistory(this.historyIndex + 1, 'redo', next.edits);
    }
    this.historyIndex += 1;
    const entry = this.history[this.historyIndex];
    this.restore(entry, 'redo', entry.edits);
    return true;
  }

  /** Ghost groups never enter legacy restore/diff or best-effort listeners. */
  replayGhostHistory(index, source, steps) {
    if (!this.ghostHistoryAdapter) return false;
    try {
      if (source === 'undo' && Array.isArray(steps)) steps = steps.slice().reverse().map(EditorSurface.invertStep);
      this.ghostHistoryAdapter.replayGhostHistory(index, source, steps);
      return true;
    } catch (error) {
      this.ghostHistoryAdapter.ghostHistoryNotice(error);
      return false;
    }
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
    if (step && edit.move) step.move = edit.move;
    if (step && edit.swap) step.swap = edit.swap;
    if (step && edit.overflow) step.overflow = edit.overflow;
    if (step && edit.ghost) step.ghost = edit.ghost;
    const entry = {
      text: this.text,
      start: sel ? sel.start : this.text.length,
      end: sel ? sel.end : this.text.length,
      source,
      time: now,
      edits: step ? [step] : null,
    };
    if (step && edit.ghost) entry.ghostHistory = true;
    // #10: an alternatives-panel step always gets its own entry, even when
    // the text is unchanged (adding, editing, moving or removing an inactive
    // alternative changes only the annotations), and never coalesces.
    if (step && (edit.alt || edit.ghost)) {
      if (edit.alt) step.alt = edit.alt;
      this.history.length = this.historyIndex + 1;
      this.history.push(entry);
      if (this.history.length > this.historyLimit) this.history.shift();
      this.historyIndex = this.history.length - 1;
      return;
    }
    const top = this.history[this.historyIndex];
    // A text move always gets its own entry, even when the text is unchanged
    // (text moved past an identical copy): the move it carries still changes
    // the annotations, and undo must replay its inverse in the model.
    if (top && top.text === entry.text && !(step && (step.move || step.swap))) {
      top.start = entry.start;
      top.end = entry.end;
      return;
    }
    this.history.length = this.historyIndex + 1;
    const coalesce =
      top &&
      this.historyIndex > 0 &&
      (source === 'typing' || source === 'delete') &&
      !top.ghostHistory && !(top.edits && top.edits.some((step) => step.ghost)) &&
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
    // A drop already handled (Overflow panel text pulled back) is not
    // inserted a second time.
    if (e.defaultPrevented) return;
    e.preventDefault();
    const data = e.dataTransfer ? e.dataTransfer.getData('text/plain') : '';
    if (!data) return;
    const offset = this.dropOffset(e);
    this.replaceRange(offset, offset, data, { source: 'drop' });
  }

  /** The text offset a drag event points at (the selection start if none). */
  dropOffset(e) {
    let offset = null;
    if (document.caretRangeFromPoint) {
      const range = document.caretRangeFromPoint(e.clientX, e.clientY);
      if (range && this.root.contains(range.startContainer)) {
        offset = this.pointToOffset(range.startContainer, range.startOffset);
      }
    }
    return offset === null || offset < 0 ? this.getSelectionOffsets().start : offset;
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
    const inverse = { ...step, start: step.start, deletedText: step.insertedText, insertedText: step.deletedText };
    // #10: undoing a panel step restores the span as it was before.
    if (step.alt) inverse.alt = { ...step.alt, span: step.alt.span, before: step.alt.after, after: step.alt.before };
    if (step.move) inverse.move = { ...step.move, ...EditorSurface.invertMove(step.move) };
    if (step.swap) inverse.swap = { ...step.swap, span: step.swap.span, from: step.swap.to, to: step.swap.from };
    if (step.overflow) inverse.overflow = { ...step.overflow, before: step.overflow.after, after: step.overflow.before };
    if (step.ghost) inverse.ghost = { ...step.ghost, change: step.ghost.change, undo: typeof step.ghost.undo === 'boolean' ? !step.ghost.undo : step.ghost.undo };
    return inverse;
  }

  /**
   * The move that undoes moving [start, end) to `to` (pre-move offsets):
   * moving the text back from where it landed. Both replace the same region.
   */
  static invertMove({ start, end, to }) {
    const len = end - start;
    return to > end
      ? { start: to - len, end: to, to: start }
      : { start: to, end: to + len, to: end };
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
    if ([last, next].some((step) => step.ghost || step.alt || step.move || step.swap || step.overflow)) return [...steps, next];
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
      const edit = {
        start: step.start,
        deletedLength: step.deletedText.length,
        deletedText: step.deletedText,
        insertedText: step.insertedText,
      };
      if (step.alt) edit.alt = step.alt; // #10
      if (step.move) edit.move = step.move;
      if (step.swap) edit.swap = step.swap;
      if (step.overflow) edit.overflow = step.overflow;
      if (step.ghost) edit.ghost = step.ghost;
      out.push({ edit, text: t });
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
    // #10: an annotation-only step (no text change) keeps every range.
    if (edit.deletedLength === 0 && edit.insertedText === '') return ranges;
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

/*
 * Knowledge-graph alternatives (issue #13; docs/design/kg-alternatives.md).
 * The matching, synonyms, capitalisation and a/an fix-up all run in Rust
 * (src/kg.rs over terraphim_lsp_core); this is only the glue:
 *
 *   editor.loadThesaurus(json)        // -> { name, concepts, skipped }; throws on bad JSON
 *   editor.clearThesaurus()
 *   editor.kgLookup(start, end)       // -> { term, range, alternatives, span } | null
 *   editor.kgAlternativesForSelection()  // thesaurus only, R-8.6
 *   editor.alternativesForSelection()    // Ctrl+Shift+G through every provider, R-8.7 (#83)
 *   editor.annotations().kg           // derived KG spans (never saved)
 *   editor.swapAlternative('kg-2-0', 1)  // swaps a KG span like any other
 */
const TeKg = {
  /** Derived KG span ids are `kg-<concept>-<n>`; block span ids never are. */
  isKgSpan(id) {
    return /^kg-\d+-\d+$/.test(String(id));
  },

  /** The R-7.3 "AI alternatives for selection" menu item (Ctrl+Shift+G). */
  menuItem(editor) {
    return {
      id: 'ai-alternatives',
      label: 'AI alternatives for selection',
      key: 'ctrl+shift+g',
      available: (ctx) => {
        if (!ctx) return false;
        if (ctx.kgTerm === undefined) ctx.kgTerm = editor.kgLookup(ctx.start, ctx.end);
        // A host provider (#83) can answer for any non-empty selection.
        return !!ctx.kgTerm || (editor.hasAlternativesProviders() && ctx.start !== ctx.end);
      },
      run: (ctx) =>
        editor.alternativesForSelection(ctx).catch((err) => {
          console.error('AI alternatives for selection failed', err);
          return null;
        }),
    };
  },
};
window.TeKg = TeKg;

/** Holders (registration entries) of each host alternatives provider id (#83), page-wide. */
const TeProviderRefs = new Map();

/** Shoelace icon names are lower-case words joined by hyphens. */
const TE_ICON_NAME = /^[a-z0-9-]+$/;

/**
 * An <sl-icon> for `name`. Configuration values (which a host page passes
 * through the embed API, #77) are never parsed as HTML: a name that is not
 * a plain icon name is dropped rather than set.
 */
function teConfigIcon(name) {
  const icon = document.createElement('sl-icon');
  if (typeof name === 'string' && TE_ICON_NAME.test(name)) icon.setAttribute('name', name);
  return icon;
}

/** An element with `text` as its text content (never HTML). */
function teTextElement(tag, text, className) {
  const el = document.createElement(tag);
  if (className) el.className = className;
  el.textContent = text == null ? '' : String(text);
  return el;
}

/**
 * Does keydown `e` match a configured shortcut such as 'ctrl+b' or
 * 'ctrl+shift+u' (R-7.4)? The modifiers must be exactly those named, Shift
 * included for letters and digits, so a config 'ctrl+x' never fires on the
 * R-7.3 Ctrl+Shift+X. Shift is ignored for other keys, which some layouts
 * need Shift to type. The key is matched by `e.key` only, so the letter the
 * layout produces is the one that counts.
 */
function teConfigShortcutMatches(e, key) {
  const parts = String(key).toLowerCase().split('+').filter(Boolean);
  const want = parts.pop();
  if (!want) return false;
  const mods = new Set(parts);
  if (mods.has('ctrl') !== e.ctrlKey || mods.has('alt') !== e.altKey || mods.has('meta') !== e.metaKey) {
    return false;
  }
  if ((e.key || '').toLowerCase() !== want) return false;
  return !/^[a-z0-9]$/.test(want) || mods.has('shift') === e.shiftKey;
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
    this.graphClient = null;
    this.graphAttachment = null;
  }

  /**
   * Tear the editor down: remove every listener it added (on the surface,
   * the document and the window), remove the DOM it created and destroy the
   * editing surface. Safe to call more than once. Returns every draft that
   * was not written into the document, tagged by where it came from, or []:
   * `{ kind: 'block', ... }` for the Blocks view's unapplied drafts (issue #19;
   * also sent as `te:blocks-drafts`) and `{ kind: 'alternative', ... }` for
   * text left in an alternatives-panel line (issue #10; also sent as
   * `te:alt-drafts`). Teardown never edits the document.
   */
  destroy() {
    if (this.destroyed) return [];
    this.destroyed = true;
    this.detachGraphDocument();
    // Older modules still support ordinary editor teardown without this export.
    if (this.graphRoleListenerApi) {
      this.graphRoleListenerApi.kg_set_role_change_listener(undefined);
      this.graphRoleListenerApi = null;
    }
    if (this.graphClient) this.graphClient.destroy();
    // The alternatives panel (issue #10) tears itself down when the editor's
    // signal aborts, so take its drafts first.
    const altDrafts = this.altPanel ? this.altPanel.destroy() || [] : [];
    this.abortController.abort();
    for (const node of this.createdNodes) {
      // Close an open dialog first so Shoelace releases its scroll lock.
      if (node.tagName === 'SL-DIALOG' && node.open) node.open = false;
      node.remove();
    }
    this.createdNodes = [];
    // Blocks view (issue #19): reports unapplied drafts, never edits.
    const drafts = (this.blocks ? this.blocks.destroy() : []).map((d) => ({ kind: 'block', ...d }));
    if (this.persistence) this.persistence.destroy(); // #76 save, open, drafts
    if (this.overflow) this.overflow.destroy();
    if (this.chrome) this.chrome.destroy();
    if (this.selectionMenu) this.selectionMenu.destroy();
    if (this.ghosts) this.ghosts.destroy();
    if (this.indicators) this.indicators.destroy();
    if (this.headings) this.headings.destroy();
    if (this.lab) this.lab.destroy();
    for (const d of altDrafts) drafts.push({ kind: 'alternative', ...d }); // #10
    if (this.decorations) this.decorations.destroy();
    if (this.surface) this.surface.destroy();
    // Host providers (#83) belong to this editor; the registry is the page's.
    for (const id of Array.from((this.alternativeProviders || new Map()).keys())) {
      this.unregisterAlternativesProvider(id);
    }
    this.warningListeners.clear();
    return drafts;
  }

  /**
   * Wire the editor to the template markup inside `root` (issue #77): the
   * element the Rust `mount_editor` rendered into, `#app` on the full page
   * or the embed wrapper's container. Every lookup is scoped to `root`, so
   * markup elsewhere on the page is never touched. Without `root`, `#app`
   * is used when present (the full page), otherwise the whole document.
   */
  initialize(root) {
    const scope = root || document.getElementById('app') || document;
    // Get DOM elements after template is rendered
    this.input = scope.querySelector('.markdown-input');
    this.toolbar = scope.querySelector('#formatting-toolbar');
    this.shortcutsList = scope.querySelector('#shortcuts-list');
    this.dialog = scope.querySelector('.shortcuts-dialog');
    this.helpButton = scope.querySelector('#show-help');

    // Check if elements exist
    if (!this.input || !this.toolbar || !this.shortcutsList || !this.dialog || !this.helpButton) {
      console.error('Required DOM elements not found');
      return;
    }
    // The editor's own root: panels mount here and the Write_On layout
    // rules key on its `te-app` class.
    this.root = scope === document
      ? this.input.closest('.te-app') || this.input.parentElement
      : scope;

    if (this.shortcuts.length === 0) {
      console.error('No shortcuts available');
      return;
    }

    this.surface = new EditorSurface(this.input);
    this.connectDocumentModel();
    this.observeRolePublications();

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
      // R-3.5 for every `#` heading in Write_On (issue #84).
      if (typeof window.TeHeadingLayer === 'function') {
        this.headings = new window.TeHeadingLayer(this, { signal: this.abortController.signal });
      }
      // Ghost layer and selection context menu (public/js/selection-menu.js,
      // issue #11).
      if (typeof window.TeGhostLayer === 'function') {
        this.ghosts = new window.TeGhostLayer(this, { signal: this.abortController.signal });
      }
      if (typeof window.TeSelectionMenu === 'function') {
        this.selectionMenu = new window.TeSelectionMenu(this, { signal: this.abortController.signal });
        // Knowledge-graph alternatives for selection (issue #13).
        this.selectionMenu.register(TeKg.menuItem(this));
      }
      // The Lab popover and its marks (public/js/lab.js, issue #14).
      if (typeof window.TeLabPopover === 'function') {
        this.lab = new window.TeLabPopover(this, { signal: this.abortController.signal });
      }
      // #10 The alternatives side panel (public/js/alternatives-panel.js).
      if (typeof window.TeAlternativesPanel === 'function') {
        this.altPanel = new window.TeAlternativesPanel(this, { signal: this.abortController.signal });
      }
    }

    // Blocks view (public/js/blocks.js, issue #19): a separate view of the
    // same body, created after the chrome so it can see the Write_On mode.
    if (typeof window.BlocksView === 'function') {
      this.blocks = new window.BlocksView(this, { signal: this.abortController.signal });
    }

    // Overflow panel (public/js/overflow.js, issue #12): after the chrome
    // (Write_On mode, the XYZ control) and the selection menu (its stash item).
    if (typeof window.TeOverflowPanel === 'function') {
      this.overflow = new window.TeOverflowPanel(this, { signal: this.abortController.signal });
    }

    // #76 #73 Save, open, autosave drafts and the Markdown export view
    // (public/js/persistence.js): last, so it sees the chrome and panels.
    if (typeof window.TePersistence === 'function') {
      this.persistence = new window.TePersistence(this, { signal: this.abortController.signal });
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
      button.setAttribute('content', String(shortcut.key));

      // Built node by node: config values are text, never markup.
      const slButton = document.createElement('sl-button');
      slButton.setAttribute('size', 'small');
      slButton.setAttribute('variant', 'default');
      slButton.appendChild(teConfigIcon(shortcut.name));
      button.appendChild(slButton);

      button.querySelector('sl-button').addEventListener('click', () => {
        this.wrapSelectedText(shortcut.prefix, shortcut.suffix);
      }, { signal: this.abortController.signal });

      this.toolbar.appendChild(button);
      this.createdNodes.push(button);
    });

    // Setup keyboard shortcuts
    this.input.addEventListener('keydown', (e) => {
      if (e.isComposing) return;
      const shortcut = this.shortcuts.find(s => teConfigShortcutMatches(e, s.key));

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
      const badge = teTextElement('sl-badge', shortcut.key);
      badge.setAttribute('variant', 'neutral');
      item.append(teConfigIcon(shortcut.name), teTextElement('span', shortcut.desc, 'shortcut-desc'), badge);
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
      item.append(teConfigIcon(cmd.icon), teTextElement('span', cmd.name));

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
   * The WASM document API: `config.bindings` (passed by the embed wrapper,
   * issue #77), else `window.wasmBindings` as in the Trunk build. Returns
   * null when the module does not expose it, in which case the editor still
   * works as a plain Markdown editor.
   */
  documentApi() {
    const api = (this.config && this.config.bindings) || window.wasmBindings;
    return api && typeof api.open_document === 'function' && typeof api.apply_edit === 'function'
      ? api
      : null;
  }

  requireDocumentApi() {
    const api = this.documentApi();
    if (!api) throw new Error('The WASM document API is not available');
    return api;
  }

  // Cohort C: synchronous, acknowledged native checkpoints. These adapters
  // never align/resync a refused transaction and never serialize model clones.
  ghostHistoryApi() {
    if (this.destroyed || this.surface.destroyed) throw new Error('Ghost history is unavailable after editor destruction');
    if (this.ghostHistoryConflict) throw new Error('Ghost history conflict: explicitly reopen the document to recover');
    if (this.ghostHistoryActive) throw new Error('A ghost history transaction is already active');
    const api = this.requireDocumentApi();
    for (const name of ['begin_trim_ghost_transaction', 'begin_document_history_replay',
      'finish_document_history_transaction', 'abort_document_history_transaction']) {
      if (typeof api[name] !== 'function') throw new Error(`Missing native checkpoint API: ${name}`);
    }
    return api;
  }

  ghostHistoryNotice(error) {
    const message = error && (error.message || error.error) || String(error);
    this.showWarning(`Ghost history refused: ${message}`, 'ghost-history');
  }

  ghostHistoryRole(api) {
    return typeof api.kg_role_snapshot_json === 'function' ? api.kg_role_snapshot_json() : null;
  }

  ghostHistoryCheckpoint(api) {
    const s = this.surface;
    return {
      history: structuredClone(s.history), historyIndex: s.historyIndex, historyLimit: s.historyLimit,
      text: s.text, selection: { ...(s.readSelection() || s.lastSelection) },
      lastSelection: { ...s.lastSelection }, decorations: structuredClone(s.decorations),
      nextDecorationId: s.nextDecorationId, pendingHint: s.pendingHint, pendingSource: s.pendingSource,
      nodes: Array.from(s.root.childNodes), role: this.ghostHistoryRole(api),
      layers: this.decorations ? new Map(Array.from(this.decorations.layers, ([name, items]) => [name, structuredClone(items)])) : null,
      registryPending: this.decorations && this.decorations.pending,
      ghosts: this.ghosts ? {ghosts: structuredClone(this.ghosts.ghosts), modelGhosts: this.ghosts.modelGhosts,
        count: this.ghosts.count, refreshCount:this.ghosts.refreshCount} : null,
    };
  }

  restoreGhostHistoryCheckpoint(saved, api) {
    const s = this.surface;
    const roleChanged = this.ghostHistoryRole(api) !== saved.role;
    Object.assign(s, {history:saved.history, historyIndex:saved.historyIndex, historyLimit:saved.historyLimit,
      text:saved.text, lastSelection:saved.lastSelection, decorations:saved.decorations,
      nextDecorationId:saved.nextDecorationId, pendingHint:saved.pendingHint, pendingSource:saved.pendingSource});
    if (saved.layers) {
      // B's separately retired fades are authoritative, not transaction-owned.
      const retired = this.decorations.layers.get('lab-trim');
      this.decorations.layers = saved.layers;
      this.decorations.pending = saved.registryPending;
      if (roleChanged) this.decorations.layers.set('lab-trim', retired || []);
    }
    if (saved.ghosts) Object.assign(this.ghosts, saved.ghosts);
    if (roleChanged) {
      s.decorations = s.decorations.filter((item) => !String(item.id).startsWith('lab-trim:'));
      s.render(); // do not resurrect DOM fades from the old role
    } else s.root.replaceChildren(...saved.nodes);
    s.setSelectionOffsets(saved.selection.start, saved.selection.end, saved.selection.direction);
    s.lastSelection = saved.lastSelection;
  }

  static ghostAcknowledgement(value, token, field) {
    if (!value || value.ok !== true || value[field] !== true || value.token !== token) {
      throw new Error(`Native ${field} acknowledgement is missing or invalid`);
    }
  }

  runGhostHistoryTransaction(api, begin, prepare) {
    const saved = this.ghostHistoryCheckpoint(api);
    let started;
    let returned = false;
    this.ghostHistoryActive = true;
    try {
      started = begin();
      returned = true;
      if (started === null) return null;
      if (!started || started.ok !== true || typeof started.token !== 'string' || !started.token) {
        throw new Error('Native begin acknowledgement is invalid');
      }
      prepare(started, saved);
      if (api.document_body() !== this.surface.getText()) throw new Error('Tentative surface/native body mismatch');
      MarkdownEditor.ghostAcknowledgement(api.finish_document_history_transaction(started.token), started.token, 'finalized');
      return started;
    } catch (error) {
      if (started && typeof started.token === 'string') {
        let retired = false;
        try {
          MarkdownEditor.ghostAcknowledgement(api.abort_document_history_transaction(started.token), started.token, 'rolledBack');
          retired = true;
          this.restoreGhostHistoryCheckpoint(saved, api);
        } catch (abortError) {
          this.ghostHistoryConflict = {token:retired ? null : started.token, nativeRetired:retired};
          throw new Error(`Ghost history conflict; ${retired ? 'native rollback acknowledged but surface restoration failed' : 'rollback not acknowledged'}. Explicitly reopen to recover. ${abortError.error || abortError.message || abortError}`);
        }
      } else if (returned) {
        // Invalid successful transport cannot be treated as an atomic refusal.
        this.ghostHistoryConflict = {token:null, nativeRetired:false};
      }
      throw error;
    } finally {
      this.ghostHistoryActive = false;
    }
  }

  paintGhostHistory(annotations, applied = []) {
    const s = this.surface;
    for (const {edit, text} of applied) {
      s.decorations = EditorSurface.mapRanges(s.decorations, edit);
      if (this.decorations) this.decorations.mapEdit(edit);
      s.text = text;
    }
    if (this.ghosts) this.ghosts.refresh(annotations);
    else if (this.decorations) this.decorations.set('ghosts', annotations.ghosts.map((g) => ({
      id:g.id, start:g.anchor.start, end:g.anchor.end, className:'te-ghost',
    })));
    s.render();
  }

  commitTrimGhosts(level, kept) {
    const s = this.surface;
    if (!Number.isInteger(s.historyLimit) || s.historyLimit < 2 || s.historyIndex < 0) {
      throw new Error('History capacity must retain a base state and one ghost operation');
    }
    const api = this.ghostHistoryApi();
    if (!Array.isArray(kept) || kept.length > 4096) throw new Error('Too many kept indices');
    const body = s.getText();
    const keptJson = JSON.stringify(kept);
    MarkdownEditor.ghostTransportBounds(body, body, keptJson);
    s.ghostHistoryAdapter = this;
    const result = this.runGhostHistoryTransaction(api,
      () => api.begin_trim_ghost_transaction(body, level, keptJson), (started, saved) => {
        if (!started.change || !started.annotations) throw new Error('Missing native ghost receipt/annotations');
        s.record('trim-ghost', s.lastSelection, {start:0,deletedText:'',insertedText:'',
          ghost:{change:structuredClone(started.change),undo:false}});
        // Recording must really retain exactly one undoable receipt, including
        // after eviction/redo truncation. Injected partial record failures abort.
        const top = s.history[s.historyIndex];
        if (s.text !== body || s.historyIndex < 1 || !top || top.text !== body || top.edits?.length !== 1 ||
            JSON.stringify(top.edits[0].ghost) !== JSON.stringify({change:started.change,undo:false})) {
          throw new Error('Ghost receipt was not retained as an undoable history step');
        }
        this.paintGhostHistory(started.annotations);
        s.setSelectionOffsets(saved.selection.start, saved.selection.end, saved.selection.direction);
      });
    if (result) this.notifyGhostHistory('insertReplacementText');
    return result;
  }

  static ghostTransportBounds(body, target, json) {
    const encoder = new TextEncoder();
    if ([body,target,json].some((text) => encoder.encode(text).length > 16 * 1024 * 1024)) {
      throw new Error('Ghost transaction transport exceeds 16 MiB');
    }
  }

  static nativeGhostHistoryOperations(body, steps, target) {
    if (!Array.isArray(steps) || !steps.some((step) => step && step.ghost)) throw new Error('Ghost history metadata is missing; text-diff fallback is forbidden');
    if (steps.length > 4096) throw new Error('Too many history operations');
    const allowed = ['start','deletedText','insertedText','ghost','alt','move','swap','overflow'];
    for (const step of steps) {
      if (Object.keys(step).some((key) => !allowed.includes(key)) ||
          ['ghost','alt','move','swap','overflow'].filter((key) => step[key]).length > 1 ||
          !Number.isInteger(step.start) || step.start < 0 || step.start > 0xffffffff ||
          typeof step.deletedText !== 'string' || typeof step.insertedText !== 'string') {
        throw new Error('Invalid or conflicting ghost-group history metadata');
      }
      if (step.ghost && (typeof step.ghost.undo !== 'boolean' || step.deletedText || step.insertedText)) {
        throw new Error('Ghost receipt must be annotation-only with an exact direction');
      }
      for (const [key, fields] of Object.entries({ghost:['change','undo'],alt:['span','before','after'],
        move:['start','end','to'],swap:['span','from','to'],overflow:['before','after']})) {
        if (step[key] && (Object.keys(step[key]).length !== fields.length ||
            fields.some((field) => !Object.hasOwn(step[key], field)))) {
          throw new Error('Unknown or missing ghost-group metadata fields');
        }
      }
    }
    const applied = EditorSurface.replaySteps(body, steps, target);
    if (!applied) throw new Error('Ghost history exact steps do not reproduce the target body');
    const operations = applied.map(({edit, text}) => {
      if (edit.ghost) return {kind:'ghost',body:text,...edit.ghost};
      const base = {body:text,start:edit.start,deletedText:edit.deletedText,insertedText:edit.insertedText};
      if (edit.alt) return {...base,kind:'alt',span:edit.alt.span,target:edit.alt.after};
      if (edit.move) return {...base,kind:'move',move:edit.move};
      if (edit.swap) return {...base,kind:'swap',...edit.swap};
      if (edit.overflow) return {...base,kind:'overflow',...edit.overflow};
      return {...base,kind:'edit'};
    });
    const json = JSON.stringify(operations);
    MarkdownEditor.ghostTransportBounds(body, target, json);
    return {applied,json};
  }

  replayGhostHistory(index, source, steps) {
    const api = this.ghostHistoryApi();
    const s = this.surface;
    const entry = s.history[index];
    const {applied,json} = MarkdownEditor.nativeGhostHistoryOperations(s.getText(), steps, entry.text);
    const result = this.runGhostHistoryTransaction(api,
      () => api.begin_document_history_replay(s.getText(), entry.text, json), (started) => {
        if (!started.annotations) throw new Error('Missing native history annotations');
        s.historyIndex = index;
        this.paintGhostHistory(started.annotations, applied);
        const last = applied[applied.length - 1];
        if (source === 'undo' && last) s.setSelectionOffsets(last.edit.start + last.edit.insertedText.length);
        else s.setSelectionOffsets(entry.start, entry.end);
      });
    if (!result) throw new Error('Native mixed history begin returned an invalid no-op');
    this.notifyGhostHistory(source === 'undo' ? 'historyUndo' : 'historyRedo', result.annotations);
  }

  notifyGhostHistory(inputType, annotations = null) {
    // Native and registry already replayed every edit. Notify observers once
    // without an edit envelope (so ranges are not mapped twice), only after
    // final acknowledgement. Suppress only the duplicate model mirror.
    try {
      if (annotations) {
        const count = annotations.setAside.spans.length + annotations.setAside.ghosts.length;
        const notice = count === 0 ? null : `${count} ${count === 1 ? 'annotation (alternatives or ghost) is' : 'annotations (alternatives or ghosts) are'} out of step with the text. ${count === 1 ? 'It is' : 'They are'} preserved and saved with the document, and ${count === 1 ? 'is' : 'are'} re-attached if the text returns.`;
        // Publish only the final acknowledged native aggregate, never a
        // tentative edit outcome or a second resync. Reuse malformed policy.
        this.reflectSetAside(count, notice);
      }
      const previous = this.suppressModelSync;
      this.suppressModelSync = true;
      try {
        this.surface.emitChange(null, inputType === 'historyUndo' ? 'undo' : inputType === 'historyRedo' ? 'redo' : 'trim');
      } finally { this.suppressModelSync = previous; }
      if (this.indicators) this.indicators.flush();
      this.surface.dispatchInput(inputType, null);
    }
    catch (error) { this.ghostHistoryNotice(error); }
  }

  /** Subscribe the model to surface edits and align it with the surface. */
  connectDocumentModel() {
    const api = this.documentApi();
    if (api) api.sync_document_body(this.surface.getText());
    this.surface.onChange((change) => this.mirrorEdit(change));
  }

  mirrorEdit(change) {
    if (this.suppressModelSync) return;
    if (this.ghostHistoryConflict) {
      this.ghostHistoryNotice(new Error('Explicitly reopen before mirroring edits after a ghost history conflict'));
      return;
    }
    const api = this.documentApi();
    if (!api) return;
    if (change.edit && change.edit.alt && typeof api.alt_restore === 'function') {
      this.mirrorAltStep(api, change.edit, change.text); // #10
      return;
    }
    const { edit, text } = change;
    if (edit.swap && typeof api.set_active_alternative === 'function') {
      this.mirrorSwap(api, edit.swap, text);
      return;
    }
    if (edit.overflow && typeof api.replay_document_overflow === 'function') {
      this.mirrorOverflow(api, edit, text);
      return;
    }
    let outcome;
    try {
      // A recorded text move (undo or redo of moveRange) is replayed as a
      // move, so the annotations inside the moved text travel with it.
      outcome = edit.move && typeof api.move_document_range === 'function'
        ? api.move_document_range(edit.move.start, edit.move.end, edit.move.to)
        : api.apply_edit(edit.start, edit.deletedLength, edit.insertedText);
    } catch (err) {
      // The model refused the edit (for example a stale ghost): fall back to
      // replacing its body with the surface text and re-anchoring by text.
      this.reflectSync(api.sync_document_body(text));
      return;
    }
    if (outcome) this.reflectSetAside(outcome.setAside, outcome.notice, outcome.warning);
  }

  /**
   * Move the text [start, end) so that it is inserted at `to` (UTF-16
   * offsets; `to` is an insertion point in the current text, outside the
   * range), keeping the alternatives and ghosts inside it (issue #44). This
   * is the call the Blocks view uses to move a block up or down.
   *
   * The document model moves first (`move_document_range`), so a move it
   * refuses (one that would split a span or ghost, or an invalid range or
   * destination) throws and leaves the text and the model untouched. The
   * surface then replaces the region from min(start, to) to max(end, to)
   * with the reordered text as ONE undo step, without mirroring it through
   * `apply_edit` (which would detach the spans and drop the ghosts inside).
   * The step carries the move, so undo and redo replay it in the model as
   * the inverse move and the move again: text and annotations both return.
   *
   * The model validates every move, even one that leaves the text as it is.
   * `to` at either end of the range is a no-op: after the model's checks it
   * returns { moved: false } and records nothing. Moving text past an
   * identical copy (for example the first "ab" of "abab" to the end) leaves
   * the text unchanged but still moves the annotations, so it is recorded as
   * an undo step with no text change that carries the move.
   * Returns { moved, start, end } (the moved text's new range) plus the
   * model outcome ({ detached, reattached, setAside, warning, notice }).
   */
  moveRange(start, end, to) {
    const api = this.requireDocumentApi();
    if (typeof api.move_document_range !== 'function') {
      throw new Error('The WASM document API cannot move text');
    }
    this.alignDocumentModel(api);
    // Throws, changing nothing, if the model refuses the move (invalid range
    // or destination, or an item it would split).
    const outcome = api.move_document_range(start, end, to);
    if (to === start || to === end) return { ...outcome, moved: false, start, end };
    const text = this.surface.getText();
    const len = end - start;
    const lo = Math.min(start, to);
    const hi = Math.max(end, to);
    const moved = text.slice(start, end);
    const reordered = to > end ? text.slice(end, to) + moved : moved + text.slice(to, start);
    const newStart = to > end ? to - len : to;
    this.suppressModelSync = true;
    try {
      this.surface.replaceRange(lo, hi, reordered, {
        source: 'move',
        move: { start, end, to },
        selectStart: newStart,
        selectEnd: newStart + len,
      });
    } finally {
      this.suppressModelSync = false;
    }
    if (outcome) this.reflectSetAside(outcome.setAside, outcome.notice, outcome.warning);
    if (this.indicators) this.indicators.flush();
    return { ...outcome, moved: true, start: newStart, end: newStart + len };
  }

  // ---------------------------------------------------------------------
  // In-place cycling of alternatives (issue #9). Kept in one block so
  // parallel additions to this class merge cleanly. The hover and keyboard
  // controls live in public/js/indicators.js; design notes in
  // docs/design/cycling.md.
  // ---------------------------------------------------------------------

  /**
   * Make alternative `index` of span `spanId` active, in place (R-2.4).
   *
   * The document model swaps first (`set_active_alternative`, which also
   * fixes a preceding "a"/"an", R-2.6, and refreshes the anchors' context),
   * so a swap it refuses (unknown span, invalid index, stale anchor) throws
   * and leaves the text and the model untouched. The surface then applies
   * the exact edit the model made as ONE undo step, without mirroring it
   * through `apply_edit` (which would detach the span). The step carries
   * { span, from, to }, so undo and redo replay `set_active` in the model:
   * the alternative and the article both come back.
   *
   * The selection is kept where it was (moved with the text after the
   * edit); a selection end inside the edited region is clamped into the
   * span's new text. Making the active alternative active again changes and
   * records nothing. Returns the model outcome
   * ({ span, from, to, edit, range, setAside, notice, ... }).
   *
   * A derived knowledge-graph span (id `kg-…`, issue #13) swaps through
   * `kg_swap_alternative` instead, which applies the core's edits
   * (capitalisation and a/an included) to the model as one edit. Its step
   * is a plain text step tagged `kg` (not `swap`): undo and redo mirror it
   * through `apply_edit`, and the KG spans re-derive from the text.
   */
  swapAlternative(spanId, index) {
    const api = this.requireDocumentApi();
    const isKg = TeKg.isKgSpan(spanId);
    const swapFn = isKg ? api.kg_swap_alternative : api.set_active_alternative;
    if (typeof swapFn !== 'function') {
      throw new Error('The WASM document API cannot swap alternatives');
    }
    this.alignDocumentModel(api);
    // Throws, changing nothing, if the model refuses the swap.
    const outcome = swapFn(String(spanId), index);
    const edit = outcome && outcome.edit;
    if (!edit) return outcome;
    const editEnd = edit.start + edit.deletedLength;
    const delta = edit.insertedText.length - edit.deletedLength;
    const { start: spanStart, end: spanEnd } = outcome.range;
    const map = (o) => {
      if (o <= edit.start) return o;
      if (o >= editEnd) return o + delta;
      return Math.max(spanStart, Math.min(spanEnd, o));
    };
    const sel = this.surface.getSelectionOffsets();
    this.suppressModelSync = true;
    try {
      const tag = { span: outcome.span, from: outcome.from, to: outcome.to };
      this.surface.replaceRange(edit.start, editEnd, edit.insertedText, {
        source: 'swap',
        ...(isKg ? { kg: tag } : { swap: tag }),
        selectStart: map(sel.start),
        selectEnd: map(sel.end),
      });
    } finally {
      this.suppressModelSync = false;
    }
    this.reflectSetAside(outcome.setAside, outcome.notice, outcome.warning);
    if (this.indicators) this.indicators.flush();
    return outcome;
  }

  /**
   * Replay a recorded swap (undo or redo of swapAlternative) in the model.
   * If the model refuses, or its body does not come out equal to the
   * surface text (for example undoing back to an article the author had
   * typed against the a/an rule), the body is re-synced from the surface,
   * so the two never diverge.
   */
  mirrorSwap(api, swap, text) {
    let outcome = null;
    try {
      outcome = api.set_active_alternative(String(swap.span), swap.to);
    } catch (err) {
      outcome = null;
    }
    if (!outcome || api.document_body() !== text) {
      this.reflectSync(api.sync_document_body(text));
      return;
    }
    this.reflectSetAside(outcome.setAside, outcome.notice, outcome.warning);
  }

  // ---------------------------------------------------------------------
  // End of in-place cycling (issue #9).
  // ---------------------------------------------------------------------

  // ---------------------------------------------------------------------
  // Knowledge-graph alternatives (issue #13). Kept in one block so parallel
  // additions to this class merge cleanly; see TeKg above and
  // docs/design/kg-alternatives.md.
  // ---------------------------------------------------------------------

  /**
   * Load a thesaurus (terraphim's thesaurus JSON, as text or an object) for
   * KG alternatives, replacing the current one, and redraw the indicators.
   * Returns { name, concepts, skipped }; throws, keeping the previous
   * thesaurus, if it cannot be read.
   */
  loadThesaurus(json) {
    const api = this.requireDocumentApi();
    if (typeof api.kg_load_thesaurus !== 'function') {
      throw new Error('The WASM document API has no knowledge-graph support');
    }
    const summary = api.kg_load_thesaurus(typeof json === 'string' ? json : JSON.stringify(json));
    if (!this.graphRoleListenerApi) this.syncGraphRole();
    this.redrawRoleIndicators();
    return summary;
  }

  /** Forget the thesaurus: no KG spans until the next load. */
  clearThesaurus() {
    const api = this.documentApi();
    if (api && typeof api.kg_clear_thesaurus === 'function') api.kg_clear_thesaurus();
    if (!this.graphRoleListenerApi) this.syncGraphRole();
    this.redrawRoleIndicators();
  }

  /** Optional, lazy Worker graph service. Ordinary editing never needs it. */
  get rolegraph() {
    if (this.destroyed) throw new Error('The editor is destroyed');
    if (this.graphClient) return this.graphClient;
    if (typeof window.TeRoleGraphClient !== 'function') throw new Error('The graph client script is unavailable');
    const api = this.requireDocumentApi();
    for (const name of ['kg_role_snapshot_json', 'kg_prepare_role', 'kg_commit_prepared_role', 'kg_discard_prepared_role', 'kg_set_role_change_listener']) {
      if (typeof api[name] !== 'function') throw new Error('The WASM module has no browser role coordination API');
    }
    this.observeRolePublications();
    const options = Object.assign({}, this.config.rolegraph || {}, {
      assetBase: this.config.rolegraphAssetBase || new URL('.', document.baseURI).href,
      roleCoordinator: {
        prepare: (text) => api.kg_prepare_role(text),
        commit: (token) => {
          // Check before publishing: activation must follow the authoritative generation.
          const current = JSON.parse(api.kg_role_snapshot_json());
          if (String(BigInt(current.generation) + 1n) !== this.graphClient.generation) {
            throw new Error('Stale graph role publication');
          }
          let result;
          // Only this synchronous commit is already activated in this Worker.
          // External prepared commits, like legacy load/clear, must synchronize.
          this.graphRoleCommitInProgress = true;
          try { result = JSON.parse(api.kg_commit_prepared_role(token)); }
          finally { this.graphRoleCommitInProgress = false; }
          this.redrawRoleIndicators();
          return result;
        },
        discard: (token) => api.kg_discard_prepared_role(token),
      },
      onStateChange: (state) => this.graphStateChanged(state),
    });
    const client = this.graphClient = new window.TeRoleGraphClient(options);
    client.attachDocument = (options) => this.attachGraphDocument(options);
    client.detachDocument = () => this.detachGraphDocument();
    this.syncGraphRole();
    return client;
  }

  /** One authoritative observer for ordinary KG and optional graph consumers. */
  observeRolePublications() {
    const api = this.documentApi();
    if (!api || typeof api.kg_set_role_change_listener !== 'function' ||
        typeof api.kg_role_snapshot_json !== 'function' || this.graphRoleListenerApi) return;
    api.kg_set_role_change_listener(() => {
      if (this.destroyed) return;
      // Every successful publication reaches Lab, including coordinated commits.
      // Event detail contains metadata only, never aliases or document text.
      try {
        const snapshot = JSON.parse(api.kg_role_snapshot_json());
        this.root.dispatchEvent(new CustomEvent('te:role-change', {
          bubbles: true,
          detail: { editor: this, generation: snapshot.generation,
            loaded: snapshot.loaded === true, protectionReady: snapshot.protectionReady === true },
        }));
      } catch (_) { /* Notification cannot undo authoritative publication. */ }
      if (this.graphRoleCommitInProgress) return;
      try { this.syncGraphRole(); } catch (_) { /* Rust also isolates observers. */ }
    });
    this.graphRoleListenerApi = api;
  }

  /** Notify only an already-requested graph; retain synchronous KG load/clear. */
  syncGraphRole() {
    if (!this.graphClient || this.destroyed) return;
    const api = this.documentApi();
    const snapshot = JSON.parse(api.kg_role_snapshot_json());
    this.graphRoleSync = this.graphClient.syncRole(snapshot);
    this.graphRoleSync.catch(() => {}); // State event reports failure; main KG stays valid.
  }

  /** Redraw is not part of role publication and cannot undo a successful commit. */
  redrawRoleIndicators() {
    try {
      if (this.indicators) this.indicators.flush();
    } catch (_) {
      // Keep the main KG/Worker associated even if a renderer fails.
      try {
        if (this.root && !this.destroyed) this.root.dispatchEvent(new CustomEvent('te:rolegraph-error', {
          bubbles: true, detail: { editor: this, code: 'render_failed' },
        }));
      } catch (_) { /* Reporting failure cannot change role commit semantics. */ }
    }
  }

  graphStateChanged(state) {
    if (!this.root || this.destroyed) return;
    this.root.dataset.rolegraphState = state.state;
    this.root.dispatchEvent(new CustomEvent('te:rolegraph-state', {
      bubbles: true, detail: Object.assign({ editor: this }, state),
    }));
  }

  /** Explicitly index original surface Markdown; never silently use clean export. */
  attachGraphDocument({ id, indexSource = 'markdown' } = {}) {
    if (typeof id !== 'string' || !id.trim() || indexSource !== 'markdown') {
      return Promise.reject(new Error('Graph attachment requires an id and markdown source'));
    }
    const client = this.rolegraph;
    this.detachGraphDocument();
    const attachment = this.graphAttachment = { id, timer: null };
    const index = () => {
      if (this.destroyed || this.graphAttachment !== attachment) return;
      clearTimeout(attachment.timer);
      attachment.timer = setTimeout(() => {
        if (this.destroyed || this.graphAttachment !== attachment || client.state !== 'ready' || !client.hasRole) return;
        client.indexDocuments([{ id, title: id, body: this.surface.getText() }]).catch((error) => {
          if (!this.destroyed && this.graphAttachment === attachment) {
            this.root.dispatchEvent(new CustomEvent('te:rolegraph-error', {
              bubbles: true, detail: { editor: this, code: error.code || 'worker_failed' },
            }));
          }
        });
      }, 120);
    };
    attachment.unsubscribe = this.surface.onChange(index);
    attachment.unsubscribeState = client.onStateChange(({ state }) => { if (state === 'ready') index(); });
    return (async () => {
      // Follow the current lane, including a load/import that superseded editor sync.
      // Only a superseded lane's failure is obsolete; current failures propagate.
      let lane, epoch;
      do {
        lane = client.lifecycle; epoch = client.epoch;
        try { await lane; } catch (error) { if (lane === client.lifecycle) throw error; }
        if (this.destroyed || this.graphAttachment !== attachment) throw new Error('Graph attachment cancelled');
      } while (lane !== client.lifecycle);
      if (client.destroyed || epoch !== client.epoch || client.state !== 'ready' || !client.hasRole) {
        throw new Error('Graph attachment requires a ready current role');
      }
      return client.indexDocuments([{ id, title: id, body: this.surface.getText() }]);
    })().catch((error) => {
      if (this.graphAttachment === attachment) this.detachGraphDocument();
      throw error;
    });
  }

  /** Detach without deleting a corpus entry or durable checkpoint. */
  detachGraphDocument() {
    const attachment = this.graphAttachment;
    if (!attachment) return;
    clearTimeout(attachment.timer);
    if (attachment.unsubscribe) attachment.unsubscribe();
    if (attachment.unsubscribeState) attachment.unsubscribeState();
    this.graphAttachment = null;
  }

  /**
   * The KG term at the selection [start, end) (UTF-16; a caret just after a
   * word counts) with its alternatives, or null. Read-only.
   */
  kgLookup(start, end) {
    const api = this.documentApi();
    if (!api || typeof api.kg_lookup_selection !== 'function') return null;
    this.alignDocumentModel(api);
    return api.kg_lookup_selection(start, end);
  }

  /**
   * "AI alternatives for selection" (Ctrl+Shift+G, R-7.3, R-8.6): append the
   * KG synonyms of the term at the selection to the span over that term
   * (created when there is none), bot-marked (`source: "ai"`, `model:
   * "kg"`) and after the writer's own alternatives. One undo step, through
   * the #10 panel path (alternativeOp). The panel, when present and in
   * Write_On mode, then shows the span. Returns the change, or null when
   * there was no KG term or nothing new to add.
   */
  kgAlternativesForSelection(ctx = null) {
    const sel = ctx || this.surface.getSelectionOffsets();
    let change = null;
    try {
      change = this.alternativeOp('kg_append', sel.start, sel.end);
    } catch (err) {
      console.warn('No knowledge-graph alternatives for the selection', err);
      return null;
    }
    return this.showAlternativesFor(sel, change);
  }

  /** Open the panel (Write_On) on the span the append touched. */
  showAlternativesFor(sel, change) {
    const term = this.kgLookup(sel.start, sel.end);
    const spanId = change ? change.span : term && term.span;
    if (spanId && this.altPanel && typeof this.altPanel.open === 'function' && this.altPanel.isWriteOn()) {
      this.altPanel.open({ spanId });
    }
    return change;
  }

  // ---------------------------------------------------------------------
  // Alternatives providers (issue #83, R-8.7; docs/design/provider-seam.md).
  // The thesaurus is the default provider (src/providers.rs). A host page
  // can add asynchronous ones, for example a future LLM provider calling
  // terraphim_server's /chat:
  //
  //   editor.registerAlternativesProvider({
  //     id: 'llm',
  //     model: 'llama-3',            // recorded as the line's model
  //     alternatives: async ({ text, body, start, end }) =>
  //       ['alt one', { text: 'alt two', model: 'llama-3-70b' }],
  //   });
  //
  // Results are appended after the thesaurus's (and earlier providers'), as
  // source "ai" with the model name, de-duplicated, as ONE undo step.
  // ---------------------------------------------------------------------

  /**
   * Register an asynchronous provider after the ones already there.
   * `provider.id` is a non-empty string, `provider.alternatives(context)` a
   * function returning (a promise of) an array of strings or
   * `{ text, model }`; `provider.model` names the model (default: the id).
   * Re-registering an id replaces the function and keeps its place.
   * Returns a function that unregisters it.
   */
  registerAlternativesProvider(provider) {
    if (!provider || typeof provider.id !== 'string' || !provider.id) {
      throw new Error('An alternatives provider needs an id');
    }
    if (typeof provider.alternatives !== 'function') {
      throw new Error('An alternatives provider needs an alternatives(context) function');
    }
    if (provider.id === 'thesaurus') throw new Error('The provider id "thesaurus" is reserved');
    const api = this.requireDocumentApi();
    if (typeof api.alternatives_provider_register !== 'function') {
      throw new Error('The WASM document API cannot register alternatives providers');
    }
    if (!this.alternativeProviders) this.alternativeProviders = new Map();
    const model = typeof provider.model === 'string' && provider.model ? provider.model : provider.id;
    // The WASM registry is the page's (one document session), so entries
    // are counted per id across editors: the id leaves it only when the
    // last editor holding it lets go.
    api.alternatives_provider_register(provider.id, model); // throws before any count changes
    const entry = { provider, model };
    const previous = this.alternativeProviders.get(provider.id);
    const holders = (TeProviderRefs.get(provider.id) || []).filter((h) => h !== previous);
    holders.push(entry);
    TeProviderRefs.set(provider.id, holders);
    this.alternativeProviders.set(provider.id, entry);
    // The handle only removes its own registration, never a later one.
    return () => this.unregisterAlternativesProvider(provider.id, entry);
  }

  /** Remove a provider registered here; false when there was none. */
  unregisterAlternativesProvider(id, entry = null) {
    if (!this.alternativeProviders || !this.alternativeProviders.has(id)) return false;
    if (entry && this.alternativeProviders.get(id) !== entry) return false;
    const held = this.alternativeProviders.get(id);
    this.alternativeProviders.delete(id);
    const api = this.documentApi();
    const holders = (TeProviderRefs.get(id) || []).filter((h) => h !== held);
    if (holders.length > 0) {
      // Others still hold the id: keep it, advertising a remaining holder's model.
      TeProviderRefs.set(id, holders);
      const last = holders[holders.length - 1];
      if (api) api.alternatives_provider_register(id, last.model);
      return true;
    }
    TeProviderRefs.delete(id);
    if (api && typeof api.alternatives_provider_unregister === 'function') {
      api.alternatives_provider_unregister(id);
    }
    return true;
  }

  /** The ids of every provider, thesaurus first, in the order they are asked. */
  alternativesProviderIds() {
    const api = this.documentApi();
    return api && typeof api.alternatives_provider_ids === 'function' ? api.alternatives_provider_ids() : [];
  }

  /** True when a host page has registered an asynchronous provider. */
  hasAlternativesProviders() {
    return !!this.alternativeProviders && this.alternativeProviders.size > 0;
  }

  /**
   * "AI alternatives for selection" through every provider (R-8.7). With no
   * host provider this is exactly kgAlternativesForSelection, and runs
   * synchronously. Otherwise each host provider is awaited (a provider that
   * throws or returns something that is not an array is skipped, with a
   * warning), then the thesaurus and host results are appended in provider
   * order as ONE undo step. If the text changed while waiting, nothing is
   * appended. Resolves to the change, or null.
   */
  async alternativesForSelection(ctx = null) {
    if (!this.hasAlternativesProviders()) return this.kgAlternativesForSelection(ctx);
    const sel = ctx ? { start: ctx.start, end: ctx.end } : this.surface.getSelectionOffsets();
    const body = this.surface.getText();
    const start = Math.min(sel.start, sel.end);
    const end = Math.max(sel.start, sel.end);
    const term = this.kgLookup(start, end);
    const target = term ? term.range : { start, end };
    const context = { text: body.slice(target.start, target.end), body, start: target.start, end: target.end };
    const entries = Array.from(this.alternativeProviders.values());
    const providers = entries;
    const answers = await Promise.all(
      providers.map(async ({ provider, model }) => {
        try {
          const result = await provider.alternatives({ ...context });
          if (!Array.isArray(result)) throw new Error('alternatives() must resolve to an array');
          // This editor's model name, not the registry's (editors may share an id).
          return result.map((r) => (typeof r === 'string' ? { text: r, model } : r && typeof r === 'object' ? { ...r, model: r.model || model } : r));
        } catch (err) {
          console.warn(`Alternatives provider ${provider.id} failed`, err);
          return [];
        }
      }),
    );
    if (this.destroyed || this.surface.getText() !== body) {
      console.warn('The text changed while alternatives were being fetched; nothing appended');
      return null;
    }
    const supplied = {};
    providers.forEach((entry, i) => {
      // A provider replaced or removed while we waited: its answers are stale.
      if (this.alternativeProviders.get(entry.provider.id) !== entry) return;
      supplied[entry.provider.id] = answers[i];
    });
    let change = null;
    try {
      change = this.alternativeOp('append_alternatives', start, end, JSON.stringify(supplied));
    } catch (err) {
      console.warn('No alternatives for the selection', err);
      return null;
    }
    return this.showAlternativesFor({ start, end }, change);
  }

  // ---------------------------------------------------------------------
  // End of knowledge-graph alternatives (issue #13).
  // ---------------------------------------------------------------------

  // ---------------------------------------------------------------------
  // Overflow stash (issue #12). Kept in one block so parallel additions to
  // this class merge cleanly. The panel lives in public/js/overflow.js.
  // ---------------------------------------------------------------------

  /**
   * Stash the text [start, end) in the Overflow (R-6.3): a move, not a copy.
   *
   * The document model goes first (`stash_document_range`): it removes the
   * text through the normal edit path, so spans whose text it touches are
   * detached and set aside and ghosts inside it are set aside (both come
   * back on undo), and appends the text to the overflow. A stash it refuses
   * (empty or invalid range, or a preserved malformed block whose save would
   * drop the overflow) throws and changes nothing. The surface then removes
   * the text as ONE undo step carrying { before, after } of the overflow, so
   * undo and redo restore the text and the overflow together (the overflow
   * half is replayed by mirrorOverflow). Returns the model outcome
   * ({ edit, overflow: { before, after }, detached, setAside, ... }).
   */
  stashRange(start, end) {
    const api = this.requireDocumentApi();
    if (typeof api.stash_document_range !== 'function') {
      throw new Error('The WASM document API cannot stash text');
    }
    this.alignDocumentModel(api);
    // Throws, changing nothing, if the model refuses the stash.
    const outcome = api.stash_document_range(Math.min(start, end), Math.max(start, end));
    const edit = outcome.edit;
    this.suppressModelSync = true;
    try {
      this.surface.replaceRange(edit.start, edit.start + edit.deletedLength, '', {
        source: 'stash',
        overflow: { before: outcome.overflow.before, after: outcome.overflow.after },
        selectStart: edit.start,
        selectEnd: edit.start,
      });
    } finally {
      this.suppressModelSync = false;
    }
    this.reflectSetAside(outcome.setAside, outcome.notice, outcome.warning);
    if (this.indicators) this.indicators.flush();
    if (this.ghosts) this.ghosts.flush();
    return outcome;
  }

  /**
   * Replay a recorded stash step (undo or redo): the text edit through the
   * normal edit path, then the overflow change, rebased onto anything typed
   * in the panel since (replay_document_overflow; an undo that cannot find
   * the stashed text intact at its recorded position leaves the overflow
   * unchanged and shows an 'overflow' notice). If the model refuses the
   * edit or its body differs from the surface text, the body is re-synced
   * from the surface, so the two never diverge.
   */
  mirrorOverflow(api, edit, text) {
    // Panel text still waiting for its debounce is in the model first, so
    // the replay is rebased onto it rather than discarding it.
    if (this.overflow) this.overflow.flush();
    let outcome = null;
    try {
      outcome = api.apply_edit(edit.start, edit.deletedLength, edit.insertedText);
    } catch (err) {
      outcome = null;
    }
    const replay = api.replay_document_overflow(edit.overflow.before, edit.overflow.after);
    if (!outcome || api.document_body() !== text) {
      this.reflectSync(api.sync_document_body(text));
    } else {
      this.reflectSetAside(outcome.setAside, outcome.notice, outcome.warning);
    }
    // The stashed text is identified by its recorded position in the
    // overflow, never by searching for it: when the panel was edited before
    // it (or it was edited itself), undo leaves the overflow alone and says
    // so, rather than removing some other copy.
    if (replay && replay.ok && replay.applied === false && this.warningKind !== 'malformed') {
      this.showWarning(
        'Undo put the text back on the page but left Overflow unchanged: the stashed text there was edited, ' +
          'or text was added before it. Remove it from Overflow by hand if you no longer need it.',
        'overflow',
      );
    }
  }

  // ---------------------------------------------------------------------
  // End of overflow stash (issue #12).
  // ---------------------------------------------------------------------

  /** Make sure the model body is exactly the surface text before reading it. */
  alignDocumentModel(api) {
    if (this.ghostHistoryConflict) throw new Error('Ghost history conflict: explicitly reopen before model alignment');
    if (this.ghostHistoryActive) {
      if (api.document_body() !== this.surface.getText()) throw new Error('Ghost transaction body conflict; implicit resync is forbidden');
      return;
    }
    // Text typed in the Overflow panel reaches the model before any read
    // (a no-op unless panel input is waiting for its debounce).
    if (this.overflow) this.overflow.flush();
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
    if (this.ghostHistoryActive) throw new Error('Cannot reopen during an active ghost transaction');
    if (this.ghostHistoryConflict && !this.ghostHistoryConflict.token && !this.ghostHistoryConflict.nativeRetired) {
      throw new Error('Native recovery token unavailable; ghost history remains blocked');
    }
    const key = typeof name === 'string' ? name : name && typeof name.name === 'string' ? name.name : null;
    let opened;
    if (this.ghostHistoryConflict && this.ghostHistoryConflict.token) {
      if (typeof api.recover_document_history_transaction !== 'function') throw new Error('Native conflict recovery is unavailable');
      const token = this.ghostHistoryConflict.token;
      const recovered = api.recover_document_history_transaction(token, String(source));
      MarkdownEditor.ghostAcknowledgement(recovered, token, 'recovered');
      if (recovered.rolledBack !== false || !recovered.opened) throw new Error('Invalid native recovery acknowledgement');
      opened = recovered.opened;
      this.ghostHistoryConflict = {token:null,nativeRetired:true}; // native retired; surface not yet canonical
    } else opened = api.open_document(String(source));
    if (this.ghostHistoryConflict && opened.kind === 'malformed') {
      throw new Error('Recovery source has malformed annotations; ghost history remains blocked');
    }
    if (key) this.documentKey = key;
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
    this.ghostHistoryConflict = null; // only after explicit canonical reopen/reset
    if (this.chrome && typeof this.chrome.documentChanged === 'function') {
      try {
        this.chrome.documentChanged();
      } catch (err) {
        console.error('chrome.documentChanged failed', err);
      }
    }
    // #76 A host's own open re-keys the dirty state and drafts too.
    if (this.persistence && typeof this.persistence.documentChanged === 'function') this.persistence.documentChanged(name);
    if (this.indicators) this.indicators.flush();
    if (this.ghosts) this.ghosts.flush();
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

  // ---------------------------------------------------------------------
  // #10 The alternatives side panel (public/js/alternatives-panel.js).
  // Kept in one block so parallel additions to this class merge cleanly;
  // design notes in docs/design/alternatives-panel.md.
  // ---------------------------------------------------------------------

  /**
   * Run a panel operation model first: `name` is one of 'create_span',
   * 'add', 'edit', 'remove', 'move' and `args` are passed to
   * the WASM export `alt_<name>`. A refused operation throws and changes
   * nothing, on either side. Otherwise the change is applied to the surface
   * as ONE undo step (applyAltChange). Returns the model's change
   * ({ span, index, before, after, edit, ... }), or null when nothing
   * changed (an alternative that already exists, an unchanged line).
   * Making an alternative active is swapAlternative (issue #9).
   */
  alternativeOp(name, ...args) {
    const api = this.requireDocumentApi();
    const fn = api[`alt_${name}`];
    if (typeof fn !== 'function') throw new Error(`The WASM document API has no alt_${name}`);
    this.alignDocumentModel(api);
    const change = fn(...args);
    if (!change) return null;
    this.applyAltChange(change);
    return change;
  }

  /**
   * Apply a change the model has already made: its body edit (when the
   * active alternative's text changed) or an empty edit at the span start
   * (annotations only), recorded as one undo step that carries
   * { span, before, after } and is not mirrored through apply_edit. Undo
   * and redo replay it with alt_restore (mirrorAltStep). The selection
   * stays where it was, moved with the text after the edit.
   */
  applyAltChange(change) {
    const alt = { span: change.span, before: change.before || null, after: change.after || null };
    const edit = change.edit;
    const sel = this.surface.getSelectionOffsets();
    let start;
    let end;
    let insert;
    let map = (o) => o;
    if (edit) {
      start = edit.start;
      end = edit.start + edit.deletedLength;
      insert = edit.insertedText;
      const delta = insert.length - edit.deletedLength;
      map = (o) => (o <= start ? o : o >= end ? o + delta : Math.min(o, start + insert.length));
    } else {
      const snap = alt.after || alt.before;
      start = snap && snap.anchor ? Math.min(snap.anchor.start, this.surface.getText().length) : 0;
      end = start;
      insert = '';
    }
    this.suppressModelSync = true;
    try {
      this.surface.replaceRange(start, end, insert, {
        source: 'alternatives',
        alt,
        selectStart: map(sel.start),
        selectEnd: map(sel.end),
      });
    } finally {
      this.suppressModelSync = false;
    }
    this.reflectSetAside(change.setAside, change.notice, change.warning);
    if (this.indicators) this.indicators.flush();
  }

  /**
   * Replay a recorded panel step (undo or redo) in the model: put span
   * `alt.span` back as `alt.after` (inverted for undo) with the step's text
   * edit. If the model refuses, or its body does not come out equal to the
   * surface text, the body is re-synced from the surface, so the two never
   * diverge.
   */
  mirrorAltStep(api, edit, text) {
    let outcome = null;
    try {
      outcome = api.alt_restore(
        String(edit.alt.span),
        JSON.stringify(edit.alt.after || null),
        edit.start,
        edit.deletedLength,
        edit.insertedText,
      );
    } catch (err) {
      outcome = null;
    }
    if (!outcome || api.document_body() !== text) {
      this.reflectSync(api.sync_document_body(text));
    } else {
      this.reflectSetAside(outcome.setAside, outcome.notice, outcome.warning);
    }
    if (this.altPanel) this.altPanel.scheduleRefresh();
  }

  // ---------------------------------------------------------------------
  // End of #10 (alternatives side panel).
  // ---------------------------------------------------------------------

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

// The full page (index.html): wait for the Rust start function to render
// the template into #app, then start the editor on it. A page that embeds
// the editor through TeraphimEditor (window.TE_EMBED, set by
// public/js/terraphim-editor.js) or that has no #app is left alone (#77).
const initEditor = () => {
  const checkElements = () => {
    const app = document.getElementById('app');
    if (window.TE_EMBED || !app) return;
    const required = [
      '.markdown-input',
      '#formatting-toolbar',
      '#shortcuts-list',
      '.shortcuts-dialog',
      '#show-help'
    ];

    if (required.every(selector => app.querySelector(selector))) {
      // Tear down any previous editor so its listeners and DOM do not leak.
      if (window.terraphimEditor && typeof window.terraphimEditor.destroy === 'function') {
        window.terraphimEditor.destroy();
      }
      // Pass the EditorConfig when initializing. This page is the editor,
      // so Ctrl+S / Ctrl+O act anywhere on it (#76, `standalone`).
      const editor = new MarkdownEditor(Object.assign({ standalone: true }, window.EditorConfig || {
        shortcuts: [],
        commands: [],
        styles: {}
      }));
      editor.initialize(app);
      window.terraphimEditor = editor;
    } else {
      // Check again in 100ms
      setTimeout(checkElements, 100);
    }
  };

  checkElements();
};

// Make sure config is loaded before initializing
document.addEventListener('DOMContentLoaded', async () => {
  if (window.TE_EMBED || !document.getElementById('app')) return;
  if (window.EditorConfig) {
    try {
      const ready = window.TerraphimRuntimeAssets?.ready;
      if (!ready || typeof ready.then !== 'function') {
        throw new Error('Local runtime asset readiness is unavailable');
      }
      await ready;
    } catch (error) {
      console.error('Editor startup failed: local runtime assets unavailable', error);
      return;
    }
    initEditor();
  } else {
    console.error('Editor configuration not found. Make sure config.js is loaded before editor.js');
  }
});
