/*
 * The alternatives side panel (issue #10; spec R-4.1 to R-4.6, R-7.3)
 * ====================================================================
 *
 * TeAlternativesPanel (`editor.altPanel`) is the left-hand panel that lists
 * the alternatives of one span, one per line, and lets the author add, edit,
 * delete, reorder and choose them. Full design notes:
 * docs/design/alternatives-panel.md
 *
 *   editor.altPanel.open()                    // span at the caret (or the word there)
 *   editor.altPanel.open({ spanId })          // a given span
 *   editor.altPanel.open({ start, end, kind })  // text without a span yet (pending)
 *   editor.altPanel.toggle() / close() / isOpen()
 *   editor.altPanel.setTab('word' | 'sentence' | 'paragraph')
 *   editor.altPanel.addAlternative(text, { source: 'human' | 'ai' })
 *   editor.altPanel.editAlternative(index, text)
 *   editor.altPanel.removeAlternative(index)
 *   editor.altPanel.moveAlternative(from, to)
 *   editor.altPanel.setActiveIndex(index)
 *   editor.altPanel.lines()                   // [{ text, source, active }]
 *
 * Opening
 * -------
 * - The `●●●` chrome control (`te:open-panel` { panel: 'alternatives' })
 *   toggles the panel and moves focus into it.
 * - "Alternatives for selection" (Ctrl+Shift+A, registered in the R-7.3
 *   selection menu) opens it on the selection: an existing span when the
 *   selection lies inside one, otherwise the trimmed selection as a pending
 *   span whose kind is inferred from the text. The menu item is offered
 *   for selections only; Ctrl+Shift+A with a collapsed caret (handled
 *   here) opens on the span at the caret, or the word there.
 * - A dot click (`te:dot`) opens or retargets the panel on that span without
 *   moving focus and without cancelling the event (issue #9 jumps to the
 *   alternative as its default action).
 * - The panel belongs to Write_On mode. Ctrl+Shift+A in plain mode turns
 *   Write_On mode on first; leaving Write_On mode closes the panel.
 *
 * Which span is shown
 * -------------------
 * Spans never overlap, so at most one span contains the caret. The tabs
 * choose the granularity: the panel shows the span at the reference point
 * (the caret when opened, or the shown span) if its kind matches the tab;
 * otherwise it offers the word, sentence or paragraph around that point as
 * a *pending* span: its text is listed as the original line and the first
 * alternative typed creates the span and that alternative in one model call
 * (one undo step). A pending range that would overlap an existing span is
 * refused with a message naming the tab to use instead. An inert span (only
 * its original) is never created: it would draw nothing and be detached,
 * with a warning, by the first keystroke inside it (R-2.7).
 * While the panel is open and the surface has focus, moving the caret into
 * another span retargets the panel; moving into plain text does not.
 *
 * Lines
 * -----
 * One `<input>` per alternative (documented choice: inputs give native,
 * accessible single-line editing; Enter, arrows and Escape are handled
 * here). Line 0 is the original and is read-only. A line is committed on
 * Enter, when it loses focus, when the tab changes and when the panel
 * closes (Escape included): changed text edits the alternative, empty text
 * deletes it, and text in the empty last line is added (whitespace-only is
 * ignored). Typed text is never silently lost: destroy() does not edit the
 * document, it reports uncommitted text as `te:alt-drafts` and returns it. The last line is always empty: Enter there adds the
 * typed text as a new alternative, which appears at once in the document
 * as a new dot (R-4.3). Each line has a leading glyph from the `source`
 * field: a dim dot for the original and the author's lines, a robot
 * (FontAwesome `fa-robot`) for AI lines; the active line is brighter with a
 * lavender glyph (R-4.4).
 *
 * Keyboard (R-4.5)
 * ----------------
 * ArrowUp/ArrowDown on a line commit it and make the previous/next line the
 * active alternative, with a live document update; focus follows. ArrowDown
 * on the last alternative goes to the empty line. Alt+ArrowUp/Alt+ArrowDown
 * reorder the focused line (never above the original). Ctrl/Cmd+Z and
 * Ctrl/Cmd+Shift+Z / Ctrl+Y undo and redo the document while the focused
 * line has no uncommitted typing. Tabs: Left/Right/Home/End (automatic
 * activation). Escape closes and returns focus to whatever opened the panel
 * (the surface with its selection, or the chrome control).
 *
 * Model first
 * -----------
 * Every change goes through `editor.alternativeOp()` (src/document.rs,
 * `alt_*` exports): the model changes first and the surface applies the
 * resulting text edit, if any, as ONE undo step that replays the span
 * snapshot on undo/redo. Making a line active goes through
 * `editor.swapAlternative()` (issue #9, its own `swap` history step).
 *
 * Listeners use the editor's AbortController signal plus the panel's own;
 * `destroy()` removes the DOM and every listener.
 */

let teAltPanelInstances = 0;

const TE_ALT_KINDS = ['word', 'sentence', 'paragraph'];
const TE_ALT_KIND_LABEL = { word: 'Word', sentence: 'Sentence', paragraph: 'Paragraph' };

/** Text helpers (pure, exposed for tests as TeAlternativesPanel.text). */
const teAltText = {
  isWordChar(ch) {
    return !!ch && /[\p{L}\p{N}_'’-]/u.test(ch);
  },

  /** [start, end) of the word at `pos`, or null. */
  wordAt(text, pos) {
    let p = pos;
    if (!teAltText.isWordChar(text[p]) && teAltText.isWordChar(text[p - 1])) p -= 1;
    if (!teAltText.isWordChar(text[p])) return null;
    let s = p;
    let e = p + 1;
    while (s > 0 && teAltText.isWordChar(text[s - 1])) s -= 1;
    while (e < text.length && teAltText.isWordChar(text[e])) e += 1;
    return { start: s, end: e };
  },

  /** Trim whitespace and a leading Markdown heading or list marker. */
  trimRange(text, start, end) {
    let s = start;
    let e = end;
    while (s < e && /\s/.test(text[s])) s += 1;
    const lead = /^(#{1,6}\s+|[-*+]\s+|\d+[.)]\s+|>\s*)/.exec(text.slice(s, e));
    if (lead) s += lead[0].length;
    while (e > s && /\s/.test(text[e - 1])) e -= 1;
    return e > s ? { start: s, end: e } : null;
  },

  /** The paragraph (between blank lines) around `pos`, trimmed. */
  paragraphAt(text, pos) {
    const before = text.lastIndexOf('\n\n', Math.max(0, pos - 1));
    const s = before === -1 ? 0 : before + 2;
    const after = text.indexOf('\n\n', pos);
    const e = after === -1 ? text.length : after;
    return teAltText.trimRange(text, s, e);
  },

  /**
   * The sentence around `pos`: within its paragraph, ending after `.`, `!`
   * or `?` (plus closing quotes or brackets) followed by whitespace, or at a
   * line break. A heading line is one sentence (R-2.3: headlines are
   * sentence-level spans).
   */
  sentenceAt(text, pos) {
    const para = teAltText.paragraphAt(text, pos);
    if (!para) return null;
    const p = Math.min(Math.max(pos, para.start), para.end);
    let s = para.start;
    const re = /[.!?]+["')\]”’]*(?=\s|$)|\n/g;
    re.lastIndex = para.start;
    let m;
    while ((m = re.exec(text)) && m.index < para.end) {
      const end = m.index + m[0].length;
      if (end > p || (end === p && m[0] !== '\n' && p === para.end)) {
        return teAltText.trimRange(text, s, m[0] === '\n' ? m.index : end);
      }
      s = end;
    }
    return teAltText.trimRange(text, s, para.end);
  },

  rangeFor(kind, text, pos) {
    if (kind === 'paragraph') return teAltText.paragraphAt(text, pos);
    if (kind === 'sentence') return teAltText.sentenceAt(text, pos);
    return teAltText.wordAt(text, pos);
  },

  /**
   * Kind of a selected text: several sentences or lines -> paragraph; a
   * phrase of up to four words without closing punctuation -> word;
   * anything else -> sentence.
   */
  inferKind(text) {
    const t = text.trim();
    if (/\n/.test(t) || /[.!?]["')\]]*\s+\S/.test(t)) return 'paragraph';
    const words = t.split(/\s+/).filter(Boolean).length;
    if (words <= 4 && !/[.!?]["')\]]*$/.test(t)) return 'word';
    return 'sentence';
  },
};

class TeAlternativesPanel {
  constructor(editor, options = {}) {
    this.editor = editor;
    this.surface = editor.surface;
    this.instance = ++teAltPanelInstances;
    this.destroyed = false;
    this.opened = false;
    this.tab = 'word';
    // { spanId } | { pending: { kind, start, end, text } } | { message } | null
    this.target = null;
    // Body offset the tabs resolve around.
    this.reference = 0;
    // Last span shown: { id, kind, start, end } (to follow it through undo).
    this.lastSpan = null;
    this.span = null;
    this.returnFocus = null;
    this.frame = 0;
    this.retargetFrame = 0;
    // Diagnostics for tests.
    this.renderCount = 0;

    this.abortController = new AbortController();
    const signal = this.abortController.signal;
    this.signal = signal;
    if (options.signal) {
      if (options.signal.aborted) this.abortController.abort();
      else options.signal.addEventListener('abort', () => this.destroy(), { signal });
    }

    this.build();

    document.addEventListener('te:open-panel', (e) => {
      const d = e.detail || {};
      if (d.panel !== 'alternatives' || (d.editor && d.editor !== this.editor)) return;
      this.returnFocus = this.chromeControl();
      this.toggle({ focus: true, fromControl: true });
    }, { signal });
    document.addEventListener('te:dot', (e) => {
      const d = e.detail || {};
      if (d.editor && d.editor !== this.editor) return;
      if (!d.spanId) return;
      // Not cancelled: the default action (issue #9) still jumps to the
      // alternative; the panel only shows the span, keeping focus where it is.
      this.open({ spanId: String(d.spanId) }, { focus: false });
    }, { signal });
    document.addEventListener('te:mode-change', (e) => {
      const d = e.detail || {};
      if (d.editor && d.editor !== this.editor) return;
      if (d.mode !== 'write-on') this.close({ restoreFocus: false });
    }, { signal });
    document.addEventListener('selectionchange', () => this.onSelectionChange(), { signal });
    this.offChange = this.surface.onChange(() => this.scheduleRefresh());
    this.surface.root.addEventListener('keydown', (e) => this.onSurfaceKeyDown(e), { signal });

    this.registerMenuItem();
  }

  static get text() {
    return teAltText;
  }

  // ---------------------------------------------------------------------
  // DOM
  // ---------------------------------------------------------------------

  build() {
    const signal = this.signal;
    const id = (name) => `te-alt-${this.instance}-${name}`;
    const root = document.createElement('aside');
    root.className = 'te-alt-panel';
    root.id = id('panel');
    root.hidden = true;
    root.setAttribute('aria-label', 'Alternatives');

    const tablist = document.createElement('div');
    tablist.className = 'te-alt-tabs';
    tablist.setAttribute('role', 'tablist');
    tablist.setAttribute('aria-label', 'Granularity');
    this.tabs = {};
    for (const kind of TE_ALT_KINDS) {
      const tab = document.createElement('button');
      tab.type = 'button';
      tab.className = 'te-alt-tab';
      tab.id = id(`tab-${kind}`);
      tab.dataset.kind = kind;
      tab.setAttribute('role', 'tab');
      tab.setAttribute('aria-controls', id('tabpanel'));
      tab.textContent = TE_ALT_KIND_LABEL[kind];
      tab.addEventListener('click', () => this.setTab(kind, { focusTab: true }), { signal });
      tablist.appendChild(tab);
      this.tabs[kind] = tab;
    }
    tablist.addEventListener('keydown', (e) => this.onTabKeyDown(e), { signal });

    const tabpanel = document.createElement('div');
    tabpanel.className = 'te-alt-tabpanel';
    tabpanel.id = id('tabpanel');
    tabpanel.setAttribute('role', 'tabpanel');

    const status = document.createElement('p');
    status.className = 'te-alt-status';
    status.setAttribute('role', 'status');
    status.setAttribute('aria-live', 'polite');

    const list = document.createElement('ul');
    list.className = 'te-alt-list';
    list.setAttribute('aria-label', 'Alternatives, one per line');
    list.addEventListener('keydown', (e) => this.onLineKeyDown(e), { signal });
    list.addEventListener('focusout', (e) => this.onLineBlur(e), { signal });
    list.addEventListener('click', (e) => this.onListClick(e), { signal });

    const hint = document.createElement('p');
    hint.className = 'te-alt-hint';
    hint.textContent = '↑↓ choose · Alt+↑↓ reorder · Enter adds · Esc closes';

    tabpanel.append(status, list, hint);
    root.append(tablist, tabpanel);
    root.addEventListener('keydown', (e) => this.onPanelKeyDown(e), { signal });

    this.root = root;
    this.tablist = tablist;
    this.tabpanel = tabpanel;
    this.statusEl = status;
    this.list = list;
    this.ids = { panel: root.id };
    const host = (this.editor.input && this.editor.input.closest('#app')) || document.body;
    host.appendChild(root);
    this.host = host;
  }

  chromeControl() {
    const chrome = this.editor.chrome;
    return chrome && chrome.root ? chrome.root.querySelector('[data-control="alternatives"]') : null;
  }

  // ---------------------------------------------------------------------
  // Model access
  // ---------------------------------------------------------------------

  modelAvailable() {
    const api = this.editor.documentApi ? this.editor.documentApi() : null;
    return !!(api && typeof api.alt_create_span === 'function' && typeof api.document_annotations === 'function');
  }

  liveSpans() {
    if (!this.modelAvailable()) return [];
    const ann = this.editor.annotations();
    return (ann && Array.isArray(ann.spans)) ? ann.spans : [];
  }

  /** The live span containing (or touching) [start, end], if any. */
  static spanAround(spans, start, end) {
    return spans.find((s) => s.anchor.start <= start && end <= s.anchor.end) || null;
  }

  // ---------------------------------------------------------------------
  // Opening and closing
  // ---------------------------------------------------------------------

  isOpen() {
    return this.opened;
  }

  isWriteOn() {
    const chrome = this.editor.chrome;
    if (chrome && typeof chrome.isWriteOn === 'function') return chrome.isWriteOn();
    return document.body.dataset.mode === 'write-on';
  }

  ensureWriteOn() {
    if (this.isWriteOn()) return;
    const chrome = this.editor.chrome;
    if (chrome && typeof chrome.setMode === 'function') chrome.setMode('write-on');
  }

  toggle(options = {}) {
    if (this.opened) {
      this.close({ restoreFocus: true });
      return false;
    }
    this.open(undefined, options);
    return true;
  }

  /**
   * Open (or retarget) the panel. `where`: undefined (the caret), { spanId },
   * or { start, end, kind? } (a range, pending unless a span holds it).
   * Options: focus (default true) moves focus to the active line.
   */
  open(where, options = {}) {
    if (this.destroyed || !this.modelAvailable()) return false;
    this.ensureWriteOn();
    const wasOpen = this.opened;
    if (!wasOpen && !options.fromControl) {
      const active = document.activeElement;
      this.returnFocus = active && this.surface.root.contains(active) ? this.surface.root : active;
    }
    if (!wasOpen) this.savedSelection = this.surface.getSelectionOffsets();
    this.resolve(where);
    this.opened = true;
    this.root.hidden = false;
    this.host.classList.add('te-alt-open');
    const control = this.chromeControl();
    if (control) {
      control.setAttribute('aria-expanded', 'true');
      control.setAttribute('aria-controls', this.root.id);
    }
    this.render();
    if (options.focus !== false) this.focusLine(this.activeLineIndex());
    return true;
  }

  close({ restoreFocus = true } = {}) {
    if (!this.opened) return;
    // Typed text is never silently lost: the focused line is committed
    // (Escape, Ctrl+Shift+A, the chrome control, leaving Write_On mode).
    this.commitFocused();
    this.opened = false;
    this.root.hidden = true;
    this.host.classList.remove('te-alt-open');
    const control = this.chromeControl();
    if (control) control.setAttribute('aria-expanded', 'false');
    const back = this.returnFocus;
    this.returnFocus = null;
    if (!restoreFocus) return;
    if (!back || back === this.surface.root || !back.isConnected) {
      this.surface.focus();
      const sel = this.savedSelection;
      if (sel) this.surface.setSelectionOffsets(sel.start, sel.end);
    } else if (typeof back.focus === 'function') {
      back.focus();
    }
  }

  /** Ctrl+Shift+A / the menu item: open on the selection. */
  openForSelection(ctx) {
    this.returnFocus = this.surface.root;
    this.savedSelection = ctx && ctx.selection ? ctx.selection : this.surface.getSelectionOffsets();
    if (!ctx || ctx.collapsed) return this.open(undefined, { fromControl: true });
    const text = this.surface.getText();
    const range = teAltText.trimRange(text, ctx.start, ctx.end) || { start: ctx.start, end: ctx.end };
    return this.open({ start: range.start, end: range.end }, { fromControl: true });
  }

  /**
   * Ctrl+Shift+A with a collapsed caret (the menu item covers selections):
   * open on the span or word at the caret. Always cancelled while the model
   * is available, so the browser's own Ctrl+Shift+A (tab search in Chrome)
   * does not fire from the editor.
   */
  onSurfaceKeyDown(e) {
    if (this.destroyed || e.defaultPrevented || e.isComposing) return;
    if (typeof teShortcutMatches !== 'function' || !teShortcutMatches(e, 'ctrl+shift+a')) return;
    if (!this.modelAvailable()) return;
    const sel = this.surface.getSelectionOffsets();
    if (sel.start !== sel.end && this.editor.selectionMenu) return;
    e.preventDefault();
    this.openForSelection({ collapsed: sel.start === sel.end, start: sel.start, end: sel.end, selection: sel });
  }

  registerMenuItem() {
    const menu = this.editor.selectionMenu;
    if (!menu || typeof menu.register !== 'function') return;
    this.offMenu = menu.register({
      id: 'alternatives',
      label: 'Alternatives for selection',
      key: 'ctrl+shift+a',
      // Offered for a selection only, so a right-click on a bare caret
      // still gets the browser's menu. Ctrl+Shift+A with a collapsed caret
      // is handled (and cancelled) by onSurfaceKeyDown.
      available: (ctx) => this.modelAvailable() && !!ctx && !ctx.collapsed,
      run: (ctx) => this.openForSelection(ctx),
    });
  }

  // ---------------------------------------------------------------------
  // Targets
  // ---------------------------------------------------------------------

  /** Work out what to show for `where` (see open()). */
  resolve(where) {
    const spans = this.liveSpans();
    const text = this.surface.getText();
    if (where && where.spanId) {
      const span = spans.find((s) => s.id === where.spanId);
      if (span) {
        this.showSpan(span);
        return;
      }
    }
    if (where && Number.isInteger(where.start) && Number.isInteger(where.end) && where.end > where.start) {
      const inside = TeAlternativesPanel.spanAround(spans, where.start, where.end);
      if (inside) {
        this.showSpan(inside);
        return;
      }
      const kind = where.kind || teAltText.inferKind(text.slice(where.start, where.end));
      this.tab = kind;
      this.reference = where.start;
      this.setPending(spans, kind, where.start, where.end);
      return;
    }
    const sel = this.surface.getSelectionOffsets();
    this.reference = sel.start;
    const at = TeAlternativesPanel.spanAround(spans, sel.start, sel.start);
    if (at) {
      this.showSpan(at);
      return;
    }
    this.resolveTab(spans);
  }

  /** Resolve the current tab around this.reference. */
  resolveTab(spans = this.liveSpans()) {
    const text = this.surface.getText();
    const pos = Math.min(this.reference, text.length);
    const at = TeAlternativesPanel.spanAround(spans, pos, pos);
    if (at && at.kind === this.tab) {
      this.showSpan(at);
      return;
    }
    const range = teAltText.rangeFor(this.tab, text, pos);
    if (!range) {
      this.target = { message: `Put the caret in a ${this.tab} to list its alternatives.` };
      this.span = null;
      return;
    }
    this.setPending(spans, this.tab, range.start, range.end);
  }

  setPending(spans, kind, start, end) {
    const text = this.surface.getText();
    const clash = spans.find((s) => start < s.anchor.end && s.anchor.start < end);
    this.span = null;
    if (clash) {
      this.target = {
        message: `This ${kind} overlaps a ${clash.kind} that already has alternatives. ` +
          `Open the ${TE_ALT_KIND_LABEL[clash.kind]} tab to see them.`,
      };
      return;
    }
    this.target = { pending: { kind, start, end, text: text.slice(start, end) } };
  }

  showSpan(span) {
    this.span = span;
    this.tab = span.kind;
    this.reference = span.anchor.start;
    this.target = { spanId: span.id };
    this.lastSpan = { id: span.id, kind: span.kind, start: span.anchor.start, end: span.anchor.end };
  }

  /** Re-read the model for the current target (after edits, undo, redo). */
  refresh() {
    if (!this.opened || this.destroyed) return;
    const spans = this.liveSpans();
    const t = this.target || {};
    if (t.spanId) {
      const span = spans.find((s) => s.id === t.spanId);
      if (span) {
        this.showSpan(span);
      } else if (this.lastSpan) {
        // The span went away (its last alternative removed, or undone):
        // offer its text again as a pending span of the same kind.
        const text = this.surface.getText();
        const { kind, start } = this.lastSpan;
        const range = teAltText.rangeFor(kind, text, Math.min(start, text.length));
        this.tab = kind;
        if (range) this.setPending(spans, kind, range.start, range.end);
        else this.resolveTab(spans);
      }
    } else if (t.pending) {
      const p = t.pending;
      const span = spans.find((s) => s.anchor.start === p.start && s.anchor.end === p.end);
      if (span) this.showSpan(span);
      else if (this.surface.getText().slice(p.start, p.end) !== p.text) this.resolveTab(spans);
      else this.setPending(spans, p.kind, p.start, p.end);
    } else {
      this.resolveTab(spans);
    }
    this.render();
  }

  scheduleRefresh() {
    if (!this.opened || this.destroyed || this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.refresh();
    });
  }

  /** Flush a pending refresh now (tests, and after operations). */
  flush() {
    if (this.frame) {
      cancelAnimationFrame(this.frame);
      this.frame = 0;
    }
    this.refresh();
  }

  /**
   * While the surface has focus, moving the caret into another span shows
   * that span. Moving into plain text keeps the current target, so an undo
   * that leaves the caret beside the span (or a click to read the text)
   * does not lose it.
   */
  onSelectionChange() {
    if (!this.opened || this.destroyed || this.retargetFrame) return;
    if (document.activeElement !== this.surface.root) return;
    this.retargetFrame = requestAnimationFrame(() => {
      this.retargetFrame = 0;
      if (!this.opened || document.activeElement !== this.surface.root) return;
      const sel = this.surface.getSelectionOffsets();
      if (this.span && this.span.anchor.start <= sel.start && sel.start <= this.span.anchor.end) return;
      const at = TeAlternativesPanel.spanAround(this.liveSpans(), sel.start, sel.start);
      if (!at || (this.span && at.id === this.span.id)) return;
      this.savedSelection = sel;
      this.showSpan(at);
      this.render();
    });
  }

  // ---------------------------------------------------------------------
  // Rendering
  // ---------------------------------------------------------------------

  /** [{ text, source, active }] for the lines shown (pending: the original only). */
  lines() {
    if (this.span) {
      return this.span.alts.map((a, i) => ({ text: a.text, source: a.source, active: i === this.span.active }));
    }
    if (this.target && this.target.pending) {
      return [{ text: this.target.pending.text, source: 'original', active: true }];
    }
    return [];
  }

  activeLineIndex() {
    if (this.span) return this.span.active;
    if (this.target && this.target.pending) return 1;
    return -1;
  }

  render() {
    this.renderCount += 1;
    for (const kind of TE_ALT_KINDS) {
      const on = kind === this.tab;
      const tab = this.tabs[kind];
      tab.setAttribute('aria-selected', on ? 'true' : 'false');
      tab.tabIndex = on ? 0 : -1;
      tab.classList.toggle('te-alt-tab--active', on);
    }
    this.tabpanel.setAttribute('aria-labelledby', this.tabs[this.tab].id);
    // Keep focus (and any uncommitted draft) on the same line across the
    // rebuild; removing the focused input must not commit it.
    const focused = this.list.contains(document.activeElement) ? document.activeElement : null;
    const focusedIndex = focused ? this.lineInputs().indexOf(focused) : -1;
    const focusedNew = !!focused && focused.classList.contains('te-alt-input--new');
    const draft = focused && focused.value !== focused.dataset.committed ? focused.value : null;
    this.rendering = true;
    try {
      this.list.textContent = '';
    } finally {
      this.rendering = false;
    }
    const t = this.target || {};
    if (t.message) {
      this.setStatus(t.message);
      this.list.hidden = true;
      if (focused) this.tabs[this.tab].focus();
      return;
    }
    this.list.hidden = false;
    const lines = this.lines();
    const total = lines.length;
    lines.forEach((line, i) => this.list.appendChild(this.renderLine(line, i, total)));
    this.list.appendChild(this.renderNewLine());
    if (t.pending) {
      this.setStatus(`New ${t.pending.kind}: type a first alternative and press Enter.`);
    } else if (!this.statusSticky) {
      this.setStatus(`${TE_ALT_KIND_LABEL[this.tab]}: ${total} alternatives, ${this.span.active + 1} of ${total} active.`);
    }
    this.statusSticky = false;
    if (focused) {
      const inputs = this.lineInputs();
      const input = focusedNew ? inputs[inputs.length - 1] : inputs[Math.min(focusedIndex, inputs.length - 1)];
      if (draft !== null && (focusedNew || !input.readOnly)) input.value = draft;
      input.focus();
    }
  }

  renderLine(line, index, total) {
    const li = document.createElement('li');
    li.className = 'te-alt-line';
    li.dataset.index = String(index);
    li.dataset.source = line.source;
    if (line.active) li.classList.add('te-alt-line--active');
    const glyph = document.createElement('i');
    glyph.setAttribute('aria-hidden', 'true');
    glyph.className = line.source === 'ai'
      ? 'te-alt-glyph te-alt-glyph--ai fa-solid fa-robot'
      : 'te-alt-glyph te-alt-glyph--human fa-solid fa-circle';
    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'te-alt-input';
    input.value = line.text;
    input.dataset.committed = line.text;
    input.spellcheck = false;
    input.setAttribute('autocomplete', 'off');
    const who = line.source === 'ai' ? 'suggested by AI' : line.source === 'original' ? 'original, read-only' : 'written by you';
    input.setAttribute('aria-label', `Alternative ${index + 1} of ${total}, ${who}${line.active ? ', active' : ''}`);
    if (index === 0) input.readOnly = true;
    li.append(glyph, input);
    if (index > 0) {
      const del = document.createElement('button');
      del.type = 'button';
      del.className = 'te-alt-delete';
      del.tabIndex = -1;
      del.setAttribute('aria-label', `Delete alternative ${index + 1}`);
      del.title = 'Delete';
      const x = document.createElement('i');
      x.className = 'fa-solid fa-xmark';
      x.setAttribute('aria-hidden', 'true');
      del.appendChild(x);
      li.appendChild(del);
    }
    return li;
  }

  renderNewLine() {
    const li = document.createElement('li');
    li.className = 'te-alt-line te-alt-line--new';
    const glyph = document.createElement('i');
    glyph.setAttribute('aria-hidden', 'true');
    glyph.className = 'te-alt-glyph te-alt-glyph--new fa-solid fa-plus';
    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'te-alt-input te-alt-input--new';
    input.dataset.committed = '';
    input.placeholder = 'New alternative';
    input.spellcheck = false;
    input.setAttribute('autocomplete', 'off');
    input.setAttribute('aria-label', 'New alternative: type it and press Enter');
    li.append(glyph, input);
    return li;
  }

  setStatus(message, sticky = false) {
    this.statusEl.textContent = message || '';
    if (sticky) this.statusSticky = true;
  }

  lineInputs() {
    return Array.from(this.list.querySelectorAll('.te-alt-input'));
  }

  focusedLineIndex() {
    const inputs = this.lineInputs();
    return inputs.indexOf(document.activeElement);
  }

  focusLine(index) {
    const inputs = this.lineInputs();
    if (inputs.length === 0) {
      const tab = this.tabs[this.tab];
      if (tab) tab.focus();
      return;
    }
    const input = inputs[Math.max(0, Math.min(index, inputs.length - 1))];
    input.focus();
    try {
      input.setSelectionRange(input.value.length, input.value.length);
    } catch (err) {
      // Not every input type supports selection ranges.
    }
  }

  // ---------------------------------------------------------------------
  // Operations (model first, through the editor)
  // ---------------------------------------------------------------------

  /** Run `fn`, reporting a refusal in the status line instead of throwing. */
  attempt(fn) {
    try {
      return fn();
    } catch (err) {
      this.setStatus(`Not changed: ${err && err.message ? err.message : err}`, true);
      this.render();
      return null;
    }
  }

  /**
   * Show the result of an operation. The model returns the span as it now
   * is, so that is rendered directly (no second model read); a span the
   * operation removed, or no change, falls back to a refresh.
   */
  after(change) {
    if (this.frame) {
      cancelAnimationFrame(this.frame);
      this.frame = 0;
    }
    if (change && change.after) {
      this.showSpan(change.after);
      this.render();
    } else {
      this.refresh();
    }
    return change;
  }

  /** Add `text` (Enter on the empty line). Creates the span when pending. */
  addAlternative(text, { source = 'human' } = {}) {
    const value = String(text);
    if (!value) return null;
    const t = this.target || {};
    if (t.pending) {
      const p = t.pending;
      return this.attempt(() => this.after(
        this.editor.alternativeOp('create_span', p.kind, p.start, p.end, value, source)));
    }
    if (!t.spanId) return null;
    return this.attempt(() => {
      const change = this.editor.alternativeOp('add', t.spanId, value, source);
      if (!change) this.setStatus(`“${value}” is already in the list.`, true);
      return this.after(change);
    });
  }

  /** Replace line `index`; empty text deletes it. */
  editAlternative(index, text) {
    const t = this.target || {};
    if (!t.spanId || index < 1) return null;
    if (String(text) === '') return this.removeAlternative(index);
    return this.attempt(() => this.after(this.editor.alternativeOp('edit', t.spanId, index, String(text))));
  }

  removeAlternative(index) {
    const t = this.target || {};
    if (!t.spanId || index < 1) return null;
    return this.attempt(() => this.after(this.editor.alternativeOp('remove', t.spanId, index)));
  }

  moveAlternative(from, to) {
    const t = this.target || {};
    if (!t.spanId) return null;
    return this.attempt(() => this.after(this.editor.alternativeOp('move', t.spanId, from, to)));
  }

  /** Make line `index` active, with a live document update (R-4.5). */
  setActiveIndex(index) {
    const t = this.target || {};
    if (!t.spanId || !this.span || index < 0 || index >= this.span.alts.length) return null;
    if (index === this.span.active) return null;
    return this.attempt(() => {
      const change = this.activate(t.spanId, index);
      this.refresh();
      return change;
    });
  }

  /**
   * Make an alternative active through issue #9's swapAlternative: model
   * first, the a/an fix-up, one `swap` history step.
   */
  activate(spanId, index) {
    return this.editor.swapAlternative(spanId, index);
  }

  /** Commit line `input` if its text changed. Returns true if it did. */
  commitLine(input) {
    if (!input || input.readOnly) return false;
    if (input.classList.contains('te-alt-input--new')) return this.commitNewLine(input);
    if (input.value === input.dataset.committed) return false;
    const index = Number(input.closest('.te-alt-line').dataset.index);
    this.editAlternative(index, input.value);
    return true;
  }

  /**
   * Commit the trailing empty line exactly as Enter does: its text is added
   * as an alternative (or creates the pending span with it), one undo step.
   * Whitespace-only text is ignored. Returns true if something was added.
   */
  commitNewLine(input) {
    const value = input.value;
    if (!value.trim()) return false;
    // Cleared first, so the re-render does not carry it over as a draft.
    input.value = '';
    this.addAlternative(value);
    return true;
  }

  /** Commit the focused line, if any (see commitLine). */
  commitFocused() {
    const active = document.activeElement;
    if (!this.list || !(active instanceof HTMLInputElement) || !this.list.contains(active)) return false;
    return this.commitLine(active);
  }

  /**
   * Uncommitted text in the lines: [{ value, index, isNew, spanId, pending }]
   * (index is the line, the empty line's being lines().length).
   */
  pendingDrafts() {
    if (!this.list) return [];
    const t = this.target || {};
    const inputs = this.lineInputs();
    const out = [];
    inputs.forEach((input, index) => {
      const isNew = input.classList.contains('te-alt-input--new');
      if (input.readOnly || input.value === (input.dataset.committed || '')) return;
      if (isNew && !input.value.trim()) return;
      out.push({
        value: input.value,
        index,
        isNew,
        spanId: t.spanId || null,
        pending: t.pending ? { ...t.pending } : null,
      });
    });
    return out;
  }

  // ---------------------------------------------------------------------
  // Events
  // ---------------------------------------------------------------------

  onLineBlur(e) {
    if (this.destroyed || !this.opened || this.rendering) return;
    const input = e.target;
    if (!(input instanceof HTMLInputElement) || !input.isConnected) return;
    if (input.value === (input.dataset.committed || '')) return;
    // Leaving a line commits it, the empty line included (a tab, the
    // document, another line). The commit re-renders the list, so focus
    // moving to another line is put back on that line afterwards.
    const to = e.relatedTarget;
    const inputs = this.lineInputs();
    const toIndex = to && this.list.contains(to) ? inputs.indexOf(to) : -1;
    const toNew = toIndex !== -1 && to.classList.contains('te-alt-input--new');
    if (!this.commitLine(input) || toIndex === -1) return;
    this.focusLine(toNew ? this.lines().length : toIndex);
  }

  onListClick(e) {
    const del = e.target.closest && e.target.closest('.te-alt-delete');
    if (!del) return;
    const index = Number(del.closest('.te-alt-line').dataset.index);
    this.removeAlternative(index);
    this.focusLine(Math.min(index, this.lineInputs().length - 1));
  }

  onLineKeyDown(e) {
    const input = e.target;
    if (!(input instanceof HTMLInputElement) || e.isComposing) return;
    const isNew = input.classList.contains('te-alt-input--new');
    const index = isNew ? this.lines().length : Number(input.closest('.te-alt-line').dataset.index);
    const count = this.lines().length;
    const plain = !e.ctrlKey && !e.metaKey && !e.shiftKey;
    if (e.key === 'Enter' && plain && !e.altKey) {
      e.preventDefault();
      if (isNew) {
        if (!this.commitNewLine(input)) return;
        this.focusLine(this.lines().length);
      } else {
        this.commitLine(input);
        this.focusLine(this.lines().length);
      }
      return;
    }
    if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && plain) {
      const step = e.key === 'ArrowUp' ? -1 : 1;
      if (e.altKey) {
        e.preventDefault();
        if (isNew || index < 1) return;
        const to = index + step;
        if (to < 1 || to >= count) return;
        if (this.commitLine(input)) return;
        this.moveAlternative(index, to);
        this.focusLine(to);
        return;
      }
      e.preventDefault();
      this.commitLine(input);
      // Text committed from the empty line is now the last alternative.
      const next = (isNew ? this.lines().length : index) + step;
      if (next < 0) return;
      if (next >= this.lines().length) {
        this.focusLine(this.lines().length);
        return;
      }
      if (this.span) this.setActiveIndex(next);
      this.focusLine(next);
    }
  }

  onPanelKeyDown(e) {
    if (this.destroyed || e.isComposing) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      // close() commits the focused line: Escape never discards typing
      // (undo reverses the commit).
      this.close({ restoreFocus: true });
      return;
    }
    const mod = e.ctrlKey || e.metaKey;
    if (!mod) return;
    if (typeof teShortcutMatches === 'function' && teShortcutMatches(e, 'ctrl+shift+a')) {
      e.preventDefault();
      this.close({ restoreFocus: true });
      return;
    }
    const key = (e.key || '').toLowerCase();
    const undo = key === 'z' && !e.shiftKey && !e.altKey;
    const redo = (key === 'z' && e.shiftKey && !e.altKey) || (key === 'y' && !e.shiftKey && !e.altKey);
    if (!undo && !redo) return;
    const input = e.target instanceof HTMLInputElement ? e.target : null;
    // Typing not yet committed is the input's own to undo.
    if (input && input.value !== (input.dataset.committed || '')) return;
    e.preventDefault();
    const index = this.focusedLineIndex();
    if (undo) this.surface.undo();
    else this.surface.redo();
    this.flush();
    this.focusLine(index === -1 ? this.activeLineIndex() : Math.min(index, this.lines().length));
  }

  onTabKeyDown(e) {
    const order = TE_ALT_KINDS;
    let i = order.indexOf(this.tab);
    if (e.key === 'ArrowRight') i = (i + 1) % order.length;
    else if (e.key === 'ArrowLeft') i = (i + order.length - 1) % order.length;
    else if (e.key === 'Home') i = 0;
    else if (e.key === 'End') i = order.length - 1;
    else if (e.key === 'ArrowDown') {
      e.preventDefault();
      this.focusLine(this.activeLineIndex());
      return;
    } else return;
    e.preventDefault();
    this.setTab(order[i], { focusTab: true });
  }

  /** Switch granularity (R-4.2) around the reference point. */
  setTab(kind, { focusTab = false } = {}) {
    if (!TE_ALT_KINDS.includes(kind)) throw new Error(`Unknown tab ${kind}`);
    this.commitFocused();
    this.tab = kind;
    if (this.opened) {
      this.resolveTab();
      this.render();
    }
    if (focusTab) this.tabs[kind].focus();
  }

  // ---------------------------------------------------------------------
  // Teardown
  // ---------------------------------------------------------------------

  /**
   * Tear down. Never changes the document (the host may already have
   * saved): uncommitted text in the lines is reported instead, as one
   * bubbling `te:alt-drafts` CustomEvent from the surface root with
   * detail { editor, drafts } (see pendingDrafts()), and returned. With
   * nothing pending there is no event and the array is empty.
   */
  destroy() {
    if (this.destroyed) return [];
    const drafts = this.pendingDrafts();
    if (drafts.length && this.surface.root) {
      this.surface.root.dispatchEvent(new CustomEvent('te:alt-drafts', {
        bubbles: true,
        detail: { editor: this.editor, drafts: drafts.map((d) => ({ ...d })) },
      }));
    }
    this.destroyed = true;
    this.opened = false;
    if (this.frame) cancelAnimationFrame(this.frame);
    if (this.retargetFrame) cancelAnimationFrame(this.retargetFrame);
    this.frame = 0;
    this.retargetFrame = 0;
    this.abortController.abort();
    if (this.offChange) this.offChange();
    if (this.offMenu) this.offMenu();
    if (this.host) this.host.classList.remove('te-alt-open');
    if (this.root) this.root.remove();
    this.root = null;
    return drafts;
  }
}

window.TeAlternativesPanel = TeAlternativesPanel;
