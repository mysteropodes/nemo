# rquickjs-sys 0.14.0: Nemo provenance (#2088, E2)

`rquickjs-sys-0.14.0/` is the complete upstream crate archive, unpacked
unmodified, followed by one two-file C patch. Nothing was removed, regenerated
or reformatted. Upstream licenses, build inputs, prebuilt bindings and
later-platform files (all `src/bindings/*`, `wasm-shim/`, `vendor/wasi-libc/`)
are preserved.

| Item | Value |
|---|---|
| Crate | `rquickjs-sys` 0.14.0 (crates.io), used by `rquickjs`/`rquickjs-core` `=0.14.0` |
| Upstream source | https://github.com/DelSkayn/rquickjs, commit `d7ef5eeae702fea24c03643064de454f1c1dd4b0`, path `sys` (from `.cargo_vcs_info.json`) |
| Archive | `rquickjs-sys-0.14.0.crate`, SHA-256 `cee271d0eeba64f0915b846cb7ae02e16faf3dfdffdca91731101d9d30fe3423` (the crates.io checksum previously in `native-engine/Cargo.lock`) |
| Bundled engine | quickjs-ng 0.16.2 (`QJS_VERSION_*` in `quickjs/quickjs.h`) |
| Files | 306 from the archive; 304 byte-identical, 2 patched |
| Licenses | crate: MIT (`Cargo.toml`); QuickJS: MIT (`quickjs/LICENSE`); wasi-libc: `vendor/wasi-libc/NOTICE.md`. All kept unchanged. |
| Patch | [`rquickjs-sys-0.14.0.quickjs.patch`](rquickjs-sys-0.14.0.quickjs.patch), apply with `patch -p1` in the crate root |

## Patched files

| File | SHA-256 before (archive) | SHA-256 after (this tree) |
|---|---|---|
| `quickjs/quickjs.c` | `3a6b52a225c21709ef1fd314183efd3af862e867ce4ef8caabcae67922a745b3` | `8aa0cc0b1d9c0239e55f716a686b5854aa144f1162374f62e1581ee46c726c68` |
| `quickjs/quickjs.h` | `979127138da79cad5ddc7effa1afb13e8b02f744da2aa36844d4cb9d8cb92de9` | `dfe570b06807549ec414c4c4aa03754fca16210fb34118315b7497c11acd97d2` |

## What the patch adds

One exported C function, `JS_EvalSingleFunction(ctx, input, input_len,
filename)`. It runs QuickJS's normal `__JS_EvalInternal` path (strict global
script, as rquickjs's default `EvalOptions`), but replaces the whole-program
parser with one that accepts exactly one function expression followed by EOF:
the first token must be `function`, the function is parsed by the existing
`js_parse_function_decl`, and any token after its closing `}` is a SyntaxError.
On success only the closure is created; no function body runs.

The C function does not fix the function kind or parameters. `JS_FUNC_NORMAL`
is only the parser's starting kind, and `function *` is still parsed as a
generator. Nemo's caller (`compile` in `native-engine/src/object_expression_runtime.rs`)
fixes both with the literal `function(value,frame){'use strict';` prefix, which
precedes every guest byte.

The patch adds a new symbol instead of a new `JS_Eval` flag, so a build
without it fails to link (`undefined symbol: JS_EvalSingleFunction`) and can
never fall back to evaluating the concatenated wrapper as a script.

## Wiring

`native-engine/Cargo.toml` (the manifest that owns `native-engine/Cargo.lock`):

```toml
[patch.crates-io]
rquickjs-sys = { path = "third_party/rquickjs-sys-0.14.0" }
```

The only lock change is that the `rquickjs-sys` 0.14.0 entry loses its
`source` and `checksum` lines. `src-tauri` does not resolve `rquickjs`; a
future workspace that enables `object_expression_runtime` must carry the same
`[patch]` at its own root, which the link failure enforces.

## Audit

```sh
mkdir /tmp/rq && tar -xzf rquickjs-sys-0.14.0.crate -C /tmp/rq
sha256sum rquickjs-sys-0.14.0.crate   # cee271d0…3423
(cd /tmp/rq/rquickjs-sys-0.14.0 && patch -p1 < <repo>/native-engine/third_party/rquickjs-sys-0.14.0.quickjs.patch)
diff -r /tmp/rq/rquickjs-sys-0.14.0 <repo>/native-engine/third_party/rquickjs-sys-0.14.0   # no output
```

Upgrading `rquickjs` requires re-vendoring the matching archive and rebasing
this patch; `[patch]` refuses a mismatched version at resolve time.
