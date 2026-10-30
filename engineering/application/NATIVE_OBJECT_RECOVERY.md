# Native crash-recovery store (R1)

Tracked under #2360 (recovery evidence) and the #922 R20 parent; parity row D07
"autosave heartbeat and recovery across native documents" had no open owner. This
step is the Rust store only. The JS writer scheduler (R2), the start-screen resume
card (R3) and Save/quit cleanup (R4) are separate steps that consume these commands.

## Why

`persistenceJSON()` of a native object project throws by design, so the 30 s timed
autosave never writes anything for native projects and a crash loses the session.
Autosave bytes must be native-owned, admitted by the strict project codec, and
never cross a JS string wrapper, so the writer is Rust-side.

## What is written

Exactly the bytes `native_project_file_snapshot` produces for Save As (the same
strict `nemo.native-project-document` file the open route admits, at most 32 MiB).
No new serializer and no JSON re-wrap. A bare drawing document is not recoverable
(`published:false, reason:"unsupported"`); a payload over 32 MiB is
`reason:"too-large"` and writes nothing.

## Layout

`<appData>/recovery/<slotKey>/` (root 0700, slot 0700, files 0600). `slotKey` is
`^[a-z0-9-]{1,64}$`, validated in Rust; JS never supplies a path.

- `rec-<unixms>-<revision>-<8 hex>.json` one immutable generation (unique name, a
  rename can never clobber).
- `manifest.json`: `{v:1, slotKey, displayName, originPath|null, appVersion,
  generations:[{file, savedAtMs, contentRevision, byteCount, documentDigest?}]}`,
  newest first, at most two. `documentDigest` is an optional caller-supplied
  64-hex hint (the UI already holds the info digest); Rust does not trust it.

## Atomic publication

temp (create_new, 0600, same directory) -> write_all -> sync_all -> unique rename ->
directory fsync -> manifest temp (same steps) -> rename over `manifest.json` ->
directory fsync -> prune. The previous manifest, and with it the previous
generation, is untouched until the manifest rename, so a failure or kill at any
earlier step leaves the last durable generation intact. The new file is removed on
a failure unless the on-disk manifest already references it; a file orphaned by a
kill between the data rename and the manifest rename is never listed and is pruned
by the next write. A prune failure is not a failed write (`cleanupPending:true`).
Symlinked root, slot or manifest are refused, never followed.

## Retention

Two generations per slot, five slots, 512 MiB, 14 days. Enforced on every list and
after every write (the slot just written is never shed by the count/byte rules).
Stale `*.tmp` older than ten minutes are removed. Anything unreadable (bad manifest,
size or format-sniff mismatch, data with no manifest) is renamed to
`<slot>.corrupt-<ms>` (or `<file>.corrupt-<ms>` for one generation), never
deleted, and is not listed again. A slot whose `originPath` file is at least as new
as its newest generation was saved afterwards and is dropped.

## Commands (private, main window only, async via spawn_blocking)

All take `requestJson` with `apiVersion:2`, `requestId`, `instanceId`; strict DTOs
(`deny_unknown_fields`, at most 4096 bytes).

| command | body | result |
|---|---|---|
| `nemo_native_object_recovery_write` | `documentId, atRevision, documentSnapshotId, slotKey, displayName, originPath?, supersedes?, documentDigest?` | `published, reason?, byteCount, savedAtMs, cleanupPending, supersededRemoved` |
| `nemo_native_object_recovery_list` | none | `entries[{slotKey, displayName, originPath, generations[{path, savedAtMs, contentRevision, byteCount, documentDigest?}]}]` newest first, `quarantined[]` |
| `nemo_native_object_recovery_discard` | `slotKey` | `discarded` |
| `nemo_native_object_recovery_quarantine` | `slotKey, file` | `quarantined, slotQuarantined` (explicit user action only) |

A stale pin is a normal `published:false, reason:"stale"` receipt, not an error.
`supersedes` removes the named slot only after the new manifest is durable.
They are not capabilities-v2/MCP operations, so no engine ownership entry exists.

## Authority lock

`native_project_file_snapshot` serializes the whole project while holding the
native authority mutex. Measured with a contender thread that takes the same lock
(`cargo test --lib recovery_lock_hold -- --ignored --nocapture`, 2600 objects,
1.58 MB, 30 samples, `NEMO_RECOVERY_OBJECTS` scales it):

| variant | release | debug |
|---|---|---|
| inline (`native_project_file_snapshot`) max stall | 8.71 ms | 83.7 ms |
| detached (`native_project_recovery_snapshot`) max stall | 0.02 ms | 0.03 ms |

Inline is under the 16 ms budget in release at this size, but the hold grows
linearly with the file (about 5.5 ms per MB in release, so about 175 ms at the
32 MiB cap) and a 30 s periodic writer would repeat it. The write command therefore
uses the detached variant: the lock covers only the O(1) acquisition of the
immutable Arc-shared snapshot (`detached_project_file_serializer` in the engine)
and the encoder runs after release. Save As keeps its existing inline path.

## Not covered

Only the active project; parked tabs are a documented gap. Version-history parity
(R6) is out of scope. After a confirmed quit-without-saving the recovery files are
kept (baseline parity; one line in R4 flips it).
