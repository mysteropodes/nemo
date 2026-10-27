# Packaged native acceptance prerequisite

Run from a clean, committed integration checkout on macOS with its built app:

```sh
NEMO_DESKTOP_APP=/absolute/path/Nemo.app node --test tests/desktop/*.test.cjs
```

The standard `test:desktop` job owns discovery and receipt wiring. Optional
`NEMO_DESKTOP_REPORT_DIR` receives a sanitized `packaged-native.json` result.
The supplied bundle must contain an executable Mach-O binary. Missing app,
unsupported platform, dirty candidate, unavailable reservations or incomplete
runtime evidence fail this direct invocation; no green skips are used.

This gate uses the current checkout's `native.cjs` and `native-runtime.cjs`,
including `stop --retain-data` and the native manifest's `processEnvironment`.
It launches two actual native processes, reserves `desktop-input` and
`gpu-reference` explicitly, and proves:

- task, process, owner, launcher source/build and app-manifest identity;
- actual child temp/cache/report environment and distinct application roots;
- distinct WebKit data stores materialized by the packaged app;
- foreign owner status/stop refusal and exclusive reservation refusal/release;
- same-task stop/relaunch preserving storage sentinels;
- process-group exit before default cleanup, while the other instance survives.

Use the same `NEMO_ISOLATION_ROOT` as other coordinated machine tasks (normally
the runtime default). A private alternate root would test an independent slot
namespace. The gate never searches for, stops or deletes other task instances.
It retains state and fails if cleanup authority or process exit is uncertain;
reconcile that exact owned run before retrying. Raw launcher responses, owner
tokens and host paths are consumed privately rather than copied into reports.

The sentinels are harness-written filesystem data. Their survival does **not**
prove Nemo document save/reload through the UI, WebKit storage APIs, rendering,
GPU output, or final R06 acceptance. The launcher identity and executable hash
also do not prove embedded app source provenance: the integration owner must
build the identified clean candidate, retain the build receipt, then execute the
remaining real UI workflow separately.

Test harness failure handling without a desktop or live slots:

```sh
node --test tests/desktop/unit/native-harness.test.cjs
```

These controls use disposable Node processes only; they cannot substitute for
the packaged-app invocation above.

## CS7 installed batch driver

`cs7-run.cjs` is an explicit manual entry point. Book the Local app/input slot
in #1062 and run the entire command under `lockf -k /tmp/nemo-heavy-job.lock`.
Build the normal app and the UI32 helper from the same clean, committed candidate.
The private JSON config names `sourceSha`, `app`, `executableSha256`, `buildProof`,
`helper`, `helperProof`, `taskId`, `grant` (`1062/<comment-id>`), an existing empty
`reportDirectory`, and an optional supported `fixtureKind`.

```sh
lockf -k /tmp/nemo-heavy-job.lock node tests/desktop/cs7-run.cjs /absolute/private-config.json
```

The driver reads **explicit** declarations from
`engineering/inventory/parity/installed/*.json`. Their schema is
`nemo.installed-parity/1`; each `rows` entry declares `id`, `issue`, `status`
(`Ready` or `Blocked`), existing `tasks` and `surfaces`, `dependsOn`, `scenario`,
and `checks` containing an `id` and family-acceptance `covers` kinds. Tasks with
no matrix surfaces may declare an empty surface list. Every Ready scenario must
exist as a reviewed `cs7-scenarios/<scenario>.cjs` module exporting
`run({session,row})`. It returns every declared assertion's `pass`, `fail` or
`unrun` status and an explicit `safeToContinue` disposition. The scenario must
assert the original product contract from actual observations; successful input
posting is never a passing assertion. `session.input` uses the existing closed
UI32 semantic command protocol and fresh runtime identity checks.

Missing declarations or scripts refuse **before app launch**. The existing
matrix's issue status and headless `source-pass` do not establish readiness.
This infrastructure does not itself declare the B3 cohort Ready or implement
its original acceptance scripts; those remain required before CS7 acceptance.

One runtime spans the batch. Input/precondition failures stop it without replay;
product failures may continue only with the scenario's explicit safe disposition.
Dependent rows remain blocked, and every unrun clause remains in the receipt.
Cleanup runs even after partial startup, input timeout or evidence-write failure.
Results retain the family result fields in an explicit
`nemo.installed-family-result/1` extension with `format: nemo.parity-family-result/1`.
`installed-pass` requires all rows, unchanged candidate identity, zero operator
failures and complete cleanup. These receipts stay in the private report folder;
they are never inserted into the headless `results/` census as `source-pass`.
