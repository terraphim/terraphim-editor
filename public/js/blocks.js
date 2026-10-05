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
 * Drafts are never lost
 * ---------------------
 * While a block editor is open the body can still change from outside (an
 * integration calling surface.replaceRange or setText, undo from elsewhere,
 * openDocument). Every surface change event carries the exact edit, and the
 * open editor's range is mapped through it:
 *   - an edit entirely before the block shifts the range; an edit entirely
 *     after it changes nothing. The textarea stays open with its draft and
 *     the other cards are re-rendered around it;
 *   - an edit that overlaps the block, or replaces the whole document
 *     (openDocument, setText), closes the editor and keeps the draft in a
 *     visible, non-blocking notice above the view, with the block text as
 *     it was when editing began (`originalText`). The notice offers
 *     "Replace block" when the block at the draft's position still holds
 *     exactly `originalText`, and "Insert as new paragraph" otherwise (it
 *     then goes after that block, or at the block index after a
 *     whole-document replacement), so unrelated text is never replaced. The
 *     label is recomputed on every change. Either is one undo step.
 *     "Discard" drops the draft.
 * A commit also checks that the block's original text is still at the mapped
 * range; if not, the draft goes to the notice instead of being applied.
 * Kept drafts follow later edits and stay available in both views.
 *
 * Teardown never mutates the document (the host may already have saved):
 * destroy() collects the open editor's draft (if it differs from the block's
 * text) and every kept draft, dispatches one bubbling `te:blocks-drafts`
 * CustomEvent from the surface root with
 *   detail: { editor, drafts: [{ value, originalText, anchor, index, isNew }] }
 * before removing its DOM, and returns the same array (MarkdownEditor.destroy
 * returns it too). With nothing pending there is no event and the array is
 * empty. pendingDrafts() returns the same list without tearing down.
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
 *   Shift+Enter                        add a paragraph below the block
 *   Ctrl+Enter / Escape (editing)      commit / cancel the edit
 *   Delete                             delete the block
 *   Tab                                the focused block's action buttons
 *                                      (edit, add below, delete); Escape
 *                                      returns to the block
 *   Ctrl+Z, Ctrl+Shift+Z / Ctrl+Y      undo / redo (the surface history)
 *
 * There is deliberately no "move block" command: moving text through the
 * edit path is a deletion plus an insertion, and the span model releases
 * ghosts inside deleted text (as it does for cut and paste), so a move would
 * lose them. Reordering needs a move operation in the model first (#44;
 * see docs/design/blocks-view.md, follow-ups).
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
 * container, the drafts notice, the toolbar group and the change
 * subscription, and shows the surface again.
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
    // Drafts kept after an outside change overlapped an open block editor.
    this.drafts = [];
    this.nextDraftId = 1;
    // Blocks was showing when Write_On mode took over.
    this.resumeAfterWriteOn = false;

    this.build();
    this.offChange = this.surface.onChange((change) => this.onSurfaceChange(change));
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

  /** Index of the block in `model` that owns `offset` (text or separator). */
  static indexIn(model, offset) {
    const blocks = model.blocks;
    for (let i = blocks.length - 1; i >= 0; i -= 1) {
      if (offset >= blocks[i].start) return i;
    }
    return 0;
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

    // Kept drafts: above the surface and the blocks, visible in both views.
    const drafts = document.createElement('div');
    drafts.className = 'te-blocks-drafts';
    drafts.hidden = true;
    drafts.tabIndex = -1;
    drafts.setAttribute('role', 'region');
    drafts.setAttribute('aria-label', 'Kept block drafts');
    drafts.setAttribute('aria-live', 'polite');
    root.parentNode.insertBefore(drafts, root);
    this.draftsElement = drafts;
    this.listen(drafts, 'click', (e) => {
      const b = e.target.closest('[data-draft-action]');
      if (!b) return;
      const id = Number(b.dataset.draftId);
      if (b.dataset.draftAction === 'apply') this.applyDraft(id);
      else this.discardDraft(id);
    });
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

  /**
   * Rebuild the cards from the body. An open block editor is kept in place
   * (its card is never detached, so the textarea keeps focus, draft and
   * selection) and the other cards are rebuilt around it. Callers that mean
   * to close the editor clear `this.editing` first.
   */
  render(focus = false) {
    this.dirty = false;
    this.model = BlocksView.parse(this.surface.getText());
    const blocks = this.model.blocks;
    if (this.focusIndex >= blocks.length) this.focusIndex = Math.max(0, blocks.length - 1);
    const cards = blocks.map((b, i) => this.renderCard(b, i, blocks.length));
    const keep = this.editing && this.editing.card.parentNode === this.list ? this.editing : null;
    if (keep) {
      const k = keep.isNew
        ? blocks.filter((b) => b.start < keep.start).length
        : BlocksView.indexIn(this.model, keep.start);
      const kept = keep.card;
      for (const child of Array.from(this.list.children)) {
        if (child !== kept) child.remove();
      }
      cards.forEach((c, i) => {
        if (!keep.isNew && i === k) return;
        if (i < k) this.list.insertBefore(c, kept);
        else this.list.appendChild(c);
      });
      keep.index = k;
      kept.dataset.index = String(k);
      if (keep.selection && document.activeElement === keep.textarea) {
        keep.textarea.setSelectionRange(...keep.selection);
      }
      return;
    }
    this.editing = null;
    const frag = document.createDocumentFragment();
    cards.forEach((c) => frag.appendChild(c));
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
    card.setAttribute(
      'aria-label',
      `${label}, block ${index + 1} of ${total}. Enter to edit, Shift+Enter to add a paragraph below, Delete to remove, Tab for actions.`,
    );
    card.setAttribute('aria-keyshortcuts', 'Enter F2 Shift+Enter Delete');

    const head = document.createElement('div');
    head.className = 'te-block-head';
    const tag = document.createElement('span');
    tag.className = 'te-block-type';
    tag.textContent = block.type === 'heading' ? `H${block.level}` : BLOCK_LABELS[block.type];
    const actions = document.createElement('span');
    actions.className = 'te-block-actions';
    const action = (name, label, icon, keys) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'te-block-action';
      b.dataset.action = name;
      // Roving tabindex: only the focused card's actions are in the tab order.
      b.tabIndex = index === this.focusIndex ? 0 : -1;
      b.setAttribute('aria-label', label);
      b.setAttribute('aria-keyshortcuts', keys);
      b.title = label;
      const i = document.createElement('i');
      i.className = icon;
      i.setAttribute('aria-hidden', 'true');
      b.appendChild(i);
      actions.appendChild(b);
    };
    action('edit', 'Edit block (Enter)', 'fa-solid fa-pen', 'Enter F2');
    action('insert', 'Add paragraph below (Shift+Enter)', 'fa-solid fa-plus', 'Shift+Enter');
    action('delete', 'Delete block (Delete)', 'fa-regular fa-trash-can', 'Delete');
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
      for (const b of c.querySelectorAll('.te-block-action')) b.tabIndex = k === i ? 0 : -1;
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
    return BlocksView.indexIn(this.model, offset);
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

  onSurfaceChange(change) {
    const edit = change && change.edit;
    const whole = !!edit && BlocksView.replacesWhole(change);
    // Kept drafts follow every change, in either view.
    if (edit && this.drafts.length) this.mapDrafts(edit, whole);
    let overlapped = null;
    if (this.editing && edit && !this.mapEditing(edit, whole)) {
      overlapped = this.editing;
      this.editing = null;
    }
    const hadFocus = this.container.contains(document.activeElement);
    if (overlapped) this.keepDraft(overlapped, edit, whole);
    else if (edit && this.drafts.length) this.renderDrafts();
    if (!this.isActive()) return;
    if (this.applying) {
      this.dirty = true;
      return;
    }
    // An outside change (open, undo from elsewhere): rebuild around any open
    // editor, keeping focus in the list if it was there.
    this.render(hadFocus && !this.editing && !overlapped);
    if (overlapped && hadFocus) this.draftsElement.focus({ preventScroll: true });
  }

  /** Whether a surface change replaced the whole document. */
  static replacesWhole(change) {
    const e = change.edit;
    const oldLength = change.text.length - e.insertedText.length + e.deletedLength;
    return change.source === 'open' || (e.start === 0 && e.deletedLength === oldLength && oldLength > 0);
  }

  /**
   * Map the open editor's range through `edit`. Returns false when the edit
   * overlaps the block (or replaces the document), true when the editor can
   * stay open.
   */
  mapEditing(edit, whole) {
    if (whole) return false;
    const ed = this.editing;
    const a = edit.start;
    const b = a + edit.deletedLength;
    const s = ed.start;
    const e = s + ed.original.length;
    if (b <= s) {
      ed.start = s + edit.insertedText.length - edit.deletedLength;
      return true;
    }
    return a >= e;
  }

  mapDrafts(edit, whole) {
    const a = edit.start;
    const b = a + edit.deletedLength;
    const delta = edit.insertedText.length - edit.deletedLength;
    for (const d of this.drafts) {
      if (d.anchor === null) continue;
      if (whole) d.anchor = null;
      else if (b <= d.anchor) d.anchor += delta;
      else if (a < d.anchor) d.anchor = a;
    }
  }

  /**
   * Keep the draft of a closed editor in the notice. A draft identical to
   * what the block held (nothing typed) is not kept: nothing is lost.
   */
  keepDraft(ed, edit, whole) {
    const value = window.EditorSurface.normaliseNewlines(ed.textarea.value);
    if (ed.isNew ? value.trim() === '' : value === ed.original) return null;
    const draft = {
      id: this.nextDraftId++,
      value,
      // The block text when editing began; Apply replaces only this.
      originalText: ed.isNew ? '' : ed.original,
      isNew: !!ed.isNew,
      index: ed.index,
      // Where to re-apply: an offset in the body, or the block index when
      // the whole document was replaced.
      anchor: whole ? null : edit ? edit.start : ed.start,
      reason: whole ? 'replaced' : 'changed',
    };
    this.drafts.push(draft);
    this.renderDrafts();
    return draft;
  }

  renderDrafts() {
    const el = this.draftsElement;
    el.textContent = '';
    el.hidden = this.drafts.length === 0;
    for (const d of this.drafts) {
      const item = document.createElement('div');
      item.className = 'te-blocks-draft';
      item.dataset.draftId = String(d.id);
      const msg = document.createElement('p');
      msg.className = 'te-blocks-draft-message';
      const icon = document.createElement('i');
      icon.className = 'fa-solid fa-circle-info';
      icon.setAttribute('aria-hidden', 'true');
      msg.append(icon, document.createTextNode(d.reason === 'replaced'
        ? ' The document was replaced while you were editing a block. Your draft is kept here.'
        : ' The block changed while you were editing it. Your draft is kept here.'));
      const pre = document.createElement('pre');
      pre.className = 'te-blocks-draft-text';
      pre.textContent = d.value;
      const actions = document.createElement('div');
      actions.className = 'te-blocks-draft-actions';
      const button = (action, label) => {
        const b = document.createElement('button');
        b.type = 'button';
        b.className = `te-blocks-draft-${action}`;
        b.dataset.draftAction = action;
        b.dataset.draftId = String(d.id);
        b.textContent = label;
        actions.appendChild(b);
      };
      const target = this.draftTarget(d);
      button('apply', target.mode === 'replace' ? 'Replace block' : 'Insert as new paragraph');
      actions.querySelector('[data-draft-action="apply"]').dataset.draftMode = target.mode;
      button('discard', 'Discard');
      item.append(msg, pre, actions);
      el.appendChild(item);
    }
  }

  /** Kept drafts, oldest first (copies). */
  keptDrafts() {
    return this.drafts.map((d) => ({ ...d }));
  }

  /**
   * Every unapplied draft, read-only copies: the kept drafts (oldest first)
   * then the open editor's draft if it differs from the block's text (or is
   * a non-blank new paragraph). Each is { value, originalText, anchor,
   * index, isNew }; `anchor` is a body offset, or null after a
   * whole-document replacement (use `index`).
   */
  pendingDrafts() {
    const out = this.drafts.map((d) => ({
      value: d.value,
      originalText: d.originalText,
      anchor: d.anchor,
      index: d.index,
      isNew: d.isNew,
    }));
    const ed = this.editing;
    if (ed) {
      const value = window.EditorSurface.normaliseNewlines(ed.textarea.value);
      if (ed.isNew ? value.trim() !== '' : value !== ed.original) {
        out.push({ value, originalText: ed.isNew ? '' : ed.original, anchor: ed.start, index: ed.index, isNew: !!ed.isNew });
      }
    }
    return out;
  }

  /**
   * Where a kept draft would go now: { mode: 'replace', block } when the
   * block at the draft's position still holds exactly the draft's
   * originalText, otherwise { mode: 'insert', at } with the offset where it
   * would be inserted as a new paragraph (after the block at its position,
   * or at the block index after a whole-document replacement).
   */
  draftTarget(draft, text = this.surface.getText()) {
    const model = BlocksView.parse(text);
    const blocks = model.blocks;
    if (blocks.length === 0) return { mode: 'insert', at: text.length };
    if (draft.anchor === null) {
      const b = blocks[Math.min(draft.index, blocks.length - 1)];
      if (!draft.isNew && draft.index < blocks.length && b.text === draft.originalText) return { mode: 'replace', block: b };
      return { mode: 'insert', at: draft.index < blocks.length ? blocks[draft.index].start : blocks[blocks.length - 1].end };
    }
    const at = Math.min(draft.anchor, text.length);
    if (draft.isNew) {
      const inside = blocks.find((b) => b.start < at && at < b.end);
      return { mode: 'insert', at: inside ? inside.end : at };
    }
    const b = blocks[BlocksView.indexIn(model, at)];
    if (b.text === draft.originalText) return { mode: 'replace', block: b };
    return { mode: 'insert', at: b.end };
  }

  /**
   * Re-apply a kept draft (see draftTarget): replace the block if it still
   * holds the draft's original text, otherwise insert the draft as a new
   * paragraph. Never replaces unrelated text. One undo step. Returns
   * 'replace' or 'insert', or null when there is no such draft.
   */
  applyDraft(id) {
    const draft = this.drafts.find((d) => d.id === id);
    if (!draft) return null;
    if (this.editing) this.commitEdit({ rerender: false });
    this.drafts = this.drafts.filter((d) => d !== draft);
    this.renderDrafts();
    const target = this.draftTarget(draft);
    let caret;
    this.applying = true;
    try {
      if (target.mode === 'replace') {
        const b = target.block;
        const d = window.EditorSurface.diff(b.text, draft.value);
        caret = b.start;
        if (d) {
          this.surface.replaceRange(b.start + d.start, b.start + d.start + d.deletedLength, d.insertedText, {
            source: 'blocks',
            selectStart: b.start,
          });
        }
      } else {
        caret = this.insertParagraph(target.at, draft.value);
      }
    } finally {
      this.applying = false;
    }
    if (this.isActive()) {
      this.render(false);
      this.setCurrent(this.blockIndexAt(caret), true);
    }
    return target.mode;
  }

  /** Drop a kept draft. */
  discardDraft(id) {
    const before = this.drafts.length;
    this.drafts = this.drafts.filter((d) => d.id !== id);
    this.renderDrafts();
    if (this.isActive() && this.drafts.length === 0 && before) this.setCurrent(this.focusIndex, true);
    return this.drafts.length < before;
  }

  /**
   * Insert `value` as a paragraph at `at`, separated from its neighbours by
   * one blank line. Returns the offset where the paragraph starts.
   */
  insertParagraph(at, value) {
    const text = this.surface.getText();
    const before = text.slice(0, at);
    const after = text.slice(at);
    const pre = at === 0 || before.endsWith('\n\n') ? '' : before.endsWith('\n') ? '\n' : '\n\n';
    const post = after.trim() === '' || after.startsWith('\n\n') ? '' : after.startsWith('\n') ? '\n' : '\n\n';
    this.surface.replaceRange(at, at, pre + value + post, { source: 'blocks', selectStart: at + pre.length });
    return at + pre.length;
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
    this.editing = { ...edit, card, textarea: ta, selection: null };
    this.listen(ta, 'input', () => {
      ta.rows = Math.max(2, ta.value.split('\n').length + 1);
    });
    // While the textarea has focus its selection is the document selection,
    // which an outside surface edit moves; remember it so render() can put
    // it back.
    const remember = () => {
      if (this.editing && this.editing.textarea === ta && document.activeElement === ta) {
        this.editing.selection = [ta.selectionStart, ta.selectionEnd, ta.selectionDirection];
      }
    };
    this.listen(document, 'selectionchange', remember);
    this.listen(ta, 'select', remember);
    this.listen(ta, 'keyup', remember);
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
      const text = this.surface.getText();
      const stale = edit.isNew
        ? edit.start > text.length
        : text.slice(edit.start, edit.start + edit.original.length) !== edit.original;
      if (stale) {
        // The block is no longer where the editor thought: keep the draft.
        this.keepDraft(edit, null, false);
      } else if (edit.isNew) {
        if (value.trim() !== '') {
          base = this.insertParagraph(edit.start, value);
          this.surface.setSelectionOffsets(base + selStart, base + selEnd);
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
    return this.openEditor(card, { index: index + 1, start: at, original: '', isNew: true });
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
    if (this.editing) this.commitEdit({ rerender: false });
    this.applying = true;
    try {
      this.surface.replaceRange(from, to, insert, { source: 'blocks', selectStart: from });
    } finally {
      this.applying = false;
    }
    this.render(false);
  }

  undo() {
    if (this.editing) this.commitEdit({ rerender: false });
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
    if (this.editing) this.commitEdit({ rerender: false });
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
    if (card && e.key === 'Escape' && e.target.classList && e.target.classList.contains('te-block-action')) {
      e.preventDefault();
      card.focus();
      return;
    }
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
        e.preventDefault();
        if (e.shiftKey) this.insertBlockAfter(index);
        else this.editBlock(index);
        break;
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

  /**
   * Remove the view's DOM and subscription and show the surface again. The
   * document is never changed here: pending drafts (see pendingDrafts) are
   * reported in one bubbling `te:blocks-drafts` event from the surface root,
   * dispatched before the DOM goes, and returned. No event when there are
   * none; the return value is then []. A second call returns [].
   */
  destroy() {
    if (this.destroyed) return [];
    const drafts = this.pendingDrafts();
    if (drafts.length && this.surface.root) {
      this.surface.root.dispatchEvent(new CustomEvent('te:blocks-drafts', {
        bubbles: true,
        detail: { editor: this.editor, drafts: drafts.map((d) => ({ ...d })) },
      }));
    }
    this.destroyed = true;
    this.editing = null;
    this.offChange();
    if (this.surface.root) this.surface.root.hidden = false;
    this.container.remove();
    this.draftsElement.remove();
    if (this.toggleGroup) this.toggleGroup.remove();
    if (this.toggleDivider) this.toggleDivider.remove();
    this.drafts = [];
    return drafts;
  }
}

window.BlocksView = BlocksView;
window.BLOCKS_VIEW_STORAGE_KEY = BLOCKS_VIEW_STORAGE_KEY;
