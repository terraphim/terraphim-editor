# Alternatives provider seam (issue #83, R-8.7)

R-8.7: "AI calls go through a pluggable provider." Until now `src/kg.rs`
called the thesaurus directly. "AI alternatives for selection" (Ctrl+Shift+G,
the menu item, `editor.kgAlternativesForSelection`, `kg_append`) now asks an
ordered list of providers. The thesaurus is still the default and the only
built-in one, so nothing changes for existing users. No LLM call is written
here (decision 12.4 defers it); this is the seam it will plug into.

## Rust: `src/providers.rs`

```rust
pub trait AlternativesProvider {
    fn id(&self) -> &str;
    fn alternatives(&self, context: &ProviderContext<'_>) -> Vec<Suggestion>;
}
pub struct Suggestion { pub text: String, pub source: Source, pub model: Option<String> }
```

* `ProviderContext` is the target span: `body`, `start`/`end` (UTF-16),
  `text`, and `supplied` (answers of asynchronous host providers, below).
* `ThesaurusProvider` (in `src/kg.rs`) is today's KG path: the synonyms of
  the KG term exactly at the target, as `source: ai`, `model: "kg"`.
* The registry is a per-thread ordered `Vec<Box<dyn AlternativesProvider>>`
  (`register_provider`, `unregister_provider`, `provider_ids`,
  `reset_providers`). Registering an existing id replaces it in place.
* `append_from_providers(session, start, end, supplied)` resolves the
  target, asks every provider in order, and appends the merged suggestions
  through `DocumentSession::append_suggestions` as ONE `AltChange`, so the
  editor records one undo step and replays it with `alt_restore`.
  `kg_append` is `append_from_providers` with nothing supplied.

Decisions:

1. **Target.** The KG term at the selection when there is one (as before),
   otherwise the selection itself when not empty, as a word, sentence or
   paragraph span by its shape. So a host provider can answer for text the
   thesaurus knows nothing about. With no KG term and no suggestions the
   call is refused with `NoTerm`, exactly as before.
2. **Order and de-duplication.** Providers are asked in registry order;
   `append_suggestions` skips empty texts, the span's own texts (the
   original included) and repeats, so the first provider to offer a text
   keeps its `source` and `model`. Asking again adds nothing (`Ok(None)`).
3. **Source.** Provider results are `source: ai` with the provider's model
   (the bot glyph, R-4.4). `Source` has no `kg` variant (see
   `kg-alternatives.md`); the thesaurus is `ai` with `model: "kg"`.
4. **Writer's span wins.** A different span overlapping the target refuses
   the append for every provider, changing nothing.

## JavaScript: `editor.registerAlternativesProvider`

The WASM document API is synchronous; a host provider is not. So the editor
awaits host providers first and passes their answers into one WASM call.

```js
const off = editor.registerAlternativesProvider({
  id: 'llm',
  model: 'llama-3',                  // default model for its lines
  alternatives: async ({ text, body, start, end }) =>
    ['option one', { text: 'option two', model: 'llama-3-70b' }],
});
// editor.alternativesProviderIds() -> ['thesaurus', 'llm']
// off() unregisters; editor.destroy() unregisters them all.
```

* Registration adds an `ExternalProvider` (id and model) to the Rust
  registry, so host providers keep their place in the order, after the
  thesaurus and earlier registrations.
* `editor.alternativesForSelection(ctx)` is what Ctrl+Shift+G and the menu
  item run. With no host provider it is exactly `kgAlternativesForSelection`
  and acts synchronously. Otherwise it awaits every host provider in
  parallel, then calls `alt_append_alternatives(start, end, supplied)` through
  `alternativeOp`, so it is one undo step with the panel opening on the span.
* A provider that throws or does not return an array is skipped with a
  warning. If the text changed while waiting, nothing is appended (offsets
  would be stale) and the promise resolves to `null`.
* The menu item is offered on a KG term as before, and additionally on any
  non-empty selection once a host provider is registered.
* `supplied` is `{ "<id>": [ "text" | { "text", "model" } ] }`; entries for
  unregistered ids and malformed entries are dropped (`parse_supplied`).

## Lifetime and races

* The `thesaurus` id is reserved: registering it throws (JS and WASM), and
  it cannot be unregistered, so a host never replaces the default.
* The WASM registry belongs to the page (one document session). Editors
  count registrations per id, so destroying one editor leaves another
  editor's provider of the same id registered; the id leaves the registry
  with the last holder. Each editor records its own model name, so two
  editors sharing an id keep their own provenance.
* The handle `registerAlternativesProvider` returns removes only its own
  registration, never a later one under the same id.
* A provider replaced or removed while its answer is in flight is dropped
  from that append (its answers are stale).

## Not here

The LLM provider itself (Ollama or OpenRouter through `terraphim_server`
`/chat`, decision 12.4): a host would register it with the hook above.
Per-provider settings, a provider picker and streaming are out of scope.

## Tests

* `tests/providers.rs` (native): default registry, unchanged default
  behaviour, a real second provider (a reverser) asked after the thesaurus
  in order, de-duplication with provenance, replace-in-place, selections
  that are not KG terms, writer's span winning, supplied host answers.
* `tests/web_providers.rs` (browser): an asynchronous host provider on a real
  timer appended after the thesaurus as one undo step with bot glyphs, the
  menu item on any selection, failing and stale providers.
