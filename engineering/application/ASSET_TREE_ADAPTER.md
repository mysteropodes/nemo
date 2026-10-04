# Asset-folder presentation adapter — P35 / #1521

`src/js/asset-tree.js` is the shared DOM helper for the Media grid and Transplant
picker. Owner: Ilya (`ivg-design`); independent reviewer: Ilya/O. This is an
asset/media presentation module, not a separate document, folder-management,
MCP or native capability. The source remains at its existing classic-script
path and creates the browser `window.SMAssetTree` facade from the same
`createAssetTree(domDocument, translate)` factory exported to CommonJS tests.
The factory and its bootstrap are privately scoped; the classic script adds no
`window.createAssetTree` global.

The injected document creates only folder, header, chevron, icon, label, count
and body elements. The helper appends the folder to the caller's container and
returns its body. The caller creates and owns every component, composition,
layer or media row. A click listener on each header toggles its own header/body
`collapsed` classes; it lives and dies with those nodes. There is no global
listener, disposal service, persisted collapse state or `opts.key` storage.
Recreating a folder applies `defaultCollapsed` afresh. Count is converted to
text, including zero; absent count is empty; absent color uses `--text-dim`.
The fixed kind colors and public label functions retain their existing names.

The translation port is invoked on every label read. The browser facade looks
up the *current* `window.SM.t` lazily and falls back to the existing strings if
it is unavailable. Language switches after script load therefore do not keep
stale cached labels. The factory has no implicit application/domain state or
storage access; the only browser global is the compatibility facade bootstrap.

`src/js/media-library.js` uses the facade in its real `render()` path for
components, other open-tab compositions, and image/video/audio kind groups.
It appends its own `.bp-item`, `.media-row` or `.media-tile` children. The
Transplant picker uses it in `renderLists()` for symbol/plain layers and media
checkbox rows. Neither consumer is moved or written by P35. Folder clicks
change DOM visibility only: they do not change the caller's rows, checkbox
selection, document, history, animation, render or export state. They do not
make a collapsed document layer invisible in Nemo's canvas.

## Evidence and limits

`tests/asset-tree.test.cjs` imports the production factory and checks the
exact public API, DOM structure, lifetime, color/count/default-collapse,
caller-owned rows and late language reads. `tests/browser/asset-tree.spec.cjs`
loads production scripts in an isolated real Chrome runtime, supplies bounded
presentation fixture data to the real Media renderer and uses the real
Transplant picker/file chooser. Playwright pointer clicks prove collapse and
expand preserve rows and checked boxes; fixture document/history snapshots
remain unchanged. Fixture setup is not evidence of native document admission,
Save/Load, or installed application behavior.

The adopted app-JS profile and provenance inventory classify the source as a
presentation adapter. Boundary, inventory and local quick checks are required
at the exact candidate SHA, followed by independent review, normal protected
integration and an identified packaged-Tauri smoke of the existing consumers
when its reserved desktop slot is released. Any unavailable desktop observation
is recorded as a limit, never inferred from browser success. Exact receipts
and final disposition belong on #1521 and its PR.
