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
