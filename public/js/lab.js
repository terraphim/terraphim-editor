/*
 * The Lab popover and its mark actions (issue #14, spec R-8.1, R-8.2)
 * ===================================================================
 *
 * TeLabPopover opens above the bottom-centre LAB pill when the Write_On
 * chrome dispatches `te:lab` (public/js/chrome.js; the pill only acts in
 * Write_On mode). Header `The Lab guide…` / `what each idea does`, then the
 * six actions in engine order (labels from `wasmBindings.lab_actions()`),
 * then the trim levels of issue #15 (R-8.3 to R-8.5; TeLabTrim in
 * public/js/trim.js fills the `data-slot="trim"` slot and owns the faded
 * preview and the status card, reachable as `lab.trim`), then the results:
 * a legend, proposed fixes with Accept, and Clear marks.
 *
 * Marking never changes text
 * --------------------------
 * An action aligns the WASM model with the surface (the same
 * `editor.alignDocumentModel()` every model read uses), calls
 * `wasmBindings.lab_mark(id)` (src/lab.rs, over crates/terraphim_lab) and
 * puts the marks, unchanged (UTF-16 offsets), in the `lab-marks` layer of
 * `editor.decorations`. It never calls `surface.setDecorations` directly.
 * One action's marks are shown at a time; running another replaces them,
 * running the same one again recomputes them.
 *
 * Each item carries `className` `te-lab-mark te-lab-mark--<kind>` (kebab
 * case: `te-lab-mark--weak-sentence`), allow-listed attributes only
 * (`aria-describedby` pointing at a visually hidden description holding the
 * engine's reason, `data-lab-kind`), and its metadata in `data`
 * ({ kind, reason, proposal, text, score }), which the registry keeps at
 * live offsets.
 *
 * Marks are transient
 * -------------------
 * The registry maps every layer through every surface edit: marks after the
 * edit move, and an edit that lands inside a mark drops it. That is the
 * Lab's definition of "clears on the next edit that touches it"; an
 * insertion exactly at a mark's edge leaves it alone. Marks are not
 * re-run on edits (weakest-sentence ranking is document-wide and would
 * move under the writer's hands); run the action again to refresh them.
 * Leaving Write_On mode clears them. The popover never calls
 * `registry.set` from inside a surface change listener (that would force a
 * re-render mid-input); it only refreshes its own legend.
 *
 * Proposals (alternatives, never rewrites)
 * ----------------------------------------
 * Typo and punctuation marks carry a proposal. They are listed in the
 * popover with an explicit Accept, which checks that the text under the
 * mark is still what was marked, then calls
 * `surface.replaceRange(start, end, proposal, { source: 'lab' })`: exactly
 * the proposal, one undo step (only typing and deleting coalesce), and the
 * accepted mark drops because the edit lands inside it. Undo restores the
 * original text.
 *
 * Accessibility: a non-modal `role="dialog"` labelled by its header; the
 * actions are a `role="menu"` of `menuitemradio` buttons with a roving
 * tabindex (Up/Down, Home/End); Escape closes and returns focus to the LAB
 * pill; a pointer press outside closes. The pill gets `aria-expanded` and
 * `aria-controls`. While the popover is closed and marks are shown, a small
 * chip beside the pill says what is marked and offers Clear marks.
 *
 * Events (bubbling, from the popover): `te:lab-marks` { editor, action,
 * count } after an action runs or marks are cleared (action null), and
 * `te:lab-accept` { editor, start, end, text, proposal } after a proposal is
 * accepted.
 *
 * Lifecycle: every listener uses this instance's AbortController, which
 * follows the editor's signal; destroy() clears the layer, removes the DOM
 * and the pill attributes. Load order: after indicators.js and trim.js,
 * before editor.js. Design notes: docs/design/lab-popover.md
 */

let teLabInstances = 0;

const TE_LAB_LAYER = 'lab-marks';

/** What each idea does: one line per action, shown for the focused action. */
const TE_LAB_HINTS = {
  typos_and_punctuation: 'Proposes fixes for misspellings and punctuation slips. Nothing changes until you accept one.',
  weakest_sentences: 'Marks the most hedged, least central sentences. Paragraph openers are left alone.',
  long_sentences: 'Marks sentences of 30 words or more.',
  convoluted_sentences: 'Marks sentences carrying many clauses, asides and breaks.',
  off_tone: 'Marks informal words that do not fit a professional register.',
  hedges_and_filler: 'Marks hedges and filler words you could fade or cut.',
};

const TE_LAB_ICONS = {
  typos_and_punctuation: 'fa-solid fa-spell-check',
  weakest_sentences: 'fa-solid fa-feather',
  long_sentences: 'fa-solid fa-ruler-horizontal',
  convoluted_sentences: 'fa-solid fa-diagram-project',
  off_tone: 'fa-solid fa-music',
  hedges_and_filler: 'fa-solid fa-cloud',
};

/** Singular and plural names per mark kind, for the legend. */
const TE_LAB_KINDS = {
  typo: ['typo', 'typos'],
  punctuation: ['punctuation slip', 'punctuation slips'],
  weak_sentence: ['weak sentence', 'weak sentences'],
  long_sentence: ['long sentence', 'long sentences'],
  convoluted_sentence: ['convoluted sentence', 'convoluted sentences'],
  off_tone: ['off-tone word', 'off-tone words'],
  hedge: ['hedge', 'hedges'],
  filler: ['filler word', 'filler words'],
};

class TeLabPopover {
  constructor(editor, options = {}) {
    this.editor = editor;
    this.surface = editor.surface;
    this.registry = editor.decorations;
    this.instance = ++teLabInstances;
    this.isOpen = false;
    this.destroyed = false;
    // Id of the action whose marks are shown, or null.
    this.action = null;
    this.actions = [];
    this.lastFocusIndex = 0;

    this.abortController = new AbortController();
    const signal = this.abortController.signal;
    if (options.signal) {
      if (options.signal.aborted) this.abortController.abort();
      else options.signal.addEventListener('abort', () => this.destroy(), { signal });
    }

    this.build();

    document.addEventListener('te:lab', (e) => {
      if (!e.detail || !e.detail.editor || e.detail.editor === this.editor) this.toggle();
    }, { signal });
    document.addEventListener('te:mode-change', (e) => {
      if (e.detail && e.detail.editor && e.detail.editor !== this.editor) return;
      if (!e.detail || e.detail.mode !== 'write-on') {
        this.close(false);
        this.clear();
        if (this.trim) this.trim.reset();
      }
    }, { signal });
    document.addEventListener('pointerdown', (e) => this.onOutsidePointer(e), { signal, capture: true });
    window.addEventListener('resize', () => this.position(), { signal });
    // The registry (subscribed first) has already mapped the marks through
    // the edit; only the legend and proposals need refreshing.
    this.offChange = this.surface.onChange(() => {
      if (this.action !== null) this.renderResults();
    });
  }

  static get LAYER() {
    return TE_LAB_LAYER;
  }

  /** The kebab-case class suffix of a mark kind. */
  static kindClass(kind) {
    return `te-lab-mark--${String(kind).replace(/_/g, '-')}`;
  }

  /** "3 weak sentences", from a kind and a count. */
  static describeCount(kind, count) {
    const names = TE_LAB_KINDS[kind] || [String(kind).replace(/_/g, ' '), String(kind).replace(/_/g, ' ')];
    return `${count} ${count === 1 ? names[0] : names[1]}`;
  }

  /** Show spaces in a short proposal so " ," and "," read differently. */
  static visible(text) {
    return String(text).replace(/ /g, '␣');
  }

  api() {
    const api = typeof this.editor.documentApi === 'function' ? this.editor.documentApi() : null;
    return api && typeof api.lab_mark === 'function' && typeof api.lab_actions === 'function' ? api : null;
  }

  pill() {
    const chrome = this.editor.chrome;
    return chrome && chrome.root ? chrome.root.querySelector('[data-control="lab"]') : null;
  }

  // ---------------------------------------------------------------------
  // DOM
  // ---------------------------------------------------------------------

  build() {
    const signal = this.abortController.signal;
    const id = (name) => `te-lab-${name}-${this.instance}`;
    const el = (tag, className, text) => {
      const node = document.createElement(tag);
      if (className) node.className = className;
      if (text !== undefined) node.textContent = text;
      return node;
    };

    const root = el('div', 'te-lab');
    root.id = id('popover');
    root.hidden = true;
    root.setAttribute('role', 'dialog');
    root.setAttribute('aria-modal', 'false');
    root.setAttribute('aria-labelledby', id('title'));
    this.root = root;

    const header = el('div', 'te-lab-header');
    const title = el('h2', 'te-lab-title', 'The Lab guide…');
    title.id = id('title');
    const sub = el('span', 'te-lab-subtitle', 'what each idea does');
    header.append(title, sub);

    const menu = el('div', 'te-lab-actions');
    menu.setAttribute('role', 'menu');
    menu.setAttribute('aria-label', 'Lab actions');
    menu.setAttribute('aria-describedby', id('hint'));
    this.menu = menu;

    const api = this.api();
    let actions = [];
    if (api) {
      try {
        actions = Array.from(api.lab_actions() || []);
      } catch (err) {
        console.error('lab_actions failed', err);
      }
    }
    this.actions = actions;
    this.items = actions.map((action, i) => {
      const item = el('button', 'te-lab-action');
      item.type = 'button';
      item.setAttribute('role', 'menuitemradio');
      item.setAttribute('aria-checked', 'false');
      item.tabIndex = i === 0 ? 0 : -1;
      item.dataset.action = action.id;
      const icon = el('i', TE_LAB_ICONS[action.id] || 'fa-solid fa-flask');
      icon.setAttribute('aria-hidden', 'true');
      const label = el('span', 'te-lab-action-label', action.label);
      const count = el('span', 'te-lab-action-count');
      count.setAttribute('aria-hidden', 'true');
      item.append(icon, label, count);
      item.addEventListener('click', () => this.run(action.id), { signal });
      item.addEventListener('focus', () => this.showHint(action.id), { signal });
      item.addEventListener('mouseenter', () => this.showHint(action.id), { signal });
      menu.appendChild(item);
      return item;
    });
    menu.addEventListener('keydown', (e) => this.onMenuKey(e), { signal });

    const hint = el('p', 'te-lab-hint');
    hint.id = id('hint');
    this.hint = hint;

    // Trim levels (R-8.3 to R-8.5, issue #15): TeLabTrim (trim.js) fills
    // this slot with its buttons and unhides it.
    const trim = el('div', 'te-lab-trim');
    trim.dataset.slot = 'trim';
    trim.hidden = true;
    this.trimSlot = trim;
    this.trim = api && typeof window.TeLabTrim === 'function'
      ? new window.TeLabTrim(this, trim, { signal })
      : null;

    const results = el('div', 'te-lab-results');
    results.hidden = true;
    const legend = el('div', 'te-lab-legend');
    legend.setAttribute('role', 'status');
    legend.setAttribute('aria-live', 'polite');
    const proposals = el('ul', 'te-lab-proposals');
    proposals.setAttribute('aria-label', 'Proposed fixes');
    const clear = el('button', 'te-lab-clear');
    clear.type = 'button';
    const clearIcon = el('i', 'fa-solid fa-eraser');
    clearIcon.setAttribute('aria-hidden', 'true');
    clear.append(clearIcon, el('span', '', 'Clear marks'));
    clear.addEventListener('click', () => this.clear(true), { signal });
    results.append(legend, proposals, clear);
    this.results = results;
    this.legend = legend;
    this.proposals = proposals;
    this.clearButton = clear;
    proposals.addEventListener('click', (e) => {
      const button = e.target.closest('[data-mark]');
      if (button && proposals.contains(button)) this.accept(button.dataset.mark, true);
    }, { signal });

    const note = el('p', 'te-lab-note');
    note.setAttribute('role', 'alert');
    note.hidden = true;
    this.note = note;
    if (!api) {
      note.textContent = 'The Lab needs the WebAssembly document model, which did not load. Reload the page to try again.';
      note.hidden = false;
    }

    root.append(header, menu, hint, trim, results, note);
    root.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        this.close(true);
      }
    }, { signal });

    // Closed-state legend beside the pill.
    const chip = el('div', 'te-lab-chip');
    chip.hidden = true;
    const chipText = el('span', 'te-lab-chip-text');
    chipText.setAttribute('role', 'status');
    const chipClear = el('button', 'te-lab-chip-clear', 'Clear marks');
    chipClear.type = 'button';
    chipClear.addEventListener('click', () => this.clear(), { signal });
    chip.append(chipText, chipClear);
    this.chip = chip;
    this.chipText = chipText;

    // Visually hidden reasons referenced by the marks' aria-describedby.
    const descriptions = el('div', 'te-lab-descriptions');
    this.descriptions = descriptions;

    const mount = (this.editor.chrome && this.editor.chrome.root) || document.body;
    mount.append(root, chip, descriptions);

    const pill = this.pill();
    if (pill) {
      pill.setAttribute('aria-haspopup', 'dialog');
      pill.setAttribute('aria-expanded', 'false');
      pill.setAttribute('aria-controls', root.id);
    }
    this.showHint(actions.length ? actions[0].id : null);
  }

  showHint(actionId) {
    this.hint.textContent = (actionId && TE_LAB_HINTS[actionId]) || '';
  }

  // ---------------------------------------------------------------------
  // Open, close, position
  // ---------------------------------------------------------------------

  toggle() {
    if (this.isOpen) this.close(true);
    else this.open();
  }

  open() {
    if (this.destroyed || this.isOpen) return;
    this.isOpen = true;
    this.root.hidden = false;
    const pill = this.pill();
    if (pill) pill.setAttribute('aria-expanded', 'true');
    this.renderResults();
    if (this.trim) this.trim.sync();
    this.position();
    const checked = this.items.findIndex((item) => item.dataset.action === this.action);
    this.focusItem(checked >= 0 ? checked : 0);
  }

  close(restoreFocus = true) {
    if (!this.isOpen) return;
    const hadFocus = this.root.contains(document.activeElement);
    this.isOpen = false;
    this.root.hidden = true;
    const pill = this.pill();
    if (pill) pill.setAttribute('aria-expanded', 'false');
    this.renderResults();
    if (this.trim) this.trim.sync();
    if (restoreFocus && hadFocus && pill) pill.focus();
  }

  onOutsidePointer(e) {
    if (!this.isOpen) return;
    const t = e.target;
    const pill = this.pill();
    if (this.root.contains(t) || this.chip.contains(t) || (pill && pill.contains(t))) return;
    this.close(false);
  }

  /** Anchor the popover above the pill and the chip beside it. */
  position() {
    if (this.destroyed) return;
    const pill = this.pill();
    if (!pill) return;
    const r = pill.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) return;
    if (this.isOpen) {
      this.root.style.left = `${r.left + r.width / 2}px`;
      this.root.style.bottom = `${Math.max(0, window.innerHeight - r.top + 10)}px`;
    }
    if (!this.chip.hidden) {
      this.chip.style.left = `${r.right + 12}px`;
      this.chip.style.top = `${r.top + r.height / 2}px`;
    }
  }

  focusItem(index) {
    if (this.items.length === 0) return;
    const i = (index + this.items.length) % this.items.length;
    this.items.forEach((item, j) => { item.tabIndex = j === i ? 0 : -1; });
    this.lastFocusIndex = i;
    this.items[i].focus();
  }

  onMenuKey(e) {
    const current = this.items.indexOf(document.activeElement);
    if (current < 0) return;
    let next = null;
    if (e.key === 'ArrowDown') next = current + 1;
    else if (e.key === 'ArrowUp') next = current - 1;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = this.items.length - 1;
    if (next === null) return;
    e.preventDefault();
    this.focusItem(next);
  }

  // ---------------------------------------------------------------------
  // Marks
  // ---------------------------------------------------------------------

  /**
   * Run action `id` over the document and show its marks. Returns the
   * engine's marks, or null when the Lab is unavailable or the call failed.
   */
  run(id) {
    if (this.destroyed) return null;
    const api = this.api();
    if (!api) return null;
    let marks;
    try {
      this.editor.alignDocumentModel(api);
      marks = Array.from(api.lab_mark(id) || []);
    } catch (err) {
      console.error('lab_mark failed', err);
      this.note.textContent = `The Lab could not run this action: ${err && err.message ? err.message : err}`;
      this.note.hidden = false;
      return null;
    }
    this.note.hidden = true;
    const text = this.surface.getText();
    const descriptions = document.createDocumentFragment();
    const items = marks.map((m, i) => {
      const markId = `m${i}`;
      const descId = `te-lab-desc-${this.instance}-${markId}`;
      const desc = document.createElement('span');
      desc.id = descId;
      const name = (TE_LAB_KINDS[m.kind] || [m.kind])[0];
      desc.textContent = `Lab: ${name}. ${m.reason}${typeof m.proposal === 'string' ? `. Proposed: “${m.proposal}”` : ''}`;
      descriptions.appendChild(desc);
      return {
        id: markId,
        start: m.start,
        end: m.end,
        className: `te-lab-mark ${TeLabPopover.kindClass(m.kind)}`,
        attributes: { 'aria-describedby': descId, 'data-lab-kind': m.kind },
        data: {
          kind: m.kind,
          reason: m.reason,
          proposal: typeof m.proposal === 'string' ? m.proposal : null,
          text: text.slice(m.start, m.end),
          score: m.score,
        },
      };
    });
    this.descriptions.replaceChildren(descriptions);
    this.action = id;
    this.keepFocus(() => this.registry.set(TE_LAB_LAYER, items));
    for (const item of this.items) {
      item.setAttribute('aria-checked', item.dataset.action === id ? 'true' : 'false');
    }
    this.renderResults();
    this.emit('te:lab-marks', { action: id, count: items.length });
    return marks;
  }

  /**
   * Run `fn` (which re-renders the surface) without losing focus: the
   * surface restores its own selection on render, which would pull focus
   * out of the popover or the chip.
   */
  keepFocus(fn) {
    const focused = document.activeElement;
    const ours = focused && (this.root.contains(focused) || this.chip.contains(focused)) ? focused : null;
    const result = fn();
    if (ours && ours.isConnected && document.activeElement !== ours && !ours.closest('[hidden]')) {
      ours.focus({ preventScroll: true });
    }
    return result;
  }

  /** The shown marks at their live offsets (with `data`). */
  marks() {
    return this.registry ? this.registry.get(TE_LAB_LAYER) : [];
  }

  /** Remove every mark. `focusMenu` keeps focus in the popover. */
  clear(focusMenu = false) {
    if (this.destroyed) return;
    const had = this.action !== null || this.marks().length > 0;
    this.action = null;
    const fromChip = this.chip.contains(document.activeElement);
    if (this.surface && !this.surface.destroyed) this.keepFocus(() => this.registry.clear(TE_LAB_LAYER));
    this.descriptions.replaceChildren();
    for (const item of this.items) item.setAttribute('aria-checked', 'false');
    this.renderResults();
    if (focusMenu && this.isOpen) this.focusItem(this.lastFocusIndex);
    // The chip hides with the marks: hand focus to the pill.
    if (fromChip && this.pill()) this.pill().focus();
    if (had) this.emit('te:lab-marks', { action: null, count: 0 });
  }

  /**
   * Apply the proposal of mark `markId` as one undo step. Returns true if
   * the text changed. Refuses (false) if the mark is gone, has no proposal
   * or no longer covers the text it marked.
   */
  accept(markId, keepFocus = false) {
    const mark = this.marks().find((m) => m.id === String(markId));
    if (!mark || !mark.data || typeof mark.data.proposal !== 'string') return false;
    if (this.surface.getText().slice(mark.start, mark.end) !== mark.data.text) return false;
    const index = Array.from(this.proposals.querySelectorAll('[data-mark]'))
      .findIndex((b) => b.dataset.mark === mark.id);
    this.surface.replaceRange(mark.start, mark.end, mark.data.proposal, { source: 'lab' });
    // The change listener has refreshed the list; keep focus in it.
    if (keepFocus && this.isOpen) {
      const buttons = this.proposals.querySelectorAll('[data-mark]');
      if (buttons.length) buttons[Math.min(Math.max(index, 0), buttons.length - 1)].focus();
      else this.clearButton.focus();
    }
    this.emit('te:lab-accept', {
      start: mark.start, end: mark.end, text: mark.data.text, proposal: mark.data.proposal,
    });
    return true;
  }

  /** Refresh the legend, proposals and the closed-state chip. */
  renderResults() {
    if (this.destroyed) return;
    const marks = this.action === null ? [] : this.marks();
    const counts = new Map();
    for (const m of marks) {
      const kind = m.data ? m.data.kind : '';
      counts.set(kind, (counts.get(kind) || 0) + 1);
    }
    const label = (this.actions.find((a) => a.id === this.action) || {}).label || '';
    const summary = Array.from(counts, ([kind, n]) => TeLabPopover.describeCount(kind, n)).join(', ');

    for (const item of this.items) {
      const count = item.querySelector('.te-lab-action-count');
      count.textContent = item.dataset.action === this.action ? String(marks.length) : '';
    }

    this.results.hidden = this.action === null;
    if (this.action !== null) {
      const legend = this.legend;
      legend.replaceChildren();
      if (marks.length === 0) {
        legend.textContent = `${label}: nothing to mark.`;
      } else {
        for (const [kind, n] of counts) {
          const entry = document.createElement('span');
          entry.className = 'te-lab-legend-entry';
          // The swatch is a live sample drawn with the mark's own class.
          const swatch = document.createElement('span');
          swatch.className = `te-lab-swatch te-lab-mark ${TeLabPopover.kindClass(kind)}`;
          swatch.setAttribute('aria-hidden', 'true');
          swatch.textContent = 'abc';
          entry.append(swatch, document.createTextNode(TeLabPopover.describeCount(kind, n)));
          legend.appendChild(entry);
        }
      }
      const rows = marks.filter((m) => m.data && typeof m.data.proposal === 'string');
      const list = document.createDocumentFragment();
      for (const m of rows) {
        const li = document.createElement('li');
        li.className = 'te-lab-proposal';
        const from = document.createElement('span');
        from.className = 'te-lab-from';
        from.textContent = TeLabPopover.visible(m.data.text);
        const arrow = document.createElement('i');
        arrow.className = 'fa-solid fa-arrow-right-long te-lab-arrow';
        arrow.setAttribute('aria-hidden', 'true');
        const to = document.createElement('span');
        to.className = 'te-lab-to';
        to.textContent = TeLabPopover.visible(m.data.proposal);
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'te-lab-accept';
        button.dataset.mark = m.id;
        button.textContent = 'Accept';
        button.setAttribute('aria-label', `Accept: replace “${m.data.text}” with “${m.data.proposal}”`);
        li.append(from, arrow, to, button);
        list.appendChild(li);
      }
      this.proposals.replaceChildren(list);
      this.proposals.hidden = rows.length === 0;
    }

    const showChip = !this.isOpen && marks.length > 0;
    this.chip.hidden = !showChip;
    if (showChip) {
      this.chipText.textContent = summary;
      this.position();
    }
  }

  emit(type, detail) {
    this.root.dispatchEvent(new CustomEvent(type, {
      bubbles: true,
      detail: Object.assign({ editor: this.editor }, detail),
    }));
  }

  /** Clear the marks, remove the DOM, the pill attributes and every listener. */
  destroy() {
    if (this.destroyed) return;
    if (this.surface && !this.surface.destroyed && this.registry) this.registry.clear(TE_LAB_LAYER);
    if (this.trim) this.trim.destroy();
    this.destroyed = true;
    this.isOpen = false;
    this.offChange();
    this.abortController.abort();
    const pill = this.pill();
    if (pill) {
      pill.removeAttribute('aria-haspopup');
      pill.removeAttribute('aria-expanded');
      pill.removeAttribute('aria-controls');
    }
    this.root.remove();
    this.chip.remove();
    this.descriptions.remove();
  }
}

window.TeLabPopover = TeLabPopover;
