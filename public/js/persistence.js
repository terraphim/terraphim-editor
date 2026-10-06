/*
 * Save, open, autosave drafts and the Markdown export view (issues #76, #73)
 * ==========================================================================
 *
 * MarkdownEditor creates one TePersistence in initialize() (after the
 * chrome and the Overflow panel) and passes its AbortController signal;
 * every listener uses that signal and MarkdownEditor.destroy() calls
 * destroy(), which also removes the DOM created here, revokes object URLs
 * and strips the dirty prefix from document.title. Design notes:
 * docs/design/persistence.md.
 *
 * Entry points (the Write_On corner controls, the plain toolbar's File
 * group and the keyboard all go through these; the keys act anywhere on the
 * page when `config.standalone` is true, as on index.html, and otherwise
 * only while focus is in the editor, its chrome, panels or dialogs):
 *
 *   save()          Ctrl+S / Cmd+S, floppy   dispatches cancelable te:save
 *   open()          Ctrl+O / Cmd+O, folder   dispatches cancelable te:open
 *   showMarkdown()  M↓, toolbar Markdown     dispatches cancelable te:markdown
 *
 * Each event bubbles from the chrome root (or the surface when there is no
 * chrome) with `detail.editor`; a host page that stores documents itself
 * calls preventDefault() and the default action below does not run.
 *
 * Save (default action of te:save). editor.saveDocument() serialises the
 * body plus the annotation block (and dispatches te:saved {text}); the text
 * is then written:
 *   - with the File System Access API (window.showSaveFilePicker, Chromium):
 *     the first save asks for a file (suggested name from the document key,
 *     else the first heading, else untitled.md); later saves write to the
 *     same handle without asking. Opening a file through the picker, or
 *     dropping one in Chromium, keeps its handle, so saving writes back.
 *   - otherwise (Firefox, Safari, or config.fileSystemAccess === false): a
 *     download of the .md through a Blob and an <a download>.
 * te:written {name, method: 'file' | 'download'} follows a completed write.
 * save() returns the saved text synchronously (null when cancelled or when
 * the document model is unavailable); `lastWrite` is the write's promise,
 * resolving to true when the text was written.
 *
 * Open (default action of te:open). window.showOpenFilePicker (accepting
 * .md, .markdown and .txt) or, without it, a hidden <input type=file>. A
 * Markdown or text file dropped anywhere on the editor (the surface, its
 * container, the Write_On page or the panels) opens too; only drops that
 * carry Files are handled, so text drags (the page, the Overflow panel) are
 * untouched. The file is read first; if the document has unsaved changes a
 * dialog then offers Save first / Discard changes / Cancel (Escape cancels).
 * The text goes through editor.openDocument(text, name), so alternatives,
 * ghosts and overflow are restored; the file name becomes the document key.
 * te:opened {name, text} follows. A host that calls editor.openDocument()
 * itself gets the same bookkeeping (documentChanged(): clean state, drafts
 * re-keyed, no file handle); a host that cancels te:save and stores the
 * text itself calls editor.persistence.markClean() once it is stored.
 *
 * Dirty state. An edit marks the document dirty at once (a dot on the save
 * controls, "• " before document.title, te:dirty-change {dirty}); after
 * `config.autosaveDelay` ms (default 1000) without edits the serialised
 * document is compared with the last saved or opened one, which also
 * catches annotation-only changes (an alternative added, a ghost, Overflow
 * typing) and clears the flag when an edit is undone.
 *
 * Drafts. On that same tick, on save and on pagehide, the serialised
 * document is kept in localStorage under
 *   terraphim-editor:draft:<draft id>
 * as JSON {v: 1, text, savedAt, name}. The draft id is `fs:<id>` for a file
 * handle (an id kept with the handle in IndexedDB), `file:<name>|<size>|
 * <lastModified>` for a file opened without a handle, and the document key
 * otherwise, so two files with the same name never share a draft. When an editor starts, or a file is
 * opened, and a draft for its key differs from the opened text (and, for a
 * file, is newer than the file's lastModified), a non-blocking notice offers
 * Restore / Discard. While the notice is up the stored draft is only
 * replaced if the new document is changed; Restore uses the copy read when
 * the notice was shown. localStorage access is wrapped in try/catch.
 *
 * Markdown export (default action of te:markdown, issue #73). A dialog with
 * editor.exportDocument() (active alternatives, no ghosted text, no
 * overflow, no annotation block) in a read-only text area, with Copy (the
 * Clipboard API, falling back to selecting the text and execCommand) and
 * Download .md (written like a save, but to a new file each time, named
 * <name>-export.md). Escape closes it and focus returns where it was.
 */

const TE_DRAFT_PREFIX = 'terraphim-editor:draft:';
const TE_DIRTY_TITLE_PREFIX = '• ';
const TE_OPEN_EXTENSIONS = ['.md', '.markdown', '.txt'];
const TE_PICKER_TYPES = [
  {
    description: 'Markdown',
    accept: { 'text/markdown': ['.md', '.markdown'], 'text/plain': ['.txt'] },
  },
];

/** Whether a File (or { name, type }) is a Markdown or text file we open. */
function teIsOpenableFile(file) {
  if (!file) return false;
  const name = String(file.name || '').toLowerCase();
  if (TE_OPEN_EXTENSIONS.some((ext) => name.endsWith(ext))) return true;
  const type = String(file.type || '');
  return type === 'text/markdown' || type === 'text/plain' || type === 'text/x-markdown';
}

/**
 * The file name suggested for a save: the document key when it is a name
 * (not the `h:` content hash the chrome falls back to), with `.md` added
 * unless it already ends in .md, .markdown or .txt; otherwise a slug of the
 * first ATX heading in `text`; otherwise `untitled.md`. Characters that are
 * not allowed in file names are replaced with `-`.
 */
function teSuggestedFileName(key, text) {
  const clean = (s) => s.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '-').trim();
  if (typeof key === 'string' && key && !key.startsWith('h:')) {
    const base = clean(key.split(/[\\/]/).pop() || '');
    if (base && base !== '.' && base !== '..') {
      const lower = base.toLowerCase();
      return TE_OPEN_EXTENSIONS.some((ext) => lower.endsWith(ext)) ? base : `${base}.md`;
    }
  }
  const heading = /^ {0,3}#{1,6}[ \t]+(.+?)[ \t]*#*[ \t]*$/m.exec(String(text || ''));
  if (heading) {
    const slug = heading[1]
      .toLowerCase()
      .replace(/[^\p{L}\p{N}]+/gu, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 60)
      .replace(/-+$/, '');
    if (slug) return `${slug}.md`;
  }
  return 'untitled.md';
}

/** `notes.md` -> `notes-export.md` (the name for a Markdown export). */
function teExportFileName(name) {
  const m = /^(.*?)(\.(md|markdown|txt))?$/i.exec(name || 'untitled.md');
  return `${m[1] || 'untitled'}-export.md`;
}

/** FNV-1a 32-bit hash, as in chrome.js (used when the chrome is absent). */
function teDraftHash(text) {
  if (typeof window.writeOnHash === 'function') return window.writeOnHash(text);
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, '0');
}

/*
 * File handle ids for draft keys: handles are structured-cloneable, so each
 * one is kept in IndexedDB (database terraphim-editor-files, store handles)
 * with a random id, and found again with isSameEntry(). Rejects when
 * IndexedDB is unavailable; the caller then falls back to name, size and
 * modification time.
 */
let teHandleDb = null;

function teIdb(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function teOpenHandleDb() {
  if (!teHandleDb) {
    const req = window.indexedDB.open('terraphim-editor-files', 1);
    req.onupgradeneeded = () => req.result.createObjectStore('handles', { keyPath: 'id' });
    teHandleDb = teIdb(req).catch((err) => {
      teHandleDb = null;
      throw err;
    });
  }
  return teHandleDb;
}

async function teHandleId(handle) {
  if (!window.indexedDB || !handle || typeof handle.isSameEntry !== 'function') throw new Error('no handle store');
  const db = await teOpenHandleDb();
  const all = await teIdb(db.transaction('handles').objectStore('handles').getAll());
  for (const rec of all) {
    try {
      if (rec.handle && (await rec.handle.isSameEntry(handle))) return rec.id;
    } catch (err) {
      // A stale or foreign entry: skip it.
    }
  }
  const id = window.crypto && typeof window.crypto.randomUUID === 'function'
    ? window.crypto.randomUUID()
    : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  await teIdb(db.transaction('handles', 'readwrite').objectStore('handles').put({ id, handle }));
  return id;
}

class TePersistence {
  constructor(editor, options = {}) {
    this.editor = editor;
    this.signal = options.signal;
    const config = editor.config || {};
    this.config = config;
    this.fsAccess = config.fileSystemAccess !== false &&
      typeof window.showSaveFilePicker === 'function' &&
      typeof window.showOpenFilePicker === 'function';
    this.autosaveDelay = Number.isFinite(config.autosaveDelay) ? Math.max(0, config.autosaveDelay) : 1000;
    this.destroyed = false;
    this.handle = null;
    this.fileName = null;
    // Bumped whenever a different document is opened (load, a host's
    // openDocument); late save and open completions check it so they never
    // touch the document that replaced theirs.
    this.generation = 0;
    this.openSeq = 0;
    // Where drafts are stored (see draftIdentity()); null while a file
    // handle's id is being looked up.
    this.draftId = null;
    this.identityReady = Promise.resolve();
    // Ctrl/Cmd+S and Ctrl/Cmd+O everywhere on the page only when the editor
    // is the page (index.html sets it); embedded, only with focus inside.
    this.standalone = config.standalone === true;
    this.dirty = false;
    this.timer = null;
    this.quiet = 0;
    this.pendingDraft = null;
    this.lastWrite = Promise.resolve(false);
    this.objectUrls = new Set();
    this.nodes = [];
    this.dialogs = [];

    this.build();
    this.offChange = editor.surface ? editor.surface.onChange(() => this.onEdit()) : () => {};
    this.listen();

    const text = editor.surface ? editor.surface.getText() : '';
    this.key = this.initialKey(text);
    this.draftId = this.key;
    this.cleanText = this.serialise();
    this.checkDraft(null);
  }

  // ---------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------

  on(el, type, fn, opts = {}) {
    el.addEventListener(type, fn, Object.assign({ signal: this.signal }, opts));
  }

  modelAvailable() {
    return typeof window.documentModelAvailable === 'function'
      ? window.documentModelAvailable(this.editor)
      : !!(this.editor.documentApi && this.editor.documentApi());
  }

  /** The document as saved (body plus block), without dispatching te:saved. */
  serialise() {
    if (!this.modelAvailable()) return null;
    try {
      const api = this.editor.documentApi();
      this.editor.alignDocumentModel(api);
      return api.save_document();
    } catch (err) {
      console.error('Serialising the document failed', err);
      return null;
    }
  }

  initialKey(text) {
    const ed = this.editor;
    if (typeof ed.documentKey === 'string' && ed.documentKey) return ed.documentKey;
    if (typeof this.config.documentKey === 'string' && this.config.documentKey) return this.config.documentKey;
    if (ed.chrome && typeof ed.chrome.key === 'string' && ed.chrome.key) return ed.chrome.key;
    return `h:${teDraftHash(text)}`;
  }

  /** Dispatch a bubbling te:* event from the chrome root, or the surface. */
  emit(type, detail = {}, cancelable = false) {
    const chrome = this.editor.chrome;
    if (chrome && !chrome.destroyed && typeof chrome.emit === 'function') {
      return chrome.emit(type, detail, cancelable);
    }
    const ev = new CustomEvent(type, {
      bubbles: true,
      cancelable,
      detail: Object.assign({ editor: this.editor }, detail),
    });
    (this.editor.input || document).dispatchEvent(ev);
    return ev;
  }

  suggestedName() {
    if (this.fileName) return this.fileName;
    const ed = this.editor;
    const key = (typeof ed.documentKey === 'string' && ed.documentKey) || this.config.documentKey || this.key;
    return teSuggestedFileName(key, ed.surface ? ed.surface.getText() : '');
  }

  // ---------------------------------------------------------------------
  // DOM: notice, dialogs, hidden file input, plain toolbar buttons
  // ---------------------------------------------------------------------

  build() {
    const notice = document.createElement('div');
    notice.className = 'te-files-notice';
    notice.setAttribute('role', 'region');
    notice.setAttribute('aria-label', 'Document notice');
    notice.hidden = true;
    const text = document.createElement('span');
    text.className = 'te-files-notice-text';
    text.setAttribute('aria-live', 'polite');
    const actions = document.createElement('span');
    actions.className = 'te-files-notice-actions';
    notice.append(text, actions);
    document.body.appendChild(notice);
    this.nodes.push(notice);
    this.notice = notice;
    this.noticeText = text;
    this.noticeActions = actions;

    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.md,.markdown,.txt,text/markdown,text/plain';
    input.className = 'te-files-input';
    input.hidden = true;
    input.tabIndex = -1;
    input.setAttribute('aria-hidden', 'true');
    this.on(input, 'change', () => {
      const file = input.files && input.files[0];
      input.value = '';
      if (file) this.lastOpen = this.openFile(file, null);
    });
    document.body.appendChild(input);
    this.nodes.push(input);
    this.fileInput = input;

    this.buildToolbar();
  }

  /** Open, Save and Markdown buttons at the start of the plain toolbar. */
  buildToolbar() {
    const container = this.editor.input && this.editor.input.closest('.editor-container');
    const toolbar = container && container.querySelector('.toolbar');
    if (!toolbar) return;
    const group = document.createElement('sl-button-group');
    group.setAttribute('label', 'File');
    group.className = 'te-files-toolbar';
    const button = (name, icon, label, tip, onClick) => {
      const tooltip = document.createElement('sl-tooltip');
      tooltip.setAttribute('content', tip);
      const b = document.createElement('sl-button');
      b.setAttribute('size', 'small');
      b.dataset.teFile = name;
      const i = document.createElement('i');
      i.className = icon;
      i.setAttribute('aria-hidden', 'true');
      const span = document.createElement('span');
      span.className = 'te-files-sr';
      span.textContent = label;
      b.append(i, span);
      this.on(b, 'click', onClick);
      tooltip.appendChild(b);
      group.appendChild(tooltip);
      return b;
    };
    button('open', 'fa-regular fa-folder-open', 'Open document', 'Open (Ctrl+O)', () => this.open());
    this.toolbarSave = button('save', 'fa-solid fa-floppy-disk', 'Save document', 'Save (Ctrl+S)', () => this.save());
    button('markdown', 'fa-brands fa-markdown', 'Markdown export', 'Markdown export', () => this.showMarkdown());
    const divider = document.createElement('sl-divider');
    divider.setAttribute('vertical', '');
    divider.className = 'te-files-toolbar-divider';
    toolbar.insertBefore(divider, toolbar.firstChild);
    toolbar.insertBefore(group, divider);
    this.nodes.push(group, divider);
  }

  /**
   * Show the notice with `message` and `actions` ([{ label, onClick }]); a
   * null message hides it.
   */
  showNotice(message, actions = [], kind = 'info') {
    if (this.destroyed) return;
    this.noticeActions.replaceChildren();
    if (!message) {
      this.notice.hidden = true;
      this.noticeText.textContent = '';
      delete this.notice.dataset.kind;
      return;
    }
    this.notice.dataset.kind = kind;
    this.noticeText.textContent = message;
    const all = actions.length ? actions : [{ label: 'Dismiss', onClick: () => this.showNotice(null) }];
    for (const action of all) {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'te-files-notice-button';
      b.dataset.action = action.name || action.label.toLowerCase();
      b.textContent = action.label;
      this.on(b, 'click', action.onClick);
      this.noticeActions.appendChild(b);
    }
    this.notice.hidden = false;
  }

  /** A native modal <dialog> owned by this module; removed on destroy(). */
  makeDialog(className, titleText) {
    const dialog = document.createElement('dialog');
    dialog.className = `te-files-dialog ${className}`;
    const id = `${className}-title`;
    dialog.setAttribute('aria-labelledby', id);
    const header = document.createElement('div');
    header.className = 'te-files-dialog-header';
    const title = document.createElement('h2');
    title.id = id;
    title.textContent = titleText;
    header.appendChild(title);
    dialog.appendChild(header);
    document.body.appendChild(dialog);
    this.nodes.push(dialog);
    this.dialogs.push(dialog);
    // A click on the backdrop (outside the box) closes it.
    this.on(dialog, 'click', (e) => {
      if (e.target === dialog) dialog.close();
    });
    return { dialog, header };
  }

  openDialog(dialog, focusEl) {
    const back = document.activeElement;
    this.on(dialog, 'close', () => {
      if (back && back.isConnected && typeof back.focus === 'function') back.focus();
    }, { once: true });
    if (typeof dialog.showModal === 'function') dialog.showModal();
    else dialog.setAttribute('open', '');
    if (focusEl) focusEl.focus();
  }

  dialogButton(parent, label, opts = {}) {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = `te-files-button ${opts.className || ''}`.trim();
    if (opts.name) b.dataset.action = opts.name;
    if (opts.icon) {
      const i = document.createElement('i');
      i.className = opts.icon;
      i.setAttribute('aria-hidden', 'true');
      b.appendChild(i);
    }
    const span = document.createElement('span');
    span.textContent = label;
    b.appendChild(span);
    if (opts.ariaLabel) b.setAttribute('aria-label', opts.ariaLabel);
    if (opts.onClick) this.on(b, 'click', opts.onClick);
    parent.appendChild(b);
    return b;
  }

  // ---------------------------------------------------------------------
  // Listeners
  // ---------------------------------------------------------------------

  listen() {
    // Ctrl+S / Cmd+S saves and Ctrl+O / Cmd+O opens, and the browser's own
    // Save page and Open file are prevented: anywhere on a standalone page,
    // but in an embedding page only while focus is in the editor, its chrome,
    // panels or dialogs, so the host's own shortcuts keep working.
    this.on(document, 'keydown', (e) => {
      if (this.destroyed || e.defaultPrevented || e.isComposing) return;
      if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
      if (!this.standalone && !this.ownsFocus(e.target)) return;
      const key = (e.key || '').toLowerCase();
      if (key === 's') {
        e.preventDefault();
        this.save();
      } else if (key === 'o') {
        e.preventDefault();
        this.open();
      }
    });
    // Annotation-only changes (panel typing, Overflow) reach the model
    // without a surface edit; check on the debounced tick.
    const nudge = () => this.schedule();
    this.on(document, 'input', nudge);
    this.on(document, 'keyup', nudge);
    this.on(document, 'pointerup', nudge);
    // Keep the latest draft when the page goes away.
    this.on(window, 'pagehide', () => this.flushDraft());
    // Files dropped on the editor open; text drags are left alone.
    this.on(document, 'dragover', (e) => {
      if (!this.isFileDrag(e)) return;
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
    });
    this.on(document, 'drop', (e) => this.onDrop(e));
  }

  /** Whether `target` is inside the editor, its chrome, panels or dialogs. */
  ownsFocus(target) {
    if (!(target instanceof Node)) return false;
    const el = target.nodeType === 1 ? target : target.parentElement;
    if (!el) return false;
    const container = this.editor.input && (this.editor.input.closest('.editor-container') || this.editor.input);
    if (container && container.contains(el)) return true;
    return !!el.closest(
      '.te-chrome, .te-overflow, .te-alt-panel, .te-blocks, .te-selection-menu, .te-files-dialog, .te-files-notice',
    );
  }

  isFileDrag(e) {
    if (this.destroyed || !e.dataTransfer) return false;
    const types = Array.from(e.dataTransfer.types || []);
    if (!types.includes('Files')) return false;
    return this.inScope(e.target);
  }

  /** Drops on the editor, its chrome and panels, or the bare page. */
  inScope(target) {
    if (!(target instanceof Node)) return false;
    if (target === document.body || target === document.documentElement || target === document) return true;
    const el = target.nodeType === 1 ? target : target.parentElement;
    if (!el) return false;
    const container = this.editor.input && (this.editor.input.closest('.editor-container') || this.editor.input);
    if (container && container.contains(el)) return true;
    return !!el.closest('.te-chrome, .te-overflow, .te-alt-panel, .te-blocks, .te-files-notice');
  }

  onDrop(e) {
    if (!this.isFileDrag(e)) return;
    e.preventDefault();
    const dt = e.dataTransfer;
    const files = Array.from(dt.files || []);
    const index = files.findIndex(teIsOpenableFile);
    if (index < 0) {
      this.showNotice(
        files.length ? `${files[0].name} is not a Markdown or text file.` : 'Nothing to open in that drop.',
        [],
        'error',
      );
      return;
    }
    const file = files[index];
    // In Chromium a dropped file can give a writable handle, so the next
    // save writes back to it; it must be requested during the drop event.
    let handlePromise = null;
    if (this.fsAccess && dt.items) {
      const items = Array.from(dt.items).filter((it) => it.kind === 'file');
      const item = items[index];
      if (item && typeof item.getAsFileSystemHandle === 'function') {
        try {
          handlePromise = item.getAsFileSystemHandle();
        } catch (err) {
          handlePromise = null;
        }
      }
    }
    const go = async () => {
      let handle = null;
      if (handlePromise) {
        try {
          const h = await handlePromise;
          if (h && h.kind === 'file') handle = h;
        } catch (err) {
          handle = null;
        }
      }
      return this.openFile(file, handle);
    };
    this.lastOpen = go();
  }

  // ---------------------------------------------------------------------
  // Dirty state and drafts
  // ---------------------------------------------------------------------

  onEdit() {
    if (this.destroyed || this.quiet > 0 || !this.modelAvailable()) return;
    this.setDirty(true);
    this.schedule();
  }

  schedule() {
    if (this.destroyed || this.quiet > 0 || !this.modelAvailable()) return;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      this.tick();
    }, this.autosaveDelay);
  }

  /** Compare with the clean text, update the flag and keep the draft. */
  tick() {
    if (this.destroyed) return;
    const text = this.serialise();
    if (text === null) return;
    const dirty = text !== this.cleanText;
    this.setDirty(dirty);
    // While a restore notice is up, only a changed document replaces the
    // stored draft (Restore uses the copy read when the notice appeared).
    if (this.pendingDraft && !dirty) return;
    this.writeDraft(text);
  }

  /** Run a pending tick now (pagehide, and before a dirty check). */
  flushDraft() {
    if (this.timer === null) return;
    clearTimeout(this.timer);
    this.timer = null;
    this.tick();
  }

  isDirty() {
    this.flushDraft();
    return this.dirty;
  }

  setDirty(dirty) {
    if (this.destroyed) return;
    const changed = dirty !== this.dirty;
    this.dirty = dirty;
    // Per keystroke this is only the flag: the DOM changes with it.
    if (!changed) return;
    this.applyTitle(dirty);
    const control = this.editor.chrome && this.editor.chrome.root
      ? this.editor.chrome.root.querySelector('[data-control="save"]')
      : null;
    for (const el of [control, this.toolbarSave]) {
      if (!el) continue;
      el.classList.toggle('te-dirty', dirty);
      if (el === control) el.setAttribute('aria-label', dirty ? 'Save document (unsaved changes)' : 'Save document');
    }
    this.emit('te:dirty-change', { dirty });
  }

  /**
   * Record the current document as saved: clears the dirty state and keeps
   * a draft. For hosts that cancel te:save and store the text themselves.
   */
  markClean() {
    if (this.destroyed) return;
    this.cancelTick();
    const text = this.serialise();
    if (text === null) return;
    this.cleanText = text;
    this.setDirty(false);
    this.writeDraft(text);
  }

  /**
   * editor.openDocument() was called by someone other than this module (a
   * host page loading its own document): take the opened text as clean,
   * re-key drafts to the new document key and forget the file handle.
   * load() and restoreDraft() open quietly and do their own bookkeeping.
   */
  documentChanged(name) {
    if (this.destroyed || this.quiet > 0) return;
    this.generation += 1;
    this.cancelTick();
    this.handle = null;
    this.fileName = typeof name === 'string' && name ? name : null;
    const key = this.editor.documentKey;
    if (typeof key === 'string' && key) this.key = key;
    this.draftId = this.key;
    this.identityReady = Promise.resolve();
    this.cleanText = this.serialise();
    this.setDirty(false);
    this.checkDraft(null);
  }

  /**
   * Prefix document.title while dirty. The title before marking is kept and
   * put back when clean, unless the page changed the title meanwhile (the
   * getter collapses whitespace, so string surgery on it is unreliable).
   */
  applyTitle(dirty) {
    if (this.config.dirtyTitle === false) return;
    if (dirty) {
      if (this.markedTitle !== undefined) return;
      this.baseTitle = document.title;
      document.title = TE_DIRTY_TITLE_PREFIX + this.baseTitle;
      this.markedTitle = document.title;
    } else if (this.markedTitle !== undefined) {
      if (document.title === this.markedTitle) document.title = this.baseTitle;
      this.markedTitle = undefined;
    }
  }

  draftKey(key = this.draftId) {
    return TE_DRAFT_PREFIX + key;
  }

  readDraft(key = this.draftId) {
    if (key === null) return null;
    try {
      const raw = window.localStorage.getItem(this.draftKey(key));
      if (!raw) return null;
      const d = JSON.parse(raw);
      return d && typeof d.text === 'string' ? d : null;
    } catch (err) {
      return null;
    }
  }

  writeDraft(text) {
    if (this.draftId === null) return; // the file's identity is still being looked up
    try {
      window.localStorage.setItem(this.draftKey(), JSON.stringify({
        v: 1, text, savedAt: Date.now(), name: this.fileName,
      }));
    } catch (err) {
      // Storage unavailable or full: saving to a file still works.
    }
  }

  removeDraft(key = this.draftId) {
    if (key === null) return;
    try {
      window.localStorage.removeItem(this.draftKey(key));
    } catch (err) {
      // Nothing to do.
    }
  }

  /**
   * Offer to restore the stored draft for the current key when it differs
   * from the opened document and, for a file, is newer than it.
   */
  checkDraft(file) {
    this.pendingDraft = null;
    const draft = this.readDraft();
    if (!draft || this.cleanText === null || draft.text === this.cleanText) {
      this.showNotice(null);
      return;
    }
    if (file && Number.isFinite(file.lastModified) && Number(draft.savedAt) <= file.lastModified) {
      this.showNotice(null);
      return;
    }
    this.pendingDraft = draft;
    const when = Number.isFinite(Number(draft.savedAt)) ? new Date(Number(draft.savedAt)) : null;
    const stamp = when
      ? when.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
      : 'an earlier session';
    const whose = draft.name ? draft.name : 'this document';
    this.showNotice(`Unsaved draft of ${whose} from ${stamp} found.`, [
      { name: 'restore', label: 'Restore', onClick: () => this.restoreDraft() },
      { name: 'discard', label: 'Discard', onClick: () => this.discardDraft() },
    ], 'draft');
  }

  restoreDraft() {
    const draft = this.pendingDraft;
    if (!draft || this.destroyed) return false;
    this.pendingDraft = null;
    this.showNotice(null);
    this.quiet += 1;
    try {
      // Keep the key the draft was stored under (a name or the hash).
      this.editor.openDocument(draft.text, this.key);
    } finally {
      this.quiet -= 1;
    }
    this.cancelTick();
    const text = this.serialise();
    this.setDirty(text !== this.cleanText);
    if (text !== null) this.writeDraft(text);
    if (this.editor.surface) this.editor.surface.focus();
    this.emit('te:draft-restored', { text: draft.text });
    return true;
  }

  discardDraft() {
    this.pendingDraft = null;
    this.showNotice(null);
    this.removeDraft();
    // Keep a draft of current unsaved work, if any.
    if (this.dirty) this.tick();
    if (this.editor.surface) this.editor.surface.focus();
  }

  cancelTick() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }

  /** Re-key to `name` after a save-as, carrying the Write_On mode with it. */
  rekey(name) {
    if (!name || name === this.key) return;
    this.key = name;
    this.editor.documentKey = name;
    const chrome = this.editor.chrome;
    if (chrome && !chrome.destroyed && typeof chrome.setMode === 'function') {
      chrome.key = name;
      chrome.setMode(chrome.mode);
    }
  }

  /** Move the stored draft from one draft id to another. */
  moveDraft(from, to) {
    if (from === null || to === null || from === to) return;
    const draft = this.readDraft(from);
    if (draft) {
      try {
        window.localStorage.setItem(this.draftKey(to), JSON.stringify(Object.assign(draft, { name: this.fileName })));
      } catch (err) {
        // Storage unavailable: nothing to move.
      }
    }
    this.removeDraft(from);
  }

  /**
   * Where a document's drafts are stored, so two files with the same name
   * never share one:
   *   - a File System Access handle: `fs:<id>`, an id kept with the handle
   *     in IndexedDB and found again with isSameEntry();
   *   - a file without a handle (file input, drop outside Chromium):
   *     `file:<name>|<size>|<lastModified>` as read at open;
   *   - otherwise (unnamed documents, a host's openDocument, downloads):
   *     the document key, as before.
   * Sets `draftId` (null until a handle's id is known; drafts are not
   * written meanwhile) and returns `identityReady`, which resolves once it
   * is set. Only applies if no other document was opened in the meantime.
   */
  setDraftIdentity({ handle = null, name = null, size = null, lastModified = null } = {}, onReady = null) {
    const gen = this.generation;
    const fileKey = name && Number.isFinite(lastModified)
      ? `file:${name}|${Number.isFinite(size) ? size : ''}|${lastModified}`
      : null;
    const fallback = fileKey || this.key;
    if (!handle) {
      this.draftId = fallback;
      this.identityReady = Promise.resolve();
      if (onReady) onReady();
      return this.identityReady;
    }
    this.draftId = null;
    this.identityReady = teHandleId(handle).then((id) => `fs:${id}`, () => fallback).then((key) => {
      if (this.destroyed || gen !== this.generation) return;
      this.draftId = key;
      if (onReady) onReady();
      if (this.dirty) this.tick();
    });
    return this.identityReady;
  }

  // ---------------------------------------------------------------------
  // Save
  // ---------------------------------------------------------------------

  /**
   * Dispatch te:save; unless cancelled, serialise with editor.saveDocument()
   * and write the text (see the header). Returns the text, or null.
   */
  save() {
    if (this.destroyed) return null;
    const available = this.modelAvailable();
    const ev = this.emit('te:save', { available }, true);
    if (ev.defaultPrevented || !available) return null;
    let text;
    try {
      text = this.editor.saveDocument();
    } catch (err) {
      console.error('saveDocument failed', err);
      return null;
    }
    this.cancelTick();
    this.writeDraft(text);
    // The document this save belongs to: if another is opened before the
    // write completes, the write still finishes (to the old file) but the
    // new document's handle, key and clean state are left alone.
    const gen = this.generation;
    const write = this.writeFile(text, this.suggestedName(), { keepHandle: true, generation: gen });
    this.lastWrite = write.then((result) => {
      if (!result || this.destroyed) return false;
      if (gen !== this.generation) {
        this.emit('te:written', { name: result.name, method: result.method, text, stale: true });
        return true;
      }
      if (result.keepDirty) {
        // Only a copy was written (the document's own file refused
        // permission): the document stays dirty, the notice says why.
        this.emit('te:written', { name: result.name, method: result.method, text, copy: true });
        return true;
      }
      this.cleanText = text;
      const now = this.serialise();
      this.setDirty(now !== null && now !== text);
      this.emit('te:written', { name: result.name, method: result.method, text });
      return true;
    });
    return text;
  }

  /**
   * Write `text`. Resolves to { name, method } or null (cancelled or
   * failed). With `keepHandle`, the first picker result is kept and reused.
   * The picker is requested synchronously so the click or key press that
   * started the save still counts as user activation.
   *
   * A kept handle may be read-only: handles from showOpenFilePicker() and
   * dropped files are, until the user grants write access. Before writing,
   * its permission is queried and, unless granted, requested (in the same
   * task chain as the click or key press, within its activation). If it is
   * refused, the text is saved elsewhere and nothing is lost: a save-as
   * picker (the chosen file becomes the document, which is then clean), or,
   * if the picker is refused too, a download of a copy, which leaves the
   * document dirty (`keepDirty`) because its own file is unchanged. A
   * cancelled picker writes nothing. The notice tells the user each time.
   */
  writeFile(text, name, { keepHandle = false, generation = this.generation } = {}) {
    if (!this.fsAccess) return Promise.resolve(this.download(text, name));
    const pick = () => {
      try {
        return window.showSaveFilePicker({ suggestedName: name, types: TE_PICKER_TYPES });
      } catch (err) {
        return Promise.reject(err);
      }
    };
    let handlePromise;
    let refused = null; // the kept file that refused write permission
    if (keepHandle && this.handle) {
      const kept = this.handle;
      handlePromise = this.ensureWritable(kept).then((ok) => {
        if (ok) return kept;
        refused = kept.name;
        return pick();
      });
    } else {
      handlePromise = pick();
    }
    return handlePromise.then(
      async (handle) => {
        try {
          const writable = await handle.createWritable();
          await writable.write(text);
          await writable.close();
        } catch (err) {
          console.error('Writing the file failed', err);
          this.showNotice(`Could not write ${handle.name}: ${err.message || err.name}.`, [], 'error');
          return null;
        }
        if (keepHandle && handle !== this.handle) {
          // The written file's size and time: the draft id falls back to
          // them when the handle cannot be kept in IndexedDB, so two files
          // with the same name still get different ids.
          let meta = {};
          try {
            const written = await handle.getFile();
            meta = { name: written.name, size: written.size, lastModified: written.lastModified };
          } catch (err) {
            meta = { name: handle.name };
          }
          if (generation === this.generation && !this.destroyed && handle !== this.handle) {
            // A first save-as: the chosen file becomes the document. Its
            // draft id is chosen once here and kept for the session (later
            // saves change the file's size and time, not the id).
            const oldDraft = this.draftId;
            this.handle = handle;
            this.fileName = handle.name;
            this.rekey(handle.name);
            this.setDraftIdentity(Object.assign({ handle }, meta), () => this.moveDraft(oldDraft, this.draftId));
          }
        }
        if (refused) this.showNotice(`No permission to write ${refused}: saved as ${handle.name} instead.`, [], 'error');
        return { name: handle.name, method: 'file' };
      },
      (err) => {
        if (err && err.name === 'AbortError') {
          // The user cancelled the picker: nothing written, still dirty.
          if (refused) this.showNotice(`No permission to write ${refused}: nothing was saved.`, [], 'error');
          return null;
        }
        // No user activation or the picker is blocked: download instead.
        console.warn('File picker unavailable, downloading instead', err);
        const result = this.download(text, name);
        if (refused) {
          result.keepDirty = true;
          this.showNotice(
            `No permission to write ${refused}: downloaded a copy as ${result.name} instead; ${refused} itself is unchanged.`,
            [],
            'error',
          );
        }
        return result;
      },
    );
  }

  /**
   * Resolve true when `handle` may be written: queryPermission({ mode:
   * 'readwrite' }) and, unless that is 'granted', requestPermission (which
   * may ask the user). Handles without the permission methods (older
   * browsers, origin-private files) are tried directly; a failed write is
   * reported by writeFile().
   */
  ensureWritable(handle) {
    const opts = { mode: 'readwrite' };
    if (!handle || typeof handle.queryPermission !== 'function') return Promise.resolve(true);
    let query;
    try {
      query = Promise.resolve(handle.queryPermission(opts));
    } catch (err) {
      query = Promise.reject(err);
    }
    return query.then(
      (state) => {
        if (state === 'granted') return true;
        if (typeof handle.requestPermission !== 'function') return false;
        return Promise.resolve(handle.requestPermission(opts)).then((next) => next === 'granted', () => false);
      },
      () => true,
    );
  }

  /** Download `text` as `name` through a Blob and an <a download>. */
  download(text, name) {
    const blob = new Blob([text], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    this.objectUrls.add(url);
    const a = document.createElement('a');
    a.href = url;
    a.download = name;
    a.className = 'te-files-download';
    a.hidden = true;
    document.body.appendChild(a);
    a.click();
    a.remove();
    // Revoke later: the download needs the URL after the click returns.
    setTimeout(() => {
      URL.revokeObjectURL(url);
      this.objectUrls.delete(url);
    }, 30000);
    return { name, method: 'download' };
  }

  // ---------------------------------------------------------------------
  // Open
  // ---------------------------------------------------------------------

  /** Dispatch te:open; unless cancelled, ask for a file and open it. */
  open() {
    if (this.destroyed) return Promise.resolve(false);
    const ev = this.emit('te:open', { available: this.modelAvailable() }, true);
    if (ev.defaultPrevented || !this.modelAvailable()) return Promise.resolve(false);
    if (!this.fsAccess) {
      this.fileInput.click();
      return Promise.resolve(false);
    }
    let picked;
    try {
      picked = window.showOpenFilePicker({ types: TE_PICKER_TYPES, multiple: false });
    } catch (err) {
      picked = Promise.reject(err);
    }
    this.lastOpen = picked.then(
      async ([handle]) => this.openFile(await handle.getFile(), handle),
      (err) => {
        if (err && err.name !== 'AbortError') {
          console.warn('File picker unavailable, using the file input', err);
          this.fileInput.click();
        }
        return false;
      },
    );
    return this.lastOpen;
  }

  /**
   * Read `file` and open it (asking first when the document is dirty).
   * Resolves to true when it was opened.
   */
  async openFile(file, handle = null) {
    if (this.destroyed) return false;
    // Only the latest open may load, and only into the document it started
    // from (a slower earlier read must not replace a later one).
    const seq = ++this.openSeq;
    const gen = this.generation;
    const current = () => !this.destroyed && seq === this.openSeq && gen === this.generation;
    if (!teIsOpenableFile(file)) {
      this.showNotice(`${file && file.name ? file.name : 'That file'} is not a Markdown or text file.`, [], 'error');
      return false;
    }
    let text;
    try {
      text = await file.text();
    } catch (err) {
      this.showNotice(`Could not read ${file.name}.`, [], 'error');
      return false;
    }
    if (!current()) return false;
    if (this.isDirty()) {
      const choice = await this.askReplace(file.name);
      if (choice === 'cancel' || !current()) return false;
      if (choice === 'save') {
        const saved = this.save();
        if (saved === null || !(await this.lastWrite) || !current()) return false;
      }
    }
    this.load(text, file.name, handle, file.lastModified, file.size);
    return true;
  }

  /** Put `text` in the editor as the document `name` (no questions asked). */
  load(text, name, handle = null, lastModified = null, size = null) {
    this.generation += 1;
    this.quiet += 1;
    try {
      this.editor.openDocument(text, name);
    } finally {
      this.quiet -= 1;
    }
    this.cancelTick();
    this.handle = handle;
    this.fileName = name || null;
    if (name) this.key = name;
    this.cleanText = this.serialise();
    this.setDirty(false);
    this.showNotice(null);
    this.pendingDraft = null;
    const file = Number.isFinite(lastModified) ? { lastModified } : null;
    this.setDraftIdentity({ handle, name, size, lastModified }, () => this.checkDraft(file));
    this.emit('te:opened', { name, text });
  }

  /** Save first / Discard changes / Cancel before replacing a dirty document. */
  askReplace(name) {
    return new Promise((resolve) => {
      const { dialog } = this.makeDialog('te-files-replace', 'Unsaved changes');
      const p = document.createElement('p');
      p.className = 'te-files-dialog-text';
      p.textContent = `Opening ${name} replaces the current document, which has unsaved changes.`;
      dialog.appendChild(p);
      const footer = document.createElement('div');
      footer.className = 'te-files-dialog-footer';
      dialog.appendChild(footer);
      let choice = 'cancel';
      const pick = (c) => () => {
        choice = c;
        dialog.close();
      };
      const first = this.dialogButton(footer, 'Save first', { name: 'save', className: 'te-files-primary', onClick: pick('save') });
      this.dialogButton(footer, 'Discard changes', { name: 'discard', onClick: pick('discard') });
      this.dialogButton(footer, 'Cancel', { name: 'cancel', onClick: pick('cancel') });
      this.on(dialog, 'close', () => {
        this.removeDialog(dialog);
        resolve(choice);
      }, { once: true });
      this.replaceDialog = dialog;
      this.openDialog(dialog, first);
    });
  }

  removeDialog(dialog) {
    dialog.remove();
    this.nodes = this.nodes.filter((n) => n !== dialog);
    this.dialogs = this.dialogs.filter((n) => n !== dialog);
    if (this.replaceDialog === dialog) this.replaceDialog = null;
    if (this.markdownDialog === dialog) this.markdownDialog = null;
  }

  // ---------------------------------------------------------------------
  // Markdown export view (issue #73)
  // ---------------------------------------------------------------------

  exportText() {
    if (this.modelAvailable()) {
      try {
        return this.editor.exportDocument();
      } catch (err) {
        console.error('exportDocument failed', err);
      }
    }
    return this.editor.surface ? this.editor.surface.getText() : '';
  }

  /** Dispatch te:markdown; unless cancelled, show the export dialog. */
  showMarkdown() {
    if (this.destroyed) return null;
    const ev = this.emit('te:markdown', {}, true);
    if (ev.defaultPrevented) return null;
    if (this.markdownDialog) {
      // One export view at a time; the close event is asynchronous.
      const old = this.markdownDialog;
      old.close();
      this.removeDialog(old);
    }
    const text = this.exportText();
    const { dialog, header } = this.makeDialog('te-files-markdown', 'Markdown');
    const close = this.dialogButton(header, '×', {
      name: 'close', className: 'te-files-close', ariaLabel: 'Close Markdown export',
      onClick: () => dialog.close(),
    });
    close.querySelector('span').setAttribute('aria-hidden', 'true');
    const note = document.createElement('p');
    note.className = 'te-files-dialog-text';
    note.id = 'te-files-markdown-note';
    note.textContent = 'Clean Markdown: the active alternatives, without ghosted text, Overflow or the annotation block.';
    const area = document.createElement('textarea');
    area.className = 'te-files-markdown-text';
    area.readOnly = true;
    area.spellcheck = false;
    area.value = text;
    area.setAttribute('aria-label', 'Exported Markdown');
    area.setAttribute('aria-describedby', note.id);
    const footer = document.createElement('div');
    footer.className = 'te-files-dialog-footer';
    const status = document.createElement('span');
    status.className = 'te-files-status';
    status.setAttribute('role', 'status');
    footer.appendChild(status);
    this.dialogButton(footer, 'Copy', {
      name: 'copy', icon: 'fa-regular fa-copy', onClick: () => { this.lastCopy = this.copyExport(area, status); },
    });
    this.dialogButton(footer, 'Download .md', {
      name: 'download', icon: 'fa-solid fa-download', className: 'te-files-primary',
      onClick: () => {
        const name = teExportFileName(this.suggestedName());
        this.lastWrite = this.writeFile(area.value, name).then((r) => {
          if (r) {
            status.textContent = r.method === 'file' ? `Saved ${r.name}.` : `Downloaded ${r.name}.`;
            this.emit('te:written', { name: r.name, method: r.method, text: area.value, export: true });
          }
          return !!r;
        });
      },
    });
    dialog.append(note, area, footer);
    this.on(dialog, 'close', () => this.removeDialog(dialog), { once: true });
    this.markdownDialog = dialog;
    this.openDialog(dialog, area);
    return dialog;
  }

  /** Copy the export: the Clipboard API, else select and execCommand. */
  async copyExport(area, status) {
    const copied = 'Copied to the clipboard.';
    if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {
      try {
        await navigator.clipboard.writeText(area.value);
        status.textContent = copied;
        return 'clipboard';
      } catch (err) {
        // Permission or focus refused: fall back below.
      }
    }
    area.focus();
    area.select();
    let ok = false;
    try {
      ok = document.execCommand('copy');
    } catch (err) {
      ok = false;
    }
    status.textContent = ok
      ? copied
      : 'Copying is blocked here: the text is selected, press Ctrl+C (Cmd+C on a Mac).';
    return ok ? 'execCommand' : 'selected';
  }

  // ---------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------

  /** Remove the DOM, timers and object URLs; restore document.title. */
  destroy() {
    if (this.destroyed) return;
    // Keep the latest unsaved work as a draft before tearing down.
    this.flushDraft();
    this.cancelTick();
    for (const d of this.dialogs) {
      if (d.open) d.close();
    }
    this.destroyed = true;
    this.offChange();
    for (const node of this.nodes) node.remove();
    this.nodes = [];
    this.dialogs = [];
    for (const url of this.objectUrls) URL.revokeObjectURL(url);
    this.objectUrls.clear();
    this.applyTitle(false);
  }
}

window.TePersistence = TePersistence;
window.teSuggestedFileName = teSuggestedFileName;
window.teExportFileName = teExportFileName;
window.teIsOpenableFile = teIsOpenableFile;
window.TE_DRAFT_PREFIX = TE_DRAFT_PREFIX;
