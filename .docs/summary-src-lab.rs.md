# Summary: src/lab.rs

## Purpose
wasm-bindgen bridge to `terraphim_lab` (#14).

## Key points
- `lab_actions()` returns `[{id, label}]` in popover order (serde ids, `LabAction::label`).
- `lab_mark(action)` runs one action over the session body and returns `[{kind, start, end, score, reason, proposal}]` in UTF-16; throws for unknown ids; never changes text; JS aligns the model first.
- `LabConfig::with_defaults()` is built once, lazily, in a thread_local `OnceCell`; role support is a follow-up.
- Core functions (`actions_json`, `marks_json`, `session_marks_json`, `parse_action`, `action_id`, `with_config`) return serde_json values and are unit tested natively, including a fixture check.
- The dependency adds about 351 KB raw (104 KB gzip) to the release wasm, mostly `terraphim_automata`'s dependency graph.
