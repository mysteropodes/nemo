# IndexedDB autosave adapter — P34 / #1520

`src/js/idb-store.js` is the browser storage adapter for the owning document-persistence
feature. Owner: Ilya (`ivg-design`); independent reviewer: Ilya/O. Its public CommonJS
factory is `createIndexedDbStore(indexedDb)`, returning async `get(key)`, `set(key,value)`
and `remove(key)`. The existing classic script creates exactly one production instance
as `window.SMIdb`, injecting an open port that lazily reads `window.indexedDB`, as the
baseline did. A throwing host getter rejects the first operation and keeps that failure
cached instead of breaking script bootstrap. No script-order or consumer change is
needed. This internal helper is not a separately discoverable capability or MCP operation.

The adapter owns the `nemo-store` database, version 1, and its sole `kv` object store.
Each instance lazily caches one open promise, including a rejected promise. Upgrade
creates `kv` only when missing. Transactions are readonly for get and readwrite for
set/remove. Read results are returned at transaction completion, with `undefined` for
missing keys. Writes return `undefined` only at completion; request success does not
establish commit. Values stay opaque: the adapter does not parse or alter document,
history, selection, animation, render or export fields. Browser quota and eviction
still apply; storage capacity and durability are not unlimited guarantees.

## Preserved errors and lifetime limits

Absent injected IndexedDB rejects with `Error('indexedDB unavailable')`; synchronous
open/transaction/request exceptions reject. Open request errors and transaction errors
reject with the platform's original error. The failed open remains cached: no retry or
replacement connection is introduced. As in the characterized baseline, blocked opens
and abort-only transactions have no terminal handler and can leave an operation pending.
No `close`, disposal, version-change handler, explicit abort, recovery, quota policy or
database deletion API is added. A successful connection lives for the instance/page
lifetime; the host owns cleanup on page/context teardown. These are retained limitations,
not new reliability guarantees. Different factory instances are test/host instances,
not an additional production autosave owner.

## Production consumers and availability

`src/index.html` retains its classic script load before `project.js`. Read-only inspection
of `project.js` confirms `autosaveWrite` still tries localStorage first, mirrors bytes to
`SMIdb.set('nemo-auto', json)`, catches rejected writes, and removes a stale localStorage
slot when its write hits quota. Startup checks `SMIdb.get('nemo-auto')` only when
localStorage is missing; Resume similarly falls through to get and catches failures as
`applyAuto(null)`. These orderings and silent catches are unchanged.

Storage availability in the browser does not grant native document admission or editing
authority. With the supported curve-workflow fixture, the real Resume control
imports `r08_curve_layer` and reveals the editor after its asynchronous load completes.
An isolated comparison substituting only the exact protected-base `c20d01e` adapter
observed identical stored bytes, layers/frames, history, current frame, start-screen
visibility and page errors. The browser test retains this observed import behavior, without
relaxing admission guards or claiming restored editing. Unavailable native/legacy editing
operations retain their existing guards. Undo/redo, selection, animation, render/export and native bridges gain
no writers or document transformations. The fixture's exact text survives storage/reload.
Tauri filesystem autosave is separate; this script can exist in its webview, but actual
installed Tauri IndexedDB availability is unverified by this browser-only slice. No
native-engine command, revision owner or packaged-desktop acceptance is claimed.

## Validation

- `node --test tests/idb-store.test.cjs`: production import, independent open/request/
  transaction events, modes, upgrade, caching, opaque bytes, missing/removal, errors and
  preserved blocked/abort-only limits. An intentionally early-resolving control fails the
  same pre-commit oracle used on the production methods.
- `npx playwright test tests/browser/idb-store.spec.cjs`: existing isolated runtime and
  fresh Chrome contexts, production facade/consumer, real IndexedDB reload/removal and
  unavailable-storage environment control. No user data is cleared.
- Adopted app-JS adapter profile and source provenance, regenerated surface inventory,
  focused boundary checks, `npm run check` and exact-head local quick validation.

Exact candidate/results belong in #1520 and its PR. Passing storage checks is not
whole-project autosave, native Resume, installed desktop, or final migration acceptance.
