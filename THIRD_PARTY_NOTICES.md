# Third-Party Notices

Auto-generated on 2026-07-20 by `scripts/generate-third-party-notices.py` — do not edit by hand, re-run the script instead (requires `cargo install cargo-license` once; `npx license-checker` needs no separate install).

This file lists every third-party dependency this project pulls in (npm packages, Rust crates for both `src-tauri` and `geometry-wasm`), plus files vendored directly (not through a package manager) and external binaries bundled at build time. Re-run this script whenever a dependency is added, removed, or upgraded, and before any build meant for distribution — a newly-added dependency can bring in a license family not yet reviewed here.

## Vendored files (not tracked by npm/cargo)

- **opentype.js** (`src/js/opentype.min.js`) — **MIT**
  Copyright (c) 2020 Frederik De Bleser — https://github.com/opentypejs/opentype.js
  Minified, unmodified. Font loading/glyph metrics (vector-text-bridge.js).

- **MP4Box.js** (`src/js/mp4box.all.min.js`) — **BSD-3-Clause**
  Copyright (c) 2012. Telecom ParisTech/TSI/MM/GPAC Cyril Concolato — https://github.com/gpac/mp4box.js
  v0.5.2, minified, unmodified. MP4/MOV demuxing (native-video-bridge.js).

## Bundled external binaries

- **ffmpeg** — **GPL (built with --enable-gpl --enable-libx264 --enable-libx265)**
  Piped as an external subprocess (src-tauri/binaries/, invoked via std::process::Command) — never linked into the Rust binary. This is "simple aggregation", not linking, which is the standard, safer pattern for shipping ffmpeg with a commercial app. It does NOT make the GPL dependency disappear, though: before selling this app, this binary should be replaced with a custom LGPL-only, decode-only ffmpeg build to be fully clean of GPL obligations.

## npm dependencies (2)

### By license

<details><summary><strong>Apache-2.0 OR MIT</strong> (2)</summary>

- `@tauri-apps/cli-darwin-arm64@2.11.4` — https://github.com/tauri-apps/tauri
- `@tauri-apps/cli@2.11.4` — https://github.com/tauri-apps/tauri

</details>


## Rust crates — src-tauri (511)

### By license

<details><summary><strong>(Apache-2.0 OR MIT) AND BSD-3-Clause</strong> (1)</summary>

- `encoding_rs@0.8.35` — https://github.com/hsivonen/encoding_rs

</details>

<details><summary><strong>(Apache-2.0 OR MIT) AND Unicode-3.0</strong> (1)</summary>

- `unicode-ident@1.0.24` — https://github.com/dtolnay/unicode-ident

</details>

<details><summary><strong>0BSD OR Apache-2.0 OR MIT</strong> (1)</summary>

- `adler2@2.0.1` — https://github.com/oyvindln/adler2

</details>

<details><summary><strong>Apache-2.0</strong> (2)</summary>

- `sync_wrapper@1.0.2` — https://github.com/Actyx/sync_wrapper
- `tao@0.35.3` — https://github.com/tauri-apps/tao

</details>

<details><summary><strong>Apache-2.0 AND ISC</strong> (1)</summary>

- `ring@0.17.14` — https://github.com/briansmith/ring

</details>

<details><summary><strong>Apache-2.0 AND MIT</strong> (1)</summary>

- `dpi@0.1.2` — https://github.com/rust-windowing/winit

</details>

<details><summary><strong>Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT</strong> (5)</summary>

- `linux-raw-sys@0.12.1` — https://github.com/sunfishcode/linux-raw-sys
- `rustix@1.1.4` — https://github.com/bytecodealliance/rustix
- `wasi@0.11.1+wasi-snapshot-preview1` — https://github.com/bytecodealliance/wasi
- `wasip2@1.0.4+wasi-0.2.12` — https://github.com/bytecodealliance/wasi-rs
- `wit-bindgen@0.57.1` — https://github.com/bytecodealliance/wit-bindgen

</details>

<details><summary><strong>Apache-2.0 OR BSD-3-Clause OR MIT</strong> (2)</summary>

- `num_enum@0.7.6` — https://github.com/illicitonion/num_enum
- `num_enum_derive@0.7.6` — https://github.com/illicitonion/num_enum

</details>

<details><summary><strong>Apache-2.0 OR BSL-1.0</strong> (1)</summary>

- `ryu@1.0.23` — https://github.com/dtolnay/ryu

</details>

<details><summary><strong>Apache-2.0 OR CC0-1.0 OR MIT-0</strong> (1)</summary>

- `dunce@1.0.5` — https://gitlab.com/kornelski/dunce

</details>

<details><summary><strong>Apache-2.0 OR ISC OR MIT</strong> (3)</summary>

- `hyper-rustls@0.27.9` — https://github.com/rustls/hyper-rustls
- `rustls-native-certs@0.8.4` — https://github.com/rustls/rustls-native-certs
- `rustls@0.23.41` — https://github.com/rustls/rustls

</details>

<details><summary><strong>Apache-2.0 OR LGPL-2.1-or-later OR MIT</strong> (2)</summary>

- `r-efi@5.3.0` — https://github.com/r-efi/r-efi
- `r-efi@6.0.0` — https://github.com/r-efi/r-efi

</details>

<details><summary><strong>Apache-2.0 OR MIT</strong> (311)</summary>

- `android_system_properties@0.1.5` — https://github.com/nical/android_system_properties
- `anyhow@1.0.103` — https://github.com/dtolnay/anyhow
- `arbitrary@1.4.2` — https://github.com/rust-fuzz/arbitrary/
- `atomic-waker@1.1.2` — https://github.com/smol-rs/atomic-waker
- `autocfg@1.5.1` — https://github.com/cuviper/autocfg
- `base64@0.21.7` — https://github.com/marshallpierce/rust-base64
- `base64@0.22.1` — https://github.com/marshallpierce/rust-base64
- `bit-set@0.8.0` — https://github.com/contain-rs/bit-set
- `bit-vec@0.8.0` — https://github.com/contain-rs/bit-vec
- `bitflags@1.3.2` — https://github.com/bitflags/bitflags
- `bitflags@2.13.0` — https://github.com/bitflags/bitflags
- `block-buffer@0.10.4` — https://github.com/RustCrypto/utils
- `bs58@0.5.1` — https://github.com/Nullus157/bs58-rs
- `bumpalo@3.20.3` — https://github.com/fitzgen/bumpalo
- `camino@1.2.4` — https://github.com/camino-rs/camino
- `cargo-platform@0.1.9` — https://github.com/rust-lang/cargo
- `cargo_toml@0.22.3` — https://gitlab.com/lib.rs/cargo_toml
- `cc@1.2.65` — https://github.com/rust-lang/cc-rs
- `cesu8@1.1.0` — https://github.com/emk/cesu8-rs
- `cfg-expr@0.15.8` — https://github.com/EmbarkStudios/cfg-expr
- `cfg-if@1.0.4` — https://github.com/rust-lang/cfg-if
- `chacha20@0.10.1` — https://github.com/RustCrypto/stream-ciphers
- `chrono@0.4.45` — https://github.com/chronotope/chrono
- `cookie@0.18.1` — https://github.com/SergioBenitez/cookie-rs
- `cookie_store@0.22.1` — https://github.com/pfernie/cookie_store
- `core-foundation-sys@0.8.7` — https://github.com/servo/core-foundation-rs
- `core-foundation@0.10.1` — https://github.com/servo/core-foundation-rs
- `core-foundation@0.9.4` — https://github.com/servo/core-foundation-rs
- `core-graphics-types@0.2.0` — https://github.com/servo/core-foundation-rs
- `core-graphics@0.25.0` — https://github.com/servo/core-foundation-rs
- `cpufeatures@0.2.17` — https://github.com/RustCrypto/utils
- `cpufeatures@0.3.0` — https://github.com/RustCrypto/utils
- `crc32fast@1.5.0` — https://github.com/srijs/rust-crc32fast
- `crossbeam-channel@0.5.15` — https://github.com/crossbeam-rs/crossbeam
- `crossbeam-utils@0.8.21` — https://github.com/crossbeam-rs/crossbeam
- `crypto-common@0.1.7` — https://github.com/RustCrypto/traits
- `ctor-proc-macro@0.0.7` — https://github.com/mmastrac/rust-ctor
- `ctor@0.8.0` — https://github.com/mmastrac/rust-ctor
- `data-url@0.3.2` — https://github.com/servo/rust-url
- `dbus@0.9.11` — https://github.com/diwic/dbus-rs
- `deranged@0.5.8` — https://github.com/jhpratt/deranged
- `derive_arbitrary@1.4.2` — https://github.com/rust-fuzz/arbitrary
- `digest@0.10.7` — https://github.com/RustCrypto/traits
- `dirs-sys@0.5.0` — https://github.com/dirs-dev/dirs-sys-rs
- `dirs@6.0.0` — https://github.com/soc/dirs-rs
- `displaydoc@0.2.6` — https://github.com/yaahc/displaydoc
- `document-features@0.2.12` — https://github.com/slint-ui/document-features
- `dtoa@1.0.11` — https://github.com/dtolnay/dtoa
- `dtor-proc-macro@0.0.6` — https://github.com/mmastrac/rust-ctor
- `dtor@0.3.0` — https://github.com/mmastrac/rust-ctor
- `dyn-clone@1.0.20` — https://github.com/dtolnay/dyn-clone
- `embed_plist@1.2.2` — https://github.com/nvzqz/embed-plist-rs
- `equivalent@1.0.2` — https://github.com/indexmap-rs/equivalent
- `erased-serde@0.4.10` — https://github.com/dtolnay/erased-serde
- `errno@0.3.14` — https://github.com/lambda-fairy/rust-errno
- `fastrand@2.4.1` — https://github.com/smol-rs/fastrand
- `fdeflate@0.3.7` — https://github.com/image-rs/fdeflate
- `field-offset@0.3.6` — https://github.com/Diggsey/rust-field-offset
- `filetime@0.2.29` — https://github.com/alexcrichton/filetime
- `find-msvc-tools@0.1.9` — https://github.com/rust-lang/cc-rs
- `flate2@1.1.9` — https://github.com/rust-lang/flate2-rs
- `fnv@1.0.7` — https://github.com/servo/rust-fnv
- `foreign-types-macros@0.2.3` — https://github.com/sfackler/foreign-types
- `foreign-types-shared@0.3.1` — https://github.com/sfackler/foreign-types
- `foreign-types@0.5.0` — https://github.com/sfackler/foreign-types
- `form_urlencoded@1.2.2` — https://github.com/servo/rust-url
- `futures-channel@0.3.32` — https://github.com/rust-lang/futures-rs
- `futures-core@0.3.32` — https://github.com/rust-lang/futures-rs
- `futures-executor@0.3.32` — https://github.com/rust-lang/futures-rs
- `futures-io@0.3.32` — https://github.com/rust-lang/futures-rs
- `futures-macro@0.3.32` — https://github.com/rust-lang/futures-rs
- `futures-sink@0.3.32` — https://github.com/rust-lang/futures-rs
- `futures-task@0.3.32` — https://github.com/rust-lang/futures-rs
- `futures-util@0.3.32` — https://github.com/rust-lang/futures-rs
- `getrandom@0.2.17` — https://github.com/rust-random/getrandom
- `getrandom@0.3.4` — https://github.com/rust-random/getrandom
- `getrandom@0.4.3` — https://github.com/rust-random/getrandom
- `glob@0.3.3` — https://github.com/rust-lang/glob
- `hashbrown@0.12.3` — https://github.com/rust-lang/hashbrown
- `hashbrown@0.17.1` — https://github.com/rust-lang/hashbrown
- `heck@0.4.1` — https://github.com/withoutboats/heck
- `heck@0.5.0` — https://github.com/withoutboats/heck
- `hex@0.4.3` — https://github.com/KokaKiwi/rust-hex
- `html5ever@0.38.0` — https://github.com/servo/html5ever
- `http@1.4.2` — https://github.com/hyperium/http
- `httparse@1.10.1` — https://github.com/seanmonstar/httparse
- `iana-time-zone-haiku@0.1.2` — https://github.com/strawlab/iana-time-zone
- `iana-time-zone@0.1.65` — https://github.com/strawlab/iana-time-zone
- `ident_case@1.0.1` — https://github.com/TedDriggs/ident_case
- `idna@1.1.0` — https://github.com/servo/rust-url/
- `idna_adapter@1.2.2` — https://github.com/hsivonen/idna_adapter
- `indexmap@1.9.3` — https://github.com/bluss/indexmap
- `indexmap@2.14.0` — https://github.com/indexmap-rs/indexmap
- `ipnet@2.12.0` — https://github.com/krisprice/ipnet
- `itoa@1.0.18` — https://github.com/dtolnay/itoa
- `jni-macros@0.22.4` — https://github.com/jni-rs/jni-rs
- `jni-sys-macros@0.4.1` — https://github.com/jni-rs/jni-sys
- `jni-sys@0.3.1` — https://github.com/jni-rs/jni-sys
- `jni-sys@0.4.1` — https://github.com/jni-rs/jni-sys
- `jni@0.21.1` — https://github.com/jni-rs/jni-rs
- `jni@0.22.4` — https://github.com/jni-rs/jni-rs
- `js-sys@0.3.103` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys
- `json-patch@3.0.1` — https://github.com/idubrov/json-patch
- `jsonptr@0.6.3` — https://github.com/chanced/jsonptr
- `keyboard-types@0.7.0` — https://github.com/pyfisch/keyboard-types
- `libappindicator-sys@0.9.0`
- `libappindicator@0.9.0`
- `libc@0.2.186` — https://github.com/rust-lang/libc
- `libdbus-sys@0.2.7` — https://github.com/diwic/dbus-rs
- `litrs@1.0.0` — https://github.com/LukasKalbertodt/litrs
- `lock_api@0.4.14` — https://github.com/Amanieu/parking_lot
- `log@0.4.33` — https://github.com/rust-lang/log
- `markup5ever@0.38.0` — https://github.com/servo/html5ever
- `mime@0.3.17` — https://github.com/hyperium/mime
- `muda@0.19.3` — https://github.com/tauri-apps/muda
- `ndk-sys@0.6.0+11769913` — https://github.com/rust-mobile/ndk
- `ndk@0.9.0` — https://github.com/rust-mobile/ndk
- `num-conv@0.2.2` — https://github.com/jhpratt/num-conv
- `num-traits@0.2.19` — https://github.com/rust-num/num-traits
- `once_cell@1.21.4` — https://github.com/matklad/once_cell
- `openssl-probe@0.2.1` — https://github.com/rustls/openssl-probe
- `osakit@0.3.1` — https://github.com/mdevils/rust-osakit
- `parking_lot@0.12.5` — https://github.com/Amanieu/parking_lot
- `parking_lot_core@0.9.12` — https://github.com/Amanieu/parking_lot
- `percent-encoding@2.3.2` — https://github.com/servo/rust-url/
- `pin-project-lite@0.2.17` — https://github.com/taiki-e/pin-project-lite
- `pkg-config@0.3.33` — https://github.com/rust-lang/pkg-config-rs
- `png@0.17.16` — https://github.com/image-rs/image-png
- `png@0.18.1` — https://github.com/image-rs/image-png
- `powerfmt@0.2.0` — https://github.com/jhpratt/powerfmt
- `proc-macro-crate@1.3.1` — https://github.com/bkchr/proc-macro-crate
- `proc-macro-crate@2.0.2` — https://github.com/bkchr/proc-macro-crate
- `proc-macro-crate@3.5.0` — https://github.com/bkchr/proc-macro-crate
- `proc-macro-error-attr@1.0.4` — https://gitlab.com/CreepySkeleton/proc-macro-error
- `proc-macro-error@1.0.4` — https://gitlab.com/CreepySkeleton/proc-macro-error
- `proc-macro2@1.0.106` — https://github.com/dtolnay/proc-macro2
- `psl-types@2.0.11` — https://github.com/addr-rs/psl-types
- `publicsuffix@2.3.0` — https://github.com/rushmorem/publicsuffix
- `quinn-proto@0.11.16` — https://github.com/quinn-rs/quinn
- `quinn-udp@0.5.15` — https://github.com/quinn-rs/quinn
- `quinn@0.11.11` — https://github.com/quinn-rs/quinn
- `quote@1.0.46` — https://github.com/dtolnay/quote
- `rand@0.10.2` — https://github.com/rust-random/rand
- `rand_core@0.10.1` — https://github.com/rust-random/rand_core
- `rand_pcg@0.10.2` — https://github.com/rust-random/rngs
- `ref-cast-impl@1.0.25` — https://github.com/dtolnay/ref-cast
- `ref-cast@1.0.25` — https://github.com/dtolnay/ref-cast
- `regex-automata@0.4.14` — https://github.com/rust-lang/regex
- `regex-syntax@0.8.11` — https://github.com/rust-lang/regex
- `regex@1.12.4` — https://github.com/rust-lang/regex
- `reqwest@0.12.28` — https://github.com/seanmonstar/reqwest
- `reqwest@0.13.4` — https://github.com/seanmonstar/reqwest
- `rustc-hash@2.1.2` — https://github.com/rust-lang/rustc-hash
- `rustc_version@0.4.1` — https://github.com/djc/rustc-version-rs
- `rustls-pki-types@1.14.1` — https://github.com/rustls/pki-types
- `rustls-platform-verifier-android@0.1.1` — https://github.com/rustls/rustls-platform-verifier
- `rustls-platform-verifier@0.7.0` — https://github.com/rustls/rustls-platform-verifier
- `rustversion@1.0.22` — https://github.com/dtolnay/rustversion
- `scopeguard@1.2.0` — https://github.com/bluss/scopeguard
- `security-framework-sys@2.17.0` — https://github.com/kornelski/rust-security-framework
- `security-framework@3.7.0` — https://github.com/kornelski/rust-security-framework
- `semver@1.0.28` — https://github.com/dtolnay/semver
- `serde-untagged@0.1.9` — https://github.com/dtolnay/serde-untagged
- `serde@1.0.228` — https://github.com/serde-rs/serde
- `serde_core@1.0.228` — https://github.com/serde-rs/serde
- `serde_derive@1.0.228` — https://github.com/serde-rs/serde
- `serde_derive_internals@0.29.1` — https://github.com/serde-rs/serde
- `serde_json@1.0.150` — https://github.com/serde-rs/json
- `serde_repr@0.1.20` — https://github.com/dtolnay/serde-repr
- `serde_spanned@0.6.9` — https://github.com/toml-rs/toml
- `serde_spanned@1.1.1` — https://github.com/toml-rs/toml
- `serde_urlencoded@0.7.1` — https://github.com/nox/serde_urlencoded
- `serde_with@3.21.0` — https://github.com/jonasbb/serde_with/
- `serde_with_macros@3.21.0` — https://github.com/jonasbb/serde_with/
- `serialize-to-javascript-impl@0.1.2` — https://github.com/chippers/serialize-to-javascript
- `serialize-to-javascript@0.1.2` — https://github.com/chippers/serialize-to-javascript
- `servo_arc@0.4.3` — https://github.com/servo/stylo
- `sha2@0.10.9` — https://github.com/RustCrypto/hashes
- `shlex@2.0.1` — https://github.com/comex/rust-shlex
- `signal-hook-registry@1.4.8` — https://github.com/vorner/signal-hook
- `signal-hook@0.3.18` — https://github.com/vorner/signal-hook
- `simd_cesu8@1.1.1` — https://github.com/seancroach/simd_cesu8
- `simdutf8@0.1.5` — https://github.com/rusticstuff/simdutf8
- `siphasher@1.0.3` — https://github.com/jedisct1/rust-siphash
- `smallvec@1.15.2` — https://github.com/servo/rust-smallvec
- `socket2@0.6.4` — https://github.com/rust-lang/socket2
- `softbuffer@0.4.8` — https://github.com/rust-windowing/softbuffer
- `stable_deref_trait@1.2.1` — https://github.com/storyyeller/stable_deref_trait
- `string_cache@0.9.0` — https://github.com/servo/string-cache
- `string_cache_codegen@0.6.1` — https://github.com/servo/string-cache
- `swift-rs@1.0.7` — https://github.com/Brendonovich/swift-rs
- `syn@1.0.109` — https://github.com/dtolnay/syn
- `syn@2.0.118` — https://github.com/dtolnay/syn
- `system-configuration-sys@0.6.0` — https://github.com/mullvad/system-configuration-rs
- `system-configuration@0.7.0` — https://github.com/mullvad/system-configuration-rs
- `system-deps@6.2.2` — https://github.com/gdesmott/system-deps
- `tao-macros@0.1.3` — https://github.com/tauri-apps/tao
- `tar@0.4.46` — https://github.com/composefs/tar-rs
- `tauri-build@2.6.3` — https://github.com/tauri-apps/tauri
- `tauri-codegen@2.6.3` — https://github.com/tauri-apps/tauri
- `tauri-macros@2.6.3` — https://github.com/tauri-apps/tauri
- `tauri-plugin-dialog@2.7.1` — https://github.com/tauri-apps/plugins-workspace
- `tauri-plugin-fs@2.5.1` — https://github.com/tauri-apps/plugins-workspace
- `tauri-plugin-http@2.5.9` — https://github.com/tauri-apps/plugins-workspace
- `tauri-plugin-process@2.3.1` — https://github.com/tauri-apps/plugins-workspace
- `tauri-plugin-shell@2.3.5` — https://github.com/tauri-apps/plugins-workspace
- `tauri-plugin-updater@2.10.1` — https://github.com/tauri-apps/plugins-workspace
- `tauri-plugin@2.6.3` — https://github.com/tauri-apps/tauri
- `tauri-runtime-wry@2.11.3` — https://github.com/tauri-apps/tauri
- `tauri-runtime@2.11.3` — https://github.com/tauri-apps/tauri
- `tauri-utils@2.9.3` — https://github.com/tauri-apps/tauri
- `tauri@2.11.3` — https://github.com/tauri-apps/tauri
- `tempfile@3.27.0` — https://github.com/Stebalien/tempfile
- `tendril@0.5.0` — https://github.com/servo/html5ever
- `thiserror-impl@1.0.69` — https://github.com/dtolnay/thiserror
- `thiserror-impl@2.0.18` — https://github.com/dtolnay/thiserror
- `thiserror@1.0.69` — https://github.com/dtolnay/thiserror
- `thiserror@2.0.18` — https://github.com/dtolnay/thiserror
- `time-core@0.1.9` — https://github.com/time-rs/time
- `time-macros@0.2.30` — https://github.com/time-rs/time
- `time@0.3.51` — https://github.com/time-rs/time
- `tokio-rustls@0.26.4` — https://github.com/rustls/tokio-rustls
- `toml@0.8.2` — https://github.com/toml-rs/toml
- `toml@0.9.12+spec-1.1.0` — https://github.com/toml-rs/toml
- `toml@1.1.2+spec-1.1.0` — https://github.com/toml-rs/toml
- `toml_datetime@0.6.3` — https://github.com/toml-rs/toml
- `toml_datetime@0.7.5+spec-1.1.0` — https://github.com/toml-rs/toml
- `toml_datetime@1.1.1+spec-1.1.0` — https://github.com/toml-rs/toml
- `toml_edit@0.19.15` — https://github.com/toml-rs/toml
- `toml_edit@0.20.2` — https://github.com/toml-rs/toml
- `toml_edit@0.25.12+spec-1.1.0` — https://github.com/toml-rs/toml
- `toml_parser@1.1.2+spec-1.1.0` — https://github.com/toml-rs/toml
- `toml_writer@1.1.1+spec-1.1.0` — https://github.com/toml-rs/toml
- `tray-icon@0.24.1` — https://github.com/tauri-apps/tray-icon
- `typeid@1.0.3` — https://github.com/dtolnay/typeid
- `typenum@1.20.1` — https://github.com/paholg/typenum
- `unic-char-property@0.9.0` — https://github.com/open-i18n/rust-unic/
- `unic-char-range@0.9.0` — https://github.com/open-i18n/rust-unic/
- `unic-common@0.9.0` — https://github.com/open-i18n/rust-unic/
- `unic-ucd-ident@0.9.0` — https://github.com/open-i18n/rust-unic/
- `unic-ucd-version@0.9.0` — https://github.com/open-i18n/rust-unic/
- `unicode-segmentation@1.13.3` — https://github.com/unicode-rs/unicode-segmentation
- `url@2.5.8` — https://github.com/servo/rust-url
- `utf-8@0.7.6` — https://github.com/SimonSapin/rust-utf8
- `utf8_iter@1.0.4` — https://github.com/hsivonen/utf8_iter
- `uuid@1.23.4` — https://github.com/uuid-rs/uuid
- `version_check@0.9.5` — https://github.com/SergioBenitez/version_check
- `wasm-bindgen-futures@0.4.76` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures
- `wasm-bindgen-macro-support@0.2.126` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro-support
- `wasm-bindgen-macro@0.2.126` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro
- `wasm-bindgen-shared@0.2.126` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared
- `wasm-bindgen@0.2.126` — https://github.com/wasm-bindgen/wasm-bindgen
- `wasm-streams@0.5.0` — https://github.com/MattiasBuelens/wasm-streams/
- `web-sys@0.3.103` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys
- `web-time@1.1.0` — https://github.com/daxpedda/web-time
- `web_atoms@0.2.5` — https://github.com/servo/html5ever
- `winapi-i686-pc-windows-gnu@0.4.0` — https://github.com/retep998/winapi-rs
- `winapi-x86_64-pc-windows-gnu@0.4.0` — https://github.com/retep998/winapi-rs
- `winapi@0.3.9` — https://github.com/retep998/winapi-rs
- `window-vibrancy@0.6.0` — https://github.com/tauri-apps/tauri-plugin-vibrancy
- `windows-collections@0.2.0` — https://github.com/microsoft/windows-rs
- `windows-core@0.61.2` — https://github.com/microsoft/windows-rs
- `windows-core@0.62.2` — https://github.com/microsoft/windows-rs
- `windows-future@0.2.1` — https://github.com/microsoft/windows-rs
- `windows-implement@0.60.2` — https://github.com/microsoft/windows-rs
- `windows-interface@0.59.3` — https://github.com/microsoft/windows-rs
- `windows-link@0.1.3` — https://github.com/microsoft/windows-rs
- `windows-link@0.2.1` — https://github.com/microsoft/windows-rs
- `windows-numerics@0.2.0` — https://github.com/microsoft/windows-rs
- `windows-registry@0.6.1` — https://github.com/microsoft/windows-rs
- `windows-result@0.3.4` — https://github.com/microsoft/windows-rs
- `windows-result@0.4.1` — https://github.com/microsoft/windows-rs
- `windows-strings@0.4.2` — https://github.com/microsoft/windows-rs
- `windows-strings@0.5.1` — https://github.com/microsoft/windows-rs
- `windows-sys@0.45.0` — https://github.com/microsoft/windows-rs
- `windows-sys@0.52.0` — https://github.com/microsoft/windows-rs
- `windows-sys@0.59.0` — https://github.com/microsoft/windows-rs
- `windows-sys@0.60.2` — https://github.com/microsoft/windows-rs
- `windows-sys@0.61.2` — https://github.com/microsoft/windows-rs
- `windows-targets@0.42.2` — https://github.com/microsoft/windows-rs
- `windows-targets@0.52.6` — https://github.com/microsoft/windows-rs
- `windows-targets@0.53.5` — https://github.com/microsoft/windows-rs
- `windows-threading@0.1.0` — https://github.com/microsoft/windows-rs
- `windows-version@0.1.7` — https://github.com/microsoft/windows-rs
- `windows@0.61.3` — https://github.com/microsoft/windows-rs
- `windows_aarch64_gnullvm@0.42.2` — https://github.com/microsoft/windows-rs
- `windows_aarch64_gnullvm@0.52.6` — https://github.com/microsoft/windows-rs
- `windows_aarch64_gnullvm@0.53.1` — https://github.com/microsoft/windows-rs
- `windows_aarch64_msvc@0.42.2` — https://github.com/microsoft/windows-rs
- `windows_aarch64_msvc@0.52.6` — https://github.com/microsoft/windows-rs
- `windows_aarch64_msvc@0.53.1` — https://github.com/microsoft/windows-rs
- `windows_i686_gnu@0.42.2` — https://github.com/microsoft/windows-rs
- `windows_i686_gnu@0.52.6` — https://github.com/microsoft/windows-rs
- `windows_i686_gnu@0.53.1` — https://github.com/microsoft/windows-rs
- `windows_i686_gnullvm@0.52.6` — https://github.com/microsoft/windows-rs
- `windows_i686_gnullvm@0.53.1` — https://github.com/microsoft/windows-rs
- `windows_i686_msvc@0.42.2` — https://github.com/microsoft/windows-rs
- `windows_i686_msvc@0.52.6` — https://github.com/microsoft/windows-rs
- `windows_i686_msvc@0.53.1` — https://github.com/microsoft/windows-rs
- `windows_x86_64_gnu@0.42.2` — https://github.com/microsoft/windows-rs
- `windows_x86_64_gnu@0.52.6` — https://github.com/microsoft/windows-rs
- `windows_x86_64_gnu@0.53.1` — https://github.com/microsoft/windows-rs
- `windows_x86_64_gnullvm@0.42.2` — https://github.com/microsoft/windows-rs
- `windows_x86_64_gnullvm@0.52.6` — https://github.com/microsoft/windows-rs
- `windows_x86_64_gnullvm@0.53.1` — https://github.com/microsoft/windows-rs
- `windows_x86_64_msvc@0.42.2` — https://github.com/microsoft/windows-rs
- `windows_x86_64_msvc@0.52.6` — https://github.com/microsoft/windows-rs
- `windows_x86_64_msvc@0.53.1` — https://github.com/microsoft/windows-rs
- `wry@0.55.1` — https://github.com/tauri-apps/wry
- `xattr@1.6.1` — https://github.com/Stebalien/xattr
- `zeroize@1.9.0` — https://github.com/RustCrypto/utils

</details>

<details><summary><strong>Apache-2.0 OR MIT OR Zlib</strong> (22)</summary>

- `bytemuck@1.25.0` — https://github.com/Lokathor/bytemuck
- `dispatch2@0.3.1` — https://github.com/madsmtm/objc2
- `lru-slab@0.1.2` — https://github.com/Ralith/lru-slab
- `miniz_oxide@0.8.9` — https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide
- `objc2-app-kit@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-cloud-kit@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-core-data@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-core-foundation@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-core-graphics@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-core-image@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-core-location@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-core-text@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-exception-helper@0.1.1` — https://github.com/madsmtm/objc2
- `objc2-io-surface@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-osa-kit@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-quartz-core@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-ui-kit@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-user-notifications@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-web-kit@0.3.2` — https://github.com/madsmtm/objc2
- `raw-window-handle@0.6.2` — https://github.com/rust-windowing/raw-window-handle
- `tinyvec@1.11.0` — https://github.com/Lokathor/tinyvec
- `tinyvec_macros@0.1.1` — https://github.com/Soveu/tinyvec_macros

</details>

<details><summary><strong>Apache-2.0 WITH LLVM-exception</strong> (1)</summary>

- `target-lexicon@0.12.16` — https://github.com/bytecodealliance/target-lexicon

</details>

<details><summary><strong>BSD-3-Clause</strong> (3)</summary>

- `alloc-no-stdlib@2.0.4` — https://github.com/dropbox/rust-alloc-no-stdlib
- `alloc-stdlib@0.2.4` — https://github.com/dropbox/rust-alloc-no-stdlib
- `subtle@2.6.1` — https://github.com/dalek-cryptography/subtle

</details>

<details><summary><strong>BSD-3-Clause AND MIT</strong> (1)</summary>

- `brotli@8.0.4` — https://github.com/dropbox/rust-brotli

</details>

<details><summary><strong>BSD-3-Clause OR MIT</strong> (1)</summary>

- `brotli-decompressor@5.0.3` — https://github.com/dropbox/rust-brotli-decompressor

</details>

<details><summary><strong>CDLA-Permissive-2.0</strong> (2)</summary>

- `webpki-root-certs@1.0.8` — https://github.com/rustls/webpki-roots
- `webpki-roots@1.0.8` — https://github.com/rustls/webpki-roots

</details>

<details><summary><strong>ISC</strong> (3)</summary>

- `libloading@0.7.4` — https://github.com/nagisa/rust_libloading/
- `rustls-webpki@0.103.13` — https://github.com/rustls/webpki
- `untrusted@0.9.0` — https://github.com/briansmith/untrusted

</details>

<details><summary><strong>MIT</strong> (115)</summary>

- `atk-sys@0.18.2` — https://github.com/gtk-rs/gtk3-rs
- `atk@0.18.2` — https://github.com/gtk-rs/gtk3-rs
- `block2@0.6.2` — https://github.com/madsmtm/objc2
- `block@0.1.6` — http://github.com/SSheldon/rust-block
- `bytes@1.12.0` — https://github.com/tokio-rs/bytes
- `cairo-rs@0.18.5` — https://github.com/gtk-rs/gtk-rs-core
- `cairo-sys-rs@0.18.2` — https://github.com/gtk-rs/gtk-rs-core
- `cargo_metadata@0.19.2` — https://github.com/oli-obk/cargo_metadata
- `cfb@0.7.3` — https://github.com/mdsteele/rust-cfb
- `cfg_aliases@0.2.1` — https://github.com/katharostech/cfg_aliases
- `combine@4.6.7` — https://github.com/Marwes/combine
- `darling@0.23.0` — https://github.com/TedDriggs/darling
- `darling_core@0.23.0` — https://github.com/TedDriggs/darling
- `darling_macro@0.23.0` — https://github.com/TedDriggs/darling
- `derive_more-impl@2.1.1` — https://github.com/JelteF/derive_more
- `derive_more@2.1.1` — https://github.com/JelteF/derive_more
- `dlopen2@0.8.2` — https://github.com/OpenByteDev/dlopen2
- `dlopen2_derive@0.4.3` — https://github.com/OpenByteDev/dlopen2
- `dom_query@0.27.0` — https://github.com/niklak/dom_query
- `embed-resource@3.0.9` — https://github.com/nabijaczleweli/rust-embed-resource
- `gdk-pixbuf-sys@0.18.0` — https://github.com/gtk-rs/gtk-rs-core
- `gdk-pixbuf@0.18.5` — https://github.com/gtk-rs/gtk-rs-core
- `gdk-sys@0.18.2` — https://github.com/gtk-rs/gtk3-rs
- `gdk@0.18.2` — https://github.com/gtk-rs/gtk3-rs
- `gdkwayland-sys@0.18.2` — https://github.com/gtk-rs/gtk3-rs
- `gdkx11-sys@0.18.2` — https://github.com/gtk-rs/gtk3-rs
- `gdkx11@0.18.2` — https://github.com/gtk-rs/gtk3-rs
- `generic-array@0.14.7` — https://github.com/fizyk20/generic-array.git
- `gio-sys@0.18.1` — https://github.com/gtk-rs/gtk-rs-core
- `gio@0.18.4` — https://github.com/gtk-rs/gtk-rs-core
- `glib-macros@0.18.5` — https://github.com/gtk-rs/gtk-rs-core
- `glib-sys@0.18.1` — https://github.com/gtk-rs/gtk-rs-core
- `glib@0.18.5` — https://github.com/gtk-rs/gtk-rs-core
- `gobject-sys@0.18.0` — https://github.com/gtk-rs/gtk-rs-core
- `gtk-sys@0.18.2` — https://github.com/gtk-rs/gtk3-rs
- `gtk3-macros@0.18.2` — https://github.com/gtk-rs/gtk3-rs
- `gtk@0.18.2` — https://github.com/gtk-rs/gtk3-rs
- `h2@0.4.15` — https://github.com/hyperium/h2
- `http-body-util@0.1.3` — https://github.com/hyperium/http-body
- `http-body@1.0.1` — https://github.com/hyperium/http-body
- `hyper-util@0.1.20` — https://github.com/hyperium/hyper-util
- `hyper@1.10.1` — https://github.com/hyperium/hyper
- `ico@0.5.0` — https://github.com/mdsteele/rust-ico
- `infer@0.19.0` — https://github.com/bojand/infer
- `is-docker@0.2.0` — https://github.com/TheLarkInn/is-docker
- `is-wsl@0.4.0` — https://github.com/TheLarkInn/is-wsl
- `javascriptcore-rs-sys@1.1.1` — https://github.com/tauri-apps/javascriptcore-rs
- `javascriptcore-rs@1.1.2` — https://github.com/tauri-apps/javascriptcore-rs
- `libredox@0.1.17` — https://gitlab.redox-os.org/redox-os/libredox.git
- `malloc_buf@0.0.6` — https://github.com/SSheldon/malloc_buf
- `memoffset@0.9.1` — https://github.com/Gilnaa/memoffset
- `minisign-verify@0.2.5` — https://github.com/jedisct1/rust-minisign-verify
- `mio@1.2.1` — https://github.com/tokio-rs/mio
- `new_debug_unreachable@1.0.6` — https://github.com/mbrubeck/rust-debug-unreachable
- `objc2-encode@4.1.0` — https://github.com/madsmtm/objc2
- `objc2-foundation@0.3.2` — https://github.com/madsmtm/objc2
- `objc2@0.6.4` — https://github.com/madsmtm/objc2
- `objc@0.2.7` — http://github.com/SSheldon/rust-objc
- `open@5.3.6` — https://github.com/Byron/open-rs
- `os_pipe@1.2.3` — https://github.com/oconnor663/os_pipe.rs
- `pango-sys@0.18.0` — https://github.com/gtk-rs/gtk-rs-core
- `pango@0.18.3` — https://github.com/gtk-rs/gtk-rs-core
- `phf@0.13.1` — https://github.com/rust-phf/rust-phf
- `phf_codegen@0.13.1` — https://github.com/rust-phf/rust-phf
- `phf_generator@0.13.1` — https://github.com/rust-phf/rust-phf
- `phf_macros@0.13.1` — https://github.com/rust-phf/rust-phf
- `phf_shared@0.13.1` — https://github.com/rust-phf/rust-phf
- `plist@1.9.0` — https://github.com/ebarnard/rust-plist/
- `precomputed-hash@0.1.1` — https://github.com/emilio/precomputed-hash
- `quick-xml@0.39.4` — https://github.com/tafia/quick-xml
- `redox_syscall@0.5.18` — https://gitlab.redox-os.org/redox-os/syscall
- `redox_users@0.5.2` — https://gitlab.redox-os.org/redox-os/users
- `rfd@0.16.0` — https://github.com/PolyMeilex/rfd
- `schannel@0.1.29` — https://github.com/steffengy/schannel-rs
- `schemars@0.8.22` — https://github.com/GREsau/schemars
- `schemars@0.9.0` — https://github.com/GREsau/schemars
- `schemars@1.2.1` — https://github.com/GREsau/schemars
- `schemars_derive@0.8.22` — https://github.com/GREsau/schemars
- `shared_child@1.1.1` — https://github.com/oconnor663/shared_child.rs
- `sigchld@0.2.4` — https://github.com/oconnor663/sigchld.rs
- `simd-adler32@0.3.9` — https://github.com/mcountryman/simd-adler32
- `slab@0.4.12` — https://github.com/tokio-rs/slab
- `soup3-sys@0.5.0` — https://gitlab.gnome.org/World/Rust/soup3-rs
- `soup3@0.5.0` — https://gitlab.gnome.org/World/Rust/soup3-rs
- `strsim@0.11.1` — https://github.com/rapidfuzz/strsim-rs
- `synstructure@0.13.2` — https://github.com/mystor/synstructure
- `tauri-winres@0.3.6` — https://github.com/tauri-apps/winres
- `tokio-macros@2.7.0` — https://github.com/tokio-rs/tokio
- `tokio-util@0.7.18` — https://github.com/tokio-rs/tokio
- `tokio@1.52.3` — https://github.com/tokio-rs/tokio
- `tower-http@0.6.11` — https://github.com/tower-rs/tower-http
- `tower-layer@0.3.3` — https://github.com/tower-rs/tower
- `tower-service@0.3.3` — https://github.com/tower-rs/tower
- `tower@0.5.3` — https://github.com/tower-rs/tower
- `tracing-core@0.1.36` — https://github.com/tokio-rs/tracing
- `tracing@0.1.44` — https://github.com/tokio-rs/tracing
- `try-lock@0.2.5` — https://github.com/seanmonstar/try-lock
- `urlpattern@0.3.0` — https://github.com/denoland/rust-urlpattern
- `version-compare@0.2.1` — https://gitlab.com/timvisee/version-compare
- `vswhom-sys@0.1.3` — https://github.com/nabijaczleweli/vswhom-sys.rs
- `vswhom@0.1.0` — https://github.com/nabijaczleweli/vswhom.rs
- `want@0.3.1` — https://github.com/seanmonstar/want
- `webkit2gtk-sys@2.0.2` — https://github.com/tauri-apps/webkit2gtk-rs
- `webkit2gtk@2.0.2` — https://github.com/tauri-apps/webkit2gtk-rs
- `webview2-com-macros@0.8.1` — https://github.com/wravery/webview2-rs
- `webview2-com-sys@0.38.2` — https://github.com/wravery/webview2-rs
- `webview2-com@0.38.2` — https://github.com/wravery/webview2-rs
- `winnow@0.5.40` — https://github.com/winnow-rs/winnow
- `winnow@0.7.15` — https://github.com/winnow-rs/winnow
- `winnow@1.0.3` — https://github.com/winnow-rs/winnow
- `winreg@0.55.0` — https://github.com/gentoo90/winreg-rs
- `x11-dl@2.21.0` — https://github.com/AltF02/x11-rs.git
- `x11@2.21.0` — https://github.com/AltF02/x11-rs.git
- `zip@4.6.1` — https://github.com/zip-rs/zip2.git
- `zmij@1.0.21` — https://github.com/dtolnay/zmij

</details>

<details><summary><strong>MIT OR Unlicense</strong> (6)</summary>

- `aho-corasick@1.1.4` — https://github.com/BurntSushi/aho-corasick
- `byteorder@1.5.0` — https://github.com/BurntSushi/byteorder
- `memchr@2.8.2` — https://github.com/BurntSushi/memchr
- `same-file@1.0.6` — https://github.com/BurntSushi/same-file
- `walkdir@2.5.0` — https://github.com/BurntSushi/walkdir
- `winapi-util@0.1.11` — https://github.com/BurntSushi/winapi-util

</details>

<details><summary><strong>MPL-2.0</strong> (5)</summary>

- `cssparser-macros@0.6.1` — https://github.com/servo/rust-cssparser
- `cssparser@0.36.0` — https://github.com/servo/rust-cssparser
- `dtoa-short@0.3.5` — https://github.com/upsuper/dtoa-short
- `option-ext@0.2.0` — https://github.com/soc/option-ext.git
- `selectors@0.36.1` — https://github.com/servo/stylo

</details>

<details><summary><strong>Unicode-3.0</strong> (18)</summary>

- `icu_collections@2.2.0` — https://github.com/unicode-org/icu4x
- `icu_locale_core@2.2.0` — https://github.com/unicode-org/icu4x
- `icu_normalizer@2.2.0` — https://github.com/unicode-org/icu4x
- `icu_normalizer_data@2.2.0` — https://github.com/unicode-org/icu4x
- `icu_properties@2.2.0` — https://github.com/unicode-org/icu4x
- `icu_properties_data@2.2.0` — https://github.com/unicode-org/icu4x
- `icu_provider@2.2.0` — https://github.com/unicode-org/icu4x
- `litemap@0.8.2` — https://github.com/unicode-org/icu4x
- `potential_utf@0.1.5` — https://github.com/unicode-org/icu4x
- `tinystr@0.8.3` — https://github.com/unicode-org/icu4x
- `writeable@0.6.3` — https://github.com/unicode-org/icu4x
- `yoke-derive@0.8.2` — https://github.com/unicode-org/icu4x
- `yoke@0.8.3` — https://github.com/unicode-org/icu4x
- `zerofrom-derive@0.1.7` — https://github.com/unicode-org/icu4x
- `zerofrom@0.1.8` — https://github.com/unicode-org/icu4x
- `zerotrie@0.2.4` — https://github.com/unicode-org/icu4x
- `zerovec-derive@0.11.3` — https://github.com/unicode-org/icu4x
- `zerovec@0.11.6` — https://github.com/unicode-org/icu4x

</details>

<details><summary><strong>UNKNOWN</strong> (1)</summary>

- `nemo@0.5.0`

</details>

<details><summary><strong>Zlib</strong> (1)</summary>

- `foldhash@0.2.0` — https://github.com/orlp/foldhash

</details>


## Rust crates — geometry-wasm (157)

### By license

<details><summary><strong>(Apache-2.0 OR MIT) AND Unicode-3.0</strong> (1)</summary>

- `unicode-ident@1.0.24` — https://github.com/dtolnay/unicode-ident

</details>

<details><summary><strong>0BSD OR Apache-2.0 OR MIT</strong> (1)</summary>

- `adler2@2.0.1` — https://github.com/oyvindln/adler2

</details>

<details><summary><strong>Apache-2.0</strong> (6)</summary>

- `approx@0.3.2` — https://github.com/brendanzab/approx
- `codespan-reporting@0.13.1` — https://github.com/brendanzab/codespan
- `gl_generator@0.14.0` — https://github.com/brendanzab/gl-rs/
- `glutin_wgl_sys@0.6.1` — https://github.com/rust-windowing/glutin
- `khronos_api@3.1.0` — https://github.com/brendanzab/gl-rs/
- `spirv@0.4.0+sdk-1.4.341.0` — https://github.com/gfx-rs/rspirv

</details>

<details><summary><strong>Apache-2.0 OR BSD-2-Clause OR MIT</strong> (2)</summary>

- `zerocopy-derive@0.8.52` — https://github.com/google/zerocopy
- `zerocopy@0.8.52` — https://github.com/google/zerocopy

</details>

<details><summary><strong>Apache-2.0 OR MIT</strong> (113)</summary>

- `allocator-api2@0.2.21` — https://github.com/zakarumych/allocator-api2
- `android_system_properties@0.1.5` — https://github.com/nical/android_system_properties
- `arrayvec@0.7.8` — https://github.com/bluss/arrayvec
- `ash@0.38.0+1.3.281` — https://github.com/ash-rs/ash
- `autocfg@1.5.1` — https://github.com/cuviper/autocfg
- `bit-set@0.9.1` — https://github.com/contain-rs/bit-set
- `bit-vec@0.9.1` — https://github.com/contain-rs/bit-vec
- `bitflags@2.13.0` — https://github.com/bitflags/bitflags
- `bumpalo@3.20.3` — https://github.com/fitzgen/bumpalo
- `cfg-if@1.0.4` — https://github.com/rust-lang/cfg-if
- `color@0.3.3` — https://github.com/linebender/color
- `console_log@1.1.0` — https://github.com/iamcodemaker/console_log
- `crc32fast@1.5.0` — https://github.com/srijs/rust-crc32fast
- `document-features@0.2.12` — https://github.com/slint-ui/document-features
- `equivalent@1.0.2` — https://github.com/indexmap-rs/equivalent
- `euclid@0.22.14` — https://github.com/servo/euclid
- `fdeflate@0.3.7` — https://github.com/image-rs/fdeflate
- `flate2@1.1.9` — https://github.com/rust-lang/flate2-rs
- `font-types@0.11.3` — https://github.com/googlefonts/fontations
- `futures-channel@0.3.32` — https://github.com/rust-lang/futures-rs
- `futures-core@0.3.32` — https://github.com/rust-lang/futures-rs
- `futures-intrusive@0.5.0` — https://github.com/Matthias247/futures-intrusive
- `futures-task@0.3.32` — https://github.com/rust-lang/futures-rs
- `futures-util@0.3.32` — https://github.com/rust-lang/futures-rs
- `geo-types@0.6.2` — https://github.com/georust/geo
- `gpu-allocator@0.28.0` — https://github.com/Traverse-Research/gpu-allocator
- `gpu-descriptor-types@0.2.0` — https://github.com/zakarumych/gpu-descriptor
- `gpu-descriptor@0.3.2` — https://github.com/zakarumych/gpu-descriptor
- `guillotiere@0.7.0` — https://github.com/nical/guillotiere
- `half@2.7.1` — https://github.com/VoidStarKat/half-rs
- `hashbrown@0.15.5` — https://github.com/rust-lang/hashbrown
- `hashbrown@0.16.1` — https://github.com/rust-lang/hashbrown
- `hashbrown@0.17.1` — https://github.com/rust-lang/hashbrown
- `indexmap@2.14.0` — https://github.com/indexmap-rs/indexmap
- `itoa@1.0.18` — https://github.com/dtolnay/itoa
- `jni-sys-macros@0.4.1` — https://github.com/jni-rs/jni-sys
- `jni-sys@0.3.1` — https://github.com/jni-rs/jni-sys
- `jni-sys@0.4.1` — https://github.com/jni-rs/jni-sys
- `js-sys@0.3.103` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys
- `khronos-egl@6.0.0` — https://github.com/timothee-haudebourg/khronos-egl
- `kurbo@0.13.1` — https://github.com/linebender/kurbo
- `libc@0.2.186` — https://github.com/rust-lang/libc
- `linebender_resource_handle@0.1.1` — https://github.com/linebender/raw_resource_handle
- `litrs@1.0.0` — https://github.com/LukasKalbertodt/litrs
- `lock_api@0.4.14` — https://github.com/Amanieu/parking_lot
- `log@0.4.33` — https://github.com/rust-lang/log
- `naga@29.0.4` — https://github.com/gfx-rs/wgpu
- `ndk-sys@0.6.0+11769913` — https://github.com/rust-mobile/ndk
- `num-traits@0.2.19` — https://github.com/rust-num/num-traits
- `once_cell@1.21.4` — https://github.com/matklad/once_cell
- `parking_lot@0.12.5` — https://github.com/Amanieu/parking_lot
- `parking_lot_core@0.9.12` — https://github.com/Amanieu/parking_lot
- `peniko@0.6.1` — https://github.com/linebender/peniko
- `pin-project-lite@0.2.17` — https://github.com/taiki-e/pin-project-lite
- `pkg-config@0.3.33` — https://github.com/rust-lang/pkg-config-rs
- `png@0.18.1` — https://github.com/image-rs/image-png
- `polycool@0.4.0` — https://github.com/linebender/kurbo
- `portable-atomic-util@0.2.7` — https://github.com/taiki-e/portable-atomic-util
- `portable-atomic@1.13.1` — https://github.com/taiki-e/portable-atomic
- `presser@0.3.1` — https://github.com/EmbarkStudios/presser
- `proc-macro2@1.0.106` — https://github.com/dtolnay/proc-macro2
- `profiling@1.0.18` — https://github.com/aclysma/profiling
- `quote@1.0.46` — https://github.com/dtolnay/quote
- `range-alloc@0.1.5` — https://github.com/gfx-rs/range-alloc
- `raw-window-metal@1.1.0` — https://github.com/rust-windowing/raw-window-metal
- `read-fonts@0.39.2` — https://github.com/googlefonts/fontations
- `renderdoc-sys@1.1.0` — https://github.com/ebkalderon/renderdoc-rs
- `robust@0.1.2`
- `rustc-hash@1.1.0` — https://github.com/rust-lang-nursery/rustc-hash
- `rustversion@1.0.22` — https://github.com/dtolnay/rustversion
- `scopeguard@1.2.0` — https://github.com/bluss/scopeguard
- `serde@1.0.228` — https://github.com/serde-rs/serde
- `serde_core@1.0.228` — https://github.com/serde-rs/serde
- `serde_derive@1.0.228` — https://github.com/serde-rs/serde
- `serde_json@1.0.150` — https://github.com/serde-rs/json
- `skrifa@0.42.1` — https://github.com/googlefonts/fontations
- `smallvec@1.15.2` — https://github.com/servo/rust-smallvec
- `static_assertions@1.1.0` — https://github.com/nvzqz/static-assertions-rs
- `svg_fmt@0.4.5` — https://github.com/nical/rust_debug
- `syn@2.0.118` — https://github.com/dtolnay/syn
- `thiserror-impl@2.0.18` — https://github.com/dtolnay/thiserror
- `thiserror@2.0.18` — https://github.com/dtolnay/thiserror
- `unicode-width@0.1.14` — https://github.com/unicode-rs/unicode-width
- `vello@0.9.0` — https://github.com/linebender/vello
- `vello_encoding@0.9.0` — https://github.com/linebender/vello
- `vello_shaders@0.9.0` — https://github.com/linebender/vello
- `version_check@0.9.5` — https://github.com/SergioBenitez/version_check
- `wasm-bindgen-futures@0.4.76` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures
- `wasm-bindgen-macro-support@0.2.126` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro-support
- `wasm-bindgen-macro@0.2.126` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro
- `wasm-bindgen-shared@0.2.126` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared
- `wasm-bindgen@0.2.126` — https://github.com/wasm-bindgen/wasm-bindgen
- `web-sys@0.3.103` — https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys
- `wgpu-core-deps-apple@29.0.4` — https://github.com/gfx-rs/wgpu
- `wgpu-core-deps-emscripten@29.0.4` — https://github.com/gfx-rs/wgpu
- `wgpu-core-deps-windows-linux-android@29.0.4` — https://github.com/gfx-rs/wgpu
- `wgpu-core@29.0.4` — https://github.com/gfx-rs/wgpu
- `wgpu-hal@29.0.4` — https://github.com/gfx-rs/wgpu
- `wgpu-naga-bridge@29.0.4` — https://github.com/gfx-rs/wgpu
- `wgpu-types@29.0.4` — https://github.com/gfx-rs/wgpu
- `wgpu@29.0.4` — https://github.com/gfx-rs/wgpu
- `windows-collections@0.3.2` — https://github.com/microsoft/windows-rs
- `windows-core@0.62.2` — https://github.com/microsoft/windows-rs
- `windows-future@0.3.2` — https://github.com/microsoft/windows-rs
- `windows-implement@0.60.2` — https://github.com/microsoft/windows-rs
- `windows-interface@0.59.3` — https://github.com/microsoft/windows-rs
- `windows-link@0.2.1` — https://github.com/microsoft/windows-rs
- `windows-numerics@0.3.1` — https://github.com/microsoft/windows-rs
- `windows-result@0.4.1` — https://github.com/microsoft/windows-rs
- `windows-strings@0.5.1` — https://github.com/microsoft/windows-rs
- `windows-sys@0.61.2` — https://github.com/microsoft/windows-rs
- `windows-threading@0.2.1` — https://github.com/microsoft/windows-rs
- `windows@0.62.2` — https://github.com/microsoft/windows-rs

</details>

<details><summary><strong>Apache-2.0 OR MIT OR Zlib</strong> (9)</summary>

- `bytemuck@1.25.0` — https://github.com/Lokathor/bytemuck
- `bytemuck_derive@1.10.2` — https://github.com/Lokathor/bytemuck
- `dispatch2@0.3.1` — https://github.com/madsmtm/objc2
- `glow@0.17.0` — https://github.com/grovesNL/glow
- `miniz_oxide@0.8.9` — https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide
- `objc2-core-foundation@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-metal@0.3.2` — https://github.com/madsmtm/objc2
- `objc2-quartz-core@0.3.2` — https://github.com/madsmtm/objc2
- `raw-window-handle@0.6.2` — https://github.com/rust-windowing/raw-window-handle

</details>

<details><summary><strong>CC0-1.0</strong> (1)</summary>

- `hexf-parse@0.2.1` — https://github.com/lifthrasiir/hexf

</details>

<details><summary><strong>ISC</strong> (1)</summary>

- `libloading@0.8.9` — https://github.com/nagisa/rust_libloading/

</details>

<details><summary><strong>MIT</strong> (17)</summary>

- `block2@0.6.2` — https://github.com/madsmtm/objc2
- `cfg_aliases@0.2.1` — https://github.com/katharostech/cfg_aliases
- `crunchy@0.2.4` — https://github.com/eira-fransham/crunchy
- `dlib@0.5.3` — https://github.com/elinorbgr/dlib
- `float_next_after@0.1.5` — https://gitlab.com/bronsonbdevost/next_afterf
- `geo-booleanop@0.3.2` — https://github.com/21re/rust-geo-booleanop
- `libm@0.2.16` — https://github.com/rust-lang/compiler-builtins
- `objc2-encode@4.1.0` — https://github.com/madsmtm/objc2
- `objc2-foundation@0.3.2` — https://github.com/madsmtm/objc2
- `objc2@0.6.4` — https://github.com/madsmtm/objc2
- `ordered-float@5.3.0` — https://github.com/reem/rust-ordered-float
- `redox_syscall@0.5.18` — https://gitlab.redox-os.org/redox-os/syscall
- `simd-adler32@0.3.9` — https://github.com/mcountryman/simd-adler32
- `slab@0.4.12` — https://github.com/tokio-rs/slab
- `wayland-sys@0.31.11` — https://github.com/smithay/wayland-rs
- `xml-rs@0.8.28` — https://github.com/kornelski/xml-rs
- `zmij@1.0.21` — https://github.com/dtolnay/zmij

</details>

<details><summary><strong>MIT OR Unlicense</strong> (3)</summary>

- `memchr@2.8.2` — https://github.com/BurntSushi/memchr
- `termcolor@1.4.1` — https://github.com/BurntSushi/termcolor
- `winapi-util@0.1.11` — https://github.com/BurntSushi/winapi-util

</details>

<details><summary><strong>Zlib</strong> (3)</summary>

- `foldhash@0.1.5` — https://github.com/orlp/foldhash
- `foldhash@0.2.0` — https://github.com/orlp/foldhash
- `slotmap@1.1.1` — https://github.com/orlp/slotmap

</details>

