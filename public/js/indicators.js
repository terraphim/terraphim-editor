/*
 * Decoration registry and inline indicators (issue #8, spec R-3.1 to R-3.6)
 * =========================================================================
 *
 * TeDecorationRegistry
 * --------------------
 * EditorSurface has a single decoration list (`setDecorations`), and every
 * call re-renders the surface DOM. Several features decorate the text (the
 * indicators here, ghosts and the selection menu in issue #11), so they do
 * not call `setDecorations` themselves. Each one owns a named *layer* in the
 * registry and replaces that layer's items; the registry merges every layer
 * into one `setDecorations` call:
 *
 *   const reg = editor.decorations;            // created by MarkdownEditor
 *   reg.set('ghosts', [{ id: 'g1', start, end, className: 'te-ghost' }]);
 *   reg.get('ghosts');                         // items as registered
 *   reg.current('ghosts');                     // live offsets (mapped through edits)
 *   reg.clear('ghosts');                       // same as set('ghosts', [])
 *   reg.batch(() => { reg.set('a', ...); reg.set('b', ...); });  // one render
 *   const off = reg.onApply(() => ...);        // after each real re-render
 *
 * Rules:
 * - Item ids are namespaced as `<layer>:<id>` on the surface, so layers
 *   cannot collide; an item without an id gets its index.
 * - Layers merge in registration order; overlapping items from different
 *   layers are fine (the surface splits them and unions their classes).
 * - Decorations the registry does not own (set directly on the surface, as
 *   older code and tests do) are preserved.
 * - Stored layer items follow every surface edit through the same
 *   `EditorSurface.mapRanges()` the surface uses (moved by edits before
 *   them, dropped by an edit inside them), so setting one layer never puts
 *   another back at stale offsets.
 * - If the merged list equals what the surface already has (same ids,
 *   offsets, classes and attributes) nothing is rendered. Because the surface
 *   moves decorations with edits, a layer that recomputes the same ranges
 *   after typing costs no DOM rebuild.
 *
 * TeIndicatorLayer
 * ----------------
 * Draws the "dots under text" for spans with alternatives, in Write_On mode
 * only (body[data-mode="write-on"]). Spans with only their original (R-2.7)
 * and set-aside spans get nothing.
 *
 * - Underline (R-3.1): CSS `text-decoration` on the decoration span classes
 *   `te-ind-word` and `te-ind-sentence` (public/css/write-on.css), so it
 *   moves with the text at no cost. Paragraph spans (`te-ind-paragraph`)
 *   have no underline (R-3.4). Headlines (a sentence span on a line starting
 *   with `#`) also get `te-ind-headline`: bold, slightly larger (R-3.5).
 * - Dots (R-3.2, R-3.3) and the paragraph gutter rule (R-3.4) are elements
 *   in an overlay (`.te-indicators`, aria-hidden, absolutely positioned and
 *   out of flow, so line height never changes) beside the surface. Positions
 *   come from the client rects of each span's *current* surface decoration,
 *   recomputed at most once per animation frame after a change, resize or
 *   scroll. Word rows are centred under the word; sentence and headline rows
 *   end at the right end of the underline; paragraphs get a rule in the left
 *   gutter and a dot column to its left. The first dot is the original;
 *   exactly one dot (the active alternative) is lit in the full accent.
 * - Model reads: the span list comes from `editor.annotations()` (the WASM
 *   document bridge). It re-syncs the body, so it runs only on a trailing
 *   debounce after typing (`delay`, default 120 ms, the preview's default),
 *   never per keystroke. `flush()` runs a pending refresh and layout now.
 * - Accessibility: the 40% underline and dim dots are below 3:1 contrast, so
 *   the active dot uses the full accent and every indicated span carries
 *   `aria-describedby` pointing at a visually hidden description such as
 *   "Word: 7 alternatives, 5 of 7 active".
 * - Dot clicks (R-3.6, where jumping to an alternative is only inferred)
 *   dispatch a bubbling, cancelable `te:dot` CustomEvent with detail
 *   { editor, spanId, index, kind, active }. Unless a listener calls
 *   preventDefault(), the default action jumps to that alternative through
 *   `editor.swapAlternative` (issue #9).
 * - In-place cycling (issue #9, R-2.4, R-3.6): while the pointer is over an
 *   indicated span's text, ArrowUp/ArrowDown (no modifiers) make the
 *   previous/next alternative active, wrapping at either end. With the caret
 *   (or the whole selection) inside or at the edge of an indicated span,
 *   Alt+ArrowUp/Alt+ArrowDown do the same for keyboard users. Any other
 *   arrow key, or an arrow with nothing hovered, is left to the browser.
 *   Each swap is one undo step; see docs/design/cycling.md.
 *
 * Load order: after chrome.js and before editor.js, which instantiates both
 * classes in MarkdownEditor.initialize() when they exist. Full design notes:
 * docs/design/indicators.md
 */

const TE_LAYER_NAME = /^[a-z][a-z0-9-]*$/;

class TeDecorationRegistry {
  constructor(surface) {
    this.surface = surface;
    // Layer name -> items as registered (unprefixed). A cleared layer stays
    // registered so its prefix is still recognised as owned.
    this.layers = new Map();
    this.batchDepth = 0;
    this.pending = false;
    this.applyListeners = new Set();
    // Number of real setDecorations calls made, for tests and diagnostics.
    this.renderCount = 0;
    // Keep stored items in step with the surface: every surface edit is
    // reported with the edit that moved its decorations.
    this.offChange = surface.onChange((change) => this.mapEdit(change && change.edit));
  }

  /** Move or drop stored items exactly as the surface moves its decorations. */
  mapEdit(edit) {
    if (!edit) return;
    for (const [name, items] of this.layers) {
      if (items.length) this.layers.set(name, EditorSurface.mapRanges(items, edit));
    }
  }

  /** Stop following surface edits. */
  destroy() {
    this.offChange();
  }

  /** Replace the items of layer `name`. Returns true if the surface re-rendered. */
  set(name, items) {
    if (typeof name !== 'string' || !TE_LAYER_NAME.test(name)) {
      throw new Error(`Invalid decoration layer name: ${name}`);
    }
    this.layers.set(name, (items || []).map((item, i) => ({
      id: item.id === undefined || item.id === null ? String(i) : String(item.id),
      start: item.start,
      end: item.end,
      className: item.className || '',
      data: item.data,
      // Sanitised as the surface will (warning once, here), so comparisons
      // with the surface's decorations are like for like.
      attributes: EditorSurface.sanitiseAttributes(item.attributes),
    })));
    return this.apply();
  }

  clear(name) {
    return this.set(name, []);
  }

  /** Items of layer `name`, with offsets moved through later edits. */
  get(name) {
    return (this.layers.get(name) || []).map((d) => ({ ...d }));
  }

  /** Registered layer names, in merge order. */
  names() {
    return Array.from(this.layers.keys());
  }

  /** Is this surface decoration id owned by a registered layer? */
  owns(id) {
    const sep = String(id).indexOf(':');
    return sep > 0 && this.layers.has(String(id).slice(0, sep));
  }

  /**
   * The surface decorations of layer `name` with their live offsets (moved
   * through edits since the last render; an edit inside one drops it) and
   * unprefixed ids.
   */
  current(name) {
    const prefix = `${name}:`;
    return this.surface.getDecorations()
      .filter((d) => d.id.startsWith(prefix))
      .map((d) => ({ ...d, id: d.id.slice(prefix.length) }));
  }

  /** Run `fn` and render once at the end, however many layers it sets. */
  batch(fn) {
    this.batchDepth += 1;
    try {
      fn();
    } finally {
      this.batchDepth -= 1;
    }
    if (this.batchDepth === 0 && this.pending) return this.apply();
    return false;
  }

  /** Subscribe to real re-renders; returns an unsubscribe function. */
  onApply(callback) {
    this.applyListeners.add(callback);
    return () => this.applyListeners.delete(callback);
  }

  /** The merged list: unowned surface decorations, then every layer. */
  merged() {
    const out = this.surface.getDecorations().filter((d) => !this.owns(d.id));
    for (const [name, items] of this.layers) {
      for (const item of items) out.push({ ...item, id: `${name}:${item.id}` });
    }
    return out;
  }

  static signature(d) {
    return `${d.start}|${d.end}|${d.className || ''}|${d.attributes ? JSON.stringify(d.attributes) : ''}`;
  }

  /** Push the merged list to the surface unless it is already there. */
  apply() {
    if (this.batchDepth > 0) {
      this.pending = true;
      return false;
    }
    this.pending = false;
    const surface = this.surface;
    if (!surface || surface.destroyed) return false;
    const len = surface.getText().length;
    // Compare after the same clamping the surface applies.
    const next = this.merged().filter((d) => {
      const s = Math.max(0, Math.min(len, d.start | 0));
      const e = Math.max(0, Math.min(len, d.end | 0));
      return e > s;
    });
    const have = new Map(surface.getDecorations().map((d) => [d.id, TeDecorationRegistry.signature(d)]));
    const same = have.size === next.length
      && next.every((d) => have.get(d.id) === TeDecorationRegistry.signature(d));
    if (same) return false;
    surface.setDecorations(next);
    this.renderCount += 1;
    for (const listener of this.applyListeners) {
      try {
        listener();
      } catch (err) {
        console.error('Decoration apply listener failed', err);
      }
    }
    return true;
  }
}

let teIndicatorInstances = 0;

class TeIndicatorLayer {
  constructor(editor, options = {}) {
    this.editor = editor;
    this.surface = editor.surface;
    this.registry = editor.decorations;
    this.delay = options.delay === undefined ? 120 : options.delay;
    this.instance = ++teIndicatorInstances;
    this.active = false;
    this.destroyed = false;
    // Span id -> { id, kind, headline, count, activeIndex, key, el, desc }.
    this.spans = new Map();
    this.timer = null;
    this.frame = 0;
    // Diagnostics for tests: model refreshes and layout passes run.
    this.refreshCount = 0;
    this.layoutCount = 0;

    this.abortController = new AbortController();
    const signal = this.abortController.signal;
    if (options.signal) {
      if (options.signal.aborted) this.abortController.abort();
      else options.signal.addEventListener('abort', () => this.destroy(), { signal });
    }

    // Span id under the pointer (issue #9), with the pointer's last client
    // coordinates. A plain arrow re-checks that the pointer is still over
    // that span's rendered text before taking the key, so a hover left
    // stale by typing, caret moves, scrolling or re-layout never swallows
    // caret movement. `hoverHeld` keeps the hover across the re-render of a
    // swap of that same span (the new text may no longer be under the
    // pointer), until the pointer moves, the layout changes or any other
    // change happens.
    this.hoverSpan = null;
    this.pointer = null;
    this.hoverHeld = false;
    this.surface.root.addEventListener('mouseover', (e) => this.onPointer(e), { signal });
    this.surface.root.addEventListener('mousemove', (e) => this.onPointer(e), { signal, passive: true });
    this.surface.root.addEventListener('mouseleave', () => this.clearHover(), { signal });
    window.addEventListener('blur', () => this.clearHover(), { signal });
    this.surface.root.addEventListener('keydown', (e) => this.onCycleKey(e), { signal });

    this.offChange = this.surface.onChange((change) => this.onSurfaceChange(change));
    this.offApply = this.registry.onApply(() => this.scheduleLayout());
    document.addEventListener('te:mode-change', (e) => {
      if (!e.detail || !e.detail.editor || e.detail.editor === this.editor) {
        this.setActive(e.detail && e.detail.mode === 'write-on');
      }
    }, { signal });
    // Layout-only changes (resize, container reflow, late fonts) can move
    // text under a still pointer, so they end a post-swap hold too.
    const relayout = () => {
      this.hoverHeld = false;
      this.scheduleLayout();
    };
    window.addEventListener('resize', relayout, { signal });
    this.surface.root.addEventListener('scroll', () => {
      this.hoverHeld = false;
      this.scheduleLayout();
    }, { signal, passive: true });
    // Observed only while active, so plain mode pays nothing per keystroke.
    if (typeof ResizeObserver === 'function') {
      this.resizeObserver = new ResizeObserver(relayout);
    }
    if (document.fonts && document.fonts.ready) {
      document.fonts.ready.then(relayout).catch(() => {});
    }

    this.setActive(this.isWriteOn());
  }

  static get LAYER() {
    return 'indicators';
  }

  isWriteOn() {
    const chrome = this.editor.chrome;
    if (chrome && typeof chrome.isWriteOn === 'function') return chrome.isWriteOn();
    return document.body.dataset.mode === 'write-on';
  }

  // ---------------------------------------------------------------------
  // Activation
  // ---------------------------------------------------------------------

  setActive(on) {
    if (this.destroyed) return;
    on = !!on;
    if (on === this.active) {
      if (on) this.flush();
      return;
    }
    this.active = on;
    if (on) {
      this.mountOverlay();
      this.flush();
    } else {
      this.cancelPending();
      this.clearHover();
      this.registry.clear(TeIndicatorLayer.LAYER);
      this.removeAllSpans();
      this.unmountOverlay();
    }
  }

  mountOverlay() {
    const host = this.surface.root.parentElement;
    if (!host) return;
    this.host = host;
    host.classList.add('te-indicator-host');
    if (!this.overlay) {
      const overlay = document.createElement('div');
      overlay.className = 'te-indicators';
      overlay.setAttribute('aria-hidden', 'true');
      overlay.addEventListener('mousedown', (e) => {
        // Keep the caret and focus in the surface when a dot is pressed.
        if (e.target.closest('.te-ind-dot')) e.preventDefault();
      }, { signal: this.abortController.signal });
      overlay.addEventListener('click', (e) => this.onDotClick(e), { signal: this.abortController.signal });
      this.overlay = overlay;
      const descriptions = document.createElement('div');
      descriptions.className = 'te-ind-descriptions';
      this.descriptions = descriptions;
    }
    host.appendChild(this.overlay);
    host.appendChild(this.descriptions);
    if (this.resizeObserver) this.resizeObserver.observe(this.surface.root);
  }

  unmountOverlay() {
    if (this.resizeObserver) this.resizeObserver.disconnect();
    if (this.overlay) this.overlay.remove();
    if (this.descriptions) this.descriptions.remove();
    if (this.host) this.host.classList.remove('te-indicator-host');
  }

  cancelPending() {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    if (this.frame) {
      cancelAnimationFrame(this.frame);
      this.frame = 0;
    }
  }

  /** Remove the overlay, the layer's decorations and every listener. */
  destroy() {
    if (this.destroyed) return;
    this.cancelPending();
    if (this.active && this.surface && !this.surface.destroyed) {
      this.registry.clear(TeIndicatorLayer.LAYER);
    }
    this.destroyed = true;
    this.active = false;
    this.offChange();
    this.offApply();
    if (this.resizeObserver) this.resizeObserver.disconnect();
    this.abortController.abort();
    this.removeAllSpans();
    this.unmountOverlay();
  }

  // ---------------------------------------------------------------------
  // Scheduling
  // ---------------------------------------------------------------------

  onSurfaceChange(change) {
    if (!this.active) return;
    // Only a swap of the hovered span itself keeps the hover held; after any
    // other change the next plain arrow re-checks the pointer (issue #9).
    const swap = change && change.edit && change.edit.swap;
    this.hoverHeld = !!swap && this.hoverSpan !== null && String(swap.span) === this.hoverSpan;
    // Geometry follows the mapped decorations at once (next frame); the model
    // read waits for a pause in typing.
    this.scheduleLayout();
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      this.refresh();
    }, this.delay);
  }

  scheduleLayout() {
    if (!this.active || this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.layout();
    });
  }

  /** Run any pending refresh and lay out now. */
  flush() {
    if (!this.active) return;
    this.cancelPending();
    this.refresh();
  }

  /** Is a debounced model refresh waiting? */
  pending() {
    return this.timer !== null;
  }

  // ---------------------------------------------------------------------
  // Model
  // ---------------------------------------------------------------------

  /** Live spans with at least one alternative besides the original. */
  readSpans() {
    const editor = this.editor;
    if (typeof editor.documentApi !== 'function' || !editor.documentApi()) return [];
    try {
      const ann = editor.annotations();
      return (ann && Array.isArray(ann.spans) ? ann.spans : [])
        .filter((s) => s && s.anchor && Array.isArray(s.alts) && s.alts.length > 1);
    } catch (err) {
      return [];
    }
  }

  static isHeadline(text, start) {
    const lineStart = text.lastIndexOf('\n', start - 1) + 1;
    return /^ {0,3}#{1,6}[ \t]/.test(text.slice(lineStart, start + 1));
  }

  static describe(kind, count, activeIndex) {
    const label = kind.charAt(0).toUpperCase() + kind.slice(1);
    return `${label}: ${count} alternatives, ${activeIndex + 1} of ${count} active`;
  }

  /** Read the model, update the layer's decorations and indicator elements. */
  refresh() {
    if (!this.active || this.destroyed) return;
    this.refreshCount += 1;
    const text = this.surface.getText();
    const items = [];
    const seen = new Set();
    for (const span of this.readSpans()) {
      const kind = span.kind === 'paragraph' || span.kind === 'sentence' ? span.kind : 'word';
      const headline = kind === 'sentence' && TeIndicatorLayer.isHeadline(text, span.anchor.start);
      const count = span.alts.length;
      const activeIndex = Math.max(0, Math.min(count - 1, span.active | 0));
      const id = String(span.id);
      seen.add(id);
      const descId = `te-ind-desc-${this.instance}-${id}`;
      const key = `${kind}|${headline}|${count}|${activeIndex}`;
      let entry = this.spans.get(id);
      if (!entry || entry.key !== key) {
        if (entry) this.removeSpanElements(entry);
        entry = this.buildSpan(id, kind, headline, count, activeIndex, descId, key);
        this.spans.set(id, entry);
      }
      const classes = ['te-ind', `te-ind-${kind}`];
      if (headline) classes.push('te-ind-headline');
      items.push({
        id,
        start: span.anchor.start,
        end: span.anchor.end,
        className: classes.join(' '),
        attributes: { 'aria-describedby': descId },
      });
    }
    for (const [id, entry] of this.spans) {
      if (!seen.has(id)) {
        this.removeSpanElements(entry);
        this.spans.delete(id);
      }
    }
    this.registry.set(TeIndicatorLayer.LAYER, items);
    // New or rebuilt indicators start hidden; place them now even when the
    // decorations were unchanged and the surface did not re-render.
    this.layout();
  }

  buildSpan(id, kind, headline, count, activeIndex, descId, key) {
    const el = document.createElement('div');
    const dots = document.createElement('div');
    dots.className = kind === 'paragraph' ? 'te-ind-col' : 'te-ind-dots';
    for (let i = 0; i < count; i += 1) {
      const dot = document.createElement('span');
      dot.className = i === activeIndex ? 'te-ind-dot te-ind-dot--active' : 'te-ind-dot';
      dot.dataset.index = String(i);
      dots.appendChild(dot);
    }
    if (kind === 'paragraph') {
      el.className = 'te-ind-para';
      const rule = document.createElement('div');
      rule.className = 'te-ind-rule';
      el.appendChild(rule);
    } else {
      el.className = kind === 'word' ? 'te-ind-row te-ind-row--centre' : 'te-ind-row te-ind-row--end';
    }
    el.appendChild(dots);
    el.dataset.spanId = id;
    el.dataset.kind = headline ? 'headline' : kind;
    el.hidden = true;
    this.overlay.appendChild(el);

    const desc = document.createElement('span');
    desc.id = descId;
    desc.textContent = TeIndicatorLayer.describe(headline ? 'headline' : kind, count, activeIndex);
    this.descriptions.appendChild(desc);
    return { id, kind, headline, count, activeIndex, key, el, desc };
  }

  removeSpanElements(entry) {
    entry.el.remove();
    entry.desc.remove();
  }

  removeAllSpans() {
    for (const entry of this.spans.values()) this.removeSpanElements(entry);
    this.spans.clear();
  }

  // ---------------------------------------------------------------------
  // Geometry
  // ---------------------------------------------------------------------

  /** Client rects of the text in [start, end), without empty fragments. */
  rects(start, end) {
    const range = this.surface.rangeForOffsets(start, end);
    return Array.from(range.getClientRects()).filter((r) => r.width > 0 && r.height > 0);
  }

  /** Position every indicator from the live decoration offsets. */
  layout() {
    if (!this.active || this.destroyed || !this.host || this.spans.size === 0) return;
    this.layoutCount += 1;
    const live = new Map(this.registry.current(TeIndicatorLayer.LAYER).map((d) => [d.id, d]));
    const hostRect = this.host.getBoundingClientRect();
    const originX = hostRect.left + this.host.clientLeft - this.host.scrollLeft;
    const originY = hostRect.top + this.host.clientTop - this.host.scrollTop;
    const root = this.surface.root;
    const rootRect = root.getBoundingClientRect();
    const contentLeft = rootRect.left + root.clientLeft + (parseFloat(getComputedStyle(root).paddingLeft) || 0);
    for (const entry of this.spans.values()) {
      const deco = live.get(entry.id);
      const rects = deco ? this.rects(deco.start, deco.end) : [];
      if (rects.length === 0) {
        entry.el.hidden = true;
        continue;
      }
      entry.el.hidden = false;
      const first = rects[0];
      const last = rects[rects.length - 1];
      let x;
      let y;
      if (entry.kind === 'paragraph') {
        x = contentLeft;
        y = first.top;
        entry.el.style.height = `${Math.max(0, last.bottom - first.top)}px`;
      } else if (entry.kind === 'word') {
        const left = Math.min(...rects.map((r) => r.left));
        const right = Math.max(...rects.map((r) => r.right));
        x = (left + right) / 2;
        y = Math.max(...rects.map((r) => r.bottom));
      } else {
        x = last.right;
        y = last.bottom;
      }
      entry.el.style.left = `${x - originX}px`;
      entry.el.style.top = `${y - originY}px`;
    }
  }

  onDotClick(e) {
    const dot = e.target.closest('.te-ind-dot');
    if (!dot || !this.overlay.contains(dot)) return;
    const holder = dot.closest('[data-span-id]');
    const entry = holder && this.spans.get(holder.dataset.spanId);
    if (!entry) return;
    const index = Number(dot.dataset.index);
    const active = index === entry.activeIndex;
    const event = new CustomEvent('te:dot', {
      bubbles: true,
      cancelable: true,
      detail: {
        editor: this.editor,
        spanId: entry.id,
        index,
        kind: entry.headline ? 'headline' : entry.kind,
        active,
      },
    });
    this.overlay.dispatchEvent(event);
    // Default action (issue #9): jump to that alternative.
    if (!event.defaultPrevented && !active) this.jump(entry.id, index);
  }

  // ---------------------------------------------------------------------
  // In-place cycling (issue #9)
  // ---------------------------------------------------------------------

  /** Track the pointer and the indicated span under it (its text, not its dots). */
  onPointer(e) {
    if (!this.active) return;
    this.pointer = { x: e.clientX, y: e.clientY };
    this.hoverHeld = false;
    this.hoverSpan = this.spanOfElement(e.target);
  }

  clearHover() {
    this.hoverSpan = null;
    this.pointer = null;
    this.hoverHeld = false;
  }

  /**
   * The indicated span whose rendered text holds `el`, or null. Walks out
   * through nested decorations (a Lab mark inside an indicated span, say).
   */
  spanOfElement(el) {
    const prefix = `${TeIndicatorLayer.LAYER}:`;
    const root = this.surface.root;
    let node = el && el.closest ? el.closest('[data-te-decoration]') : null;
    while (node && root.contains(node) && node !== root) {
      const token = (node.getAttribute('data-te-decoration') || '').split(' ').find((t) => t.startsWith(prefix));
      if (token) return token.slice(prefix.length);
      node = node.parentElement ? node.parentElement.closest('[data-te-decoration]') : null;
    }
    return null;
  }

  /** The indicated span rendered under the last pointer position, or null. */
  spanAtPointer() {
    if (!this.pointer || typeof document.elementsFromPoint !== 'function') return null;
    const root = this.surface.root;
    for (const el of document.elementsFromPoint(this.pointer.x, this.pointer.y)) {
      if (el === root || !root.contains(el)) continue;
      const id = this.spanOfElement(el);
      if (id !== null) return id;
    }
    return null;
  }

  /**
   * The hovered span id, if it is still indicated and the pointer is still
   * over its rendered text (or the hover is held across a swap of it).
   * A hover that fails the check is dropped.
   */
  hoveredSpan() {
    const id = this.hoverSpan;
    if (id === null || !this.spans.has(id)) return null;
    if (this.hoverHeld || this.spanAtPointer() === id) return id;
    this.hoverSpan = null;
    this.hoverHeld = false;
    return null;
  }

  /**
   * The indicated span holding the whole selection (a caret at either edge
   * of the span counts as inside), or null.
   */
  spanAtSelection() {
    const sel = this.surface.getSelectionOffsets();
    const live = this.registry.current(TeIndicatorLayer.LAYER);
    const hit = live.find((d) => d.start <= sel.start && sel.end <= d.end);
    return hit && this.spans.has(hit.id) ? hit.id : null;
  }

  commandMenuOpen() {
    const menu = this.editor.commandMenu;
    return !!menu && menu.isConnected && menu.style.display !== 'none';
  }

  onCycleKey(e) {
    if (!this.active || this.destroyed || e.isComposing) return;
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
    if (e.ctrlKey || e.metaKey || e.shiftKey || this.commandMenuOpen()) return;
    // Bring the span list up to date before choosing (a refresh may still
    // be waiting after typing, undo or redo).
    if (this.pending()) this.flush();
    const id = e.altKey ? this.spanAtSelection() : this.hoveredSpan();
    if (id === null) return;
    e.preventDefault();
    this.cycle(id, e.key === 'ArrowDown' ? 1 : -1);
  }

  /**
   * Make the alternative `step` places from the active one active, wrapping
   * at either end (ArrowDown from the last returns to the original).
   * Returns the model outcome, or null if nothing changed.
   */
  cycle(spanId, step) {
    if (this.pending()) this.flush();
    const entry = this.spans.get(String(spanId));
    if (!entry || entry.count < 2) return null;
    const next = (((entry.activeIndex + step) % entry.count) + entry.count) % entry.count;
    return this.jump(entry.id, next);
  }

  /** Make alternative `index` of `spanId` active. A refused swap changes nothing. */
  jump(spanId, index) {
    if (typeof this.editor.swapAlternative !== 'function') return null;
    try {
      return this.editor.swapAlternative(spanId, index);
    } catch (err) {
      console.warn('Could not swap the alternative', err);
      return null;
    }
  }
}

window.TeDecorationRegistry = TeDecorationRegistry;
window.TeIndicatorLayer = TeIndicatorLayer;
