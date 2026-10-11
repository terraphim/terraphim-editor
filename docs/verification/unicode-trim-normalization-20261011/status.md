# Unicode KG-protected Trim

Reuses published Terraphim `NormalizedTermValue` (terraphim_types1.22.3 via terraphim_lab) for full-source contextual Unicode lowercase, and existing terraphim_automata2.1 CompiledMatcher. Canonical NFC comes from unicode-normalization0.1.25; existing unicode-segmentation dependency supplies original graphemes. Terraphim's term type lowercases/trims; it does not itself implement NFC or full Unicode case folding. Matching does not transliterate Straße to STRASSE.

Complete source graphemes map normalized matches back to original byte boundaries; interior expansion/contraction boundaries are refused. Original whitespace runs fold only in the matching copy. NUL framing preserves leading/trailing whitespace through the existing term type. Full lowercase output is checked against each source scalar's expansion, allowing Rust's contextual sigma value change; unknown transformations fail closed. Original spelling, UTF16 cut range validation and exact cut/tidy retention checks remain enforced.

Initial regression: three Unicode tests failed against the whitelist; collision refusal passed. Independent review then found sigma context across punctuation missed by a per-word adapter; implementation now normalizes the whole source, with lower-key/uppercase-body middot/apostrophe/colon regressions.

Final native:110 library tests +6 original protection tests +6 Unicode regressions pass. Strict native all-test Clippy and formatting pass. Browser logs recorded separately; the browser test checks actual reduction, exact protected spellings, Done ghost creation, saved annotated content, reopen and actual undo/redo. This is automated real Chrome/WASM behavior, not native Safari acceptance or all41 qualification. Zed fork untouched.
