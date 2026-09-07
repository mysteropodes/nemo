# R14 Lane H — bundled MCP executable in an installed desktop package

- Issue/task: R14 acceptance bullets 4 and 5 — the MCP executable ships inside an
  installed Nemo desktop package, and a client can attach to it and select the
  intended running instance with nothing else on its resolution path.
- Human owner: Cyril Drouin
- Agent/session: Honey, Lane H
- Disposition: review-ready
- Base SHA: `66ece06` (`Merge pull request #992: integrate opacity commands and bundled Rust MCP`)
- Candidate SHA and dirty digest: base `66ece06`, plus the receipt files listed
  below. No source, build-script or configuration file is modified by this lane.
- Branch/worktree ID: `honey/r14-packaging`, worktree `buzz-7f3a1c924b6d`
- Changed repository-relative paths: `engineering/remediation/receipts/R14-packaging/**`
  (evidence only)
- Intended behavior and preserved invariants: none changed. This lane produces
  evidence about an existing build path.
- Contract/schema/state-authority changes: none.
- Platform/runtime/backend: darwin 25.6.0 arm64, Apple M4 Max, rustc 1.91.1,
  Nemo 0.7.0-alpha.4, unsigned local build.

## Build and artifacts

Built locally with `npm run build`. No hosted workflow was dispatched.

The first build completed both bundles and then failed on the updater signing
step, which needs `TAURI_SIGNING_PRIVATE_KEY` — Cyril's personal key, correctly
absent. Rebuilt with `createUpdaterArtifacts:false` for a clean exit; that flag
affects only the updater signature, not the app payload.

| Artifact | SHA-256 |
|---|---|
| `Nemo_0.7.0-alpha.4_aarch64.dmg` (38 025 816 B) | `6fc3bdca79f0fe602a28320f58ad4f1bb9416422740e766fb18082eb3cadaa5d` |
| `Contents/MacOS/nemo-mcp` (3 662 976 B) | `93f346fa05ba62446c31a49a0c19f5b30418d0cbdc032819eb54a3b6799cd1de` |
| `Contents/MacOS/nemo` (39 027 056 B) | `a5c0882ebc51ba0706aaffe39340b9448aa59ca919c479bbb7438d0c36a91245` |

`nemo-mcp` carries the same digest at every stage — staged by
`scripts/build-mcp-sidecar.cjs`, copied into `Nemo.app` by Tauri, inside the
mounted disk image, and in the installed package. Tauri copies the sidecar
verbatim; nothing rewrites it.

Installed the way a user does: mount the DMG, copy `Nemo.app` out, eject
(`transcripts/install-from-dmg.txt`). Installed to `~/Applications` so that the
existing `/Applications/Nemo.app` — Cyril's 2026-07-20 build — was not
overwritten.

## Verification

| Command or interaction | Result | Evidence | Limitation |
|---|---|---|---|
| Sidecar present in installed `Contents/MacOS/` | pass | `transcripts/install-from-dmg.txt` | — |
| Digest identical staged → app → DMG → installed | pass | table above | — |
| Sidecar needs no bundled dylibs (`otool -L`: only `/usr/lib/libiconv.2.dylib`, `/usr/lib/libSystem.B.dylib`) | pass | `transcripts/attach-*/binary.txt` | — |
| Clean-room handshake: `initialize` → `nemo` 0.1.0 | pass | `transcripts/attach-1-handshake/` | — |
| `tools/list` advertises `nemo_discover`, `nemo_query`, `nemo_command` | pass | `transcripts/attach-1-handshake/stdout.jsonl` | — |
| No toolchain resolvable by name (node, npm, npx, cargo, rustc, python3, python, git, cc, clang, make) | pass | `transcripts/*/environment.txt` | — |
| Session runs from `/`, not the checkout | pass | `transcripts/*/environment.txt` (`cwd=/`) | — |
| Registry overrides unset — default per-user path used | pass | `transcripts/*/environment.txt` | — |
| `nemo_discover` against an isolated empty registry → `{"apiVersion":1,"instances":[]}` | pass | re-derived on the installed package | — |
| Unregistered instance rejected with a specific message | pass | `transcripts/attach-3-select/ANALYSIS.txt` id=99 | — |
| Explicit `instance_id` reaches the intended instance, 4 running | pass | `transcripts/attach-3-select/ANALYSIS.txt` — 5/5, 4 distinct documentIds | — |
| Live server holds **0** handles into any source checkout | pass | `transcripts/isolation-live-process.txt` | — |
| Live server: `cwd=/`, only executable is the installed binary | pass | `transcripts/isolation-live-process.txt` | — |
| `scripts/bundle-ffmpeg-dylibs.py` on the built app | **fail** | `transcripts/dylib-bundling.txt` | Pre-existing ffmpeg/Homebrew issue, unrelated to MCP — see findings |
| Updater signing | not-run | — | Requires Cyril's personal key |
| Windows / Linux packaging | not-run | — | darwin/arm64 only |

### How bullet 5 is established

The clean-room harness (`clean-attach.sh`) re-executes itself under `env -i`
with a single empty directory as `PATH`, then drives the bundled executable over
stdio with newline-delimited JSON-RPC. `HOME` is deliberately preserved, because
`nemo-mcp/src/registry.rs` resolves the per-user registry under `$HOME` when
neither `NEMO_MCP_REGISTRY` nor `NEMO_TAURI_DATA_DIR` is set — which is the
honest configuration for a normally installed client.

Four Nemo instances were running. Each `nemo_query` returned the instanceId it
asked for **and a documentId unique to that instance**. That is what makes it a
discrimination test rather than an echo test: each app owns its own document
identity, so matching both rules out the server answering from whichever
instance it happened to reach. A fifth call, for a never-registered UUID, was
refused.

Replies are correlated by JSON-RPC id, never by line position — rmcp dispatches
tool calls concurrently and they arrive out of order.

### Why "no source checkout on the resolution path" is more than an assertion

An empty `PATH` shows no toolchain can be found *by name*. It does not by itself
show the server never reaches into a checkout by absolute path. So the server
was also inspected with `lsof` while it was mid-session
(`transcripts/isolation-live-process.txt`): its working directory is `/`, the
only executable mapped is the installed `Nemo.app` binary, and the number of
handles referencing any source checkout is **0**. Its stdout and stderr were
deliberately redirected to `/tmp` rather than into the repository, so that the
one handle it does hold outside the app bundle is an output sink and not a
resolution input.

## Findings

**1. The recorded sidecar SHA-256 is not reproducible, and cannot be.** PR #992
recorded `d7db869d14fe60dc452433d2f98c5efb812c999e0c1a9ee75f62524c4fc25853`. The
release binary embeds its own absolute build path — `strings` on the staged
sidecar returns
`<HOME>/.buzz/REPOS/nemo-worktrees/buzz-7f3a1c924b6d/nemo-mcp/src/server.rs`.
Four checkouts of identical source therefore give four digests:

| Checkout | Source | SHA-256 |
|---|---|---|
| `nemo-pr992` | `ade71cb` | `8f2b62ff…` |
| `nemo-r14-fixture` | `ade71cb` | `2adbbf90…` |
| `buzz-7f3a1c924b6d` | `66ece06` | `93f346fa…` |
| PR #992 record | `6c718b8` | `d7db869d…` |

It is deterministic *within* a checkout — three rebuilds here all produced
`93f346fa…` — so the digest identifies the directory it was built in, not the
source. Anyone told to re-derive and match it will fail. Either drop the pinned
hash from acceptance, or make it meaningful with `[profile.release] trim-paths =
"all"` in `nemo-mcp/Cargo.toml` (stable on rustc 1.91.1). `nemo-mcp/` is outside
this lane's write scope; reported to integration.

**2. `scripts/bundle-ffmpeg-dylibs.py` fails on this machine — pre-existing, not
MCP-related.** It aborts with `dependency not resolvable: /opt/homebrew/opt/svt-av1/lib/libSvtAv1Enc.4.dylib`.
`npm run doctor` reports `26 external, 2 missing`: `libvpx.12.dylib` and
`libSvtAv1Enc.4.dylib` (the machine has `libSvtAv1Enc.3`). This is the exact
failure CLAUDE.md §7 predicts when a `brew upgrade` moves a major version away
from what the committed ffmpeg sidecar was linked against, and it originates
from R04 (`3a3038d`), long before this work. Remedy per CLAUDE.md §7: re-run
`scripts/rebuild-ffmpeg-lgpl.sh` and commit the sidecar.

It does not affect this lane. `nemo-mcp` needs no bundled dylibs at all, so it
is unaffected either way; only ffmpeg-based video export on a machine without
those Homebrew majors is at risk. One consequence worth noting: because the
script aborts, its ad-hoc re-sign step never runs, so the installed `nemo-mcp`
still carries Tauri's linker-adhoc signature and its digest equals the staged
one. Had the script completed, the in-package digest would necessarily differ
from the staged digest — a second reason a single pinned hash is ambiguous
unless the receipt says which stage it refers to.

**3. The discovery registry has no liveness check.** Full evidence in
`transcripts/registry-liveness-finding.txt`. `Registration::drop` prunes a
record only on graceful exit, and `read_endpoints()` validates shape but never
checks that anything still listens. Measured immediately after the selection
transcript: 4 records advertised, 0 live. A client selecting a stale record gets
`unavailable - Connection refused (os error 61)`, indistinguishable from a
transient failure — while a never-registered instance gets the specific and
actionable `Selected instance is absent; discover again after reconnect`. The
good message exists; it is simply unreachable for the stale case. Outside this
lane's write scope.

**4. The three tools disagree on the instance field name.** From `tools/list` on
the installed binary:

- `nemo_discover` returns `instanceId` (camelCase)
- `nemo_query` requires `instance_id` (snake_case)
- `nemo_command` accepts `instanceId` (camelCase)

So a client feeding `nemo_discover` output straight back into `nemo_query` fails
with `failed to deserialize parameters: missing field 'instance_id'`, but the
same value works unchanged in `nemo_command`. This was hit while building the
transcripts. It bears directly on bullet 5, since it is exactly the discover →
select round trip. Outside this lane's write scope.

## Review and risk

- Independent reviewer: none yet; for integration review.
- Known failures/untested scope: findings 1–4 above; darwin/arm64 only; unsigned
  build; updater signing not exercised.
- Rollback: evidence-only, nothing to roll back.
- Required downstream revalidation: if `nemo-mcp/Cargo.toml` gains `trim-paths`,
  every recorded sidecar digest in this receipt changes and must be re-derived.

## Ownership

- Working state preserved at: worktree `buzz-7f3a1c924b6d`, branch
  `honey/r14-packaging`.
- Exact next action: integration decides finding 1 (drop the pinned hash, or
  make it reproducible); findings 2–4 need owners outside this lane.
- Product acceptance owner/result: pending. Bullets 4 and 5 are demonstrated on
  darwin/arm64 by the transcripts here; a green branch does not close the gate.

## Reproducing

```sh
./clean-attach.sh ~/Applications/Nemo.app transcripts/attach-1-handshake requests-handshake.jsonl 6
./clean-attach.sh ~/Applications/Nemo.app transcripts/attach-2-discover  requests-discover.jsonl  6
./clean-attach.sh ~/Applications/Nemo.app transcripts/attach-3-select    requests-select.jsonl   10
```

`requests-select.jsonl` names specific instanceIds and is only meaningful
against the instances that were running when it was generated; regenerate it
from the live registry before re-running that third transcript.
