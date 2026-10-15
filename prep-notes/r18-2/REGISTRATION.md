# R18.2 prep — registration NOT committed (shared files owned by Ilya's team)

Branch `claude/r18-2-prep` (local only) commits the kernels, tests and fixtures.
To compile/test them, add exactly these lines (verified locally, then reverted):

## native-engine/src/evaluation.rs — after the `expression_time` child

```rust
// R18.2: private Motion parent-chain affine kernel.
#[path = "motion_parent_chain.rs"]
#[allow(dead_code)]
pub(crate) mod motion_parent_chain;
// R18.2: private expression-control read rules.
#[path = "expression_controls.rs"]
#[allow(dead_code)]
pub(crate) mod expression_controls;
```

## native-engine/src/lib.rs — in `test_modules!`, after `expression_time_tests`

```rust
    "test-evaluation" => motion_parent_chain_tests = "../tests/motion_parent_chain.rs";
    "test-evaluation" => expression_controls_tests = "../tests/expression_controls.rs";
```

## engineering/boundaries/profiles/rust.profile.json

- native-evaluation module `files`: append `"motion_parent_chain.rs", "expression_controls.rs"`
  (NOT in `publicApi`).
- tests module `files`: append `"motion_parent_chain.rs", "expression_controls.rs"`.

## engineering/boundaries/profiles/rust.coverage.json

Two new note keys in the style of `nativeExpressionTimeRegistration`, e.g.
`nativeMotionParentChainRegistration` and `nativeExpressionControlsRegistration`
(private crate-only children of evaluation.rs, tests under existing test-evaluation,
import nothing in the crate, no new layer/edge/feature/baseline/ceiling/floor change).

## tests/native-engine-enforcement.test.cjs (~line 336, as N22F b528aca7 did)

In `'Cargo features, targets and declaration macros are one exact contract'`, the
`name === 'evaluation'` expected list gains, after
`'expression_time_tests = "../tests/expression_time.rs"'`:

```js
      'motion_parent_chain_tests = "../tests/motion_parent_chain.rs"',
      'expression_controls_tests = "../tests/expression_controls.rs"'
```

## Run

```
cd native-engine
cargo test --locked --features test-evaluation --test evaluation motion_parent_chain -- --nocapture
cargo test --locked --features test-evaluation --test evaluation expression_controls
cargo clippy --locked --all-targets --features test-evaluation -- -D warnings   # pre-existing errors elsewhere
```

Oracle generators (untracked here): `oracle-gen/parent_chain.mjs`, `oracle-gen/controls.mjs`
(`node oracle-gen/<x>.mjs <repo>`; reads motion.js via `git show $PIN_SHA:src/js/motion.js`,
default PIN 660ac5ef79f9a1da54cfabbe5279aeb76a941f73; output is deterministic).
