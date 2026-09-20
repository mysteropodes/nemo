# Native opacity engine scaffold

N06/#1338 reserves the crate and module seams for the first admitted opacity
slice. It supplies **no runtime behavior**, writable document, renderer or
application dispatcher. `publish = false`, `default = []`, and every external
dependency is optional. The default test command builds the empty library and
runs zero behavior tests; that is scaffold validation only.

The authority contract is [ADR-001](../engineering/application/ADR-001-native-document-and-evaluation.md).
The [execution plan](../engineering/remediation/EXECUTION_PLAN.en.md) controls
admission and serialized ownership. N07–N15 add the files reserved below;
features for missing files deliberately fail to compile. Host transports come
in N16, production ownership changes only in N20, and installed desktop/browser
acceptance belongs to N21. There is no JS fallback or editable mirror here.

## Features, future files and direction

Production feature names match the named test targets. `viewport` deliberately
uses `desktop_viewport.rs`, the exact N13-owned filename. A feature enables only
its lower-level requirements; issue sequencing is a separate acceptance graph.
For example N10 waits for N09 acceptance, but immutable evaluation does not need
history code. Export uses the compositor without depending on a desktop window.

| Leaf / feature | Future files under `src/` | Lower-level features | Optional external dependencies |
| --- | --- | --- | --- |
| N07 / `codec` | `document.rs`, `codec.rs` | none | serde (derive), serde_json |
| N08 / `commands` | `revision.rs`, `commands.rs`, `request_receipts.rs` | codec | none |
| N09 / `history` | `transaction.rs`, `history.rs` | commands | none |
| N10 / `evaluation` | `evaluation.rs` | commands | none |
| N11 / `scheduler` | `scheduler.rs`, `resource_leases.rs` | evaluation | none |
| N12 / `compositor` | `render_scene.rs`, `compositor.rs` | scheduler | vello 0.9, wgpu 29, pollster 0.4 |
| N13 / `viewport` | `desktop_viewport.rs` | compositor | raw-window-handle 0.6 |
| N14 / `export_job` | `export_job.rs`, `png_output.rs` | history, compositor | png 0.18 |
| N15 / `application` | `application.rs`, `protocol.rs` | history, export_job | none |

`Cargo.lock` freezes resolved versions, including optional dependencies. The GPU
dependency family matches the accepted N04 proof; this reserves dependencies,
not its proof implementation. PNG is the only admitted export format. The crate
does not depend on Tauri, MCP, JS, a feasibility crate or the legacy geometry
engine. Future hosts depend on the engine, never the reverse.

Document values and immutable revisions form the bottom of the module graph.
Commands own mutations and receipts; history/transactions operate through that
authority. Evaluation reads immutable inputs, scheduling manages their leases,
and compositor/viewport/export consume scheduled evaluation. Application dispatch
calls those public modules; lower modules never call application dispatch or a
host. Codec must not depend on commands, evaluation, GPU or hosts; evaluator and
scheduler must not depend on a concrete compositor, window or exporter. Within
each leaf, split value/port types from concrete effects to maintain that direction.
Cargo feature inclusion is checked now; a Rust import graph is not enforced by
this scaffold. N19 owns the later dependency enforcement outcome.

## Test harness convention

Each explicit `[[test]]` target points to `src/lib.rs` and requires a distinct
`test-<target>` feature. That feature enables the corresponding production
feature. Under `cfg(test)`, the harness declares the real `tests/<target>.rs`
module; `viewport` instead uses `tests/desktop_viewport.rs`. Future test files
should import the library as `nemo_native_engine`, as normal integration tests
do. They are modules in this harness, so crate-root-only attributes belong in
the scaffold, not those files. `autotests = false` prevents duplicate automatic
targets when those files arrive.

The declaration macros contain only module declarations and feature/path
attributes. Rustfmt does not expand them, so the exact scaffold format command
can run before future files exist. Once files arrive, the explicit per-file
rustfmt command below is also required: Cargo fmt cannot discover modules hidden
inside macros. Cargo reports the intentional shared-target source path as a
warning; it is not a missing feature or passing placeholder suite.

Run one named target with its matching test feature for focused validation.
Enabling multiple test features includes multiple suites in each selected
harness; a broad `cargo test --all-features` would repeat them and cannot work
before every implementation exists. N06 does not claim that command as a gate.
Enabling only a production feature never activates its integration target.
Selecting a test feature with an absent implementation or test file fails rather
than substituting an empty suite.

## Exact local commands

From the repository root, keep build outputs in the already ignored receipt
directory; `native-engine/target` is not covered by the existing ignore rules:

```sh
export CARGO_TARGET_DIR="$PWD/reports/native-engine-target"
cargo fmt --manifest-path native-engine/Cargo.toml --check
cargo test --locked --manifest-path native-engine/Cargo.toml
cargo metadata --locked --no-deps --format-version 1 --manifest-path native-engine/Cargo.toml
node --test tests/nemo-boundaries-rust.test.cjs
npm run check
```

The following N06 negative control must fail with `requires the features` and
name `test-codec`, rather than failing because an unknown target was supplied:

```sh
cargo test --locked --manifest-path native-engine/Cargo.toml --no-default-features --test codec
```

Apply the same control to all nine target names listed above. Once a
leaf's implementation and predecessors have landed, its focused command is:

```sh
cargo test --locked --manifest-path native-engine/Cargo.toml --no-default-features --features test-codec --test codec
cargo test --locked --manifest-path native-engine/Cargo.toml --no-default-features --features test-commands --test commands
cargo test --locked --manifest-path native-engine/Cargo.toml --no-default-features --features test-history --test history
cargo test --locked --manifest-path native-engine/Cargo.toml --no-default-features --features test-evaluation --test evaluation
cargo test --locked --manifest-path native-engine/Cargo.toml --no-default-features --features test-scheduler --test scheduler
cargo test --locked --manifest-path native-engine/Cargo.toml --no-default-features --features test-compositor --test compositor
cargo test --locked --manifest-path native-engine/Cargo.toml --no-default-features --features test-viewport --test viewport
cargo test --locked --manifest-path native-engine/Cargo.toml --no-default-features --features test-export_job --test export_job
cargo test --locked --manifest-path native-engine/Cargo.toml --no-default-features --features test-application --test application
```

Also format every actually present Rust source after adding files. This command
includes non-ignored untracked candidate files as well as committed files:

```sh
git ls-files -co --exclude-standard native-engine | rg '\.rs$' | xargs rustfmt --edition 2021 --check
```

The default crate is independent of GPU/window availability. Passing its checks
does not establish rendering, export, save/load, undo/redo, MCP, browser or
installed-app behavior. Future GPU and desktop checks must report unavailable
hardware/surfaces explicitly. No hosted build is authorized by these commands.

## Source registration and inherited debt

N06 registers `native-engine/src/lib.rs` and the five already accepted N03/N04
Rust proof files in `engineering/boundaries/profiles/rust.profile.json`. The
proofs are grouped by crate in the `tests` layer because they are isolated
feasibility fixtures, including their host/build sources, not production engine
or host modules. Labels classify source; they do not enforce imports.

At base `bdb44fa6e5b3953aea0e6816fa676bdc0e97f0fe`, N03 `src/lib.rs` has 655
nonblank lines and N04 `src/native_surface.rs` has 814. N06 records these exact
pre-existing sizes in the baseline and coverage policy with accountable,
expiring no-growth exceptions, with no headroom above the measured counts. It
does not edit or exclude those sources. Earlier baseline entries and ordinary limits remain intact;
the new engine source receives no exception. Future source additions must update
the serialized profile/coverage tests in their own leaf, before N19.
