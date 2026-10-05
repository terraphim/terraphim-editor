/*
 * Trim levels, the faded preview and the status card (issue #15, spec R-8.3
 * to R-8.5)
 * ==========================================================================
 *
 * TeLabTrim is created by TeLabPopover (public/js/lab.js) and fills its
 * `data-slot="trim"` with the five level buttons (R-8.3): Original ·
 * Slight trim ~10% · Tighten more ~20% · Even sharper ~30% · Cut in half
 * ~50%. Reachable as `editor.lab.trim`.
 *
 * Engine and bridge
 * -----------------
 * The spans come from the deterministic trim engine (crates/terraphim_lab,
 * `trim_plan`) through src/lab.rs: `trim_plan_json()` (computed once per
 * version of the body and cached with it), `trim_status(level, kept)` (the
 * card numbers plus the active pieces to fade, keeps applied and outer cuts
 * split around kept ranges) and `trim_make_cuts(level, kept)` (typed edits).
 * Every call aligns the model with the surface first. The card's count line
 * is the engine's `card_text()` verbatim, so it always equals the engine.
 *
 * Preview, never a rewrite (R-8.4)
 * --------------------------------
 * A level fades its active pieces in the `lab-trim` layer of the decoration
 * registry (never `setDecorations` directly), class `te-trim-faded`. That
 * class uses the ghost fade token (`--te-ghost-opacity`) but is not
 * `te-ghost`: trim previews and user ghosts stay distinguishable, and the
 * preview never calls `ghost_range`/`revive_range`, so no ghost is ever
 * written into the document model and user-set ghosts are untouched.
 * Preview items carry no attributes (as with ghosts), so the indicators'
 * `aria-describedby` on shared text is never overwritten; the card and the
 * walk-through line announce what would go. Previewing, keeping, walking,
 * Done and Original never touch the undo history.
 *
 * Keep (R-8.5)
 * ------------
 * While the card is shown, clicking a faded span keeps the OUTERMOST faded
 * cut under the pointer: the rendered segment lists every decoration over
 * it (`data-te-decoration`), each `lab-trim` piece on it covers the whole
 * segment, and the widest of their cuts (by the plan's own range) is kept.
 * Keeping a sentence therefore also keeps the fillers faded inside it. The
 * card and the preview are recomputed with one `trim_status` call. Kept ids
 * survive level changes within one plan; Original forgets them. Keyboard:
 * Walk through, then Keep (or K) on the current cut.
 *
 * Make the cuts
 * -------------
 * `trim_make_cuts` returns the engine's edits on the current body (cuts plus
 * the punctuation, capitalisation, whitespace and emptied-block tidy-ups).
 * They are applied with `EditorSurface.replaceRanges(edits, { source:
 * 'trim' })`: each edit goes through the normal edit path (the model mirrors
 * it with `apply_edit`, so spans and ghosts follow the usual rules) but the
 * whole set is ONE history entry; undo replays the inverses and the model
 * re-attaches the annotations the cuts detached, so text and annotations
 * both come back. A plan whose body no longer matches is refused by the
 * bridge: the plan is recomputed and nothing is cut.
 *
 * Walk through (decision)
 * -----------------------
 * Navigation only: it steps through the active pieces in document order
 * (Previous / Next, ArrowLeft / ArrowRight), marks the current one
 * (`te-trim-current`) and scrolls it into view, announces its reason, and
 * offers Keep (or K). It never cuts one piece at a time: Make the cuts at
 * the end applies whatever is still faded as one undo step. Stop (or
 * Escape) leaves the walk; the card stays.
 *
 * Done (decision)
 * ---------------
 * "Done dismisses the card and leaves the ghosting as is": the card closes
 * and the faded preview stays exactly as it is, with the level still
 * selected in the popover. Done does NOT write ghosts into the document:
 * the preview is transient, as the spec's trim spans are a preview of the
 * cuts, not user ghosts. Picking the selected level again reopens the card.
 * Clicks on faded text only keep while the card is shown.
 *
 * Edits while trimming (decision)
 * -------------------------------
 * Any surface edit that is not the trim's own (source 'trim') ends the trim
 * at once (stale: no keep, no cut) and, on the next tick (never inside the
 * change listener, so typing is not re-rendered mid-input), clears the
 * preview, the kept set and the card; the level returns to Original and a
 * polite live region says why. Stale cuts are never applied.
 *
 * Original, leaving Write_On and destroy() clear everything. Events
 * (bubbling, from the card): `te:trim` { editor, level, status } after the
 * preview changes and `te:trim-cut` { editor, level, edits } after Make the
 * cuts. Listeners use this instance's AbortController (following the
 * Lab's). Load order: before lab.js. Design notes in this header.
 */

let teTrimInstances = 0;

const TE_TRIM_LAYER = 'lab-trim';

const TE_TRIM_HINT = 'Faded words would go. Click one to keep it.';

class TeLabTrim {
  constructor(lab, slot, options = {}) {
    this.lab = lab;
    this.editor = lab.editor;
    this.surface = lab.surface;
    this.registry = lab.registry;
    this.slot = slot;
    this.instance = ++teTrimInstances;
    this.destroyed = false;
    // Selected level id ('original', 'slight', ...).
    this.level = 'original';
    // The cached plan (from trim_plan_json) and the text it was made for.
    this.plan = null;
    this.planText = null;
    this.kept = new Set();
    // Last trim_status result.
    this.status = null;
    this.active = [];
    this.cardOpen = false;
    // Index into this.active while walking, or -1.
    this.walkIndex = -1;
    this.stale = false;
    this.applying = false;
    this.resetTimer = null;

    this.abortController = new AbortController();
    const signal = this.abortController.signal;
    if (options.signal) {
      if (options.signal.aborted) this.abortController.abort();
      else options.signal.addEventListener('abort', () => this.destroy(), { signal });
    }

    this.build();

    this.surface.root.addEventListener('click', (e) => this.onSurfaceClick(e), { signal });
    this.offChange = this.surface.onChange((change) => this.onSurfaceChange(change));
  }

  static get LAYER() {
    return TE_TRIM_LAYER;
  }

  api() {
    const api = typeof this.editor.documentApi === 'function' ? this.editor.documentApi() : null;
    return api && typeof api.trim_plan_json === 'function' && typeof api.trim_status === 'function' &&
      typeof api.trim_make_cuts === 'function' ? api : null;
  }

  /** True while a level other than Original is previewed. */
  get trimming() {
    return this.level !== 'original';
  }

  // ---------------------------------------------------------------------
  // DOM
  // ---------------------------------------------------------------------

  build() {
    const signal = this.abortController.signal;
    const el = (tag, className, text) => {
      const node = document.createElement(tag);
      if (className) node.className = className;
      if (text !== undefined) node.textContent = text;
      return node;
    };
    const id = (name) => `te-trim-${name}-${this.instance}`;
    const levels = [
      ['original', 'Original', ''],
      ['slight', 'Slight trim', '~10%'],
      ['tighten', 'Tighten more', '~20%'],
      ['sharper', 'Even sharper', '~30%'],
      ['half', 'Cut in half', '~50%'],
    ];

    // Level buttons in the popover slot: a radio group with a roving tabindex.
    const label = el('span', 'te-trim-label', 'Trim');
    label.id = id('label');
    const group = el('div', 'te-trim-levels');
    group.setAttribute('role', 'radiogroup');
    group.setAttribute('aria-labelledby', label.id);
    this.levelButtons = levels.map(([level, name, target]) => {
      const b = el('button', 'te-trim-level');
      b.type = 'button';
      b.dataset.level = level;
      b.setAttribute('role', 'radio');
      b.setAttribute('aria-checked', level === 'original' ? 'true' : 'false');
      b.tabIndex = level === 'original' ? 0 : -1;
      b.append(el('span', 'te-trim-level-name', name));
      if (target) b.append(el('span', 'te-trim-level-target', target));
      b.addEventListener('click', () => this.select(level, { focusCard: true }), { signal });
      group.appendChild(b);
      return b;
    });
    group.addEventListener('keydown', (e) => this.onLevelKey(e), { signal });
    this.slot.replaceChildren(label, group);
    this.slot.hidden = false;

    // The floating status card (R-8.4), bottom-centre.
    const card = el('section', 'te-trim-card');
    card.id = id('card');
    card.hidden = true;
    card.tabIndex = -1;
    card.setAttribute('aria-labelledby', id('level'));
    const head = el('div', 'te-trim-card-head');
    const levelName = el('strong', 'te-trim-card-level');
    levelName.id = id('level');
    const count = el('span', 'te-trim-card-count');
    count.setAttribute('role', 'status');
    count.setAttribute('aria-live', 'polite');
    head.append(levelName, count);
    // The meter: the document as a bar, the part that would go faded with
    // the same token as the faded words.
    const meter = el('div', 'te-trim-meter');
    meter.setAttribute('aria-hidden', 'true');
    const meterKeep = el('span', 'te-trim-meter-keep');
    meter.append(meterKeep);
    const hint = el('p', 'te-trim-card-hint', TE_TRIM_HINT);

    const walk = el('div', 'te-trim-walk');
    walk.hidden = true;
    const walkText = el('p', 'te-trim-walk-text');
    walkText.setAttribute('aria-live', 'polite');
    const walkButtons = el('div', 'te-trim-walk-buttons');
    const button = (cls, icon, text, onClick, keys) => {
      const b = el('button', cls);
      b.type = 'button';
      if (icon) {
        const i = el('i', icon);
        i.setAttribute('aria-hidden', 'true');
        b.append(i);
      }
      b.append(el('span', '', text));
      if (keys) b.setAttribute('aria-keyshortcuts', keys);
      b.addEventListener('click', onClick, { signal });
      return b;
    };
    this.prevButton = button('te-trim-prev', 'fa-solid fa-chevron-left', 'Previous', () => this.step(-1), 'ArrowLeft');
    this.keepButton = button('te-trim-keep', 'fa-solid fa-thumbtack', 'Keep', () => this.keepCurrent(), 'K');
    this.nextButton = button('te-trim-next', 'fa-solid fa-chevron-right', 'Next', () => this.step(1), 'ArrowRight');
    this.stopButton = button('te-trim-stop', '', 'Stop', () => this.stopWalk(true), 'Escape');
    walkButtons.append(this.prevButton, this.keepButton, this.nextButton, this.stopButton);
    walk.append(walkText, walkButtons);

    const actions = el('div', 'te-trim-actions');
    this.cutButton = button('te-trim-cut', 'fa-solid fa-scissors', 'Make the cuts', () => this.makeCuts());
    this.walkButton = button('te-trim-walk-start', 'fa-solid fa-shoe-prints', 'Walk through', () => this.startWalk());
    this.doneButton = button('te-trim-done', '', 'Done', () => this.done());
    actions.append(this.cutButton, this.walkButton, this.doneButton);

    card.append(head, meter, hint, walk, actions);
    card.addEventListener('keydown', (e) => this.onCardKey(e), { signal });
    this.card = card;
    this.levelName = levelName;
    this.count = count;
    this.meterKeep = meterKeep;
    this.walk = walk;
    this.walkText = walkText;

    // Always-present live region for messages when the card is hidden.
    const live = el('div', 'te-trim-live');
    live.setAttribute('role', 'status');
    live.setAttribute('aria-live', 'polite');
    this.live = live;

    const mount = (this.editor.chrome && this.editor.chrome.root) || document.body;
    mount.append(card, live);
  }

  onLevelKey(e) {
    const current = this.levelButtons.indexOf(document.activeElement);
    if (current < 0) return;
    const n = this.levelButtons.length;
    let next = null;
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = (current + 1) % n;
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') next = (current - 1 + n) % n;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = n - 1;
    if (next === null) return;
    e.preventDefault();
    this.levelButtons.forEach((b, i) => { b.tabIndex = i === next ? 0 : -1; });
    this.levelButtons[next].focus();
  }

  onCardKey(e) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      if (this.walkIndex >= 0) this.stopWalk(true);
      else this.done();
      return;
    }
    if (this.walkIndex < 0 || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === 'ArrowRight') { e.preventDefault(); this.step(1); }
    else if (e.key === 'ArrowLeft') { e.preventDefault(); this.step(-1); }
    else if (e.key === 'k' || e.key === 'K') { e.preventDefault(); this.keepCurrent(); }
  }

  // ---------------------------------------------------------------------
  // Levels and the preview
  // ---------------------------------------------------------------------

  /**
   * Select level `level`. Original restores the pre-trim state; any other
   * level previews its cuts and shows the card (closing the popover).
   * Returns the status, or null.
   */
  select(level, opts = {}) {
    if (this.destroyed) return null;
    if (level === 'original') {
      this.reset();
      return null;
    }
    const api = this.api();
    if (!api) return null;
    this.cancelPendingReset();
    this.stale = false;
    this.level = level;
    const status = this.refresh();
    if (!status) return null;
    this.cardOpen = true;
    this.lab.close(false);
    this.sync();
    if (opts.focusCard) this.card.focus({ preventScroll: true });
    return status;
  }

  /** The plan for the current text, recomputed when the text changed. */
  ensurePlan(api) {
    const text = this.surface.getText();
    if (this.plan && this.planText === text) return this.plan;
    this.plan = api.trim_plan_json();
    this.planText = text;
    this.kept.clear();
    return this.plan;
  }

  /** Recompute the status and the preview (one bridge call when the plan is fresh). */
  refresh() {
    const api = this.api();
    if (!api || !this.trimming) return null;
    let status;
    try {
      this.editor.alignDocumentModel(api);
      this.ensurePlan(api);
      status = api.trim_status(this.level, JSON.stringify(Array.from(this.kept)));
    } catch (err) {
      // A stale plan (the body moved under it): recompute once.
      try {
        this.plan = null;
        this.ensurePlan(api);
        status = api.trim_status(this.level, JSON.stringify([]));
      } catch (err2) {
        console.error('trim failed', err2);
        this.announce(`Trim is unavailable: ${err2 && err2.message ? err2.message : err2}`);
        return null;
      }
    }
    this.status = status;
    this.active = Array.from(status.active || []);
    if (this.walkIndex >= this.active.length) this.walkIndex = this.active.length - 1;
    this.draw();
    this.renderCard();
    this.emit('te:trim', { level: this.level, status });
    return status;
  }

  /** Put the active pieces in the registry layer. */
  draw() {
    const current = this.walkIndex >= 0 ? this.active[this.walkIndex] : null;
    const items = this.active.map((c) => ({
      id: `${c.id}-${c.start}`,
      start: c.start,
      end: c.end,
      className: c === current ? 'te-trim-faded te-trim-current' : 'te-trim-faded',
      data: { cut: c.id, reason: c.reason, tier: c.tier },
    }));
    this.keepFocus(() => this.registry.set(TE_TRIM_LAYER, items));
  }

  /** Run `fn` (which re-renders the surface) without losing focus from our UI. */
  keepFocus(fn) {
    const focused = document.activeElement;
    const ours = focused && (this.card.contains(focused) || this.lab.root.contains(focused)) ? focused : null;
    const result = fn();
    if (ours && ours.isConnected && document.activeElement !== ours && !ours.closest('[hidden]')) {
      ours.focus({ preventScroll: true });
    }
    return result;
  }

  /** Show or hide the card (hidden while the popover is open) and mark levels. */
  sync() {
    if (this.destroyed) return;
    for (const b of this.levelButtons) {
      const on = b.dataset.level === this.level;
      b.setAttribute('aria-checked', on ? 'true' : 'false');
      b.tabIndex = on ? 0 : -1;
    }
    this.card.hidden = !(this.cardOpen && !this.lab.isOpen);
    if (this.cardOpen) this.surface.root.dataset.teTrimming = '';
    else delete this.surface.root.dataset.teTrimming;
  }

  renderCard() {
    const status = this.status;
    if (!status) return;
    this.levelName.textContent = status.label;
    this.count.textContent = status.cardText;
    const before = status.words_before || 0;
    const share = before ? (100 * status.words_after) / before : 100;
    this.meterKeep.style.width = `${share}%`;
    const nothing = this.active.length === 0;
    this.cutButton.disabled = nothing;
    this.walkButton.disabled = nothing;
    if (this.walkIndex >= 0) this.renderWalk();
  }

  // ---------------------------------------------------------------------
  // Keep
  // ---------------------------------------------------------------------

  onSurfaceClick(e) {
    if (!this.cardOpen || this.stale || !this.trimming) return;
    const seg = e.target && e.target.closest ? e.target.closest('[data-te-decoration]') : null;
    if (!seg || !this.surface.root.contains(seg)) return;
    const cut = this.cutAtSegment(seg);
    if (cut !== null) this.keep(cut);
  }

  /** The outermost faded cut covering a rendered segment, or null. */
  cutAtSegment(seg) {
    const ids = (seg.getAttribute('data-te-decoration') || '').split(/\s+/)
      .filter((t) => t.startsWith(`${TE_TRIM_LAYER}:`))
      .map((t) => t.slice(TE_TRIM_LAYER.length + 1));
    if (ids.length === 0) return null;
    const items = this.registry.get(TE_TRIM_LAYER).filter((item) => ids.includes(item.id));
    return this.outermost(items.map((item) => item.data && item.data.cut));
  }

  /** Of the cut ids given, the one whose plan range is widest (outermost). */
  outermost(cutIds) {
    const cuts = this.plan ? this.plan.cuts : [];
    let best = null;
    for (const cid of cutIds) {
      const c = cuts.find((x) => x.id === cid);
      if (!c) continue;
      const width = c.end - c.start;
      const bestWidth = best ? best.end - best.start : -1;
      if (width > bestWidth || (width === bestWidth && c.start < best.start)) best = c;
    }
    return best ? best.id : null;
  }

  /** Keep cut `cutId` (a plan cut id). Returns the new status, or null. */
  keep(cutId) {
    if (this.destroyed || this.stale || !this.trimming) return null;
    if (!this.plan || !this.plan.cuts.some((c) => c.id === cutId)) return null;
    this.kept.add(cutId);
    const status = this.refresh();
    if (status) this.announce(`Kept. ${status.cardText}`);
    return status;
  }

  // ---------------------------------------------------------------------
  // Walk through
  // ---------------------------------------------------------------------

  startWalk() {
    if (this.active.length === 0) return;
    this.walkIndex = 0;
    this.walk.hidden = false;
    this.draw();
    this.renderWalk();
    this.revealCurrent();
    this.nextButton.focus({ preventScroll: true });
  }

  step(delta) {
    if (this.walkIndex < 0 || this.active.length === 0) return;
    const n = this.active.length;
    this.walkIndex = (this.walkIndex + delta + n) % n;
    this.draw();
    this.renderWalk();
    this.revealCurrent();
  }

  /** Keep the cut under the walk cursor (its outermost faded cut). */
  keepCurrent() {
    if (this.walkIndex < 0) return null;
    const piece = this.active[this.walkIndex];
    if (!piece) return null;
    const covering = this.active.filter((c) => c.start <= piece.start && piece.end <= c.end).map((c) => c.id);
    const status = this.keep(this.outermost(covering));
    if (this.active.length === 0) this.stopWalk(true);
    else {
      this.draw();
      this.renderWalk();
      this.revealCurrent();
      if (!this.card.contains(document.activeElement)) this.keepButton.focus({ preventScroll: true });
    }
    return status;
  }

  stopWalk(focus = false) {
    if (this.walkIndex < 0) return;
    this.walkIndex = -1;
    this.walk.hidden = true;
    this.draw();
    if (focus && this.cardOpen) this.walkButton.focus({ preventScroll: true });
  }

  renderWalk() {
    const c = this.active[this.walkIndex];
    if (!c) return;
    const text = this.surface.getText().slice(c.start, c.end).trim();
    const short = text.length > 60 ? `${text.slice(0, 57)}…` : text;
    this.walkText.textContent = `${this.walkIndex + 1} of ${this.active.length}: ${c.reason}, “${short}”`;
  }

  revealCurrent() {
    const c = this.active[this.walkIndex];
    if (!c) return;
    const el = this.surface.root.querySelector(`[data-te-decoration~="${TE_TRIM_LAYER}:${c.id}-${c.start}"]`);
    if (el && typeof el.scrollIntoView === 'function') el.scrollIntoView({ block: 'nearest' });
  }

  // ---------------------------------------------------------------------
  // Make the cuts, Done, Original
  // ---------------------------------------------------------------------

  /**
   * Delete every still-faded span as ONE undo step. Returns the applied
   * edits, or null when refused (stale plan, nothing to cut, no API).
   */
  makeCuts() {
    if (this.destroyed || this.stale || !this.trimming) return null;
    const api = this.api();
    if (!api) return null;
    let made;
    try {
      this.editor.alignDocumentModel(api);
      made = api.trim_make_cuts(this.level, JSON.stringify(Array.from(this.kept)));
    } catch (err) {
      // Refused: the body changed since the plan. Recompute, cut nothing.
      this.plan = null;
      this.refresh();
      this.announce('The text changed, so the trim was recomputed. Nothing was cut.');
      return null;
    }
    const edits = Array.from(made.edits || []);
    if (edits.length === 0) return null;
    const level = this.level;
    this.applying = true;
    let applied;
    try {
      applied = this.surface.replaceRanges(edits, { source: 'trim' });
    } finally {
      this.applying = false;
    }
    if (this.surface.getText() !== made.text) console.error('trim: surface text differs from the engine result');
    const words = this.status ? this.status.cardText : '';
    this.reset(false);
    this.announce(`Cuts made. ${words}`);
    this.surface.focus();
    this.emit('te:trim-cut', { level, edits: applied });
    return applied;
  }

  /** Close the card, leaving the faded preview as it is (see header). */
  done() {
    if (!this.cardOpen) return;
    this.stopWalk(false);
    const hadFocus = this.card.contains(document.activeElement);
    this.cardOpen = false;
    this.sync();
    const pill = this.lab.pill();
    if (hadFocus && pill) pill.focus();
  }

  /** Back to Original: no preview, no kept set, no card. */
  reset(focusPill = false) {
    if (this.destroyed) return;
    this.cancelPendingReset();
    const hadFocus = this.card.contains(document.activeElement);
    this.level = 'original';
    this.kept.clear();
    this.status = null;
    this.active = [];
    this.walkIndex = -1;
    this.walk.hidden = true;
    this.cardOpen = false;
    this.stale = false;
    if (this.surface && !this.surface.destroyed && this.registry.get(TE_TRIM_LAYER).length) {
      this.keepFocus(() => this.registry.clear(TE_TRIM_LAYER));
    }
    this.sync();
    if ((focusPill || hadFocus) && this.lab.pill()) this.lab.pill().focus();
  }

  onSurfaceChange(change) {
    if (this.destroyed || this.applying || !this.trimming) return;
    if (change && change.source === 'trim') return;
    // The plan no longer matches the text: stop at once, clear next tick.
    this.stale = true;
    this.cardOpen = false;
    this.card.hidden = true;
    if (this.resetTimer === null) {
      this.resetTimer = setTimeout(() => {
        this.resetTimer = null;
        if (!this.stale) return;
        this.reset(false);
        this.announce('Trim cleared: the text changed.');
      }, 0);
    }
  }

  cancelPendingReset() {
    if (this.resetTimer !== null) {
      clearTimeout(this.resetTimer);
      this.resetTimer = null;
    }
  }

  announce(text) {
    this.live.textContent = text;
  }

  emit(type, detail) {
    this.card.dispatchEvent(new CustomEvent(type, {
      bubbles: true,
      detail: Object.assign({ editor: this.editor }, detail),
    }));
  }

  /** Clear the preview, remove the DOM and every listener. */
  destroy() {
    if (this.destroyed) return;
    this.cancelPendingReset();
    if (this.surface && !this.surface.destroyed && this.registry) this.registry.clear(TE_TRIM_LAYER);
    if (this.surface && this.surface.root) delete this.surface.root.dataset.teTrimming;
    this.destroyed = true;
    this.offChange();
    this.abortController.abort();
    this.card.remove();
    this.live.remove();
    this.slot.replaceChildren();
    this.slot.hidden = true;
  }
}

window.TeLabTrim = TeLabTrim;
