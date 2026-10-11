# Independent native VS Code review — 2026-10-11

Finite saved outputs now qualify the ghost persistence regression on this native fixture. All 41 requirements remain unaccepted in full. The prior ghost loss and Save-button click failure are retained; CmdS success does not establish Save-button success.

Independently read all seven public saved files and checked every span/ghost UTF-16 anchor slice. `before-ghost-fix.saved.md` lacks ghost and exactly equals `ghost-undo-saved.md` (692bytes, hashc4cf9052…). `ghost-saved.md` has one valid ghost42–59 and active thumbtack alternative (931bytes, hash8b92c314…08424). Both after/redo receipts match that actual saved snapshot hash; their `path` historically points to writing.md, whose current contents have since advanced. This is not a contradiction when receipts are treated as historical snapshots. Stash removes the exact paragraph into Overflow (928bytes); pull restores it once to body and retainsOverflow (982bytes), matching final writing.md. No drag/drop result is inferred from these shortcut outputs.

Export actual77-byte file exactly equals the receipt's literal expected string, SHA0cfe09781de48df01b80fd7c2a6dd5be61ec829a4dd33c352382410b48de3512. Ghost phrase and annotation block are absent, pulled paragraph appears once, final two newlines retained. General punctuation/whitespace cleanup is not inferred beyond this literal example.

Installed-WASM reopen receipt contains three concept 1 queries, literal `synonyms:: paperclip, eraser, thumbtack`, with ghost count1→0→1 for saved/undo/final files. This is reported real installed-WASM execution on native saved bytes, not a native custom webview KG query. The reviewer checked receipt/file consistency but did not rerun native UI or independently execute those queries.

Raw video independently hashed `682a62a5ebafebcbadcc47c6344e81690ad8ac5811b0201cc55da933044f37eb`; ffprobe352.140000s,8,144,259bytes,H2641140×720/yuv420p. Recorder normal finish log uses rounded351s/pre-finalization byte count. Inspected18 samples about10,30,…350s and full-resolution210/270s. Samples show public writing.md, Restricted Mode editor in an owned public window, ghost dimming, stash body removal andpull restoration and retained Overflow. No personal avatar/desktop was visible in these bounded samples. Full resolution needed for small controls. This recording starts from existing three alternatives: their input creation is not demonstrated here. It does not record the later r14 CSS build, and no r14 native acceptance is claimed.

Source fixes reviewed: adapter subscribes annotation-only `te:ghosts-change`; save/export synchronously serialise+submit before command barrier (includingOverflow flush); provider save returning false throws and rejects. Native after saved bytes qualify the one previously missing ghost. Other host focus/history/export concurrency clauses remain unaccepted.

CSS regression independent log review: before Save/Open hit-test returns `save covered by te-alt-panel; open covered by te-alt-panel`; after minimal chrome group z-index 21 change, real Chrome WASM test: 1 PASS. The test actually uses `elementFromPoint`, avoiding programmatic click bypass, but does not itself click Save/Open or prove native host callback. Firefox log contains startup without result and is UNQUALIFIED. Clean affee07/r14 assets are later candidate build evidence, not the earlier video identity.

| Requirement | Status | Finite evidence / remaining scope |
|---|---|---|
| R-2.1 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-2.2 | PARTIAL: finite native evidence, full clause unaccepted | Saved s1 word anchor10–19, paperclip/eraser/thumbtack, active2; this video starts from existing alternatives, not their creation. |
| R-2.3 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-2.4 | PARTIAL: finite native evidence, full clause unaccepted | Final thumbtack output exists; actual cycling operation not qualified by this recording. |
| R-2.5 | PARTIAL: finite native evidence, full clause unaccepted | Later native removal/undo/redo saved snapshots retain valid alternatives and ghost+Overflow; finite active2→0→2 history, not allgranularities. |
| R-2.6 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-2.7 | PARTIAL: finite native evidence, full clause unaccepted | Native deletion of thumbtack leaves paperclip+eraser and saved KG membership excludes thumbtack. Emptying every non-original alternative/indicator disappearance still NOT DEMONSTRATED. |
| R-3.1 | PARTIAL: finite native evidence, full clause unaccepted | Word underline visible210/270s; exact width/baseline/theme not measured. |
| R-3.2 | PARTIAL: finite native evidence, full clause unaccepted | Word dots visible210/270s; exactgeometry/countforall granularities not qualified. |
| R-3.3 | PARTIAL: finite native evidence, full clause unaccepted | Active dot styling visible; full RGB/alpha/transitions not independently measured. |
| R-3.4 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-3.5 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-3.6 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-4.1 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-4.2 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-4.3 | PARTIAL: finite native evidence, full clause unaccepted | Actual alternative panel/list visible in r14 history samples; full tabs/input creation not qualified. |
| R-4.4 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-4.5 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-4.6 | PARTIAL: finite native evidence, full clause unaccepted | Native alternative removal Undo exactlyrestores pull-saved3altactive2, Redo exactlyrestores2altoriginal; GhostUndo/Redo and ReviveUndo separately saved. Full panel-input/bot-linehistory remains unaccepted. |
| R-5.1 | PARTIAL: finite native evidence, full clause unaccepted | Native ghost visibly dims phrase; saved UTF-16 42–59 equalsGhost this aside.; exactopacity/countallcases absent. |
| R-5.2 | PARTIAL: finite native evidence, full clause unaccepted | Native contextmenuRevive followed by actualSave removesghost; Undo savedfinalwriting equals priorpull includingghost. Exact allrange cases absent. |
| R-5.3 | PARTIAL: finite native evidence, full clause unaccepted | Ghost annotation persists alongside alternatives and laterOverflow, all saved anchors valid. |
| R-6.1 | PARTIAL: finite native evidence, full clause unaccepted | ActualrightOverflowpanelvisible210/270s; keyboard toggle behavior notfullyqualified. |
| R-6.2 | PARTIAL: finite native evidence, full clause unaccepted | Overflow contains exact stashed paragraph; editing/arbitrary Markdown behavior notqualified. |
| R-6.3 | PARTIAL: finite native evidence, full clause unaccepted | Saved stash removes paragraph from body, puts exactliteralinto Overflow; native frame210s. |
| R-6.4 | PARTIAL: finite native evidence, full clause unaccepted | Pull body restores paragraph and retainsOverflow copy; native frame270s. No native drag/drop success asserted. |
| R-6.5 | PARTIAL: finite native evidence, full clause unaccepted | Saved pull andfinal writing exact 982-byte source with Overflow+ghost+alternatives. |
| R-7.1 | PARTIAL: finite native evidence, full clause unaccepted | Embedded custom editor layout visible; external Restricted Mode chrome and window inset differ from source video. Exact parity unaccepted. |
| R-7.2 | PARTIAL: finite native evidence, full clause unaccepted | Later r14 nativeSave coordinateclick whileAltpanelopen produces saved2altremoval and reportedcleandirty state; basicSave qualified. EarlieroccludedSave failure retained; Open/allcontrols notfullyqualified. |
| R-7.3 | PARTIAL: finite native evidence, full clause unaccepted | Ghost and stash operations observedvia native inputs; complete context menu contents notqualified. |
| R-7.4 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-8.1 | PARTIAL: finite native evidence, full clause unaccepted | Native Labpopover visible; bundledlocalrole enables actualprotectedtrim. Exactpixel/focus/outsideclickclauses notfullyqualified. |
| R-8.2 | PARTIAL: finite native evidence, full clause unaccepted | Actualnative markobservations ononefixture only; zero long/convoluted results validobservations, no semanticquality proof. |
| R-8.3 | PARTIAL: finite native evidence, full clause unaccepted | Native Slight101→91 andSharper101→71 observed; allfivelevels notdemonstrated. |
| R-8.4 | PARTIAL: finite native evidence, full clause unaccepted | Donebody unchanged;9newvalidghosts30words;2old detachedannotationspreserved/warned andexcluded. Exactprotected-range/targetconstraints notallqualified. |
| R-8.5 | PARTIAL: finite native evidence, full clause unaccepted | Done/Undo/Redo/reloadSave exactbytes; Walk/Keep/Make/Next/Previous absent. |
| R-8.6 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-8.7 | NOT DEMONSTRATED | No refreshed native operation evidence for this clause; browser/protocol results do not transfer. |
| R-9.1 | PARTIAL: finite native evidence, full clause unaccepted | Native TextDocument/disk actual Markdown plus annotations. |
| R-9.2 | PARTIAL: finite native evidence, full clause unaccepted | Ninevalidnewtrimanchors independentlysliced;2detachedexistingannotations intentionallypreserved, notcountedasvalid. |
| R-9.3 | PARTIAL: finite native evidence, full clause unaccepted | Literal actual export 77 bytes equals independent expected, drops ghost and annotation block, includes pulled body once. |

## Supplemental r14 native controls and history evidence

This is a separate later recording, `/private/tmp/vscode-native-controls-history-demo-20261011.mp4`, SHA-256 `1f650535c49d54240be8ed1be3a471f6ddb3c6b329471ad49c17d600f3370f08`, 4,563,856bytes,189.251667s. Thirteen15-second samples show actual alternative panel, visibleSave/Open, wordchanges, nativeRevive menu and restoredghost. The parent reports a real coordinateSaveclick and dirty-cleared state; actual saved removal bytes independently corroborate the resulting saved transaction. This qualifies basicSave on this later build/window; the earlier occluded click remains a preserved failure. Parent reports only the sharedCSS asset/source changed between builds; the reviewer has not independently reconstructed the installed122-asset comparison here. No isolated VSCode profile is claimed: this was the normal profile in RestrictedMode, with an isolated owned public window.

Independently verified `alternative-removal-saved.md` has two alternatives, originalactive0, onevalidghost, SHA `c097979af98c2ff0b1d5103e0039f4ea118e87e03cf64e482099bb7377849e1b`. `alternative-undo-saved.md` exactlyequals prior `pull-saved.md` (three alternatives, active2, oneghost; SHA b1524fc7…2ef81); `alternative-redo-saved.md` exactlyequals removal. `revive-saved.md` removes the ghost while retaining allthree alternatives andOverflow; itsSHA `1641c1372738d7340e617c0bed522b92ebaf314710273ef6299bc973e532500d`. Finalwriting again exactlyequals pull, corroborating Undo ofRevive. Every anchor slice remains valid.

Installed-WASM history receipt reports exact authoredgroup paperclip/eraser afterremoval andredo, withthumbtack absent; Undo restores allthree underlocalconcept1. This remains installed-WASM reopening actualnativefiles, not nativewebviewquery proof. Manualremoval ofonealternative does not qualify emptyingallnon-originalalternatives/indicatorremoval; currentwordpanel input/generation/AIhistory/full41 remainunaccepted. Firefoxhit-test staysUNQUALIFIED.

## Supplemental local-role Lab evidence

The adapter now fetches bundled `roles/default.json`, checks HTTPstatus and invokes the existing shared `loadThesaurus`; ordinary load/parse failures show a notice and continue editor initialization. Source review found no definite new blocker. The16th real-WASM test checks bundled-role loading, actual protected trim availability and KG decision→judgment lookup; it does not exercise adapter fetch failure/startup UI. The reviewed log reports16PASS,0FAIL. An indefinitely stalled fetch has no explicit timeout, so failure recovery is only established for requests that resolve/reject, not a never-settling request.

Separate native video `/private/tmp/vscode-native-local-role-lab-demo-20261011.mp4`, SHA-256 `107d297c622000ec58bfdb73f5bc48feebc8e036f282ae0d98b6122f07c2c410`; ffprobe313.305s,13,350,742bytes. Eighteen18-second samples show Lab popover, protected trim controls, dimmed proposals, mark results and explicit detached-annotation warning. No personal avatar or unrelated content was observed in this bounded inspection; small result labels require full-resolution playback. This remains an owned public window in the normal RestrictedMode profile.

Independently compared all five actual `lab-*.md` files. Done preserves baseline body exactly and adds nine new valid ghosts g2–g10 totaling30 whitespace-delimited words. All nine UTF16 ranges independently equal their own text. Baseline has one stale span and one stale ghost; these two detached annotations remain preserved and explicitly warned, and are excluded from new valid ranges/word totals. Undo bytes exactly equal baseline SHA538aeeec…63934; Done/Redo/webview-reload-then-Save all exactly equal SHAb6b8da8d…212fe. Reloaded bytes prove persistence of this one Done result, not universal reattachment of malformed/stale annotations.

Native receipt reports Slight101→91 and Sharper101→71 previews, eight finite mark observations (including zero long/convoluted marks). Counts and labels alone do not establish semantic correctness, precision/recall or all five levels. Walk/Keep/Make/Next/Previous and full provider lifecycle remain unqualified by this fixture. R-8 rows below receive supplemental finite evidence without full acceptance.
