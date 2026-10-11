# Private object snapshot Save As

N25E3/#1667 adds `nemo_native_object_save_as(requestJson)` to the existing
main-window Tauri invoke handler. It is a private create-new file consumer of
N25E1's serializer, followed by N25E2's accepted file reopen in its local oracle.
Public `saveAvailable` remains false. UI Save, picker, clean state, recents,
autosave, overwrite, public MCP, installed acceptance and M1 remain separate gates.

The strict original UTF-8 request is at most 4096 bytes. It contains only
`apiVersion`, `requestId`, `instanceId`, `documentId`, `atRevision`,
`documentSnapshotId`, `destinationPath`, and optional `cancelledBeforeDispatch`.
Duplicate decoded keys, unknown fields, invalid types/IDs, unsafe revisions,
relative/NUL paths and an empty/dot/dot-dot final name refuse. Cancellation is
pre-dispatch only; there is no post-publication cancellation or rollback promise.

The consumer admits a current pinned serializer request through the same
ApplicationMcp, RevisionSync and NativeAuthority. It retains the exact returned
UTF-8 bytes and verifies correlated identity, revision and snapshot. It neither
constructs another owner nor rebuilds JSON from selected-object reads. The existing
complete serializer response remains bounded at 4096 bytes; oversize refuses.

Disk I/O occurs after serialization releases authority. An edit, undo, redo or
release after acquisition cannot alter the retained bytes. The receipt identifies
the immutable snapshot written; it does not assert the current owner's revision
or mark a document clean. Save does not advance content revision/history.

The existing filesystem scope must allow both the resolved new destination and
unique same-parent staging name. Scope is never expanded. On the current Unix
desktop, exclusive staging uses mode0600, complete write and file sync. A hard link
publishes the completed file as one new directory entry. Existing files (including
empty), directories, symlinks and dangling symlinks refuse unchanged; concurrent
destination creation also refuses. No truncate, delete-target, rename-over or
unsupported-hard-link fallback exists. Non-Unix publication reports unavailable.

Before publication, failure has no partial final artifact. Cleanup removes only
a staging path whose regular-file identity matches the owned open descriptor.
Detected substitution preserves the foreign path and reports cleanupPending.
After the hard-link commit, cleanup failure returns published=true and
cleanupPending=true, rather than a retryable uncommitted failure. The receipt has
API/request/instance/document/revision/snapshot, byteCount and publication/cleanup
disposition; private paths are omitted from receipts and error text.

Canonical scope, creation, identity check, link and cleanup remain separate path
operations. Adversarial parent/staging replacement is not solved; this is not a
TOCTOU-safe sandbox or path-stability guarantee. File sync does not establish
directory-entry fsync or power-loss durability. Overwrite, durable recovery and
descriptor-relative security require their own admitted contracts.

Independent controls embed complete original and edited JSON literals frozen
before implementation (fixture SHA256
`7b46c36412dbb1ca6462d1b3222a39a687c958d4262bd8ced35e2c98d2612e96`).
Actual fill1/undo2/redo3, exact disk bytes, unchanged full scoped geometry/paint,
production file reopen into fresh runtime identity/revision0/empty history,
strict refusals, pre-link faults, detected substitution, concurrent destination,
snapshot retention and post-commit cleanup have separate assertions. These are
source/local tests; no installed client, public Save or full desktop credit follows.

The original draft formatter syntax refusal is retained, followed by its explicitly
authorized escape correction. Normal formatting revealed a 553-nonblank test file;
Lead admitted a second failure-test child before source split. All test bodies and
assertions were moved unchanged: parent309/child248 nonblank, both within hard500.
These draft corrections are distinct from focused validation, which requires a
separately allocated resource slot and nonzero executed fixture counts.
