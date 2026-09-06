# R03 document browser acceptance

This opt-in harness checks the browser portion of the R03 document-contract fixture gate. It
loads the committed migration and text projects through Nemo's real browser Open flow in
headless Chromium. The application performs `importJSON` and `exportJSON`; the harness then
checks the resulting documents against the independently committed expectations. Node writes
the returned `exportJSON()` string to disk for reopening; browser Save/download UI is not
exercised.

The harness executes these logical expectations after the original import and again after
opening the application's exported round-trip document in a fresh browser context:

- `migration.migration.matteSourceUid`
- `migration.migration.easingCurveDefault`
- `migration.migration.legacyStrokeFallback`
- `migration.migration.tweenSpanKey`
- `migration.migration.framePadding`
- the eleven `text.document.*`, `text.bounds.*`, `text.text-units.*`, and
  `text.text-group.*` expectations in `text/expected.json`

The white legacy stroke fallback accepts equivalent CSS hex spellings (`#fff`, `#ffffff`,
or opaque `#ffffffff`); its independent expectation is still the committed `#fff` value.
Every other expectation is compared structurally and exactly.
The glyph bounds expectation uses the inserted, loaded Paper.js path's actual curve bounds
after export and again after fresh reopen. Segment anchor bounds alone cannot detect a
changed Bezier handle. This bounds check does not establish full shape/topology equality for
arbitrary edits that preserve the same bounding rectangle.

Playwright remains external to the repository. Point `NEMO_PLAYWRIGHT_MODULE` at an installed
Playwright module when it is not resolvable from this checkout:

```sh
NEMO_PLAYWRIGHT_MODULE=/absolute/scratch/node_modules/playwright \
  node tests/fixtures/browser-acceptance.cjs /absolute/output/r03-browser-acceptance
```

The driver uses a port-zero loopback runtime, four fresh browser contexts, and the browser's
temporary profile. It verifies served application bytes against the checkout and checks the
runtime identity before and after the workflow. Its JSON receipt records the source, fixture,
expectation, browser, Playwright, and output hashes. The task-owned runtime root is removed
after Chromium and the server close; only the requested output directory remains.
`NEMO_CHROME_EXECUTABLE` can select a Chrome/Chromium binary outside the platform's usual
install path; the receipt hashes the exact executable used.

To run permanent negative controls after the baseline, add `--negative-controls`:

```sh
NEMO_PLAYWRIGHT_MODULE=/absolute/scratch/node_modules/playwright \
  node tests/fixtures/browser-acceptance.cjs /absolute/output/r03-controls --negative-controls
```

The baseline remains 16 independent expectations executed twice (32 executions). Controls
use four additional fresh contexts and are recorded separately in `negativeControls`:

- Change the inserted `s_text_2` glyph's first `handleOut` to `[0, -100]`.
- Translate the inserted glyph by `[1, 0]`.

Each control exports through the real application, writes a separate round-trip artifact,
and reopens it in a fresh context. It verifies the corruption was serialized, all segments
and closed topology survived reopening, and the same geometry assertion rejects the loaded
bounds. A missing glyph, unrelated exception, lost corruption, or false pass fails the run.
The receipt and both exported/reopened control documents retain the evidence. Baseline and
control runs close their contexts, browser and owned runtime; controls do not edit fixtures,
expectations, or application source.

This is document-backend acceptance. It does not render the text pixel gate, benchmark a GPU,
exercise browser Save/download UI, open a native GUI, build or run Tauri, exercise native file
dialogs, or establish package/release acceptance or full R03 closure. Fixture projects,
expected files, the manifest, and application source remain unchanged.
