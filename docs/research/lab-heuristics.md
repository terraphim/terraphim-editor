# Lab heuristics: weakest-sentence ranking and trim selection

Research spike for `terraphim/terraphim-editor#3` (epic `#1`). Written 2026-10-04.

Spec context: `docs/requirements/alternative-control.md` §8 (R-8.2 mark actions, R-8.3 to R-8.5 trim levels and review) and §12. Decision 4 rules out an LLM in v1, and decision 5 puts all six mark actions and all four trim levels in v1. Four of the mark actions already have obvious deterministic rules. The two that did not are **"Mark the weakest sentences"** and **trim selection**. This note proposes a deterministic heuristic for each and measures both on real text.

All numbers below come from the prototype in `research/lab-heuristics/`. None are estimates. The way to reproduce them is at the end.

## 1. Recommendation

**Weakness ranking: a weighted linear score per sentence, using five features computed from KG lists and the document itself.**

```
weakness = 0.35 * hedge_filler_density      (KG lists, normalised to the doc maximum)
         + 0.30 * (1 - lexical_centrality)  (how little of the sentence's vocabulary recurs elsewhere)
         + 0.15 * neighbour_redundancy      (max Jaccard overlap with sentences within +/-2)
         + 0.10 * function_word_ratio
         + 0.10 * (1 - kg_concept_coverage) (role-thesaurus concepts mentioned, capped at 2)
```

"Mark the weakest sentences" marks the top 15% (at least one). Sentences under 5 words are never marked. Length is deliberately left out, because "Mark sentences that run long" is a separate R-8.2 action and would otherwise be counted twice.

**Trim selection: one ranked list of fadeable spans at three granularities, with nested levels and a fit step.**

1. Candidates are spans that can be deleted verbatim, never rewritten:
   - word-level: cuttable hedges and fillers from the KG lists;
   - clause-level: parentheticals, em-dash asides and tails, and comma asides that open with a known connective (`, which ...`, `, however`, `, especially ...`);
   - sentence-level: whole sentences.
2. The ranking is by tier, then by the host sentence's weakness (descending), then by position. The tiers are: word (0), clause (1), sentence (2), and paragraph-opening or short sentence (3). Tier 3 is only used at "Cut in half", or as a fallback when a level would otherwise miss the tolerance.
3. Each level starts from the previous level's selection, so the levels are nested. Pass 1 walks the ranked list and accepts a candidate only if it does not overshoot the target word count. Pass 2 then repeatedly accepts whichever remaining candidate brings the count closer to the target: the best-ranked one that lands within 1% of the document, otherwise the closest one.
4. Cuts are counted as a union of words, so a faded filler inside a later-faded sentence is not counted twice. Percentages use the same word definition as the status card.

**Result:** every level on all three fixtures came within 0.4 percentage points of its target. The acceptance criterion was +/-3pp.

Why this pair:
- **One score drives both features.** The weakest-sentence mark and the sentence tier of the trim use the same `weakness` score. A sentence the Lab marks as weak is therefore one of the first whole sentences a trim fades, and the two features never contradict each other.
- **Deterministic and explainable.** Each mark and each cut has a `reason` the UI can show (`filler "quite"`, `aside "which"`, `weak sentence`).
- **Mapped onto `terraphim_automata` already.** The lists are KG markdown files, and the matcher uses the same automaton configuration as `terraphim_automata::find_matches`. The one gap is the word-boundary filter (§3.3).
- **The light levels behave like the demo.** At ~10% the cut is mostly faded words and asides, not whole sentences, which is what the demo shows ("Faded words would go", R-8.4).

## 2. Constraints taken from the spec

- R-8.2 and R-8.4: mark and trim **never rewrite**. Every output is a span to fade (the ghost treatment from R-5.1). "Make the cuts" deletes the spans verbatim.
- R-8.4: the status card reports `N -> M words · -P%`, so word counting must be defined once and used everywhere.
- R-8.5: "Click one to keep it" and "Walk through" both need stable span identity and an order to walk in.
- Decision 4: no LLM, so `terraphim_automata`-style KG lists are the only external knowledge.
- Decision 3: ghosted text still counts towards words and characters. While a trim preview is showing, the editor's word count therefore stays at the original value, and only the status card shows the post-cut number.

## 3. KG lists

### 3.1 What is needed

| Concept file | Used by | Cuttable in trim | Entries (prototype) |
|---|---|---|---|
| `lab-filler.md` | "Mark hedges and filler", weakness, word-level trim | yes | really, very, quite, just, actually, of course, at all, a bit, sort of, ... (27) |
| `lab-hedge.md` | "Mark hedges and filler", weakness, word-level trim | yes | perhaps, probably, somehow, generally, i fancy, it would appear, ... (23) |
| `lab-hedge-phrase.md` | "Mark hedges and filler", weakness | **no** (deleting them breaks the verb phrase) | seems to, appears to, might, could be, almost, ... (21) |
| `lab-tone.md` | "Mark words that don't fit the tone" | no | stuff, gonna, awesome, chum, lots of, ... (20) |
| role thesaurus concepts | weakness (KG concept coverage) | n/a | the active role's existing KG, for example `~/.config/terraphim/kg/` |

The prototype's lists live in `research/lab-heuristics/kg/`. A stand-in role thesaurus (five concepts: language server, zed, span model, thesaurus, wasm) lives in `research/lab-heuristics/kg/domain/`.

Splitting hedges into a **cuttable** list and a **mark-only** list is the main design point. Every entry in a cuttable list has to survive deletion on its own, together with its leading space and, where there is one, the comma before it. "Seemed to" fails that test ("it seemed to start off" becomes "it start off"), so it can only be marked.

### 3.2 Format and where the lists should live

The files use the Terraphim KG markdown format: one concept per file, the file stem is the concept name, and a `synonyms::` line holds the terms. This is the same format as `~/.config/terraphim/kg/*.md` (for example `context rot.md`), and the one `terraphim_automata::markdown_directives` parses (`synonyms::`, plus `priority::`, `trigger::` and `pinned::`, which the Lab does not need).

The concept names have a `lab-` prefix on purpose. The terraphim thesaurus builder also adds each concept name as a pattern, and a bare `filler.md` would then match the word "filler" in prose. The prototype reproduces that behaviour for the domain KG (the concept name is included as a term) and leaves it out for the Lab lists.

Nothing reusable exists today. A search of `~/.config/terraphim/kg/` found no hedge or filler lists; the only hit for those words was `judge-semantic.md`, which is unrelated.

Recommendation:
1. **Ship defaults inside the editor.** `terraphim-editor` runs as WASM with no file system, so the default lists have to be embedded at build time, for example as a thesaurus JSON loaded with `terraphim_automata::load_thesaurus_from_json`. The natural home is the shared crate `crates/terraphim_alternatives` (`#2`), so that `terraphim_lsp` gets the same lists (`#16`).
2. **Allow per-role overrides.** If the active role's KG contains `lab-hedge.md`, `lab-filler.md` and so on, those files replace or extend the defaults. Tone lists are register-specific (professional, academic, casual), so the role should choose one.
3. The prototype writes `research/lab-heuristics/out/thesaurus.json` in the `{"name", "data": {term: {"id", "nterm"}}}` shape, which shows the lists compile to the format `terraphim_automata` loads.

### 3.3 How the matcher maps to `terraphim_automata`

`terraphim_automata` 1.21.0 (`src/matcher.rs`) builds its automaton in `find_matches` like this:

```rust
AhoCorasick::builder()
    .match_kind(MatchKind::LeftmostLongest)
    .ascii_case_insensitive(true)
```

It applies **no word-boundary check**, and it drops patterns shorter than 2 bytes (`MIN_FIND_PATTERN_LENGTH`). The prototype uses the same builder settings (crate `aho-corasick` 1.x, which is what `terraphim_automata` depends on) and adds two things:

1. **A word-boundary post-filter.** A match is kept only if the characters before and after it are not word characters, checked with `char` iteration rather than raw bytes, because the fixtures contain `’`, `“` and `—`. Without the filter, `very` matches inside `every`, `just` inside `justice` and `so` inside `also` (see the test `matcher_respects_word_boundaries`). When this moves to `terraphim_automata`, the Lab should call `find_matches(text, &thesaurus, true)` and filter `Matched.pos` the same way. An optional `whole_words` flag in `terraphim_automata` would be the cleaner fix upstream.
2. **Curly-apostrophe variants.** `ascii_case_insensitive` does not treat `’` and `'` as equal, so each term containing `'` is also added with `’`. Normalising the text before matching would be the alternative.

All list entries are at least 2 bytes long, so `MIN_FIND_PATTERN_LENGTH` never drops one (this is checked by a test).

## 4. Weakness ranking

### 4.1 Features

| Feature | Computation | Why it signals weakness |
|---|---|---|
| Hedge and filler density | (matches from `lab-filler`, `lab-hedge` and `lab-hedge-phrase`) / words, normalised to the document maximum | Padding and hedging dilute a claim |
| Lexical centrality (inverted) | Mean over the sentence's content lemmas of `ln(1 + number of other sentences containing the lemma)`, normalised | Sentences whose vocabulary appears nowhere else are off the document's thread |
| Neighbour redundancy | Max Jaccard overlap of content-lemma sets with sentences within +/-2 | The sentence repeats its neighbours |
| Function-word ratio | Stopwords / words | Low information density |
| KG concept coverage (inverted) | Distinct role-thesaurus concepts matched, divided by 2 and capped at 1 | Sentences that carry the role's key concepts matter more |

"Content lemmas" are lowercased words that are not stopwords, with the possessive and a plural `s` removed. This is deliberately crude, but it is deterministic and fast enough to run in WASM on every keystroke debounce.

The weights are a starting point chosen by inspection. A real tuning pass would need a small set of documents labelled by a human, which this spike does not have.

### 4.2 Results (qualitative: there is no ground truth)

Below are the top three marks per fixture, with my judgement of each. The full tables, with every feature value, are in `research/lab-heuristics/out/report.md`.

**three-men-ch1** (47 sentences, 8 marked)

| Rank | Weakness | Sentence | Judgement |
|---:|---:|---|---|
| 1 | 0.731 | Then, all of a sudden, it seemed to start off. | Fair: it is filler-heavy, but it is a comic beat |
| 2 | 0.643 | I felt rather hurt about this at first; it seemed somehow to be a sort of slight. | Good: hedged three times |
| 3 | 0.562 | We were all feeling seedy, and we were getting quite nervous about it. | Fair: it is a set-up line for the chapter |

**walden-economy** (20 sentences, 3 marked)

| Rank | Weakness | Sentence | Judgement |
|---:|---:|---|---|
| 1 | 0.545 | Perhaps these pages are more particularly addressed to poor students. | Good: a hedged aside |
| 2 | 0.484 | I should not obtrude my affairs so much on the notice of my readers if very particular inquiries had not been ... | Mixed: it is padded, but it is the paragraph's thesis |
| 3 | 0.478 | I should not talk so much about myself if there were anybody else whom I knew as well. | Good: a throwaway justification |

**zed-plugin-fit** (24 sentences, 4 marked)

| Rank | Weakness | Sentence | Judgement |
|---:|---:|---|---|
| 1 | 0.747 | That is optional and can wait until an agent actually produces them. | Good: a hedge on a hedge |
| 2 | 0.450 | **Sequencing:** today there is only one committed consumer. | Poor: it is a section lead-in |
| 3 | 0.389 | That contract (`crates/terraphim_engine_events`) covers agent evolution and approvals (...), not document state. | Good: supporting detail that can go |

Overall, the hedge and filler feature produces the convincing marks. Centrality catches tangents in essay prose, but it penalises lead-in sentences that introduce new vocabulary.

### 4.3 Failure modes

- **Short emphatic sentences.** Before the short-sentence guard, `**No.**` (the answer to the whole document) ranked second weakest, and "I had them all." (a punchline) was marked. Their features are degenerate: few content words and a high function ratio. Fix: sentences under 5 words are never marked, and in trim they sit in tier 3 with paragraph openers. A 6-word punchline ("I crawled out a decrepit wreck.") still gets through, so 5 is a judgement call.
- **Lead-in and thesis sentences.** New vocabulary lowers centrality ("**Sequencing:** today there is only one committed consumer."). A possible mitigation is a position bonus for paragraph openers. The trim already protects openers below 50%, but the mark does not.
- **Humour and voice.** Comic writing deliberately piles on fillers ("all of a sudden", "a bit"). A heuristic cannot tell intended voice from padding. This is the strongest argument for the later LLM provider (R-8.7), not for more rules.
- **Filler double-counting.** A sentence full of fillers scores as weak and can be cut whole at the next level, even though fading its fillers already fixed it ("We were sitting in my room ... I mean, of course."). One refinement is to recompute weakness for the next level with the already-faded words excluded.

## 5. Trim selection

### 5.1 Span conventions (what "Make the cuts" deletes)

Each span is a byte range into the document body that can be deleted verbatim:

| Granularity | Span covers | Example |
|---|---|---|
| Word, mid-sentence | leading space + term; the comma before it too if the term is bracketed by punctuation | `it seemed ~~somehow~~ to be` ; `study diseases~~, generally~~.` |
| Word, sentence-initial | term + following comma + space | `~~Perhaps~~ these pages` |
| Parenthetical | leading space + `( ... )` (single-character enumerations such as `(a)` are skipped) | `the spec ~~(§3–§7)~~ cannot` |
| Em-dash pair | first dash + aside; the closing dash is kept | `plunged into~~—some fearful, devastating scourge, I know~~—and` |
| Em-dash tail | dash + tail up to the terminator; skipped when the tail starts with a resumptive word (even, these, this, such, ...) | `a touch~~—hay fever, I fancy it was~~.` |
| Comma aside | leading comma + aside, up to the next comma or terminator; skipped when the next clause starts with *but* or *yet* | `my mode of life~~, which some would call impertinent~~, though` |
| Sentence | the gap before it + the sentence (an opener takes the gap after it instead; a list item's opener also takes its marker) | |

Headings, fenced code, tables, block quotes and inline code are never cut. List items are treated as prose: without that, documents that are mostly lists could not reach 50% (see §6.4).

### 5.2 Why the fit step matters (ablation)

All values are deltas from the target, measured by the prototype:

| Fixture | Level | Recommended (all granularities + fit) | Sentences only + fit | Naive rank prefix (no fit) |
|---|---|---:|---:|---:|
| three-men-ch1 | Slight trim | -0.0pp | -0.2pp | +0.4pp |
| three-men-ch1 | Tighten more | +0.3pp | +0.0pp | +0.4pp |
| three-men-ch1 | Even sharper | +0.0pp | +0.4pp | +0.6pp |
| three-men-ch1 | Cut in half | +0.1pp | -0.1pp | +3.5pp |
| walden-economy | Slight trim | +0.0pp | -0.3pp | +0.0pp |
| walden-economy | Tighten more | -0.1pp | +0.0pp | +0.7pp |
| walden-economy | Even sharper | -0.1pp | +0.0pp | **+11.3pp** |
| walden-economy | Cut in half | +0.0pp | -0.6pp | +1.2pp |
| zed-plugin-fit | Slight trim | -0.3pp | -0.3pp | +1.2pp |
| zed-plugin-fit | Tighten more | -0.3pp | -0.8pp | +0.4pp |
| zed-plugin-fit | Even sharper | +0.4pp | +1.4pp | +0.4pp |
| zed-plugin-fit | Cut in half | +0.1pp | +0.6pp | +0.1pp |

Two findings:
- **The fit step is what hits the tolerance.** Taking the ranked list in order until the target is passed fails on two cells, because Walden has a single 114-word sentence (+11.3pp at 30%) and Three Men overshoots at 50% (+3.5pp). With the skip-and-fit passes, even a sentence-only trim lands within 1.4pp.
- **Mixed granularity is about quality, not accuracy.** Sentence-only trims also hit the numbers, but at ~10% they delete whole sentences where the recommended method fades fillers and asides. Fading at the smaller granularity is what makes "Slight trim" feel slight.

### 5.3 Nesting

Levels are nested by construction: everything faded at 10% is still faded at 20%. The UI can therefore compute once and switch between levels instantly. A span the user chose to keep ("Click one to keep it") stays kept when they move to a higher level, as long as the UI applies the keep set as a filter on top of the selection. This is tested in `levels_are_nested`.

## 6. Evaluation

### 6.1 Fixtures and licences

| Fixture | Source | Licence | Words |
|---|---|---|---:|
| `three-men-ch1.md` | Jerome K. Jerome, *Three Men in a Boat* (1889), chapter I, opening; Project Gutenberg eBook #308 | Public domain. Gutenberg header and footer removed; `[Picture: ...]` markers removed | 853 |
| `walden-economy.md` | Henry David Thoreau, *Walden* (1854), "Economy", first three paragraphs; Project Gutenberg eBook #205 | Public domain. Gutenberg header and footer removed | 670 |
| `zed-plugin-fit.md` | This repository, `docs/requirements/zed-plugin-fit.md` (prose paragraphs only: "Answer", the non-goal and hooks paragraphs, "Shared model" and the "Embedded trailing block" item) | MIT (this repository's `LICENSE`) | 401 |

The three cover different registers: chatty comic narrative full of fillers, long periodic 19th-century sentences, and terse modern technical prose full of code spans and references.

### 6.2 Target vs achieved

Copied from `cargo run` output:

| Fixture | Words | Level | Target | Target words | Cut words | Achieved | Delta (pp) | Within +/-3pp |
|---|---:|---|---:|---:|---:|---:|---:|---|
| three-men-ch1 | 853 | Slight trim | 10% | 85 | 85 | 10.0% | -0.0 | yes |
| three-men-ch1 | 853 | Tighten more | 20% | 171 | 173 | 20.3% | +0.3 | yes |
| three-men-ch1 | 853 | Even sharper | 30% | 256 | 256 | 30.0% | +0.0 | yes |
| three-men-ch1 | 853 | Cut in half | 50% | 427 | 427 | 50.1% | +0.1 | yes |
| walden-economy | 670 | Slight trim | 10% | 67 | 67 | 10.0% | +0.0 | yes |
| walden-economy | 670 | Tighten more | 20% | 134 | 133 | 19.9% | -0.1 | yes |
| walden-economy | 670 | Even sharper | 30% | 201 | 200 | 29.9% | -0.1 | yes |
| walden-economy | 670 | Cut in half | 50% | 335 | 335 | 50.0% | +0.0 | yes |
| zed-plugin-fit | 401 | Slight trim | 10% | 40 | 39 | 9.7% | -0.3 | yes |
| zed-plugin-fit | 401 | Tighten more | 20% | 80 | 79 | 19.7% | -0.3 | yes |
| zed-plugin-fit | 401 | Even sharper | 30% | 120 | 122 | 30.4% | +0.4 | yes |
| zed-plugin-fit | 401 | Cut in half | 50% | 201 | 201 | 50.1% | +0.1 | yes |

**The acceptance criterion is met: all 12 cells are within +/-3pp, and the largest deviation is 0.4pp.** The test `every_fixture_level_within_three_points` asserts this. A second test, `reported_cut_matches_text_after_make_the_cuts`, checks that the word count of the text after deleting the spans equals `total - cut`, so the status-card number is honest.

### 6.3 The cuts at "Tighten more" (~20%)

~~Struck~~ text is what the editor would fade. Full renders for every level are in `research/lab-heuristics/out/<fixture>-<pct>.md`.

**zed-plugin-fit**: 401 -> 322 words, -19.7%

> **No.** The Zed plan ~~(`terraphim/zed-terraphim#1`)~~ is a thin `wasm32-wasip2` adapter over LSP, MCP and ACP. Zed's extension API has no way to draw span decorations, add panels, bind hover-plus-arrow keys or set per-span opacity, so most of the spec ~~(§3–§7)~~ cannot be built there. `terraphim-editor`~~, which owns its DOM~~, is the only existing target where the spec can be built as written.
>
> Zed can still get a reduced version through `terraphim_lsp`: AI alternatives as code actions, Lab "mark" results as diagnostics, and an approximate trim preview. That only works if the span model and annotation format are shared between both targets ~~(see "Shared model" below)~~.
>
> Because of the last non-goal, even the thesaurus-backed alternatives provider ~~(R-8.7)~~ cannot run inside the extension. It would have to live in `terraphim_lsp`~~, which the extension starts as a language server~~.
>
> There are no hooks for editor decorations, gutter rendering, panels or views, keymaps or input handling, context-menu items, or text opacity. Inside Zed, the extension can only add UI through an LSP server, using whatever LSP features Zed itself renders.
>
> The span model should **not** go into `EngineEvent`. ~~That contract (`crates/terraphim_engine_events`) covers agent evolution and approvals (`EvolutionProposed/Approved/Rejected/Applied`, `AllowOnce/AllowAlways/Reject/RejectAlways`), not document state.~~
>
> **Sequencing:** today there is only one committed consumer. Zed's Z2 is blocked behind #3224, and terraphim-ai main CI is red ~~(#3325)~~. ~~So the crate starts as a workspace crate inside this repo (`crates/terraphim_alternatives`, `terraphim/terraphim-editor#2`).~~ It is extracted and published only when `terraphim_lsp` adopts it ~~(`terraphim/terraphim-editor#16`, server work in `terraphim/terraphim-ai#3409`)~~.
>
> TACP/`EngineEvent` only needs an event if alternatives can come from an *agent* rather than the editor ~~(e.g. `AlternativesProposed { doc, span_id, alts, source: ai, model }`)~~. ~~That is optional and can wait until an agent actually produces them.~~
>
> **Embedded trailing block ~~(not a sidecar)~~.** The shared crate still owns the schema. It now also parses and writes the trailing fenced block and splits the body from the annotations. In Zed and other LSP clients the block will be visible as raw text, and edits to it are user edits. `terraphim_lsp` must therefore (a) exclude the block from diagnostics, marks and re-anchoring, and (b) treat a hand-edited or malformed block as recoverable: keep the body, report one diagnostic, and do not drop annotations silently. An LSP `foldingRange` for the block would hide it in clients that honour folding; whether Zed collapses LSP folding ranges by default needs checking in the Z0 spike.

**walden-economy**: 670 -> 537 words, -19.9%

> When I wrote the following pages, or rather the bulk of them, I lived alone, in the woods, a mile from any neighbor, in a house which I had built myself, on the shore of Walden Pond, in Concord, Massachusetts, and earned my living by the labor of my hands only. I lived there two years and two months. At present I am a sojourner in civilized life again.
>
> I should not obtrude my affairs so much on the notice of my readers if ~~very~~ particular inquiries had not been made by my townsmen concerning my mode of life~~, which some would call impertinent~~, though they do not appear to me ~~at all~~ impertinent, but~~, considering the circumstances~~, ~~very~~ natural and pertinent. ~~Some have asked what I got to eat; if I did not feel lonesome; if I was not afraid; and the like.~~ Others have been curious to learn what portion of my income I devoted to charitable purposes; and some~~, who have large families~~, how many poor children I maintained. I will therefore ask those of my readers who feel no particular interest in me to pardon me if I undertake to answer some of these questions in this book. In most books, the _I_, or first person, is omitted; in this it will be retained; that~~, in respect to egotism~~, is the main difference. We commonly do not remember that it is~~, after all~~, always the first person that is speaking. ~~I should not talk so much about myself if there were anybody else whom I knew as well. Unfortunately, I am confined to this theme by the narrowness of my experience.~~ Moreover, I, on my side, require of every writer, first or last, a simple and sincere account of his own life, and not merely what he has heard of other men’s lives; some such account as he would send to his kindred from a distant land; for if he has lived sincerely, it must have been in a distant land to me. ~~Perhaps these pages are more particularly addressed to poor students.~~ As for the rest of my readers, they will accept such portions as apply to them. I trust that none will stretch the seams in putting on the coat, for it may do good service to him whom it fits.
>
> I would fain say something, not so much concerning the Chinese and Sandwich Islanders as you who read these pages~~, who are said to live in New England~~; something about your condition~~, especially your outward condition or circumstances in this world~~, in this town, what it is, whether it is necessary that it be as bad as it is, whether it cannot be improved as well as not. ~~I have travelled a good deal in Concord; and everywhere, in shops, and offices, and fields, the inhabitants have appeared to me to be doing penance in a thousand remarkable ways.~~ What I have heard of Brahmins sitting exposed to four fires and looking in the face of the sun; or hanging suspended, with their heads downward, over flames; or looking at the heavens over their shoulders “until it becomes impossible for them to resume their natural position, while from the twist of the neck nothing but liquids can pass into the stomach;” or dwelling, chained for life, at the foot of a tree; or measuring with their bodies, like caterpillars, the breadth of vast empires; or standing on one leg on the tops of pillars,—even these forms of conscious penance are hardly more incredible and astonishing than the scenes which I daily witness. The twelve labors of Hercules were trifling in comparison with those which my neighbors have undertaken; for they were only twelve, and had an end; but I could never see that these men slew or captured any monster or finished any labor. They have no friend Iolas to burn with a hot iron the root of the hydra’s head, but as soon as one head is crushed, two spring up.

**three-men-ch1**: 853 -> 680 words, -20.3%

> There were four of us—George, and William Samuel Harris, and myself, and Montmorency. ~~We were sitting in my room, smoking, and talking about how bad we were—bad from a medical point of view I mean, of course.~~
>
> We were all feeling seedy, and we were getting ~~quite~~ nervous about it. Harris said he felt such extraordinary fits of giddiness come over him at times, that he hardly knew what he was doing; and then George said that _he_ had fits of giddiness too, and hardly knew what _he_ was doing. With me, it was my liver that was out of order. I knew it was my liver that was out of order, because I had ~~just~~ been reading a patent liver-pill circular, in which were detailed the various symptoms by which a man could tell when his liver was out of order. I had them all.
>
> It is a most extraordinary thing, but I never read a patent medicine advertisement without being impelled to the conclusion that I am suffering from the particular disease therein dealt with in its most virulent form. The diagnosis seems in every case to correspond exactly with all the sensations that I have ever felt.
>
> I remember going to the British Museum one day to read up the treatment for some slight ailment of which I had a touch~~—hay fever, I fancy it was~~. I got down the book, and read all I came to read; and then, in an unthinking moment, I idly turned the leaves, and began to indolently study diseases~~, generally~~. I forget which was the first distemper I plunged into~~—some fearful, devastating scourge, I know~~—and, before I had glanced half down the list of “premonitory symptoms,” it was borne in upon me that I had ~~fairly~~ got it.
>
> I sat for awhile, frozen with horror; and then, in the listlessness of despair, I again turned over the pages. I came to typhoid fever~~—read the symptoms~~—discovered that I had typhoid fever, must have had it for months without knowing it~~—wondered what else I had got; turned up St. Vitus’s Dance~~—found~~, as I expected~~, that I had that too,~~—began to get interested in my case, and determined to sift it to the bottom, and so started alphabetically~~—read up ague, and learnt that I was sickening for it, and that the acute stage would commence in about another fortnight. Bright’s disease, I was relieved to find, I had only in a modified form, and~~, so far as that was concerned~~, I might live for years. ~~Cholera I had, with severe complications; and diphtheria I seemed to have been born with.~~ I plodded conscientiously through the twenty-six letters, and the only malady I could conclude I had not got was housemaid’s knee.
>
> I felt rather hurt about this at first; it seemed ~~somehow~~ to be a ~~sort of~~ slight. Why hadn’t I got housemaid’s knee? Why this invidious reservation? After a while~~, however~~, less grasping feelings prevailed. I reflected that I had every other known malady in the pharmacology, and I grew less selfish, and determined to do without housemaid’s knee. ~~Gout, in its most malignant stage, it would appear, had seized me without my being aware of it; and zymosis I had evidently been suffering with from boyhood.~~ There were no more diseases after zymosis, so I concluded there was nothing else the matter with me.
>
> I sat and pondered. I thought what an interesting case I must be from a medical point of view, what an acquisition I should be to a class! Students would have no need to “walk the hospitals,” if they had me. I was a hospital in myself. All they need do would be to walk round me, and, after that, take their diploma.
>
> Then I wondered how long I had to live. ~~I tried to examine myself.~~ I felt my pulse. I could not at first feel any pulse ~~at all~~. ~~Then, all of a sudden, it seemed to start off. I pulled out my watch and timed it. I made it a hundred and forty-seven to the minute.~~ I tried to feel my heart. I could not feel my heart. It had stopped beating. I have since been induced to come to the opinion that it must have been there all the time, and must have been beating, but I cannot account for it. I patted myself all over my front, from what I call my waist up to my head, and I went ~~a bit~~ round each side, and a little way up the back. But I could not feel or hear anything. I tried to look at my tongue. I stuck it out as far as ever it would go, and I shut one eye, and tried to examine it with the other. I could only see the tip, and the only thing that I could gain from that was to feel more certain than before that I had scarlet fever.
>
> I had walked into that reading-room a happy, healthy man. ~~I crawled out a decrepit wreck.~~

A reading note on the Three Men render: in "too,~~—began ... alphabetically~~—read up ague, ... fortnight. Bright’s disease, ... and~~, so far as that was concerned~~", the text between the two struck runs is kept. Only the dash aside "—began ... alphabetically" and the comma aside ", so far as that was concerned" are faded.

### 6.4 Spans at "Cut in half" (zed-plugin-fit), with the level that first fades each

| First level | Granularity | Words | Reason | Span |
|---|---|---:|---|---|
| Slight trim | Clause | 2 | parenthetical | (`terraphim/zed-terraphim#1`) |
| Slight trim | Clause | 2 | parenthetical | (§3–§7) |
| Slight trim | Clause | 4 | aside "which" | , which owns its DOM |
| Slight trim | Clause | 4 | parenthetical | (see "Shared model" below) |
| Slight trim | Clause | 1 | parenthetical | (R-8.7) |
| Slight trim | Clause | 1 | parenthetical | (`crates/terraphim_engine_events`) |
| Slight trim | Clause | 2 | parenthetical | (`EvolutionProposed/Approved/Rejected/Applied`, ...) |
| Slight trim | Clause | 1 | parenthetical | (#3325) |
| Slight trim | Clause | 3 | parenthetical | (`crates/terraphim_alternatives`, `terraphim/terraphim-editor#2`) |
| Slight trim | Clause | 7 | parenthetical | (`terraphim/terraphim-editor#16`, server work in `terraphim/terraphim-ai#3409`) |
| Slight trim | Clause | 8 | parenthetical | (e.g. `AlternativesProposed { ... }`) |
| Slight trim | Word | 1 | filler "actually" | actually |
| Slight trim | Clause | 3 | parenthetical | (not a sidecar) |
| Tighten more | Clause | 8 | aside "which" | , which the extension starts as a language server |
| Tighten more | Sentence | 13 | weak sentence | That contract (`crates/terraphim_engine_events`) covers agent evolution ... |
| Tighten more | Sentence | 14 | weak sentence | So the crate starts as a workspace crate inside this repo ... |
| Tighten more | Sentence | 12 | weak sentence | That is optional and can wait until an agent actually produces them. |
| Even sharper | Sentence | 18 | weak sentence | `terraphim-editor`, which owns its DOM, is the only existing target ... |
| Even sharper | Sentence | 13 | weak sentence | Zed's Z2 is blocked behind #3224, and terraphim-ai main CI is red (#3325). |
| Even sharper | Sentence | 17 | weak sentence | It is extracted and published only when `terraphim_lsp` adopts it ... |
| Even sharper | Sentence | 7 | weak sentence | The shared crate still owns the schema. |
| Cut in half | Sentence | 17 | weak sentence | It now also parses and writes the trailing fenced block ... |
| Cut in half | Sentence | 34 | weak sentence | `terraphim_lsp` must therefore (a) exclude the block from diagnostics ... |
| Cut in half | Sentence | 28 | weak sentence | An LSP `foldingRange` for the block would hide it in clients ... |

### 6.5 Robustness on whole repository documents (not fixtures)

`cargo run -- <file>` prints the table for any file. Run on the full Markdown documents, which include tables, lists and code:

| File | Words | 10% | 20% | 30% | 50% |
|---|---:|---:|---:|---:|---:|
| `docs/requirements/alternative-control.md` | 2497 | 10.0% | 20.0% | 29.9% | 50.0% |
| `docs/requirements/zed-plugin-fit.md` | 1621 | 9.9% | 19.9% | 30.1% | 50.0% |
| `README.md` | 187 | 9.6% | 19.8% | 29.9% | 50.3% |

An earlier version protected list items. With that version, `zed-plugin-fit.md` and `README.md` reached only about 33% at "Cut in half", because protected words count in the denominator but cannot be cut. A document made mostly of tables or code can still fall short. In that case the status card must report the achieved percentage honestly (for example `-38%` on "Cut in half") instead of pretending to hit 50%.

### 6.6 Trim failure modes seen on the fixtures

- **Dependent clauses.** In Walden, removing ", considering the circumstances" and "very" leaves "impertinent, but, natural and pertinent", which has a stray comma. Two guards came out of this spike: resumptive dash tails are skipped (before that guard, the main clause "—even these forms of conscious penance are ..." was cut and left a fragment), and comma asides followed by *but* or *yet* are skipped. A punctuation tidy-up when the cuts are applied (collapse `, ,` and drop a comma left after a conjunction) would be a mechanical fix-up of the same kind as the a/an rule (R-2.6), and needs a decision because it edits text outside the spans.
- **Capitalisation.** Fading a sentence-initial hedge ("~~Perhaps~~ these pages") leaves a lower-case start after "Make the cuts". The fix-up is the same kind as above.
- **References in technical prose.** In `zed-plugin-fit`, parentheticals holding issue numbers and paths are the first things faded. For a requirements document that removes traceability. A rule worth adding: never fade a parenthetical that contains `#\d+`, `R-\d`, or a backticked identifier, or make it a per-role setting.
- **Voice.** Comic sentences ("I crawled out a decrepit wreck.", "Then, all of a sudden, it seemed to start off.") read as weak to every feature. The user's "Click one to keep it" is the intended safety net, which is why the review UI matters as much as the heuristic.

- **Fixture-shaped aside openers.** Six entries in `ASIDE_OPENERS` (`src/lib.rs`) were added after reading these fixtures and are not general connectives: "i fancy", "as i expected", "so far as", "in respect to", "in its most", "if any". They influence *which* clauses are faded, not whether the targets are hit. Ablation (2026-10-04, from the structural review of PR #25): with those six removed, all 12 cells stay within +/-3pp (largest deviation -0.6pp, Walden at 10%), and the 20 tests pass. The production list should start from the general connectives only and grow from labelled documents, not from these fixtures.

## 7. What the UI needs from the API (#14, #15)

The heuristics belong on the Rust side (in `crates/terraphim_alternatives`, compiled to WASM), in line with spec §11. The surface below is a proposal. The prototype implements everything except `kept`, `SpanId` and the `mark` entry point.

```rust
pub struct LabInput<'a> {
    pub body: &'a str,          // Markdown body with the trailing annotation block already stripped (decision 2)
    pub lists: &'a LabLists,    // compiled from embedded defaults + role overrides (§3.2)
    pub kept: &'a [SpanId],     // spans the user clicked to keep; excluded from selection
}

pub enum Granularity { Word, Clause, Sentence }

pub struct LabSpan {
    pub id: SpanId,             // stable: hash of (granularity, anchor text, occurrence index)
    pub start: usize,           // byte offsets into `body`, on char boundaries
    pub end: usize,
    pub granularity: Granularity,
    pub reason: Reason,         // Filler("quite") | Hedge("perhaps") | Aside("which") | Parenthetical | DashAside | WeakSentence | ...
    pub score: f32,             // host-sentence weakness, for display or debugging
    pub words: u32,             // words this span removes on its own
}

pub struct TrimPlan {
    pub total_words: u32,
    pub spans: Vec<LabSpan>,                 // every span selected at any level, in document order
    pub first_level: Vec<u8>,                // parallel to `spans`: 0..=3 = 10/20/30/50
    pub levels: [LevelSummary; 4],           // target_words, cut_words, achieved_pct
}

pub struct Mark { pub span: LabSpan, pub action: MarkAction } // WeakSentence | Hedge | Filler | Tone | Long | Convoluted

pub fn trim_plan(input: &LabInput) -> TrimPlan;
pub fn mark(input: &LabInput, action: MarkAction) -> Vec<Mark>;
```

What the UI gets from this:
- **One call per document version.** Switching between Original, 10%, 20%, 30% and 50% is a filter on `first_level <= L`, with no recompute, because the levels are nested. Recompute only after an edit, with a debounce.
- **Span granularity.** Word, clause and sentence spans are all ordinary ghost spans (R-5.3). Overlaps are resolved by the API: when a sentence is selected, the UI shows the sentence span. The fillers inside it remain in `spans` for the case where the user keeps the sentence.
- **Keep.** "Click one to keep it" adds the id to `kept`. The next `trim_plan` call keeps that span out and re-fits the level, so the card's `-P%` stays accurate. If a kept span overlaps a selected sentence, keeping the sentence keeps everything inside it.
- **Walk-through order: document order, not rank order.** The reader judges each cut in context. Jumping around the document by score would force them to re-read surrounding text at every step. `spans` is already sorted by `start`. Within a walk-through, the UI can show `reason` ("filler: quite", "weak sentence") as the explanation.
- **The status card** (`535 -> 480 words · -10%`) comes from `total_words` and `levels[L].cut_words` (minus kept). It uses the same word definition as the editor's word count: word characters joined by `'`, `’`, `-`, `.` or `/`, with em dashes and Markdown syntax as separators.
- **Mark actions share the type.** `mark(WeakSentence)` returns the top 15% of sentences by `weakness`, excluding sentences under 5 words. `mark(Hedge | Filler | Tone)` returns the KG matches. Showing marks and trims together is just a union of spans.
- **Anchoring.** Trim spans are transient (they exist only while the card is open), so they do not need the R-9.2 re-anchoring. "Make the cuts" deletes them from a known document version. If the user edits while the card is open, discard the plan and recompute.

## 8. Open questions for review

1. Should "Make the cuts" apply a punctuation and capitalisation tidy-up (§6.6)? That would edit a character or two outside the faded spans.
2. Should parentheticals with references be protected by default, or only for a "technical" role?
3. Is 15% the right share for "Mark the weakest sentences", or should it be a fixed count per 500 words?
4. Should paragraph openers be protected for the weakest-sentence mark as well as for trim?
5. When an LLM provider arrives (after v1), should it re-rank these deterministic candidates (keeping the never-rewrite guarantee), or propose its own spans?

## 9. Reproduce

```bash
# from the repository root; the crate is outside the workspace ([workspace] table in its Cargo.toml)
cargo test --manifest-path research/lab-heuristics/Cargo.toml   # 20 tests, no mocks
cargo run  --manifest-path research/lab-heuristics/Cargo.toml   # prints the table, writes research/lab-heuristics/out/
cargo run  --manifest-path research/lab-heuristics/Cargo.toml -- README.md docs/requirements/alternative-control.md
```

Why a Rust crate rather than Python: the production code will be Rust compiled to WASM (spec §11), the matcher is the same `aho-corasick` crate that `terraphim_automata` uses, and the byte-offset and `char`-boundary handling the UI depends on is exercised exactly as it will be in production. The only dependency is `aho-corasick`.

Layout:

```
research/lab-heuristics/
  Cargo.toml            standalone crate, empty [workspace] table
  src/lib.rs            word definition, KG loading and matching, sentence splitting, weakness, candidates, trim
  src/main.rs           runs the fixtures and writes out/
  tests/acceptance.rs   +/-3pp acceptance, nesting, determinism, protection, word and sentence rules, KG matching
  kg/                   lab-filler, lab-hedge, lab-hedge-phrase, lab-tone (Terraphim KG markdown)
  kg/domain/            stand-in role thesaurus for KG concept coverage
  fixtures/             the three documents in §6.1
  out/                  generated: report.md, <fixture>-<pct>.md renders, thesaurus.json
```
