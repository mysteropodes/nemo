# R03 document browser acceptance

This opt-in harness closes the browser half of the R03 document-contract fixture gate. It
loads the committed migration and text projects through Nemo's real browser Open flow in
headless Chromium. The application performs `importJSON` and `exportJSON`; the harness then
checks the resulting documents against the independently committed expectations.

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

This is document-backend acceptance. It does not render the text pixel gate, benchmark a GPU,
open a native GUI, build Tauri, exercise native file dialogs, or establish package/release
acceptance. Fixture projects, expected files, the manifest, and application source remain
unchanged.
