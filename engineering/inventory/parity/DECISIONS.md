# Owner decisions ledger

Recorded for H-owner-decision-ledger (#2349, #2339). Source: the gap audit at `06dde7895`, every entry carrying a `ruling` field.

## Rule

On 2026-11-05 Ilya ruled that every open product question implements the **baseline behaviour** (`1065e9d8`, read with `git show 1065e9d8:src/js/<file>`) unless a deviation is listed under "Exceptions to baseline" below. A typed `unavailable` refusal is not an answer to a product question. If the baseline itself cannot settle a choice, or the work would need a legacy writer, the work stops and the question goes back to the owners.

This also covers open questions that have no entry below (for example key colour format beyond C-key-color, bake cache and render queue ownership, gradients over two stops, multi-frame operations, custom shaders, native curve editor, bake/break apart, lip sync, component preview window, duplicator effectors, Direct Select centerline edit): baseline behaviour.

Under section 7 of the execution plan, product decisions are coordinated with the other team, and both humans accept the remaining limitations at promotion. Cyrill must therefore see the exceptions below before promotion.

## Exceptions to baseline

| Id | Issue | Decision |
|---|---|---|
| T-split-chars-raster | #2267 | Raster text and "Split by character" are retired on native. Vector text with Text Animator selectors replaces them. |
| N-storyboard-frozen | #2233, #2116 | StoryBoard stays frozen, as in the baseline build. Its surfaces are recorded as a disposition and are not implemented. |

## Smaller documented differences

- An eraser press that misses writes nothing. The baseline promoted a keyframe and added an undo entry.
- Eraser bites are one engine revision each, but one undo step for the whole stroke.

## Decisions that implement the baseline

| Id | Issue | Question | Answer |
|---|---|---|---|
| F-sdk-undogroup | #2354 | How many history entries does a script run make? | One. The engine opens and commits a history group around the drain. |
| F-elements-rename-names | #2376 | Where is an element name stored? | Optional name on the item record, written through `item.edit`. Older files still load. |
| F-elements-vis-solo | #2376 | What do eye and solo mean in Elements? | Hidden is a saved item flag honoured by render, export and hit-test. Solo lasts for the session only. |
| F-history-clear | #2376 | What does History Clear do? | Engine `history.clear` drops undo and redo entries and keeps content and revision lineage. |
| A-image-text-flip-skew | #2372 | Can images and text blocks flip and shear? | Yes. Optional stored flipX/flipY and shear; the refusal is removed. |
| D-rive-route | #2357 | Who talks to the Rive Editor MCP for Rive export? | The Rust host, inside `job.export.rive.*`. Unreachable gives a typed unavailable. No JS fallback. |
| D-audio-decode | #2359 | Where are mp3/ogg/m4a decoded? | In the webview with WebAudio, as in the baseline. Rust stores the sealed encoded bytes and metadata. Undecodable files get a typed refusal. |
| D-embedded-video | #2359 | What does "Embedded" video mean? | The encoded file is sealed in the project under the retention budget. Over budget it stays linked with a typed `too_large`. Convert copies in and out. |
| D-reference-video | #1698 | How is a video reference decoded? | Live, on the native decoder session. Linked locator, viewport only, not exported. |
| C-key-color | #2524 | How is a key colour label stored? | Optional hex string `color` on the key, absent means none, kept verbatim. `track.edit` takes `colorKeys{color\|null}`. |
| C-revision-view | #2371 | What does Revision view do? | Baseline semantics from `timeline.js@1065e9d8`: revision tag per cell, filter shown read-only. |
| C-motion-inline-rows | #2521, #2522, #2525 | Where do Motion property rows live? | Inline under each layer as in the baseline, sharing one presenter with the inspector. Panel and grid use one list. |
| S-team-sync | #2172, #2355, #2373 | How does team sync behave? | Baseline: owner fields on native records, per-strokeId union, conflict clones with `revisionParentId`, Revision view. |
| Z-parent-b | #2116 | What is Parent B? | Second parent blended with the first. Saved format mirrors `parentLayerUidB`. |
| AA-reparent-expr | #2116 | Reparent a layer with expressions or animated transforms? | The pose is re-solved so the layer stays put at the current frame. The refusal is removed. |
| A-legacy-import-1-core, -2-animation, -3-media; D-legacy-import-save-semantics | #2355, #1049 | How is an old project opened and saved? | As a new native document named after the old file. First Save goes through Save As. The old file is never modified. A required field with no native owner is a typed refusal; anything else opens with a visible warning. |
| D-team-review-in-denominator | #2355, #2381 | Is shared-folder team review in the baseline denominator? | Yes. In scope and implemented natively. |
| D-retire-delete-vs-gate | #2355 | Delete retired writers or hide them behind a gate? | Delete them from production source at promotion. No hidden gate. Git history and R24 fixtures keep the baseline. |
| D-cs1b-mcp-trust-boundary | #2363 | Is MCP-side pre-validation a trust boundary? | Keep it, generated from the same fragments as the engine. The engine stays the authority. |
| H-owner-decision-ledger | #2349, #2339 | Default for open product questions? | Baseline behaviour; deviations listed here. |

## Not covered, human gates

These stay with the humans and are not decided by this ledger:

- The plan amendment, PR #2352 (CS9 #2344), which approves the baseline `1065e9d8`.
- The review of the medium and low confidence rows in `mapping.json`.
- Final promotion, #2368.

## Résumé pour Cyrill

Ilya a tranché le 2026-11-05 : toute question produit ouverte suit le comportement de la baseline. Exceptions à valider avant la promotion :

- Le texte raster et « Split by character » sont retirés sur le moteur natif, remplacés par le texte vectoriel et les sélecteurs du Text Animator (#2267).
- StoryBoard reste gelé comme dans la baseline ; il n'est pas implémenté (#2233, #2116).
- Un clic de gomme qui rate n'écrit rien (la baseline créait une clé et une entrée d'annulation).
- Chaque morsure de gomme est une révision du moteur, mais un seul pas d'annulation pour le trait.

Selon la section 7 du plan, ces choix sont coordonnés entre les deux équipes et acceptés par les deux humains à la promotion.
