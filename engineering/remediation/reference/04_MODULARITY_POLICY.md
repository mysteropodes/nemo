<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Modularity and file-size policy

> **Policy reference — 2026-09-07.** The
> [English execution checklist](../EXECUTION_PLAN.en.md) / [French copy](../EXECUTION_PLAN.fr.md)
> governs current migration and acceptance. Existing reviewed profile JSON and source
> define the enforced rules; the broader target below must not be mistaken for delivered
> tooling or an instruction to repeat initial adoption.

Source at `66ece0641708122eb8447e85ad8dd7e3402aaf6c` already contains the
[bounded checker and adopted profiles](../../boundaries/README.md), source discovery and
size ratchets. It does not establish complete application dependency enforcement or all
Rust-source classification. P10/P11 and the registration/schema leaves close those named
gaps. A line budget is a warning against mixed responsibilities, not a substitute for a
coherent API; never raise a reviewed ceiling just to obtain a pass.

## Logical layers

```text
bootstrap/       selects and wires concrete adapters
features/        timeline, drawing, inspector and media UI
application/     commands, transactions, history and use cases
domain/          document, identity, tracks and evaluation contracts
ports/           renderer, media, persistence and scheduler interfaces
adapters/        Paper, WebGPU/WASM, Tauri and browser implementations
shared/          small domain-independent values only
```

Dependency rules:

- domain imports only domain/shared and has no DOM, Paper, Tauri or concrete renderer access;
- application imports domain/ports/shared;
- features call public application APIs and own only their UI state;
- adapters implement ports; bootstrap composes them;
- no private cross-module deep imports, new cycles, implicit globals or UI imports into domain;
- a global `services`/`context` object cannot bypass boundaries;
- typed events are for true asynchronous fan-out, not every function call;
- every migrated module declares owner/reviewer, public API, state owner, dependencies,
  lifecycle, fixtures and performance invariants.

## Size profile reference

Count nonblank physical lines including comments after formatting, and separately report total
physical lines. Reconcile these reference budgets with the selected path's adopted profile;
a policy change needs review rather than silently replacing its protected-base ratchet.

| Profile | Warn | Hard maximum |
|---|---:|---:|
| Domain/application JS or TS | 300 | 400 |
| Feature UI JS or TS | 350 | 500 |
| Platform/engine adapter | 300 | 500 |
| Rust production module | 350 | 500 |
| Test file | 450 | 600 |
| Stylesheet | 250 | 350 |
| Handwritten config/bootstrap | 150 | 250 |

Function-body soft warning: 60 nonblank lines. Hard review/exception threshold: 100.
Cyclomatic/branch complexity warns at 12 and requires explicit review above 20.

Do not pass the gate by deleting useful comments, minifying, moving code into arbitrary tiny
files or hiding branches behind meaningless helpers.

## Enforcement target and current boundary

The current checker uses bounded lexical JS analysis and separate language-neutral size
counting; it is not a complete AST/type system or a Rust dependency graph. The tools below
describe possible enforcement mechanisms. Use the checklist's named outcomes and extend
existing checks; do not install a parallel toolchain just because it appears in this list.

- ESLint handles JS/TS line/function/complexity and global rules.
- dependency-cruiser enforces imports, cycles and forbidden layer edges.
- an AST rule inventories legacy `window.SM*` and other implicit global access.
- rustfmt, Clippy, module visibility and Cargo metadata enforce Rust boundaries.
- one cross-language script applies profiles and validates the exception register.
- schemas/types are generated from one source and checked for drift.
- dynamic loaders/plugin entry points are declared rather than omitted from the graph.

Prove the checker with deliberate negative controls: oversized module, forbidden import,
implicit global, stale generated schema and missing capability registration. Retain useful
controlled regression fixtures in the test harness; leave no violation in production source.

## Legacy migration

1. Record an exact-path baseline for current violations. New/migrated code follows policy
   immediately; an oversized legacy file may shrink but cannot silently exceed its ceiling.
   The complete frozen census must eventually migrate every handwritten monolith; a temporary
   ceiling is not a permanent remediation-completion exemption.
2. Characterize one responsibility through real behavior fixtures.
3. Extract it behind a narrow compatibility facade with one state owner.
4. Migrate from classic global script order toward ESM with a bootstrap that waits for required
   modules. Do not assume changing a script tag or package type solves load order.
5. Add JSDoc/checkJs or TypeScript incrementally at boundaries.
6. Route every caller and consumer, validate the real app, then retire the legacy writer.
7. Avoid broad formatting/renames while others work in the same legacy region.

Suggested order: key lookup/easing → track evaluation → document codecs/identity → command/
undo transactions → timeline UI → renderer adapters → media/export.

## Exceptions

Each exception names:

- exact path and profile/rule;
- current measured ceiling;
- reason the module remains coherent;
- accountable human owner;
- tracking issue and review/expiry date;
- affected contracts/tests and shrink/removal condition.

CI rejects unowned, expired or increased baselines. Vendored/generated/minified/data assets
use a separate provenance and integrity policy; do not split a shader catalog or translation
table merely to satisfy an application-code line budget.
