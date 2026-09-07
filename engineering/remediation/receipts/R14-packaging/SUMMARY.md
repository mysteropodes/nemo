# R14 Lane H — bundled MCP executable in an installed desktop package

- Issue/task: [R14](https://github.com/mysteropodes/nemo/issues/910) acceptance
  bullets 4 and 5 — the `nemo-mcp` executable ships inside an installed Nemo
  desktop package, and a client can attach to it and select the intended running
  instance with nothing else on its resolution path.
- Human owner: Cyril Drouin
- Agent/session: Honey, Lane H
- Disposition: review-ready
- Base SHA: `66ece0641708122eb8447e85ad8dd7e3402aaf6c`
  (`Merge pull request #992: integrate opacity commands and bundled Rust MCP`)
- Candidate SHA and dirty digest: base `66ece06` plus the files under this
  directory. No source, build-script or configuration file is modified by this
  lane. The lane was started on PR #992's head `ade71cb`; #992 merged mid-build
  and the branch fast-forwarded. `git diff ade71cb 66ece06` over `nemo-mcp/`,
  `src-tauri/tauri.conf.json`, `src-tauri/src/application_mcp.rs`,
  `scripts/build-mcp-sidecar.cjs` and `scripts/bundle-ffmpeg-dylibs.py` is empty,
  so the merge changed nothing this lane depends on. Everything recorded below
  was rebuilt clean at `66ece06`.
- Branch/worktree ID: `honey/r14-packaging`, worktree `buzz-7f3a1c924b6d`
- Changed repository-relative paths:
  `engineering/remediation/receipts/R14-packaging/**` (evidence only)
- Intended behavior and preserved invariants: none changed. This lane produces
  evidence about an existing build path.
- Contract/schema/state-authority changes: none.
- Platform/runtime/backend: darwin 25.6.0 arm64, Apple M4 Max, rustc 1.91.1,
  cargo 1.91.1, node v24.4.1, Nemo 0.7.0-alpha.4, unsigned local build.

## Build and artifacts

Built locally. **No GitHub Actions workflow was enabled, dispatched or rerun at
any point**, per `engineering/ci/README.md` and CLAUDE.md §7.

Commands, in order (`transcripts/build.txt`):

```sh
npm install
npm run build -- --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'
python3 scripts/bundle-ffmpeg-dylibs.py src-tauri/target/release/bundle/macos/Nemo.app
npm run doctor
npm run build -- --bundles app,dmg --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

Two notes on that sequence.

`createUpdaterArtifacts` is overridden **on the command line only**; the
committed `tauri.conf.json` is untouched. Updater artifacts need
`TAURI_SIGNING_PRIVATE_KEY`, which is Cyril's personal secret and correctly
absent here. Left at its committed `true`, the build writes the `.app` and the
`.dmg` and *then* fails at the signing step — so the first attempt produced
usable bundles with exit 1. The override only suppresses the updater `.tar.gz`
and its signature; the app payload is identical.

The app is built, patched, and only then wrapped in a DMG, because Tauri has no
post-app/pre-bundle hook. This mirrors `.github/workflows/release.yml`, which
already splits the build for exactly this reason. Building `app,dmg` in one pass
produces a DMG around the *unpatched* app.

| Artifact | Bytes | SHA-256 |
|---|---|---|
| `Nemo_0.7.0-alpha.4_aarch64.dmg` | 38 025 816 | `6fc3bdca79f0fe602a28320f58ad4f1bb9416422740e766fb18082eb3cadaa5d` |
| `Contents/MacOS/nemo-mcp` | 3 662 976 | `93f346fa05ba62446c31a49a0c19f5b30418d0cbdc032819eb54a3b6799cd1de` |
| `Contents/MacOS/nemo` | 39 027 056 | `a5c0882ebc51ba0706aaffe39340b9448aa59ca919c479bbb7438d0c36a91245` |

`nemo-mcp` carries the same digest at every stage — staged by
`scripts/build-mcp-sidecar.cjs` into `src-tauri/binaries/`, copied into
`Nemo.app` by Tauri's `externalBin`, present inside the mounted disk image, and
present in the installed package. Nothing rewrites it in transit.

**Build identity.** The sidecar itself carries none: `nemo_mcp::BUILD_SOURCE_ID`
is referenced only by `src-tauri/src/application_mcp.rs`, so the linker drops it
from the standalone binary, and the executable has no `--version` (arguments are
ignored; it goes straight to MCP stdio and exits
`ConnectionClosed("initialize request")`). Identity is carried by the *app* and
surfaced through the transport: `strings` on `Contents/MacOS/nemo` contains
`66ece0641708122eb8447e85ad8dd7e3402aaf6c` exactly once and the superseded
`ade71cb…` zero times, and `nemo_discover` reports
`buildId: 0.7.0-alpha.4:66ece0641708122eb8447e85ad8dd7e3402aaf6c`.
That end-to-end match is what ties the running instance to this source.

Installed the way a user does — mount the DMG, copy `Nemo.app` out, eject
(`transcripts/install-from-dmg.txt`). Installed to `~/Applications`, a standard
per-user macOS install location, deliberately **not** `/Applications`, so
Cyril's existing `/Applications/Nemo.app` (2026-07-20 build) was not destroyed.

## Verification

All attach transcripts were produced against `~/Applications/Nemo.app` — the
installed package, never the build tree.

| Command or interaction | Result | Evidence | Limitation |
|---|---|---|---|
| Sidecar present in installed `Contents/MacOS/` | pass | `transcripts/install-from-dmg.txt` | — |
| Digest identical staged → app → DMG → installed | pass | `transcripts/build.txt`, `transcripts/install-from-dmg.txt` | — |
| Sidecar links only base macOS (`/usr/lib/libiconv.2.dylib`, `/usr/lib/libSystem.B.dylib`) | pass | `transcripts/attach-*/binary.txt` | — |
| Homebrew-linked deps per bundled executable: `nemo` 0, `nemo-mcp` 0, `ffmpeg` 26 | pass | `transcripts/build.txt` | — |
| Clean-room `initialize` → `nemo` 0.1.0, protocol `2025-06-18` | pass | `transcripts/attach-1-handshake/stdout.jsonl` id=1 | — |
| Clean-room `tools/list` → `nemo_command`, `nemo_discover`, `nemo_query` | pass | `transcripts/attach-1-handshake/stdout.jsonl` id=2 | — |
| No toolchain resolvable by name (node, npm, npx, cargo, rustc, python3, python, git, cc, clang, make) | pass | `transcripts/*/environment.txt` | — |
| Session runs from `/`, not the checkout | pass | `transcripts/*/environment.txt` (`cwd=/`) | — |
| `NEMO_MCP_REGISTRY` and `NEMO_TAURI_DATA_DIR` both unset — default per-user registry used | pass | `transcripts/*/environment.txt` | — |
| `nemo_discover` lists 4 live instances, each with `buildId` `0.7.0-alpha.4:66ece06…` and a distinct `documentId` | pass | `transcripts/attach-2-discover/stdout.jsonl` id=3, `transcripts/ANALYSIS.txt` | — |
| Explicit `instance_id` reaches the intended instance (ids 10 and 11, 4 instances live) | pass | `transcripts/attach-3-select/stdout.jsonl`, `transcripts/ANALYSIS.txt` | — |
| Never-registered instance id refused with `Selected instance is absent; discover again after reconnect` (id 12) | pass | `transcripts/attach-3-select/stdout.jsonl` | — |
| Stale record: `nemo_query` → `Connection refused (os error 61)`, `nemo_discover` → 0 instances | pass | `transcripts/attach-5-stale/stdout.jsonl` | See finding 3 |
| Live server holds **0** handles into any source checkout | pass | `transcripts/isolation-live-process.txt` | — |
| Live server: `cwd=/`, only mapped executable is the installed binary + `/usr/lib/dyld` | pass | `transcripts/isolation-live-process.txt` | — |
| `scripts/bundle-ffmpeg-dylibs.py` run after the build | **fail (exit 1)** | `transcripts/dylib-bundling.txt` | Pre-existing ffmpeg/Homebrew drift, unrelated to MCP — finding 2 |
| `npm run doctor` reports sidecar dylib state | pass (exit 0, reports 2 missing) | `transcripts/doctor.txt` | Reports the finding-2 failure |
| Updater signing | not-run | — | Requires Cyril's personal key |
| Windows / Linux / macOS x86_64 packaging | not-run | — | darwin/arm64 only on this host |

### How bullet 5 is established

`clean-attach.sh` re-executes itself under `env -i` with a single **empty
directory** as `PATH`, then drives the bundled executable over stdio with
newline-delimited JSON-RPC. `HOME` is deliberately preserved, because
`nemo-mcp/src/registry.rs` resolves the per-user registry under `$HOME` when
neither `NEMO_MCP_REGISTRY` nor `NEMO_TAURI_DATA_DIR` is set — that is the honest
configuration, since the point is that a normally installed client looks where a
normally installed app registers.

Four instances of the installed package were running. Each `nemo_query` returned
**the instanceId it asked for and a documentId unique to that instance**, both
matching what `nemo_discover` had reported moments earlier. The documentId is
what makes this a discrimination test rather than an echo test: each app owns its
own document identity, which the client never supplied, so a match rules out the
server answering from whichever instance it happened to reach. A third call, for
a never-registered UUID, was refused. Instance→PID attribution was established
independently of the protocol, by mapping each record's port to its listening
process with `lsof`.

Replies are correlated by JSON-RPC id, never by line position — rmcp dispatches
tool calls concurrently and they arrive out of order. An earlier read of these
transcripts by line position inverted two results before this was noticed.

### Why "no source checkout on the resolution path" is more than an assertion

An empty `PATH` shows no toolchain can be found *by name*. It does not by itself
show the server never reaches into a checkout by absolute path. So the server was
also inspected with `lsof` mid-session (`transcripts/isolation-live-process.txt`):
working directory `/`, the only mapped executables are the installed
`Nemo.app/Contents/MacOS/nemo-mcp` and `/usr/lib/dyld`, and the count of handles
referencing any source checkout is **0**. Its stdout/stderr were redirected to
`/tmp` rather than into the repository on purpose — a first run wrote them inside
the repo, which made the process hold two handles in the checkout purely because
that is where its output went. Those were output sinks, never resolution inputs,
but the claim is cleaner without them.

## Findings

**1. The sidecar digest is checkout-dependent, so a single pinned hash is not
reproducible.** The release binary embeds its own absolute build path: `strings`
on the installed sidecar returns
`/Users/cyril/.buzz/REPOS/nemo-worktrees/buzz-7f3a1c924b6d/nemo-mcp/src/server.rs`.
Two checkouts of the same source therefore give different digests — this
worktree `93f346fa…`, the `nemo-r14-fixture` checkout `2adbbf90…`. It is
deterministic *within* a checkout (rebuilds here reproduced `93f346fa…`,
including across `ade71cb` → `66ece06`, since the sidecar's source files are
byte-identical between them and `BUILD_SOURCE_ID` is stripped from it). So the
digest identifies the directory it was built in as much as the source, and
anyone told to re-derive and match a pinned value will fail. Either drop the
pinned hash from acceptance, or make it meaningful with
`[profile.release] trim-paths = "all"` in `nemo-mcp/Cargo.toml` (stable on rustc
1.91.1). Not verified here: digests attributed to other checkouts/PR records that
I could not rebuild — `nemo-pr992` has no staged sidecar. `nemo-mcp/` is outside
this lane's write scope; reported to integration.

**2. `scripts/bundle-ffmpeg-dylibs.py` fails on this machine — pre-existing, not
MCP-related.** It aborts with
`dependency not resolvable on this machine: /opt/homebrew/opt/svt-av1/lib/libSvtAv1Enc.4.dylib`.
`npm run doctor` agrees: `sidecar dylibs 26 external, 2 missing on this machine`
— `libvpx.12.dylib` and `libSvtAv1Enc.4.dylib`, where the host has libvpx 1.15.2
(`libvpx.11.dylib`) and svt-av1 3.1.2 (`libSvtAv1Enc.3.dylib`). Doctor also shows
`sidecar runs MISSING` with the matching dyld error, so the committed ffmpeg
binary cannot execute here at all. This is exactly the failure CLAUDE.md §7
predicts when a `brew upgrade` moves a major version away from what the committed
ffmpeg sidecar was linked against. Remedy per §7: re-run
`scripts/rebuild-ffmpeg-lgpl.sh` and commit the sidecar — which writes
`src-tauri/binaries/ffmpeg-aarch64-apple-darwin`, outside this lane's paths. The
alternative, `brew upgrade libvpx svt-av1`, mutates Cyril's machine outside the
repository and can cascade into his system ffmpeg 8.0; not done unilaterally.

`bundle-ffmpeg-dylibs.py` **is** in this lane's write scope and was deliberately
left alone. Its hard exit is correct: softening it to skip unresolvable
dependencies is precisely the "ships fine, crashes the instant Export is used"
bug the script exists to prevent, and §7 records that this exact softening
already cost a release once.

It does not affect this lane. `nemo-mcp` has zero Homebrew dependencies, so the
dylib pass is a no-op for it either way; only ffmpeg-based video export on a
machine without those Homebrew majors is at risk. One consequence worth
recording: because the script aborts, its ad-hoc re-sign step never runs, so the
installed `nemo-mcp` still carries Tauri's signature and its digest equals the
staged one. Had the script completed, the in-package digest would necessarily
differ from the staged digest — a second reason a pinned hash is ambiguous unless
the receipt says which stage it refers to.

**3. Discovery records outlive their instance, and only `nemo_discover` notices.**
Full evidence in `transcripts/registry-liveness-finding.txt`,
`transcripts/registry-lifecycle.txt` and `transcripts/attach-5-stale/`.

A controlled test quit an instance the way a user does (LaunchServices quit, not
a kill) and its record **remained on disk** — so the leak is not limited to
crashes and kills, as first assumed; `Registration::drop` does not run on a
normal exit either, because the registration lives in a spawned task and the
process exits without unwinding it. Every ordinary run leaves a record behind;
this session's launches left five.

`nemo_discover` is **not** affected — it probes every endpoint and lists only
responders, returning 0 instances while a stale record sat on disk and 4 while 4
were live. That was checked rather than assumed. The gap is in the non-probing
path: `read_endpoints()` validates shape only and never checks that anything
still listens, so `nemo_query`/`nemo_command` return
`unavailable / Connection refused (os error 61)` for a stale id — an OS error
that reads like a transient fault — where a never-registered id gets the specific
and actionable `Selected instance is absent; discover again after reconnect`.
After Nemo restarts, a client holding the previous id gets the unhelpful one.
Secondary effect: the registry grows by one file per launch forever, each costing
`nemo_discover` a connection attempt. Outside this lane's write scope.

**4. The three tools disagree on the instance field name.** From `tools/list` on
the installed binary (`transcripts/attach-1-handshake/stdout.jsonl`):

- `nemo_discover` **returns** `instanceId` (camelCase)
- `nemo_query` **requires** `instance_id` (snake_case)
- `nemo_command` **accepts** `instanceId` (camelCase)

`Query` in `nemo-mcp/src/server.rs` has no `#[serde(rename_all = "camelCase")]`,
unlike `ApplicationRequest`. So a client feeding `nemo_discover` output straight
into `nemo_query` fails on a missing field, while the same value works unchanged
in `nemo_command`. This bears directly on bullet 5, since it is exactly the
discover → select round trip. Outside this lane's write scope.

## Review and risk

- Independent reviewer: none yet; submitted for integration review by Fizz.
- Known failures/untested scope: findings 1–4; darwin/arm64 only; unsigned build;
  updater signing not exercised; no export/ffmpeg path exercised (finding 2).
- Data/compatibility/rollback: evidence-only, nothing to roll back. Host state
  touched and restored: every instance launched from `~/Applications` during this
  lane has exited (some quit, one SIGKILLed on purpose for the stale-record test,
  several terminated by something outside this session), and the discovery records
  they orphaned were deleted after confirming no listener remained on any of their
  ports — the registry is empty. Cyril's `/Applications/Nemo.app` was never
  modified, and the separate instance running from another agent's fixture
  checkout under its own `NEMO_TAURI_DATA_DIR` was left alone.
- Required downstream revalidation: if `nemo-mcp/Cargo.toml` gains `trim-paths`,
  every sidecar digest recorded here changes and must be re-derived.

## Ownership

- Working state preserved at: worktree `buzz-7f3a1c924b6d`, branch
  `honey/r14-packaging`.
- Exact next action: integration decides finding 1 (drop the pinned hash, or make
  it reproducible); findings 2–4 need owners outside this lane.
- Product acceptance owner/result: pending. Bullets 4 and 5 are demonstrated on
  darwin/arm64 by the transcripts here. A green branch or merged PR does not close
  the gate.

## Reproducing

From this directory, with at least one Nemo instance running from
`~/Applications/Nemo.app`:

```sh
./clean-attach.sh ~/Applications/Nemo.app /tmp/ev/attach-1-handshake requests-handshake.jsonl 6
./clean-attach.sh ~/Applications/Nemo.app /tmp/ev/attach-2-discover  requests-discover.jsonl  15
```

Write transcripts outside the repository, as above, or the server will hold
handles inside the checkout and muddy the isolation claim.

The selection transcript names specific instanceIds, which only exist while those
instances are running. Regenerate its request file from the live registry
(`~/Library/Application Support/com.strokemotion.app/mcp/*.json`) before
re-running it; the exact file used is preserved as
`transcripts/attach-3-select/requests.jsonl`.
