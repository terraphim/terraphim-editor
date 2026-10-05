//! Helpers for the save, open, draft and Markdown export tests (issues #76
//! and #73), shared by `tests/web_files*.rs`.
//!
//! Headless Chrome defines `showSaveFilePicker` and `showOpenFilePicker`
//! but they throw `SecurityError` without a real user gesture, so the tests
//! create editors with `fileSystemAccess: false` (a documented editor
//! option, not a stub) to exercise the download and `<input type=file>`
//! paths, and use real origin-private (OPFS) file handles for the
//! write-back paths. Files are real `File` objects put into a real
//! `<input type=file>` or a real `DragEvent` through `DataTransfer`.
//!
//! Downloads are observed with a page-level click listener on the
//! `<a download>` the editor creates: it records the name and URL, calls
//! `preventDefault()` (so the browser writes nothing to disk) and the test
//! reads the Blob back with `fetch()`. No editor code and no browser API is
//! replaced.

use super::*;

pub const FILES_HELPERS_JS: &str = r##"
window.teFiles = {
  // Recreate the editor on the current page with test-friendly options:
  // the download and file-input paths, and a short autosave delay.
  reinit(extra, opts) {
    const old = window.__teEditor;
    if (old) old.destroy();
    if (!(opts && opts.keepDrafts)) teTest.resetDrafts();
    const config = Object.assign({}, window.EditorConfig, { fileSystemAccess: false, autosaveDelay: 40 }, extra || {});
    const ed = new MarkdownEditor(config);
    ed.initialize();
    window.__teEditor = ed;
    return ed;
  },
  // Record every <a download> click and stop the browser downloading.
  captureDownloads() {
    if (window.__teDownloads) window.__teDownloads.stop();
    const list = [];
    const onClick = (e) => {
      const a = e.target;
      if (!(a instanceof HTMLAnchorElement) || !a.hasAttribute('download')) return;
      e.preventDefault();
      list.push({ name: a.download, href: a.href, connected: a.isConnected });
    };
    document.addEventListener('click', onClick, true);
    const cap = { list, stop() { document.removeEventListener('click', onClick, true); window.__teDownloads = null; } };
    window.__teDownloads = cap;
    return cap;
  },
  async read(download) {
    return (await fetch(download.href)).text();
  },
  file(text, name, lastModified) {
    return new File([text], name, { type: name.endsWith('.txt') ? 'text/plain' : 'text/markdown', lastModified: lastModified || Date.now() });
  },
  setInputFile(input, file) {
    const dt = new DataTransfer();
    dt.items.add(file);
    input.files = dt.files;
    input.dispatchEvent(new Event('change', { bubbles: true }));
  },
  drag(target, dt) {
    const over = new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt });
    target.dispatchEvent(over);
    const drop = new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt });
    target.dispatchEvent(drop);
    return { over: over.defaultPrevented, drop: drop.defaultPrevented };
  },
  dropFile(target, file) {
    const dt = new DataTransfer();
    dt.items.add(file);
    return teFiles.drag(target, dt);
  },
  wait(ms) { return new Promise((r) => setTimeout(r, ms)); },
  // The full persistence fixture as the editor saves it (the writer
  // normalises the separator before the block, so this is the canonical
  // text whose open -> save round trip is exact).
  canonicalFull(ed) {
    ed.persistence.load(teFixtures.full.md, 'canonical.md');
    return ed.saveDocument();
  },
  type(ed, text) {
    const s = ed.surface;
    s.focus();
    s.setSelectionOffsets(s.getText().length);
    document.execCommand('insertText', false, text);
  },
  // Put the page back to the welcome document without a page reload: the
  // state a reload would give (the model reopened, the template text).
  welcome() {
    return window.__teWelcome;
  },
  resetToWelcome() {
    const ed = window.__teEditor;
    if (ed) ed.destroy();
    window.__teEditor = null;
    window.wasmBindings.open_document(window.__teWelcome);
    document.querySelector('.markdown-input').textContent = window.__teWelcome;
  },
};
"##;

/// Fresh editor plus the file helpers and fixtures. Remembers the welcome
/// text so a test can simulate a reload.
pub fn fresh_files_editor() -> Document {
    let document = fresh_full_editor();
    install_fixtures();
    if js_eval("typeof window.teFiles").as_string().as_deref() != Some("object") {
        js_eval(FILES_HELPERS_JS);
    }
    js_eval("window.__teWelcome = window.__teEditor.surface.getText(); 0");
    document
}

/// Run an async JavaScript function body and await its string result (an
/// empty string means the test passed). A thrown error is returned as text.
pub async fn js_async(body: &str) -> String {
    // A step that never settles (a dialog nobody answers) reports itself
    // instead of hanging the binary until the runner's budget kills it.
    let src = format!(
        "Promise.race([(async () => {{ {body}\n }})(), \
           new Promise((r) => setTimeout(() => r('timed out after 6 s'), 6000))]) \
         .then((r) => (typeof r === 'string' ? r : String(r)), \
         (e) => 'threw: ' + ((e && e.stack) || e))"
    );
    let promise: Promise = js_eval(&src)
        .dyn_into()
        .expect("snippet should return a promise");
    JsFuture::from(promise)
        .await
        .expect("wrapped promise never rejects")
        .as_string()
        .unwrap_or_else(|| "non-string result".to_string())
}
