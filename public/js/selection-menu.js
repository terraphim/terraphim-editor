/*
 * Ghost layer and selection context menu (issue #11; spec R-5.1 to R-5.3,
 * R-7.3, R-7.4)
 * ========================================================================
 *
 * TeGhostLayer (`editor.ghosts`)
 * ------------------------------
 * Ghosting is an independent layer of body ranges kept by the WASM document
 * model (decision 2026-10-04): a ghost may cover or partly overlap spans
 * with alternatives, touching or overlapping ghosts merge, reviving part of
 * a ghost trims or splits it, and ghosts follow edits (they are elastic).
 * The model does all of that; this class only asks it and draws the result.
 *
 *   editor.ghosts.ghost(start, end)    // -> { ok, id, annotations } | { ok: false, error, kind }
 *   editor.ghosts.revive(start, end)   // -> { ok, changed, annotations } | { ok: false, ... }
 *   editor.ghosts.toggle()             // Ctrl+/ on the current selection
 *   editor.ghosts.targetFor({ start, end })  // -> { action: 'ghost' | 'revive', start, end } | null
 *   editor.ghosts.flush()              // read the model and redraw now
 *
 * - Offsets are UTF-16 code units, as everywhere on the surface.
 * - Drawing: the `ghosts` layer of the decoration registry (indicators.js),
 *   items with the class `te-ghost` and no attributes, so the indicators'
 *   `aria-describedby` on shared text is never overwritten. The fade
 *   (public/css/selection-menu.css) uses `--te-ghost-opacity` on the text
 *   colour, so the selection highlight and indicator underlines on ghosted
 *   text stay visible: ghosted text is still readable, selectable, editable
 *   and counted (R-5.1, decision 3). Export drops it (R-9.3).
 * - Refresh: after `ghost`/`revive` the layer is redrawn at once from the
 *   annotations the call returns (no second model read). Typing elsewhere
 *   needs no model read: the registry moves the decorations with the text,
 *   and a trailing debounce (`delay`, default 120 ms like the indicators)
 *   confirms against the model. An edit inside a ghost drops its surface
 *   decoration (EditorSurface.mapRanges) while the model resizes the ghost,
 *   so that case is redrawn on the next animation frame instead. When the
 *   model has no ghosts at all (live or set aside) no edit can create one,
 *   so typing reads nothing.
 * - Undo: ghosts are annotations, not text, so ghost and revive are not
 *   entries in the surface's text undo history. Ctrl+/ (or the menu) is the
 *   reversal. Text undo/redo still moves ghosts, because the model mirrors
 *   every replayed edit.
 * - Every change dispatches a bubbling `te:ghosts-change` CustomEvent from
 *   the surface with detail { editor, action, start, end, id, changed,
 *   ghosts }, so whoever owns storage can mark the document as changed.
 *
 * TeSelectionMenu (`editor.selectionMenu`)
 * ----------------------------------------
 * The R-7.3 context menu: a dark rounded panel (tokens `--te-color-popover`,
 * `--te-radius-popover`, `--te-font-mono`, `--te-color-text-dim`) with the
 * label on the left and the dim shortcut on the right. Available in plain
 * and Write_On mode alike.
 *
 * - Opens on `contextmenu` over a non-empty selection in the surface, and on
 *   the Menu key or Shift+F10 (positioned under the selection). Right-click
 *   or Ctrl+/ with a collapsed caret inside a ghost offers Revive for the
 *   whole ghost (R-5.2: "right-click ghosted text"). With nothing to offer
 *   the native menu is left alone.
 * - Items are pluggable and appear in the R-7.3 order (`SLOTS`). An item
 *   that is not implemented, or not available for the selection, is not
 *   rendered at all (hidden, never disabled). Only Ghost it / Revive is
 *   built in; #10, #13 and #12 register theirs:
 *
 *     editor.selectionMenu.register({
 *       id: 'alternatives',              // slot from SLOTS (or any new id)
 *       label: 'Alternatives for selection',   // or (ctx) => string
 *       key: 'ctrl+shift+a',             // shortcut, also handled on the surface
 *       available: (ctx) => true,        // ctx: { editor, start, end, collapsed, text }
 *       run: (ctx) => { ... },
 *     });
 *
 * - Keyboard: ArrowUp/ArrowDown (wrapping), Home/End, Enter/Space run the
 *   item, Escape or Tab close and return focus and the selection to the
 *   surface. ARIA `menu`/`menuitem`, `aria-keyshortcuts` on each item.
 * - Closes on a click outside, any scroll, a resize or focus leaving it.
 *   Listeners use the editor's AbortController signal (plus one per open
 *   menu, aborted on close); `destroy()` removes the menu and all of them.
 *
 * Shortcuts (R-7.4): `ctrl+/` matches Ctrl (not Cmd) with `/` (by `key`, or
 * by `code` "Slash"; Shift is ignored for punctuation so layouts where `/`
 * needs Shift work). Letter shortcuts need the exact Shift state. config.js binds
 * ctrl+b/i/k/l/h; MarkdownEditor matches those as `ctrl+<key>` ignoring
 * Shift, so a future ctrl+a/g/x entry there would also fire on the R-7.3
 * Ctrl+Shift item. The command palette ignores `/` with Ctrl, and the
 * surface only takes Ctrl/Cmd+Z/Y. See docs/design/ghosts-and-menu.md.
 *
 * Load order: after indicators.js (the registry) and before editor.js,
 * which instantiates both classes in MarkdownEditor.initialize().
 */

class TeGhostLayer {
  constructor(editor, options = {}) {
    this.editor = editor;
    this.surface = editor.surface;
    this.registry = editor.decorations;
    this.delay = options.delay === undefined ? 120 : options.delay;
    this.destroyed = false;
    this.timer = null;
    this.frame = 0;
    // Live ghosts as last read: [{ id, start, end }].
    this.ghosts = [];
    // Live plus set-aside ghosts in the model at the last read; while it is
    // zero, edits cannot produce a ghost and nothing is read.
    this.modelGhosts = 0;
    // Items in the registry layer after the last refresh or edit.
    this.count = 0;
    // Diagnostics for tests: model refreshes run.
    this.refreshCount = 0;

    this.abortController = new AbortController();
    if (options.signal) {
      if (options.signal.aborted) this.abortController.abort();
      else options.signal.addEventListener('abort', () => this.destroy(), { signal: this.abortController.signal });
    }
    this.offChange = this.surface.onChange(() => this.onSurfaceChange());
    this.flush();
  }

  static get LAYER() {
    return 'ghosts';
  }

  /** The WASM document API, if it offers ghosting. */
  api() {
    const editor = this.editor;
    const api = typeof editor.documentApi === 'function' ? editor.documentApi() : null;
    return api && typeof api.ghost_range === 'function' && typeof api.revive_range === 'function'
      ? api
      : null;
  }

  available() {
    return !this.destroyed && !!this.api();
  }

  // ---------------------------------------------------------------------
  // Scheduling
  // ---------------------------------------------------------------------

  onSurfaceChange() {
    if (this.destroyed) return;
    const now = this.registry.get(TeGhostLayer.LAYER).length;
    const dropped = now < this.count;
    this.count = now;
    if (dropped) {
      // The edit landed inside a ghost: the model resized it, the surface
      // dropped it. Redraw before the next paint.
      if (this.timer !== null) {
        clearTimeout(this.timer);
        this.timer = null;
      }
      if (!this.frame) {
        this.frame = requestAnimationFrame(() => {
          this.frame = 0;
          this.refresh();
        });
      }
    } else if (this.modelGhosts > 0 && !this.frame) {
      if (this.timer !== null) clearTimeout(this.timer);
      this.timer = setTimeout(() => {
        this.timer = null;
        this.refresh();
      }, this.delay);
    }
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

  /** Is a debounced or per-frame refresh waiting? */
  pending() {
    return this.timer !== null || this.frame !== 0;
  }

  /** Read the model and redraw now. */
  flush() {
    this.refresh();
  }

  // ---------------------------------------------------------------------
  // Model
  // ---------------------------------------------------------------------

  /** Live ghosts from the model (body aligned first), as [{ id, start, end }]. */
  read() {
    this.refresh();
    return this.ghosts.map((g) => ({ ...g }));
  }

  /**
   * Redraw from `annotations` (as returned by ghost_range/revive_range or
   * editor.annotations()); reads the model when none are given.
   */
  refresh(annotations) {
    if (this.destroyed || !this.surface || this.surface.destroyed) return;
    this.cancelPending();
    let ann = annotations;
    if (!ann && this.api()) {
      try {
        ann = this.editor.annotations();
      } catch (err) {
        ann = null;
      }
    }
    if (!ann && !this.api()) return;
    this.refreshCount += 1;
    const live = ann && Array.isArray(ann.ghosts) ? ann.ghosts : [];
    const setAside = ann && ann.setAside && Array.isArray(ann.setAside.ghosts) ? ann.setAside.ghosts.length : 0;
    this.ghosts = live
      .filter((g) => g && g.anchor)
      .map((g) => ({ id: String(g.id), start: g.anchor.start, end: g.anchor.end }));
    this.modelGhosts = this.ghosts.length + setAside;
    this.registry.set(TeGhostLayer.LAYER, this.ghosts.map((g) => ({
      id: g.id,
      start: g.start,
      end: g.end,
      className: 'te-ghost',
    })));
    this.count = this.registry.get(TeGhostLayer.LAYER).length;
  }

  /** The live ghost containing [start, end) (inclusive ends), if any. */
  containing(start, end, ghosts = this.ghosts) {
    return ghosts.find((g) => g.start <= start && end <= g.end) || null;
  }

  /**
   * What Ctrl+/ does for `sel` ({ start, end }), read from the model:
   * - a selection lying entirely inside one ghost is revived (touching
   *   ghosts merge, so "entirely ghosted" always means one ghost);
   * - any other non-empty selection is ghosted (merging with neighbours);
   * - a collapsed caret inside a ghost revives that whole ghost;
   * - otherwise null (nothing to do).
   */
  targetFor(sel) {
    if (!this.available() || !sel) return null;
    const start = Math.min(sel.start, sel.end);
    const end = Math.max(sel.start, sel.end);
    const ghosts = this.read();
    if (start === end) {
      const g = this.containing(start, end, ghosts);
      return g ? { action: 'revive', start: g.start, end: g.end, whole: true } : null;
    }
    return this.containing(start, end, ghosts)
      ? { action: 'revive', start, end }
      : { action: 'ghost', start, end };
  }

  ghost(start, end) {
    return this.change('ghost', start, end);
  }

  revive(start, end) {
    return this.change('revive', start, end);
  }

  /** Run a target from targetFor(). */
  apply(target) {
    if (!target) return { ok: false, error: 'Nothing to ghost or revive', kind: 'empty' };
    return this.change(target.action, target.start, target.end);
  }

  /** Ctrl+/: ghost or revive the current selection (see targetFor()). */
  toggle(sel = this.surface.getSelectionOffsets()) {
    return this.apply(this.targetFor(sel));
  }

  change(action, start, end) {
    const api = this.destroyed ? null : this.api();
    if (!api) return { ok: false, error: 'The WASM document API does not offer ghosting', kind: 'unavailable' };
    const s = Math.max(0, Math.min(start, end) | 0);
    const e = Math.max(0, Math.max(start, end) | 0);
    // The model must hold exactly the surface text before ranges are used.
    this.editor.alignDocumentModel(api);
    const result = action === 'ghost' ? api.ghost_range(s, e) : api.revive_range(s, e);
    if (!result || !result.ok) {
      console.warn(`Could not ${action} ${s}..${e}:`, result && result.error);
      return result || { ok: false, error: 'No result', kind: 'other' };
    }
    this.refresh(result.annotations);
    this.surface.root.dispatchEvent(new CustomEvent('te:ghosts-change', {
      bubbles: true,
      detail: {
        editor: this.editor,
        action,
        start: s,
        end: e,
        id: result.id === undefined ? null : result.id,
        changed: result.changed === undefined ? true : !!result.changed,
        ghosts: this.ghosts.map((g) => ({ ...g })),
      },
    }));
    return result;
  }

  /** Remove the layer's decorations and stop listening. */
  destroy() {
    if (this.destroyed) return;
    this.cancelPending();
    if (this.surface && !this.surface.destroyed) this.registry.clear(TeGhostLayer.LAYER);
    this.destroyed = true;
    this.offChange();
    this.abortController.abort();
  }
}

/** Display text and aria-keyshortcuts value for a shortcut such as 'ctrl+shift+a'. */
function teShortcutLabels(key) {
  const parts = String(key).split('+').filter(Boolean);
  const name = { ctrl: 'Ctrl', shift: 'Shift', alt: 'Alt', meta: 'Cmd' };
  const aria = { ctrl: 'Control', shift: 'Shift', alt: 'Alt', meta: 'Meta' };
  const last = parts[parts.length - 1] || '';
  const keyText = last.length === 1 ? last.toUpperCase() : last;
  return {
    text: parts.slice(0, -1).map((p) => name[p] || p).concat(keyText).join('+'),
    aria: parts.slice(0, -1).map((p) => aria[p] || p).concat(keyText).join('+'),
  };
}

/** Does keydown `e` match a shortcut such as 'ctrl+/' or 'ctrl+shift+a'? */
function teShortcutMatches(e, key) {
  const parts = String(key).toLowerCase().split('+').filter(Boolean);
  const want = parts[parts.length - 1];
  if (!want || e.isComposing) return false;
  const mods = new Set(parts.slice(0, -1));
  // Exactly the modifiers named: Ctrl means Ctrl on every platform (R-7.3);
  // Cmd is not an alias (Safari binds Cmd+/, macOS browsers Cmd+Shift+A/G).
  if (mods.has('ctrl') !== e.ctrlKey) return false;
  if (mods.has('meta') !== e.metaKey) return false;
  if (mods.has('alt') !== e.altKey) return false;
  const pressed = (e.key || '').toLowerCase();
  if (/^[a-z0-9]$/.test(want)) {
    if (mods.has('shift') !== e.shiftKey) return false;
    return pressed === want || e.code === `Key${want.toUpperCase()}` || e.code === `Digit${want}`;
  }
  // Punctuation: Shift may be needed to type it on some layouts.
  const codes = { '/': 'Slash', '.': 'Period', ',': 'Comma', ';': 'Semicolon' };
  return pressed === want || (codes[want] !== undefined && e.code === codes[want] && !e.shiftKey);
}

let teSelectionMenuInstances = 0;

class TeSelectionMenu {
  constructor(editor, options = {}) {
    this.editor = editor;
    this.surface = editor.surface;
    this.instance = ++teSelectionMenuInstances;
    this.items = [];
    this.element = null;
    this.visible = [];
    this.activeIndex = -1;
    this.context = null;
    this.openController = null;
    this.destroyed = false;

    this.abortController = new AbortController();
    const signal = this.abortController.signal;
    if (options.signal) {
      if (options.signal.aborted) this.abortController.abort();
      else options.signal.addEventListener('abort', () => this.destroy(), { signal });
    }
    const root = this.surface.root;
    root.addEventListener('contextmenu', (e) => this.onContextMenu(e), { signal });
    root.addEventListener('keydown', (e) => this.onSurfaceKeyDown(e), { signal });

    if (editor.ghosts) this.register(TeSelectionMenu.ghostItem(editor));
  }

  /** R-7.3 order. Items with other ids follow, in registration order. */
  static get SLOTS() {
    return ['alternatives', 'ai-alternatives', 'ghost', 'stash'];
  }

  /** The built-in Ghost it / Revive item (R-5.1, R-5.2). */
  static ghostItem(editor) {
    return {
      id: 'ghost',
      key: 'ctrl+/',
      label: (ctx) => (ctx.ghost && ctx.ghost.action === 'revive' ? 'Revive' : 'Ghost it'),
      available: (ctx) => {
        const layer = editor.ghosts;
        if (!layer || !layer.available()) return false;
        if (ctx.ghost === undefined) ctx.ghost = layer.targetFor(ctx);
        return !!ctx.ghost;
      },
      run: (ctx) => editor.ghosts.apply(ctx.ghost || editor.ghosts.targetFor(ctx)),
    };
  }

  /** Add (or replace, by id) a menu item. Returns an unregister function. */
  register(item) {
    if (!item || typeof item.id !== 'string' || typeof item.run !== 'function') {
      throw new Error('A selection menu item needs an id and a run function');
    }
    this.unregister(item.id);
    this.items.push(item);
    const slots = TeSelectionMenu.SLOTS;
    const rank = (it) => {
      const i = slots.indexOf(it.id);
      return i === -1 ? slots.length : i;
    };
    // Stable: registration order breaks ties.
    this.items = this.items.map((it, i) => [it, i]).sort((a, b) => rank(a[0]) - rank(b[0]) || a[1] - b[1]).map((x) => x[0]);
    return () => this.unregister(item.id);
  }

  unregister(id) {
    this.items = this.items.filter((it) => it.id !== id);
  }

  /** Registered item ids, in menu order. */
  ids() {
    return this.items.map((it) => it.id);
  }

  isOpen() {
    return !!this.element;
  }

  /** The action context for selection `sel` ({ start, end }). */
  contextFor(sel) {
    const start = Math.min(sel.start, sel.end);
    const end = Math.max(sel.start, sel.end);
    return {
      editor: this.editor,
      start,
      end,
      collapsed: start === end,
      text: this.surface.getText().slice(start, end),
      selection: { start: sel.start, end: sel.end, direction: sel.direction },
    };
  }

  /** Items available for `ctx`, in menu order. */
  availableItems(ctx) {
    return this.items.filter((it) => {
      try {
        return typeof it.available !== 'function' || !!it.available(ctx);
      } catch (err) {
        console.error(`Selection menu item "${it.id}" failed`, err);
        return false;
      }
    });
  }

  // ---------------------------------------------------------------------
  // Opening
  // ---------------------------------------------------------------------

  onContextMenu(e) {
    if (this.destroyed) return;
    if (this.isOpen()) {
      // The Menu key fires keydown (which opened the menu) and contextmenu.
      e.preventDefault();
      return;
    }
    let sel = this.surface.getSelectionOffsets();
    const pointer = !(e.clientX === 0 && e.clientY === 0) && e.button !== -1;
    if (sel.start === sel.end && pointer) {
      const at = this.offsetAtPoint(e.clientX, e.clientY);
      if (at >= 0) sel = { start: at, end: at, direction: 'none' };
    }
    const point = pointer ? { x: e.clientX, y: e.clientY } : null;
    if (this.open(sel, point)) e.preventDefault();
  }

  onSurfaceKeyDown(e) {
    if (this.destroyed || e.isComposing) return;
    if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey)) {
      if (this.open(this.surface.getSelectionOffsets(), null)) e.preventDefault();
      return;
    }
    if (!(e.ctrlKey || e.metaKey)) return;
    const item = this.items.find((it) => it.key && teShortcutMatches(e, it.key));
    if (!item) return;
    const ctx = this.contextFor(this.surface.getSelectionOffsets());
    if (this.availableItems(ctx).includes(item)) {
      e.preventDefault();
      this.run(item, ctx);
    }
  }

  /** UTF-16 offset of the text at viewport point (x, y), or -1. */
  offsetAtPoint(x, y) {
    let node = null;
    let offset = 0;
    if (document.caretRangeFromPoint) {
      const r = document.caretRangeFromPoint(x, y);
      if (r) {
        node = r.startContainer;
        offset = r.startOffset;
      }
    } else if (document.caretPositionFromPoint) {
      const p = document.caretPositionFromPoint(x, y);
      if (p) {
        node = p.offsetNode;
        offset = p.offset;
      }
    }
    return node ? this.surface.pointToOffset(node, offset) : -1;
  }

  /**
   * Open the menu for selection `sel` at viewport `point` ({ x, y }; under
   * the selection when null). Returns false, and shows nothing, when no
   * item is available.
   */
  open(sel, point = null) {
    if (this.destroyed) return false;
    this.close({ restoreFocus: false });
    const ctx = this.contextFor(sel);
    const items = this.availableItems(ctx);
    if (items.length === 0) return false;
    this.context = ctx;
    this.visible = items;

    const menu = document.createElement('div');
    menu.className = 'te-selection-menu';
    menu.id = `te-selection-menu-${this.instance}`;
    menu.setAttribute('role', 'menu');
    menu.setAttribute('aria-label', 'Selection actions');
    menu.setAttribute('aria-orientation', 'vertical');
    menu.tabIndex = -1;
    items.forEach((item, i) => {
      const row = document.createElement('div');
      row.className = 'te-selection-menu-item';
      row.setAttribute('role', 'menuitem');
      row.tabIndex = -1;
      row.dataset.item = item.id;
      row.dataset.index = String(i);
      const label = document.createElement('span');
      label.className = 'te-selection-menu-label';
      label.textContent = typeof item.label === 'function' ? item.label(ctx) : String(item.label || item.id);
      row.appendChild(label);
      if (item.key) {
        const keys = teShortcutLabels(item.key);
        row.setAttribute('aria-keyshortcuts', keys.aria);
        const hint = document.createElement('span');
        hint.className = 'te-selection-menu-key';
        hint.setAttribute('aria-hidden', 'true');
        hint.textContent = keys.text;
        row.appendChild(hint);
      }
      menu.appendChild(row);
    });
    document.body.appendChild(menu);
    this.element = menu;
    this.surface.root.setAttribute('aria-controls', menu.id);
    this.position(point || this.selectionPoint(ctx));

    this.openController = new AbortController();
    const signal = this.openController.signal;
    menu.addEventListener('keydown', (e) => this.onMenuKeyDown(e), { signal });
    // Keep the surface selection while pressing an item.
    menu.addEventListener('mousedown', (e) => e.preventDefault(), { signal });
    menu.addEventListener('click', (e) => {
      const row = e.target.closest('.te-selection-menu-item');
      if (row && menu.contains(row)) this.activate(Number(row.dataset.index));
    }, { signal });
    menu.addEventListener('mousemove', (e) => {
      const row = e.target.closest('.te-selection-menu-item');
      if (row && menu.contains(row)) this.setActive(Number(row.dataset.index));
    }, { signal });
    menu.addEventListener('focusout', (e) => {
      if (!e.relatedTarget || !menu.contains(e.relatedTarget)) {
        // Focus left for something else (Tab, a click elsewhere): close
        // without pulling focus back.
        if (e.relatedTarget) this.close({ restoreFocus: false });
      }
    }, { signal });
    const outside = (e) => {
      if (this.element && !this.element.contains(e.target)) this.close({ restoreFocus: false });
    };
    document.addEventListener('mousedown', outside, { signal, capture: true });
    document.addEventListener('touchstart', outside, { signal, capture: true, passive: true });
    // Any scroll (capture catches scrolling containers) or resize closes it.
    window.addEventListener('scroll', () => this.close({ restoreFocus: false }), { signal, capture: true, passive: true });
    window.addEventListener('resize', () => this.close({ restoreFocus: false }), { signal });
    window.addEventListener('blur', () => this.close({ restoreFocus: false }), { signal });

    this.setActive(0);
    return true;
  }

  /**
   * Viewport point under the last line of the selection, at its left edge.
   * Decorations split a line into several rects, so the whole line is used.
   */
  selectionPoint(ctx) {
    const range = this.surface.rangeForOffsets(ctx.start, ctx.end);
    const rects = Array.from(range.getClientRects()).filter((r) => r.width > 0 && r.height > 0);
    if (rects.length > 0) {
      const lastTop = Math.max(...rects.map((r) => r.top));
      const line = rects.filter((r) => r.bottom > lastTop);
      return {
        x: Math.min(...line.map((r) => r.left)),
        y: Math.max(...line.map((r) => r.bottom)) + 4,
      };
    }
    const caret = this.surface.getCaretRect(ctx.end);
    return { x: caret.left, y: caret.bottom + 4 };
  }

  /** Place the menu at `point`, kept inside the viewport. */
  position(point) {
    const menu = this.element;
    const margin = 8;
    const box = menu.getBoundingClientRect();
    const vw = document.documentElement.clientWidth || window.innerWidth;
    const vh = document.documentElement.clientHeight || window.innerHeight;
    let left = point.x;
    let top = point.y;
    if (left + box.width > vw - margin) left = vw - margin - box.width;
    if (top + box.height > vh - margin) top = Math.max(margin, point.y - box.height - 8);
    menu.style.left = `${Math.max(margin, left)}px`;
    menu.style.top = `${Math.max(margin, top)}px`;
  }

  rows() {
    return this.element ? Array.from(this.element.querySelectorAll('.te-selection-menu-item')) : [];
  }

  setActive(index) {
    const rows = this.rows();
    if (rows.length === 0) return;
    const i = ((index % rows.length) + rows.length) % rows.length;
    this.activeIndex = i;
    rows.forEach((row, j) => row.classList.toggle('te-selection-menu-item--active', j === i));
    if (document.activeElement !== rows[i]) rows[i].focus({ preventScroll: true });
  }

  onMenuKeyDown(e) {
    const n = this.visible.length;
    switch (e.key) {
      case 'ArrowDown':
        this.setActive(this.activeIndex + 1);
        break;
      case 'ArrowUp':
        this.setActive(this.activeIndex - 1);
        break;
      case 'Home':
        this.setActive(0);
        break;
      case 'End':
        this.setActive(n - 1);
        break;
      case 'Enter':
      case ' ':
        this.activate(this.activeIndex);
        break;
      case 'Escape':
      case 'Tab':
        this.close({ restoreFocus: true });
        break;
      default: {
        // An item's own shortcut works while the menu is open.
        const index = this.visible.findIndex((it) => it.key && teShortcutMatches(e, it.key));
        if (index === -1) return;
        this.activate(index);
      }
    }
    e.preventDefault();
    e.stopPropagation();
  }

  /** Run visible item `index` on the selection the menu was opened for. */
  activate(index) {
    const item = this.visible[index];
    const ctx = this.context;
    if (!item || !ctx) return;
    this.close({ restoreFocus: true });
    this.run(item, ctx);
  }

  run(item, ctx) {
    try {
      return item.run(ctx);
    } catch (err) {
      console.error(`Selection menu item "${item.id}" failed`, err);
      return undefined;
    }
  }

  /** Close the menu; with restoreFocus, focus the surface and reselect. */
  close({ restoreFocus = true } = {}) {
    if (!this.element) return;
    const ctx = this.context;
    if (this.openController) this.openController.abort();
    this.openController = null;
    this.element.remove();
    this.element = null;
    this.visible = [];
    this.activeIndex = -1;
    this.context = null;
    if (this.surface.root) this.surface.root.removeAttribute('aria-controls');
    if (restoreFocus && ctx && !this.surface.destroyed) {
      this.surface.focus();
      const sel = ctx.selection;
      this.surface.setSelectionOffsets(sel.start, sel.end, sel.direction === 'none' ? undefined : sel.direction);
    }
  }

  destroy() {
    if (this.destroyed) return;
    this.close({ restoreFocus: false });
    this.destroyed = true;
    this.items = [];
    this.abortController.abort();
  }
}

window.TeGhostLayer = TeGhostLayer;
window.TeSelectionMenu = TeSelectionMenu;
