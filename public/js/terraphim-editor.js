/*
 * TeraphimEditor: the embeddable editor (issue #77)
 * =================================================
 *
 * The last file of terraphim-editor.min.js (scripts/build-dist.sh). A host
 * page with nothing but a container gets the whole editor:
 *
 *   <link rel="stylesheet" href="terraphim-editor.min.css">
 *   <div id="editor" style="height: 80vh"></div>
 *   <script src="terraphim-editor.min.js"></script>
 *   <script type="module">
 *     const editor = await TeraphimEditor.create(document.getElementById('editor'));
 *     editor.setValue('# Hello');
 *   </script>
 *
 * create() loads the WebAssembly module (once per page), renders the editor
 * markup into a `.terraphim-editor-container` it appends to the element
 * (the Rust `mount_editor`), and starts a MarkdownEditor scoped to that
 * container. Design notes: docs/design/embedding.md.
 *
 * Options (all optional):
 *   value          initial Markdown (default: empty document)
 *   wasmUrl        the wasm-bindgen `_bg.wasm`; default
 *                  `terraphim_editor_bg.wasm` next to this script
 *   glueUrl        the wasm-bindgen ES module glue; default
 *                  `terraphim_editor.js` next to this script
 *   bindings       an already-initialised wasm-bindgen module (or any object
 *                  with the same exports); skips loading altogether
 *   standalone     true when the editor is the page: Ctrl+S / Ctrl+O act
 *                  anywhere and files dropped on the bare page open (default
 *                  false: only while focus is in the editor)
 *   dirtyTitle     prefix document.title while unsaved (default: standalone)
 *   loadShoelace   load the Shoelace components from the CDN (default true)
 *   config         overrides merged over window.EditorConfig (shortcuts,
 *                  commands, documentKey, fileSystemAccess, autosaveDelay)
 *
 * One editor per page: the document session and the live preview live in
 * the WebAssembly module as single instances, so a second create() while
 * one is alive rejects; destroy() the first one.
 *
 * The page must not mark itself as the full editor page: this file sets
 * `window.TE_EMBED = true`, which stops the module start function and the
 * index.html bootstrap at the end of editor.js from taking over any #app.
 */

window.TE_EMBED = true;

// Captured now: document.currentScript is null once the script has run.
const TE_SCRIPT_BASE = (() => {
  const script = document.currentScript;
  return script && script.src ? script.src : document.baseURI;
})();

const TE_SHOELACE_BASE = 'https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/';
const TE_SHOELACE_COMPONENTS = [
  'input/input.js',
  'icon/icon.js',
  'button/button.js',
  'button-group/button-group.js',
  'tooltip/tooltip.js',
  'split-panel/split-panel.js',
  'dialog/dialog.js',
  'divider/divider.js',
  'badge/badge.js',
];

class TeraphimEditor {
  /**
   * Create an editor in `element` and resolve to it once it is ready.
   * Rejects (leaving `element` as it was) when the module cannot be loaded
   * or another editor is alive on the page.
   */
  static async create(element, options = {}) {
    const editor = new TeraphimEditor(element, options);
    await editor.mount();
    return editor;
  }

  /**
   * Load (once per page) and initialise the wasm-bindgen module. Resolves
   * to the module's exports. Defaults resolve next to this script; explicit
   * URLs resolve against the page.
   */
  static loadModule(options = {}) {
    if (!TeraphimEditor.modulePromise) {
      const glueUrl = new URL(options.glueUrl || new URL('terraphim_editor.js', TE_SCRIPT_BASE).href, document.baseURI).href;
      const wasmUrl = new URL(options.wasmUrl || new URL('terraphim_editor_bg.wasm', TE_SCRIPT_BASE).href, document.baseURI).href;
      TeraphimEditor.modulePromise = (async () => {
        const glue = await import(glueUrl);
        await glue.default({ module_or_path: wasmUrl });
        return glue;
      })();
      // A failed load may be retried (a wrong URL, a network error).
      TeraphimEditor.modulePromise.catch(() => {
        TeraphimEditor.modulePromise = null;
      });
    }
    return TeraphimEditor.modulePromise;
  }

  /** Load the Shoelace theme and components from the CDN (once). */
  static async loadShoelace() {
    if (!document.querySelector('link[href*="shoelace"][href$="light.css"]')) {
      const link = document.createElement('link');
      link.rel = 'stylesheet';
      link.href = `${TE_SHOELACE_BASE}themes/light.css`;
      document.head.appendChild(link);
    }
    // Without a <script src> pointing at Shoelace it cannot infer where its
    // icons live; tell it, as index.html does.
    const basePath = await import(`${TE_SHOELACE_BASE}utilities/base-path.js`);
    if (typeof basePath.setBasePath === 'function') basePath.setBasePath(TE_SHOELACE_BASE);
    await Promise.all(TE_SHOELACE_COMPONENTS.map(async (component) => {
      if (!customElements.get(`sl-${component.split('/')[0]}`)) {
        await import(`${TE_SHOELACE_BASE}components/${component}`);
      }
    }));
  }

  /**
   * Prefer TeraphimEditor.create(element, options). The constructor plus
   * initialize() is the earlier API, kept as an alias of mount().
   */
  constructor(targetElement, options = {}) {
    if (!(targetElement instanceof Element)) {
      throw new TypeError('TeraphimEditor needs a container element');
    }
    this.targetElement = targetElement;
    this.options = Object.assign({}, options);
    this.editor = null;
    this.container = null;
    this.bindings = null;
    this.listeners = [];
  }

  /** Deprecated alias of mount(), kept for `new TeraphimEditor(el).initialize()`. */
  async initialize() {
    return this.mount();
  }

  /** Render and start the editor. Resolves to this. */
  async mount() {
    // Re-mounting must not leak the previous editor.
    this.destroy();
    const active = TeraphimEditor.active;
    if (active && active !== this) {
      throw new Error('TeraphimEditor supports one editor per page; destroy() the existing editor first');
    }
    TeraphimEditor.active = this;
    try {
      const opts = this.options;
      if (opts.loadShoelace !== false) await TeraphimEditor.loadShoelace();
      const bindings = opts.bindings || (await TeraphimEditor.loadModule(opts));
      if (!bindings || typeof bindings.mount_editor !== 'function') {
        throw new Error('The editor module does not export mount_editor');
      }
      this.bindings = bindings;
      // The editor scripts reach the module through config.bindings; the
      // global is set too (when free) for code that looks there.
      if (!window.wasmBindings) window.wasmBindings = bindings;

      const container = document.createElement('div');
      container.className = 'terraphim-editor-container';
      this.targetElement.appendChild(container);
      this.container = container;
      bindings.mount_editor(container, typeof opts.value === 'string' ? opts.value : '');

      const standalone = opts.standalone === true;
      const config = Object.assign(
        {},
        window.EditorConfig || { shortcuts: [], commands: [] },
        opts.config || {},
        {
          standalone,
          dirtyTitle: opts.dirtyTitle === undefined ? standalone : opts.dirtyTitle !== false,
          bindings,
        },
      );
      const editor = new MarkdownEditor(config);
      editor.initialize(container);
      if (!editor.surface) {
        editor.destroy();
        throw new Error('The editor could not start in its container');
      }
      this.editor = editor;
      return this;
    } catch (err) {
      this.destroy();
      throw err;
    }
  }

  /**
   * Remove the editor: its listeners, every node it created (inside the
   * container and on <body>), the container and the module's preview
   * state. The host element is left as it was. Returns the drafts that were
   * not written into the document (see MarkdownEditor.destroy()). Safe to
   * call more than once.
   */
  destroy() {
    let drafts = [];
    for (const { type, wrapped } of this.listeners) document.removeEventListener(type, wrapped);
    this.listeners = [];
    if (this.editor) drafts = this.editor.destroy() || [];
    this.editor = null;
    if (this.bindings && typeof this.bindings.unmount_editor === 'function' && this.container) {
      this.bindings.unmount_editor();
    }
    if (this.container) this.container.remove();
    this.container = null;
    if (TeraphimEditor.active === this) TeraphimEditor.active = null;
    return drafts;
  }

  /** The MarkdownEditor (surface, chrome, panels), or null before mount. */
  get markdownEditor() {
    return this.editor;
  }

  /** Save, open, drafts and the Markdown export view (TePersistence). */
  get persistence() {
    return this.editor ? this.editor.persistence || null : null;
  }

  /** The body text on the surface (no annotation block). */
  getValue() {
    return this.requireEditor().surface.getText();
  }

  /** Replace the body text; recorded as one undo step, the preview follows. */
  setValue(markdown) {
    this.requireEditor().surface.setText(String(markdown));
  }

  /**
   * Open `.md` source with its annotation block (alternatives, ghosts,
   * overflow); `name` becomes the document key. Returns { body, warning,
   * unresolved }.
   */
  openDocument(source, name) {
    return this.requireEditor().openDocument(source, name);
  }

  /**
   * The document as `.md`: body plus the annotation block. Also dispatches
   * `te:saved`. Storing it is the host's job (or use persistence.save()).
   */
  saveDocument() {
    return this.requireEditor().saveDocument();
  }

  /** Clean Markdown: active alternatives, no ghosted text, no block. */
  exportDocument() {
    return this.requireEditor().exportDocument();
  }

  /** Whether Write_On mode is on. */
  isWriteOn() {
    const chrome = this.requireEditor().chrome;
    return !!chrome && chrome.isWriteOn();
  }

  /** Turn Write_On mode on or off. */
  setWriteOn(on) {
    const chrome = this.requireEditor().chrome;
    if (chrome) chrome.setMode(on ? 'write-on' : 'plain');
  }

  /**
   * Listen for this editor's `te:*` events (te:saved, te:mode-change,
   * te:dirty-change, ...). Events from its menus and dialogs on <body> are
   * included; events from anything else on the page are not.
   */
  on(type, callback) {
    const wrapped = (e) => {
      if (!this.editor) return;
      const fromEditor = e.detail && e.detail.editor === this.editor;
      if (fromEditor || (this.container && this.container.contains(e.target))) callback(e);
    };
    document.addEventListener(type, wrapped);
    this.listeners.push({ type, callback, wrapped });
    return () => this.off(type, callback);
  }

  /** Remove a listener added with on(). */
  off(type, callback) {
    this.listeners = this.listeners.filter((l) => {
      if (l.type !== type || l.callback !== callback) return true;
      document.removeEventListener(type, l.wrapped);
      return false;
    });
  }

  requireEditor() {
    if (!this.editor) throw new Error('The editor is not mounted');
    return this.editor;
  }
}

TeraphimEditor.active = null;
TeraphimEditor.modulePromise = null;

// Make it available globally
window.TeraphimEditor = TeraphimEditor;
