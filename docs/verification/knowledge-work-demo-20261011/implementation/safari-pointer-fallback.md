# Safari cross-panel pointer regression

Approved work package 2 requires actual selected text to move into Overflow by drag. Current native Safari input collapses/extends selection instead of producing an HTML drag; the recording does not establish whether the root cause is the input transport or browser drag initiation. Existing HTML drag/drop remains supported.

Bounded repair: retain a pre-existing page selection only when primary left mouse pointerdown lands inside its actual selected range. Complete the fallback only for a moved pointer released over the visible Overflow panel, with unchanged entire document source and matching pointer identity. Reuse the existing stash transaction. Same-page selection, click without movement, changed source, cancellation, Escape, window blur, native dragstart, closed panel and read-only modes must refuse mutation. Do not cancel normal pointerdown or native drag initiation. HTML dragstart supersedes this fallback to prevent duplicate mutations.

Tests first: actual DOM plus actual WASM model, exact body and Overflow, single undo transaction, undo/redo, save/reopen, stale source/cancellation/native drag refusal. Programmatic events prove the fallback contract only. Native trusted Safari output acceptance remains required after an admitted preview build; neither these tests nor a recording’s existence closes issue118.

Panel-to-page drag retains its existing HTML path in this bounded change; it requires independent native verification and is not claimed fixed by a page-to-panel regression.
