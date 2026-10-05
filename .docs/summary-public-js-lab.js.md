# Summary: public/js/lab.js

## Purpose
The Lab popover and its mark actions (#14, R-8.1, R-8.2). Design: `docs/design/lab-popover.md`; styles: `public/css/lab.css`.

## Key points
- `TeLabPopover` (`editor.lab`) toggles on `te:lab` (LAB pill, Write_On only): non-modal `role=dialog` above the pill, header `The Lab guide…` / `what each idea does`, the six actions from `wasmBindings.lab_actions()` as a `role=menu` of `menuitemradio` with roving tabindex and a hint line, a hidden `data-slot=trim` for #15, and results (live-swatch legend, proposals with Accept, Clear marks).
- `run(id)` aligns the model, calls `lab_mark`, and sets the `lab-marks` registry layer (classes `te-lab-mark te-lab-mark--<kind>`, `aria-describedby` reason, `data-lab-kind`, `data` {kind, reason, proposal, text, score}); one action at a time; never changes text.
- Transient: registry mapping drops marks an edit lands inside; no re-run on edit; plain mode clears.
- `accept(id)` verifies the text, then `replaceRange(..., {source: 'lab'})` as one undo step.
- Escape and outside press close; focus kept across re-renders; a chip beside the pill while closed. Events `te:lab-marks`, `te:lab-accept`. `destroy()` clears the layer and removes DOM, pill ARIA and listeners.
- Loaded after `indicators.js`, before `editor.js`. Tests: `tests/web_lab.rs`.
