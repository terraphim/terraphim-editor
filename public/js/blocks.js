/*
 * Blocks view (issue #19)
 * =======================
 *
 * A third way of looking at the open document, alongside the plain editor
 * and Write_On mode: the Markdown body is shown as a vertical list of
 * top-level blocks (headings, paragraphs, lists, code fences, blockquotes,
 * tables, rules), each rendered with the same Rust converter as the preview
 * and editable on its own. Full design: docs/design/blocks-view.md.
 *
 * Single source of truth
 * ----------------------
 * The view holds no document content of its own. Blocks are parsed on demand
 * from `editor.surface.getText()`, which the document bridge
 * (src/document.rs) keeps identical to the model body; the annotation block
 * never reaches the surface, so it is never shown and never lost. Every
 * block is a contiguous slice of the body (`text` plus the trailing
 * separator `sep`, with any leading blank lines in `lead`), so
 * `BlocksView.serialise(BlocksView.parse(body)) === body` by construction;
 * the block type is classification only.
 *
 * A committed block edit is reduced to the minimal changed region
 * (EditorSurface.diff) and applied with `surface.replaceRange(...,
 * { source: 'blocks' })`. That is the same edit path as typing: the surface
 * maps decorations, mirrors the edit into the span model with `apply_edit`
 * (alternatives and ghosts outside the changed region re-anchor), records
 * one undo step and dispatches `input` so the preview re-renders. Edits are
 * committed on blur, Ctrl+Enter or when leaving the view, never per
 * keystroke.
 *
 * Controls
 * --------
 * Plain mode: a "View" button group (Text | Blocks) in the toolbar. Write_On
 * mode has no Blocks control: the toolbar is hidden there by write-on.css,
 * and entering Write_On switches back to the text surface (the Blocks view
 * resumes when plain mode returns). Blocks is not part of the Write_On spec.
 *
 * Keyboard (the list uses a roving tabindex):
 *   ArrowUp / ArrowDown / Home / End   move between blocks
 *   Enter or F2                        edit the focused block
 *   Ctrl+Enter / Escape (editing)      commit / cancel the edit
 *   Delete                             delete the block
 *   Ctrl+Z, Ctrl+Shift+Z / Ctrl+Y      undo / redo (the surface history)
 *
 * There is deliberately no "move block" command: moving text through the
 * edit path is a deletion plus an insertion, and the span model releases
 * ghosts inside deleted text (as it does for cut and paste), so a move would
 * lose them. Reordering needs a move operation in the model first (see
 * docs/design/blocks-view.md, follow-ups).
 *
 * Events: every view change dispatches a bubbling `te:view-change`
 * { view: 'text' | 'blocks', editor } from the blocks container.
 *
 * Preference: the last view is stored per viewer (not per document) in
 * localStorage under `terraphim-editor:view`, wrapped in try/catch. It is a
 * UI preference only; no document content is ever stored.
 *
 * Lifecycle: MarkdownEditor creates the view after the chrome and passes its
 * AbortController signal; every listener uses it. destroy() removes the
 * container, the toolbar group and the change subscription, and shows the
 * surface again.
 */

const BLOCKS_VIEW_STORAGE_KEY = 'terraphim-editor:view';

const BLOCK_LABELS = {
  heading: 'Heading',
  paragraph: 'Paragraph',
  list: 'List',
  code: 'Code',
  quote: 'Quote',
  table: 'Table',
  rule: 'Rule',
};

const RE_BLANK = /^[ \t\r]*$/;
const RE_FENCE = /^ {0,3}(`{3,}|~{3,})/;
const RE_ATX = /^ {0,3}(#{1,6})(?:[ \t]|\r?$)/;
const RE_RULE = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t\r]*$/;
const RE_SETEXT = /^ {0,3}(=+|-+)[ \t\r]*$/;
const RE_QUOTE = /^ {0,3}>/;
const RE_LIST = /^ {0,3}(?:[-+*]|\d{1,9}[.)])(?:[ \t]|\r?$)/;
const RE_INDENTED = /^(?: {4}|\t)/;
const RE_TABLE_DELIM = /^ {0,3}\|?[ \t]*:?-+:?[ \t]*(?:\|[ \t]*:?-+:?[ \t]*)*\|?[ \t\r]*$/;

class BlocksView {
  constructor(editor, options = {}) {
    this.editor = editor;
    this.surface = editor.surface;
    this.signal = options.signal;
    this.view = 'text';
    this.destroyed = false;
    this.model = { lead: '', blocks: [] };
    this.focusIndex = 0;
    this.editing = null;
    // Set while this view applies an edit, so the change subscription does
    // not rebuild the DOM under the textarea that is being committed.
    this.applying = false;
    this.dirty = false;
    // Blocks was showing when Write_On mode took over.
    this.resumeAfterWriteOn = false;

    this.build();
    this.offChange = this.surface.onChange(() => this.onSurfaceChange());
    this.listen(document, 'te:mode-change', (e) => this.onModeChange(e));

    // Restore the viewer's last view (never in Write_On mode).
    if (BlocksView.loadPreference() === 'blocks') {
      if (this.writeOnActive()) this.resumeAfterWriteOn = true;
      else this.setView('blocks', { persist: false, focus: false });
    }
  }

  // ---------------------------------------------------------------------
  // Parsing (pure; no DOM)
  // ---------------------------------------------------------------------

  /**
   * Split Markdown into top-level blocks. Returns { lead, blocks } where
   * each block is { type, level, start, end, text, sep } with UTF-16
   * offsets; text === source.slice(start, end) and sep runs to the next
   * block (or the end of the source). Lossless for any input.
   */
  static parse(source) {
    const src = String(source);
    const lines = [];
    let at = 0;
    while (at <= src.length) {
      const nl = src.indexOf('\n', at);
      const end = nl < 0 ? src.length : nl;
      lines.push({ start: at, end, text: src.slice(at, end) });
      if (nl < 0) break;
      at = nl + 1;
    }
    const blank = (i) => RE_BLANK.test(lines[i].text);
    const raw = [];
    let i = 0;
    const n = lines.length;
    while (i < n) {
      if (blank(i)) {
        i += 1;
        continue;
      }
      const first = i;
      const line = lines[i].text;
      let type = 'paragraph';
      let level = 0;
      let last = i;
      let m;
      if ((m = RE_FENCE.exec(line))) {
        type = 'code';
        const fence = m[1];
        const close = new RegExp('^ {0,3}' + (fence[0] === '`' ? '`' : '~') + '{' + fence.length + ',}[ \\t\\r]*$');
        last = n - 1;
        for (let j = i + 1; j < n; j += 1) {
          if (close.test(lines[j].text)) {
            last = j;
            break;
          }
        }
      } else if ((m = RE_ATX.exec(line))) {
        type = 'heading';
        level = m[1].length;
      } else if (RE_RULE.test(line)) {
        type = 'rule';
      } else if (RE_QUOTE.test(line)) {
        type = 'quote';
        while (last + 1 < n && !blank(last + 1)) last += 1;
      } else if (RE_LIST.test(line)) {
        type = 'list';
        const kind = BlocksView.listKind(line);
        let j = i + 1;
        while (j < n) {
          if (!blank(j)) {
            last = j;
            j += 1;
            continue;
          }
          // A loose list continues after blank lines when the next content
          // line is indented (a continuation) or is another item.
          let k = j;
          while (k < n && blank(k)) k += 1;
          if (k < n && (/^(?: {2,}|\t)/.test(lines[k].text) || BlocksView.listKind(lines[k].text) === kind)) {
            j = k;
          } else {
            break;
          }
        }
      } else if (RE_INDENTED.test(line)) {
        type = 'code';
        let j = i + 1;
        while (j < n && (blank(j) || RE_INDENTED.test(lines[j].text))) {
          if (!blank(j)) last = j;
          j += 1;
        }
      } else if (line.includes('|') && i + 1 < n && RE_TABLE_DELIM.test(lines[i + 1].text) && lines[i + 1].text.includes('-')) {
        type = 'table';
        last = i + 1;
        while (last + 1 < n && !blank(last + 1)) last += 1;
      } else {
        // Paragraph: runs until a blank line or a line that starts another
        // block; a setext underline turns it into a heading.
        while (last + 1 < n) {
          const next = lines[last + 1].text;
          if (RE_BLANK.test(next)) break;
          const setext = RE_SETEXT.exec(next);
          if (setext) {
            last += 1;
            type = 'heading';
            level = setext[1][0] === '=' ? 1 : 2;
            break;
          }
          if (RE_FENCE.test(next) || RE_ATX.test(next) || RE_QUOTE.test(next) || RE_RULE.test(next)) break;
          if (/^ {0,3}(?:[-+*]|1[.)])[ \t]+\S/.test(next)) break;
          last += 1;
        }
      }
      raw.push({ type, level, start: lines[first].start, end: lines[last].end });
      i = last + 1;
    }
    const blocks = raw.map((b, k) => {
      const nextStart = k + 1 < raw.length ? raw[k + 1].start : src.length;
      return {
        type: b.type,
        level: b.level,
        start: b.start,
        end: b.end,
        text: src.slice(b.start, b.end),
        sep: src.slice(b.end, nextStart),
      };
    });
    const lead = blocks.length ? src.slice(0, blocks[0].start) : src;
    return { lead, blocks };
  }

  /** The marker kind of a list item line ('-', '+', '*', '.' or ')'), or null. */
  static listKind(line) {
    const m = /^ {0,3}(?:([-+*])|\d{1,9}([.)]))(?:[ \t]|\r?$)/.exec(line);
    return m ? m[1] || m[2] : null;
  }

  /** Inverse of parse(): lead plus every block's text and separator. */
  static serialise(model) {
    let out = model.lead || '';
    for (const b of model.blocks) out += b.text + b.sep;
    return out;
  }

  static loadPreference() {
    try {
      return window.localStorage.getItem(BLOCKS_VIEW_STORAGE_KEY);
    } catch (e) {
      return null;
    }
  }

  static savePreference(view) {
    try {
      window.localStorage.setItem(BLOCKS_VIEW_STORAGE_KEY, view);
    } catch (e) {
      // Storage unavailable: the view still works for this session.
    }
  }

  // ---------------------------------------------------------------------
  // DOM
  // ---------------------------------------------------------------------

  listen(target, type, fn, options = {}) {
    target.addEventListener(type, fn, { ...options, signal: this.signal });
  }

  build() {
    const root = this.surface.root;

    // Toolbar group: Text | Blocks (plain mode only; the toolbar is hidden in
    // Write_On mode).
    const toolbar = this.editor.toolbar ? this.editor.toolbar.parentNode : null;
    if (toolbar) {
      const divider = document.createElement('sl-divider');
      divider.setAttribute('vertical', '');
      divider.className = 'te-view-divider';
      const group = document.createElement('sl-button-group');
      group.className = 'te-view-toggle';
      group.setAttribute('label', 'View');
      const button = (view, label, icon) => {
        const b = document.createElement('sl-button');
        b.setAttribute('size', 'small');
        b.dataset.view = view;
        b.setAttribute('aria-pressed', 'false');
        b.setAttribute('title', `${label} view`);
        const i = document.createElement('i');
        i.className = `${icon} te-view-icon`;
        i.setAttribute('aria-hidden', 'true');
        b.append(i, document.createTextNode(` ${label}`));
        this.listen(b, 'click', () => this.setView(view));
        group.appendChild(b);
        return b;
      };
      this.textButton = button('text', 'Text', 'fa-solid fa-align-left');
      this.blocksButton = button('blocks', 'Blocks', 'fa-solid fa-layer-group');
      const anchor = this.editor.toolbar.nextSibling;
      toolbar.insertBefore(divider, anchor);
      toolbar.insertBefore(group, anchor);
      this.toggleGroup = group;
      this.toggleDivider = divider;
    }

    const container = document.createElement('div');
    container.className = 'te-blocks';
    container.hidden = true;
    container.setAttribute('aria-label', 'Document blocks');
    const list = document.createElement('div');
    list.className = 'te-blocks-list';
    list.setAttribute('role', 'list');
    list.setAttribute('aria-label', 'Blocks');
    const add = document.createElement('button');
    add.type = 'button';
    add.className = 'te-blocks-add';
    const addIcon = document.createElement('i');
    addIcon.className = 'fa-solid fa-plus';
    addIcon.setAttribute('aria-hidden', 'true');
    add.append(addIcon, document.createTextNode(' Add paragraph'));
    this.listen(add, 'click', () => this.insertBlockAfter(this.model.blocks.length - 1));
    container.append(list, add);
    root.parentNode.insertBefore(container, root.nextSibling);
    this.container = container;
    this.list = list;
    this.addButton = add;

    this.listen(list, 'keydown', (e) => this.onKeyDown(e));
    this.listen(list, 'click', (e) => this.onClick(e));
    this.listen(list, 'dblclick', (e) => {
      const card = e.target.closest('.te-block');
      if (card && !e.target.closest('textarea')) this.editBlock(Number(card.dataset.index));
    });
    this.listen(list, 'focusin', (e) => {
      const card = e.target.closest('.te-block');
      if (card && e.target === card) this.setCurrent(Number(card.dataset.index), false);
    });
  }

  render(focus = false) {
    this.dirty = false;
    this.editing = null;
    this.model = BlocksView.parse(this.surface.getText());
    const blocks = this.model.blocks;
    if (this.focusIndex >= blocks.length) this.focusIndex = Math.max(0, blocks.length - 1);
    const frag = document.createDocumentFragment();
    blocks.forEach((b, i) => frag.appendChild(this.renderCard(b, i, blocks.length)));
    if (blocks.length === 0) {
      const empty = document.createElement('p');
      empty.className = 'te-blocks-empty';
      empty.textContent = 'The document is empty. Add a paragraph to start.';
      frag.appendChild(empty);
    }
    this.list.textContent = '';
    this.list.appendChild(frag);
    if (focus) this.focusCard(this.focusIndex);
  }

  renderCard(block, index, total) {
    const card = document.createElement('div');
    card.className = `te-block te-block-${block.type}`;
    card.setAttribute('role', 'listitem');
    card.dataset.index = String(index);
    card.dataset.type = block.type;
    card.tabIndex = index === this.focusIndex ? 0 : -1;
    const label = BLOCK_LABELS[block.type] + (block.level ? ` ${block.level}` : '');
    card.setAttribute('aria-label', `${label}, block ${index + 1} of ${total}. Press Enter to edit.`);

    const head = document.createElement('div');
    head.className = 'te-block-head';
    const tag = document.createElement('span');
    tag.className = 'te-block-type';
    tag.textContent = block.type === 'heading' ? `H${block.level}` : BLOCK_LABELS[block.type];
    const actions = document.createElement('span');
    actions.className = 'te-block-actions';
    const action = (name, label, icon) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'te-block-action';
      b.dataset.action = name;
      b.tabIndex = -1;
      b.setAttribute('aria-label', label);
      b.title = label;
      const i = document.createElement('i');
      i.className = icon;
      i.setAttribute('aria-hidden', 'true');
      b.appendChild(i);
      actions.appendChild(b);
    };
    action('edit', 'Edit block (Enter)', 'fa-solid fa-pen');
    action('insert', 'Add paragraph below', 'fa-solid fa-plus');
    action('delete', 'Delete block (Delete)', 'fa-regular fa-trash-can');
    head.append(tag, actions);

    const body = document.createElement('div');
    body.className = 'te-block-body';
    const html = this.renderMarkdown(block.text);
    if (html === null) {
      const pre = document.createElement('pre');
      pre.className = 'te-block-source';
      pre.textContent = block.text;
      body.appendChild(pre);
    } else {
      // Same converter, and the same trust level, as the live preview.
      body.innerHTML = html;
    }
    card.append(head, body);
    return card;
  }

  /** HTML from the Rust converter, or null when it is not exposed. */
  renderMarkdown(text) {
    const api = window.wasmBindings;
    if (!api || typeof api.render_markdown !== 'function') return null;
    try {
      return api.render_markdown(text);
    } catch (e) {
      return null;
    }
  }

  cards() {
    return Array.from(this.list.querySelectorAll(':scope > .te-block'));
  }

  setCurrent(index, focus = true) {
    const cards = this.cards();
    if (cards.length === 0) return;
    const i = Math.max(0, Math.min(cards.length - 1, index));
    this.focusIndex = i;
    cards.forEach((c, k) => {
      c.tabIndex = k === i ? 0 : -1;
    });
    // Native focus scrolls the card into view only when it is off screen.
    if (focus) cards[i].focus();
  }

  focusCard(index) {
    this.setCurrent(index, true);
  }

  // ---------------------------------------------------------------------
  // View switching
  // ---------------------------------------------------------------------

  isActive() {
    return this.view === 'blocks';
  }

  writeOnActive() {
    const chrome = this.editor.chrome;
    return !!(chrome && typeof chrome.isWriteOn === 'function' && chrome.isWriteOn());
  }

  /**
   * Show 'text' (the editing surface) or 'blocks'. The caret follows: the
   * block containing the surface selection gets focus, and on the way back
   * the caret goes to the edited position or the start of the focused block.
   */
  setView(view, { persist = true, focus = true } = {}) {
    if (this.destroyed) return;
    const next = view === 'blocks' ? 'blocks' : 'text';
    if (next === 'blocks' && this.writeOnActive()) return;
    if (persist) BlocksView.savePreference(next);
    if (next === this.view) return;
    if (next === 'blocks') {
      const sel = this.surface.getSelectionOffsets();
      this.view = 'blocks';
      this.render(false);
      this.focusIndex = this.blockIndexAt(sel.start);
      this.surface.root.hidden = true;
      this.container.hidden = false;
      this.setCurrent(this.focusIndex, focus);
    } else {
      let target = null;
      if (this.editing) {
        target = this.commitEdit({ rerender: false });
      }
      if (!target) {
        const b = this.model.blocks[this.focusIndex];
        target = b ? { start: b.start, end: b.start } : null;
      }
      this.view = 'text';
      this.editing = null;
      this.container.hidden = true;
      this.surface.root.hidden = false;
      this.list.textContent = '';
      if (target) {
        if (focus) this.surface.root.focus({ preventScroll: true });
        this.surface.setSelectionOffsets(target.start, target.end);
      }
    }
    this.updateToggle();
    this.container.dispatchEvent(new CustomEvent('te:view-change', {
      bubbles: true,
      detail: { view: this.view, editor: this.editor },
    }));
  }

  toggle() {
    this.setView(this.isActive() ? 'text' : 'blocks');
  }

  updateToggle() {
    for (const [b, view] of [[this.textButton, 'text'], [this.blocksButton, 'blocks']]) {
      if (!b) continue;
      const on = this.view === view;
      b.setAttribute('aria-pressed', on ? 'true' : 'false');
      b.setAttribute('variant', on ? 'primary' : 'default');
    }
    this.container.dataset.view = this.view;
  }

  /** Index of the block that owns `offset` (its text or trailing separator). */
  blockIndexAt(offset) {
    const blocks = this.model.blocks;
    for (let i = blocks.length - 1; i >= 0; i -= 1) {
      if (offset >= blocks[i].start) return i;
    }
    return 0;
  }

  onModeChange(e) {
    const mode = e.detail && e.detail.mode;
    if (mode === 'write-on') {
      if (this.isActive()) {
        this.resumeAfterWriteOn = true;
        this.setView('text', { persist: false, focus: false });
      }
    } else if (mode === 'plain' && this.resumeAfterWriteOn) {
      this.resumeAfterWriteOn = false;
      this.setView('blocks', { persist: false, focus: false });
    }
  }

  onSurfaceChange() {
    if (!this.isActive()) return;
    if (this.applying) {
      this.dirty = true;
      return;
    }
    // An outside change (open, undo from elsewhere): rebuild, keeping focus
    // in the list if it was there.
    const hadFocus = this.container.contains(document.activeElement);
    this.render(hadFocus);
  }

  // ---------------------------------------------------------------------
  // Editing
  // ---------------------------------------------------------------------

  /** Open the inline editor for block `index`. Returns the textarea. */
  editBlock(index) {
    if (!this.isActive()) return null;
    if (this.editing) this.commitEdit();
    const cards = this.cards();
    const block = this.model.blocks[index];
    const card = cards[index];
    if (!block || !card) return null;
    this.setCurrent(index, false);
    return this.openEditor(card, { index, start: block.start, original: block.text, isNew: false });
  }

  openEditor(card, edit) {
    const ta = document.createElement('textarea');
    ta.className = 'te-block-editor';
    ta.value = edit.original;
    ta.spellcheck = false;
    ta.setAttribute('aria-label', 'Edit block Markdown. Ctrl+Enter to save, Escape to cancel.');
    ta.rows = Math.max(2, edit.original.split('\n').length + 1);
    const body = card.querySelector('.te-block-body');
    if (body) body.hidden = true;
    card.classList.add('te-block-editing');
    card.appendChild(ta);
    this.editing = { ...edit, card, textarea: ta };
    this.listen(ta, 'input', () => {
      ta.rows = Math.max(2, ta.value.split('\n').length + 1);
    });
    this.listen(ta, 'blur', (e) => {
      // Focus moving somewhere specific (another card, a toolbar button)
      // must not be pulled back into the list by the commit.
      if (this.editing && this.editing.textarea === ta) this.commitEdit({ focus: !e.relatedTarget });
    });
    ta.focus({ preventScroll: true });
    ta.setSelectionRange(ta.value.length, ta.value.length);
    return ta;
  }

  /**
   * Apply the open editor's text to the body through the surface and close
   * it. Returns the body selection matching the textarea selection, or null
   * when nothing was being edited.
   */
  commitEdit({ rerender = true, focus } = {}) {
    const edit = this.editing;
    if (!edit) return null;
    this.editing = null;
    const ta = edit.textarea;
    const value = window.EditorSurface.normaliseNewlines(ta.value);
    const selStart = Math.min(ta.selectionStart, value.length);
    const selEnd = Math.min(ta.selectionEnd, value.length);
    let base = edit.start;
    this.applying = true;
    try {
      if (edit.isNew) {
        if (value.trim() !== '') {
          const at = edit.start;
          const text = this.surface.getText();
          const before = text.slice(0, at);
          // Keep the new paragraph separated from its neighbours by one blank line.
          const pre = at === 0 || before.endsWith('\n\n') ? '' : before.endsWith('\n') ? '\n' : '\n\n';
          const post = edit.beforeNext ? '\n\n' : '';
          base = at + pre.length;
          this.surface.replaceRange(at, at, pre + value + post, { source: 'blocks', selectStart: base + selStart, selectEnd: base + selEnd });
          this.focusIndex = edit.index;
        }
      } else if (value !== edit.original) {
        const d = window.EditorSurface.diff(edit.original, value);
        const from = edit.start + d.start;
        this.surface.replaceRange(from, from + d.deletedLength, d.insertedText, {
          source: 'blocks',
          selectStart: edit.start + selStart,
          selectEnd: edit.start + selEnd,
        });
      }
    } finally {
      this.applying = false;
    }
    if (rerender && this.isActive()) {
      const hadFocus = focus === undefined ? this.container.contains(document.activeElement) || document.activeElement === document.body : focus;
      this.render(false);
      if (this.model.blocks.length) {
        this.focusIndex = this.blockIndexAt(base + selStart);
        this.setCurrent(this.focusIndex, hadFocus);
      }
    }
    return { start: base + selStart, end: base + selEnd };
  }

  /** Close the open editor without applying it. */
  cancelEdit() {
    const edit = this.editing;
    if (!edit) return;
    this.editing = null;
    this.render(false);
    this.setCurrent(edit.index, true);
  }

  /** Open an editor for a new paragraph after block `index` (-1: at the top). */
  insertBlockAfter(index) {
    if (!this.isActive()) return null;
    if (this.editing) this.commitEdit();
    const blocks = this.model.blocks;
    const text = this.surface.getText();
    let at;
    if (index + 1 < blocks.length) at = blocks[index + 1].start;
    else if (blocks.length) at = blocks[blocks.length - 1].end;
    else at = text.length;
    const card = document.createElement('div');
    card.className = 'te-block te-block-paragraph te-block-new';
    card.setAttribute('role', 'listitem');
    card.dataset.index = String(index + 1);
    const cards = this.cards();
    const ref = cards[index + 1] || null;
    if (ref) this.list.insertBefore(card, ref);
    else this.list.appendChild(card);
    const empty = this.list.querySelector('.te-blocks-empty');
    if (empty) empty.remove();
    return this.openEditor(card, { index: index + 1, start: at, original: '', isNew: true, beforeNext: index + 1 < blocks.length });
  }

  /** Delete block `index` together with one adjoining separator. */
  deleteBlock(index) {
    const blocks = this.model.blocks;
    const b = blocks[index];
    if (!b) return;
    let from = b.start;
    let to = b.end + b.sep.length;
    if (index === blocks.length - 1) {
      // Keep the document's trailing text; take the separator before instead.
      to = b.end;
      if (index > 0) from = blocks[index - 1].end;
    }
    this.apply(from, to, '');
    this.focusIndex = Math.max(0, Math.min(index, this.model.blocks.length - 1));
    this.setCurrent(this.focusIndex, true);
  }

  apply(from, to, insert) {
    this.applying = true;
    try {
      this.surface.replaceRange(from, to, insert, { source: 'blocks', selectStart: from });
    } finally {
      this.applying = false;
    }
    this.render(false);
  }

  undo() {
    this.applying = true;
    let done;
    try {
      done = this.surface.undo();
    } finally {
      this.applying = false;
    }
    if (done) this.afterHistory();
    return done;
  }

  redo() {
    this.applying = true;
    let done;
    try {
      done = this.surface.redo();
    } finally {
      this.applying = false;
    }
    if (done) this.afterHistory();
    return done;
  }

  afterHistory() {
    // The surface put its caret on the restored edit; follow it.
    const { start } = this.surface.getSelectionOffsets();
    this.render(false);
    this.focusIndex = this.blockIndexAt(start);
    this.setCurrent(this.focusIndex, true);
  }

  onClick(e) {
    const button = e.target.closest('.te-block-action');
    if (!button) return;
    const card = button.closest('.te-block');
    const index = Number(card.dataset.index);
    switch (button.dataset.action) {
      case 'edit': this.editBlock(index); break;
      case 'insert': this.insertBlockAfter(index); break;
      case 'delete': this.deleteBlock(index); break;
      default: break;
    }
  }

  onKeyDown(e) {
    if (e.isComposing) return;
    const mod = e.ctrlKey || e.metaKey;
    if (e.target.classList && e.target.classList.contains('te-block-editor')) {
      if (e.key === 'Escape') {
        e.preventDefault();
        this.cancelEdit();
      } else if (e.key === 'Enter' && mod) {
        e.preventDefault();
        this.commitEdit();
      }
      return;
    }
    const card = e.target.closest && e.target.closest('.te-block');
    if (!card || e.target !== card) return;
    const index = Number(card.dataset.index);
    const key = e.key;
    if (mod && !e.altKey) {
      const k = key.toLowerCase();
      if (k === 'z' && !e.shiftKey) {
        e.preventDefault();
        this.undo();
      } else if ((k === 'z' && e.shiftKey) || (k === 'y' && !e.shiftKey)) {
        e.preventDefault();
        this.redo();
      }
      return;
    }
    switch (key) {
      case 'ArrowDown':
        e.preventDefault();
        this.setCurrent(index + 1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        this.setCurrent(index - 1);
        break;
      case 'Home':
        e.preventDefault();
        this.setCurrent(0);
        break;
      case 'End':
        e.preventDefault();
        this.setCurrent(this.model.blocks.length - 1);
        break;
      case 'Enter':
      case 'F2':
        e.preventDefault();
        this.editBlock(index);
        break;
      case 'Delete':
        e.preventDefault();
        this.deleteBlock(index);
        break;
      default:
        break;
    }
  }

  // ---------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------

  /** Remove the view's DOM and subscription and show the surface again. */
  destroy() {
    if (this.destroyed) return;
    this.destroyed = true;
    this.editing = null;
    this.offChange();
    if (this.surface.root) this.surface.root.hidden = false;
    this.container.remove();
    if (this.toggleGroup) this.toggleGroup.remove();
    if (this.toggleDivider) this.toggleDivider.remove();
  }
}

window.BlocksView = BlocksView;
window.BLOCKS_VIEW_STORAGE_KEY = BLOCKS_VIEW_STORAGE_KEY;
