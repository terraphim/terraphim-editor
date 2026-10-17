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
 * document, it reports uncommitted text as `te:alt-drafts` and returns it.
 * The last line is always empty, and a new alternative is typed there live
 * (issue #106, R-4.3, video 1:24-1:30): the first character adds it and
 * makes it active, so the document shows it at once with a new dot, and
 * each further keystroke updates the document. Enter (which then offers a
 * fresh empty line), leaving the line or closing the panel keeps it as ONE
 * undo step; Escape cancels it and brings back the previous active
 * alternative and its text. Each line has a leading glyph from the `source`
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
 * activation). Escape closes (cancelling a new alternative being typed) and
 * returns focus to what last opened or showed the panel: the surface with
 * its selection when that was the text, a shortcut, the menu, a dot or
 * Ctrl+Shift+G (even with the panel first opened from `●●●`), so Ctrl+Z
 * works straight away; the chrome control when it was `●●●`.
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

/** Shared Rust targeting adapters, exposed for tests as TeAlternativesPanel.text. */
const teAltText = {
  binding(api, name) {
    if (!api || typeof api[name] !== 'function') {
      throw new Error('Shared text WASM bindings are unavailable. Reload with the current editor WASM package.');
    }
    return api[name];
  },

  /** [start, end) of the word at `pos`, or null. */
  wordAt(text, pos, api = window.wasmBindings) {
    return teAltText.rangeFor('word', text, pos, api);
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

  /** The mdast prose paragraph/title around `pos`, in original UTF-16. */
  paragraphAt(text, pos, api = window.wasmBindings) {
    return teAltText.rangeFor('paragraph', text, pos, api);
  },

  /**
   * Shared Unicode/Markdown sentence policy, not a second JavaScript parser.
   */
  sentenceAt(text, pos, api = window.wasmBindings) {
    return teAltText.rangeFor('sentence', text, pos, api);
  },

  rangeFor(kind, text, pos, api = window.wasmBindings) {
    if (!Number.isInteger(pos) || pos < 0 || pos > text.length) throw new Error('Caret offset is outside the current text.');
    return teAltText.binding(api, 'text_range_at')(text, pos, kind);
  },

  /**
   * Kind of a selected text: several sentences or lines -> paragraph; a
   * phrase of up to four words without closing punctuation -> word;
   * anything else -> sentence.
   */
  inferKind(text, api = window.wasmBindings) {
    return teAltText.binding(api, 'text_infer_kind')(text);
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
    // The new alternative being typed in the add line (issue #106), or null:
    // { spanId, index, base } where base is the surface history entry from
    // before it began (so its steps can be folded into one, or undone).
    this.live = null;
    // Set while a live addition is being changed: the model step moves the
    // surface selection, which blurs the line being typed in, and that blur
    // must not end the addition.
    this.liveBusy = false;
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
      // A dot is in the text, so Escape goes back to the text, where the
      // jump leaves the caret (no saved selection), even when focus is in a
      // panel line opened from `●●●`.
      this.open({ spanId: String(d.spanId) }, { focus: false, fromControl: true });
      this.returnFocus = this.surface.root;
      this.savedSelection = null;
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
    list.addEventListener('input', (e) => this.onLineInput(e), { signal });
    list.addEventListener('compositionend', (e) => this.onLineInput(e), { signal });
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
    // The editor's own root (#app on the full page, the embed container
    // otherwise, issue #77).
    const host = this.editor.root || (this.editor.input && this.editor.input.closest('.te-app')) || document.body;
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
    if (chrome && typeof chrome.setMode === 'function') return chrome.setMode('write-on');
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
    const beforeMode=this.surface.getText(),entering=this.ensureWriteOn();
    if(this.editor.authority && entering && typeof entering.then==='function')return entering.then(()=>{
      if(where && Number.isInteger(where.start) && beforeMode!==this.surface.getText()) {
        this.setStatus('Text changed while saving the block; select the range again.',true);return false;
      }
      return this.open(where,options);
    }).catch(error=>{this.setStatus(`Alternatives not opened: ${error?.message||error}`,true);return false;});
    const wasOpen = this.opened;
    if (!wasOpen) this.savedSelection = this.surface.getSelectionOffsets();
    if (!options.fromControl) {
      // Opened, or shown again, from anywhere but the panel itself: from
      // the text (a shortcut, the menu, a dot, Ctrl+Shift+G showing a span),
      // Escape goes back to the text with this selection, so Ctrl+Z works
      // straight away, even when the panel was first opened from the `●●●`
      // control; from a chrome control, back to that control.
      const active = document.activeElement;
      if (!wasOpen || !(active && this.root.contains(active))) {
        const chromeRoot = this.editor.chrome && this.editor.chrome.root;
        const fromChrome = !!active && !!chromeRoot && chromeRoot.contains(active);
        this.returnFocus = fromChrome ? active : this.surface.root;
        if (!fromChrome) this.savedSelection = this.surface.getSelectionOffsets();
      }
    }
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
    if(this.editor.authority && !this.authorityLocal) return this.authorityClose({restoreFocus});
    if (!this.opened) return;
    // Typed text is never silently lost: the focused line is committed
    // (Escape, Ctrl+Shift+A, the chrome control, leaving Write_On mode), and
    // a new alternative being typed is kept as one undo step.
    this.finishLive();
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
    try {
      this.resolveTarget(where);
    } catch (error) {
      this.targetingRefusal(error);
    }
  }

  targetingRefusal(error) {
    this.span = null;
    this.target = { message: String(error && error.message ? error.message : error) };
  }

  resolveTarget(where) {
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
      if (where.start < 0 || where.end > text.length) throw new Error('Selected range is outside the current text.');
      const kind = where.kind || teAltText.inferKind(text.slice(where.start, where.end), this.editor.documentApi());
      if (!TE_ALT_KINDS.includes(kind)) throw new Error(`Unknown text kind: ${kind}`);
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
    const pos = this.reference;
    const at = TeAlternativesPanel.spanAround(spans, pos, pos);
    if (at && at.kind === this.tab) {
      this.showSpan(at);
      return;
    }
    let range;
    try {
      range = teAltText.rangeFor(this.tab, text, pos, this.editor.documentApi());
    } catch (error) {
      this.targetingRefusal(error);
      return;
    }
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
    try {
      this.refreshTarget();
    } catch (error) {
      this.targetingRefusal(error);
      this.render();
    }
  }

  refreshTarget() {
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
        const range = teAltText.rangeFor(kind, text, Math.min(start, text.length), this.editor.documentApi());
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
      return this.span.alts.map((a, i) => ({ text: a.text, source: a.source, model: a.model, active: i === this.span.active }));
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
      // The routine count is for screen readers only: the video shows no
      // status line (issue #107). Messages that ask for something stay visible.
      this.setStatus(`${TE_ALT_KIND_LABEL[this.tab]}: ${total} alternatives, ${this.span.active + 1} of ${total} active.`, false, { quiet: true });
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
    const who = line.source === 'ai'
      ? (line.model === 'kg' ? 'knowledge graph synonym' : 'suggested by AI')
      : line.source === 'original' ? 'original, read-only' : 'written by you';
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

  setStatus(message, sticky = false, { quiet = false } = {}) {
    this.statusEl.textContent = message || '';
    this.statusEl.classList.toggle('te-alt-status--quiet', quiet);
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
      const result = fn();
      if (result && typeof result.then === 'function') return result.catch(err => {
        this.setStatus(`Not changed: ${err && err.message ? err.message : err}`,true);
        this.render(); return null;
      });
      return result;
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
    if (change && typeof change.then === 'function') return change.then(value => this.after(value));
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
      if(change && typeof change.then==='function') return change.then(result=>{if(!result)this.setStatus(`“${value}” is already in the list.`,true);return this.after(result);});
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
      if(change && typeof change.then==='function') return change.then(result=>{this.refresh();return result;});
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
    return this.editAlternative(index, input.value) !== null;
  }

  /**
   * Commit text put in the add line without typing (typing makes it live,
   * see onLineInput): it is added as an alternative (or creates the pending
   * span with it) and made active, as one undo step. Whitespace-only text is
   * ignored. Returns true if something was added.
   */
  commitNewLine(input) {
    const value = input.value;
    if (!value.trim()) return false;
    // Cleared first, so the re-render does not carry it over as a draft.
    input.value = '';
    // History trimming waits until the steps are folded (see liftLimit).
    const limit = this.liftLimit();
    const base = this.historyTop();
    if (this.addActive(value) !== -1) this.foldSince(base);
    this.restoreLimit(limit);
    return true;
  }

  // ---------------------------------------------------------------------
  // The new alternative, live while it is typed (issue #106, R-4.3)
  // ---------------------------------------------------------------------
  //
  // The first character typed in the add line adds the alternative (or
  // creates the pending span with it) and makes it active, so the document
  // shows it at once; each further keystroke edits it, model first, and the
  // document follows. Focus moves to its line, which stays the one being
  // typed in. Enter, leaving the line, the arrows, a tab change or closing
  // the panel keep it, with every step folded into ONE undo step. Escape
  // cancels it: the steps are undone, so the alternative goes and the
  // previous active one and its text come back. Emptying the line (or
  // leaving only spaces) cancels it the same way and leaves the add line
  // with what was typed.

  /** The surface history entry now on top (where a live addition began). */
  historyTop() {
    return this.surface.history[this.surface.historyIndex] || null;
  }

  /** How many history steps were recorded after entry `base` (-1: gone). */
  stepsSince(base) {
    const at = this.surface.history.indexOf(base);
    return at === -1 ? -1 : this.surface.historyIndex - at;
  }

  /** Fold the steps recorded after `base` into one undo step. */
  foldSince(base) {
    const steps = this.stepsSince(base);
    if (steps > 1) this.surface.squashHistory(steps, 'alternatives');
    return steps;
  }

  /**
   * Add `value` and make it active. Returns its line index, or -1 when the
   * model refused (a duplicate, an overlap; the status line says why).
   */
  addActive(value) {
    const change = this.addAlternative(value);
    if (!change || !this.span) return -1;
    const index = this.span.alts.findIndex((a, i) => i > 0 && a.text === value);
    if (index === -1) return -1;
    this.setActiveIndex(index);
    return index;
  }

  /** Line index of a line input (the add line: lines().length). */
  lineIndexOf(input) {
    if (input.classList.contains('te-alt-input--new')) return this.lines().length;
    return Number(input.closest('.te-alt-line').dataset.index);
  }

  onLineInput(e) {
    if(this.editor.authority) {
      if(this.destroyed||!this.opened||this.rendering||e.isComposing||!(e.target instanceof HTMLInputElement)||e.target.readOnly)return;
      this.authorityInput(e.target);return;
    }
    if (this.destroyed || !this.opened || this.rendering || e.isComposing) return;
    const input = e.target;
    if (!(input instanceof HTMLInputElement) || input.readOnly) return;
    if (input.classList.contains('te-alt-input--new')) {
      if (!this.live) this.startLive(input);
      return;
    }
    if (this.live && this.lineIndexOf(input) === this.live.index) this.updateLive(input);
  }

  authorityLineTask(work) {
    const authorityGeneration=this.editor.authorityQueueGeneration;
    const pending=(this.authorityLineQueue||Promise.resolve()).then(()=> {
      if(authorityGeneration!==this.editor.authorityQueueGeneration)throw new Error('Alternative intent belongs to a recovered authority');
      if(this.destroyed)throw new Error('Alternative panel disposed; draft retained');
      return work();
    });
    this.authorityLineQueue=pending.catch(error=>this.setStatus(`Not changed: ${error?.message||error}`,true));
    return pending;
  }

  authorityInput(input) {
    const value=input.value,caret=[input.selectionStart,input.selectionEnd];
    const isNew=input.classList.contains('te-alt-input--new');
    const index=this.lineIndexOf(input);
    const liveInput=isNew||this.authorityLiveStarting||(this.live&&index===this.live.index);
    const group=this.authorityInputGroup||0;
    const key=liveInput?`alternative-live-${group}`:`alternative-${index}`;
    const sequence=this.editor.authorityReportDraft(key,{text:value,index,isNew,caret});
    if(!liveInput)return;
    if(!this.live)this.authorityLiveStarting=true;
    this.authorityLineTask(async()=> {
      if(this.authorityCancelledGroups?.has(group)) {this.editor.authorityClearDraft(key,sequence);return;}
      if(!this.live) {
        if(!value.trim()){this.authorityLiveStarting=false;this.editor.authorityClearDraft(key,sequence);return;}
        const limit=this.liftLimit(),base=this.historyTop();
        this.live={spanId:null,index:-1,base,refused:null,limit,group};
        try {
          const change=await this.addAlternative(value);
          if(!change||!this.span) {this.endLive();return;}
          const actualIndex=Number.isInteger(change.index)?change.index:this.span.alts.findIndex((alt,i)=>i>0&&alt.text===value);
          if(actualIndex<1)throw new Error('Native alternative index is unavailable');
          this.live.spanId=this.span.id;this.live.index=actualIndex;
          await this.activate(this.span.id,actualIndex);
        } catch(error) {if(this.live&&this.stepsSince(base)===0)this.endLive();throw error;}
        finally {this.authorityLiveStarting=false;}
      } else if(!value.trim()) {
        await this.authorityCancelAccepted({keep:value,caret});
      } else {
        const unchanged=this.lines()[this.live.index]?.text===value;
        const change=unchanged?true:await this.editAlternative(this.live.index,value);
        this.live.refused=change?null:value;
      }
      if(!this.live || this.live.refused===null)this.editor.authorityClearDraft(key,sequence);
      const currentGroup=this.authorityInputGroup||0;
      const later=this.editor.authorityDrafts.get(`alternative-live-${currentGroup}`);
      const display=later?.input.text??(group===currentGroup?null:'');
      this.whileBusy(()=>this.restoreLine(group===currentGroup&&this.live?this.live.index:this.lines().length,display,later?.input.caret||caret));
    }).catch(error=>this.setStatus(`Not accepted: ${error?.message||error}`,true));
  }

  async flushAuthorityInputs() {
    await (this.authorityLineQueue||Promise.resolve());
    const input=document.activeElement;
    if(input instanceof HTMLInputElement && this.list?.contains(input) && !input.readOnly && !this.live && input.value!==(input.dataset.committed||'')) {
      await this.authorityCommitInput(input);
    }
    await (this.authorityLineQueue||Promise.resolve());
  }

  authorityCommitInput(input) {
    if(!input||input.readOnly)return Promise.resolve(false);
    if(input.classList.contains('te-alt-input--new')) {this.authorityInput(input);return this.authorityLineQueue.then(()=>true);}
    const index=this.lineIndexOf(input),value=input.value,key=`alternative-${index}`;
    const sequence=this.editor.authorityReportDraft(key,{text:value,index,isNew:false});
    return this.authorityLineTask(async()=> {
      if(value===(this.lines()[index]?.text||'')){this.editor.authorityClearDraft(key,sequence);return false;}
      const result=await this.editAlternative(index,value);
      if(result)this.editor.authorityClearDraft(key,sequence);
      return !!result;
    });
  }

  async authorityCancelAccepted({keep='',caret=null}={}) {
    if(!this.live)return;
    const busy=this.liveBusy;this.liveBusy=true;
    try {
      const {base}=this.live;this.endLive();
      const steps=this.foldSince(base);
      if(steps>=1) {await this.surface.undo();this.surface.forgetRedo();}
      this.surface.trimHistory();await this.editor.authorityHistoryChanged();
      if(this.opened){this.refresh();this.restoreLine(this.lines().length,keep,caret);}
    } finally {this.liveBusy=busy;}
  }

  authorityCancelLive(options={}) {
    const group=this.authorityInputGroup||0;
    this.authorityInputGroup=group+1;
    if(!this.authorityCancelledGroups)this.authorityCancelledGroups=new Set();
    this.authorityCancelledGroups.add(group);
    return this.authorityLineTask(async()=> {
      if(this.live?.group===group)await this.authorityCancelAccepted(options);
      else if(this.opened)this.whileBusy(()=>this.restoreLine(this.lines().length,options.keep||'',options.caret||null));
      this.authorityLiveStarting=false;
      const key=`alternative-live-${group}`,draft=this.editor.authorityDrafts.get(key);
      if(draft)this.editor.authorityClearDraft(key,draft.sequence);
      this.authorityCancelledGroups.delete(group);
    });
  }

  async authorityClose(options) {
    await this.flushAuthorityInputs();await this.finishLive();
    this.authorityLocal=true;
    try {return this.close(options);}finally{this.authorityLocal=false;}
  }

  async authoritySetTab(kind,options) {
    await this.flushAuthorityInputs();await this.finishLive();
    this.authorityLocal=true;
    try {return this.setTab(kind,options);}finally{this.authorityLocal=false;}
  }

  startLive(input) {
    const value = input.value;
    const t = this.target || {};
    if (!value.trim() || (!t.spanId && !t.pending)) return;
    const caret = [input.selectionStart, input.selectionEnd];
    // History trimming waits until the addition is folded, from before its
    // first step, so `base` (and the state before it) is never shifted out,
    // even when the history is already at its limit.
    const limit = this.liftLimit();
    const base = this.historyTop();
    input.value = '';
    const index = this.whileBusy(() => this.addActive(value));
    if (index === -1) {
      // Refused: the typed text stays in the add line.
      this.restoreLimit(limit);
      this.restoreLine(this.lines().length, value, caret);
      return;
    }
    this.live = { spanId: this.span.id, index, base, refused: null, limit };
    this.restoreLine(index, null, caret);
  }

  updateLive(input) {
    const value = input.value;
    const caret = [input.selectionStart, input.selectionEnd];
    if (!value.trim()) {
      this.cancelLive({ keep: value, caret });
      return;
    }
    if (value === input.dataset.committed) return;
    const { index } = this.live;
    const change = this.whileBusy(() => this.editAlternative(index, value));
    // A refused edit (a duplicate) keeps the typed text in the line; the
    // model keeps the last text it accepted until the line is fixed.
    this.live.refused = change ? null : value;
    this.restoreLine(index, value, caret);
  }

  whileBusy(fn) {
    const was = this.liveBusy;
    this.liveBusy = true;
    try {
      return fn();
    } finally {
      this.liveBusy = was;
    }
  }

  /**
   * Keep the live addition as it is: one undo step. If the line still holds
   * text the model refused, the line is put back to the text it kept, and
   * the status line says so.
   */
  finishLive() {
    if(this.editor.authority && !this.authorityLocal) return this.authorityLineTask(async()=> {
      if(this.live?.refused!==null && this.live)throw new Error('Alternative text was refused; correct it or press Escape');
      this.authorityLocal=true;try {this.finishLive();}finally{this.authorityLocal=false;}
      await this.editor.authorityHistoryChanged();
    });
    if (!this.live) return;
    const { base, index, refused } = this.live;
    this.endLive();
    this.foldSince(base);
    this.surface.trimHistory();
    if (refused !== null) {
      const input = this.lineInputs()[index];
      const kept = input ? input.dataset.committed : '';
      if (input) input.value = kept;
      this.setStatus(`“${refused}” is already in the list; kept “${kept}”.`, true);
    }
  }

  /** Forget the live addition (trimming resumes after the fold). */
  endLive() {
    this.surface.historyLimit = this.live.limit;
    this.live = null;
  }

  /** Stop history trimming; returns the limit to restore. */
  liftLimit() {
    const limit = this.surface.historyLimit;
    this.surface.historyLimit = Infinity;
    return limit;
  }

  /** Put the history limit back and apply it. */
  restoreLimit(limit) {
    this.surface.historyLimit = limit;
    this.surface.trimHistory();
  }

  /**
   * Undo the live addition (Escape, or an emptied line). `keep` is put back
   * in the add line, which gets focus with `caret`.
   */
  cancelLive({ keep = '', caret = null } = {}) {
    if(this.editor.authority && !this.authorityLocal) return this.authorityCancelLive({keep,caret});
    if (!this.live) return;
    const { base } = this.live;
    this.endLive();
    const steps = this.foldSince(base);
    if (steps >= 1) {
      this.surface.undo();
      // Cancelled, not undone: redo does not bring it back.
      this.surface.forgetRedo();
    }
    this.surface.trimHistory();
    this.flush();
    if (this.opened) this.restoreLine(this.lines().length, keep, caret);
  }

  /** Focus line `index`, optionally with `value`, and put the caret back. */
  restoreLine(index, value, caret) {
    const inputs = this.lineInputs();
    const input = inputs[Math.max(0, Math.min(index, inputs.length - 1))];
    if (!input) return;
    if (value !== null && value !== undefined && input.value !== value) input.value = value;
    input.focus();
    const end = input.value.length;
    try {
      input.setSelectionRange(Math.min(caret ? caret[0] : end, end), Math.min(caret ? caret[1] : end, end));
    } catch (err) {
      // Not every input type supports selection ranges.
    }
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
    if(this.editor.authority) {
      if(this.destroyed||!this.opened||this.rendering||this.liveBusy||this.editor.authorityPainting||!(e.target instanceof HTMLInputElement)||!e.target.isConnected)return;
      const input=e.target;
      if(this.live||this.authorityLiveStarting)void this.finishLive().catch(error=>this.setStatus(String(error),true));
      else if(input.value!==(input.dataset.committed||''))void this.authorityCommitInput(input).catch(error=>this.setStatus(String(error),true));
      return;
    }
    if (this.destroyed || !this.opened || this.rendering) return;
    const input = e.target;
    if (!(input instanceof HTMLInputElement) || !input.isConnected) return;
    if (this.liveBusy) return;
    if (this.live && !input.classList.contains('te-alt-input--new') && this.lineIndexOf(input) === this.live.index) {
      this.finishLive();
    }
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
    if(this.editor.authority){void this.removeAlternative(index).then(()=>this.focusLine(Math.min(index,this.lineInputs().length-1)));return;}
    this.removeAlternative(index);
    this.focusLine(Math.min(index, this.lineInputs().length - 1));
  }

  onLineKeyDown(e) {
    if(this.editor.authority) {this.authorityLineKeyDown(e);return;}
    const input = e.target;
    if (!(input instanceof HTMLInputElement) || e.isComposing) return;
    const isNew = input.classList.contains('te-alt-input--new');
    const index = isNew ? this.lines().length : Number(input.closest('.te-alt-line').dataset.index);
    const count = this.lines().length;
    const plain = !e.ctrlKey && !e.metaKey && !e.shiftKey;
    if (e.key === 'Enter' && plain && !e.altKey && this.live && this.live.refused !== null && index === this.live.index) {
      // Enter does not keep text the model refused: the line stays live
      // until it is changed (or Escape cancels it).
      e.preventDefault();
      this.setStatus(`“${this.live.refused}” is already in the list.`, true);
      return;
    }
    if ((e.key === 'Enter' || e.key === 'ArrowUp' || e.key === 'ArrowDown') && plain) this.finishLive();
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
    if(this.editor.authority) {this.authorityPanelKeyDown(e);return;}
    if (this.destroyed || e.isComposing) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      // A new alternative being typed is cancelled (issue #106). Otherwise
      // close() commits the focused line: Escape never discards an edit of
      // an existing line (undo reverses the commit).
      if (this.live) this.cancelLive();
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
    // A live addition is one step: undo removes all of it.
    this.finishLive();
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

  authorityLineKeyDown(e) {
    const input=e.target;
    if(!(input instanceof HTMLInputElement)||e.isComposing)return;
    const plain=!e.ctrlKey&&!e.metaKey&&!e.shiftKey;
    if(!plain||!['Enter','ArrowUp','ArrowDown'].includes(e.key))return;
    e.preventDefault();
    if(e.key==='Enter' && this.live && this.live.refused!==null){this.setStatus('Alternative text was refused; correct it or press Escape.',true);return;}
    const index=this.lineIndexOf(input),isNew=input.classList.contains('te-alt-input--new');
    const finishing=(this.live||this.authorityLiveStarting)?this.finishLive():input.value!==(input.dataset.committed||'')?this.authorityCommitInput(input):Promise.resolve();
    if(e.key==='Enter') {
      this.authorityInputGroup=(this.authorityInputGroup||0)+1;
      this.whileBusy(()=>{const fresh=this.lineInputs().at(-1);if(fresh?.classList.contains('te-alt-input--new')){fresh.value='';fresh.focus();}});
    }
    const run=async()=> {
      await finishing;
      if(e.key==='Enter'){
        const pending=this.editor.authorityDrafts.get(`alternative-live-${this.authorityInputGroup||0}`);
        this.whileBusy(()=>this.restoreLine(this.lines().length,pending?.input.text??null,pending?.input.caret));return;
      }
      const next=index+(e.key==='ArrowUp'?-1:1);
      if(e.altKey) {if(!isNew&&index>0&&next>0&&next<this.lines().length)await this.moveAlternative(index,next);}
      else if(next>=0&&next<this.lines().length)await this.setActiveIndex(next);
      this.focusLine(Math.max(0,Math.min(next,this.lines().length)));
    };
    void run().catch(error=>this.setStatus(`Not accepted: ${error?.message||error}`,true));
  }

  authorityPanelKeyDown(e) {
    if(this.destroyed||e.isComposing)return;
    const key=(e.key||'').toLowerCase(),mod=e.ctrlKey||e.metaKey;
    if(e.key==='Escape') {
      e.preventDefault();e.stopPropagation();
      const pending=this.editor.authorityDrafts.has(`alternative-live-${this.authorityInputGroup||0}`);
      const cancel=(this.live||this.authorityLiveStarting||pending)?this.authorityCancelLive():Promise.resolve();
      void cancel.then(()=>this.authorityClose({restoreFocus:true})).catch(error=>this.setStatus(String(error),true));return;
    }
    if(mod&&typeof teShortcutMatches==='function'&&teShortcutMatches(e,'ctrl+shift+a')){e.preventDefault();void this.authorityClose({restoreFocus:true});return;}
    const undo=mod&&key==='z'&&!e.shiftKey&&!e.altKey,redo=mod&&((key==='z'&&e.shiftKey&&!e.altKey)||(key==='y'&&!e.shiftKey&&!e.altKey));
    if(!undo&&!redo)return;
    const input=e.target instanceof HTMLInputElement?e.target:null;
    if(input&&!this.live&&!this.authorityLiveStarting&&input.value!==(input.dataset.committed||''))return;
    e.preventDefault();e.stopPropagation();
    void this.finishLive().then(()=>undo?this.surface.undo():this.surface.redo()).then(()=>this.flush()).catch(error=>this.setStatus(String(error),true));
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
    if(this.editor.authority && !this.authorityLocal) return this.authoritySetTab(kind,{focusTab});
    if (!TE_ALT_KINDS.includes(kind)) throw new Error(`Unknown tab ${kind}`);
    this.finishLive();
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
    // Native close/backup settles the opt-in authority before teardown.
    // Teardown itself only reports drafts and never starts a new writer.
    if(!this.editor.authority)this.finishLive();
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
