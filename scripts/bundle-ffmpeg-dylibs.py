#!/usr/bin/env python3
"""Make a built Nemo.app self-contained w.r.t. the ffmpeg shared libraries.

WHY THIS EXISTS (2026-07, native video engine graduation): the Rust binary
links the ffmpeg C libraries (libavcodec & co) at ABSOLUTE Homebrew paths
(/opt/homebrew/opt/ffmpeg/lib/...) — pkg-config's doing at compile time.
A build shipped as-is CRASHES AT LAUNCH on any machine without Homebrew
ffmpeg (dyld: Library not loaded). This script rewrites the bundle to be
self-contained, the standard macOS way:

  1. BFS-collect every /opt/homebrew dylib the binary (transitively) needs.
  2. Copy them into Contents/Frameworks/.
  3. install_name_tool: point every reference at @rpath/<name>, give each
     copied dylib a matching @rpath id, and add an rpath entry on the main
     binary (@executable_path/../Frameworks).
  4. Ad-hoc re-sign everything deepest-first (install_name_tool invalidates
     code signatures; unsigned dylibs won't load on arm64 macOS at all).

USAGE (after `npm run build`):
  python3 scripts/bundle-ffmpeg-dylibs.py \
      "src-tauri/target/release/bundle/macos/Nemo.app"

Then package/publish that .app as usual (publish-update.sh). Idempotent —
running it twice is safe (already-@rpath references are left alone).

LICENSING NOTE (matters for the commercial build, decided 2026-07 to ship
this way for the BETA only): Homebrew's ffmpeg is built WITH GPL components
(x264/x265 encoders). Bundling those dylibs makes the distributed bundle
GPL-encumbered even though Nemo only DECODES (decoders are LGPL). Before
selling: build a custom LGPL-only ffmpeg (decode-only, no x264/x265) and
bundle that instead — smaller too (~15MB vs ~150MB+).
"""
import os
import shutil
import subprocess
import sys

HOMEBREW_PREFIXES = ("/opt/homebrew/", "/usr/local/")


def raw_deps(binary):
    """All dylib load paths of `binary`, as written in its load commands."""
    out = subprocess.check_output(["otool", "-L", binary], text=True)
    return [line.strip().split(" (")[0] for line in out.splitlines()[1:] if line.strip()]


def deps_of(binary):
    """Direct dylib load paths of `binary` that live under Homebrew."""
    return [p for p in raw_deps(binary) if p.startswith(HOMEBREW_PREFIXES)]


def resolve_relative(ref, loader_real_dir, search_dirs):
    """Resolve an @rpath/@loader_path reference to a real file, if we can.
    Homebrew dylibs sometimes reference SIBLINGS this way (libwebp →
    @rpath/libsharpyuv.0.dylib) — missed by an absolute-prefix-only scan,
    which is exactly how the first version shipped a bundle that died at
    launch on libsharpyuv (caught by this script's own dyld launch test)."""
    name = ref.split("/", 1)[1] if "/" in ref else ref
    candidates = [os.path.join(loader_real_dir, name)]
    candidates += [os.path.join(d, name) for d in search_dirs]
    for c in candidates:
        if os.path.exists(c):
            return os.path.realpath(c)
    return None


def own_id(dylib):
    out = subprocess.check_output(["otool", "-D", dylib], text=True).splitlines()
    return out[1].strip() if len(out) > 1 else None


def main():
    if len(sys.argv) != 2 or not sys.argv[1].endswith(".app"):
        sys.exit(__doc__)
    app = os.path.abspath(sys.argv[1])
    macos_dir = os.path.join(app, "Contents", "MacOS")
    fw_dir = os.path.join(app, "Contents", "Frameworks")
    os.makedirs(fw_dir, exist_ok=True)
    binaries = [os.path.join(macos_dir, f) for f in os.listdir(macos_dir)
                if not f.startswith(".")]
    main_bin = next((b for b in binaries if os.access(b, os.X_OK) and not b.endswith("ffmpeg")), binaries[0])

    # 1. BFS over the Homebrew dependency graph. Two edge kinds:
    #    - absolute /opt/homebrew/... references (the common case), and
    #    - @rpath/@loader_path SIBLING references between Homebrew dylibs
    #      (libwebp → @rpath/libsharpyuv...), resolved against the
    #      referencing lib's own real directory + every dir seen so far.
    needed = {}     # reference as written -> basename
    real_of = {}    # basename -> real file path to copy
    search_dirs = set()
    to_visit = [(d, os.path.dirname(os.path.realpath(main_bin))) for d in deps_of(main_bin)]
    while to_visit:
        ref, loader_dir = to_visit.pop()
        base = os.path.basename(ref)
        if base in real_of:
            needed.setdefault(ref, base)
            continue
        if ref.startswith(HOMEBREW_PREFIXES):
            real = os.path.realpath(ref)
        else:
            real = resolve_relative(ref, loader_dir, search_dirs)
        if not real or not os.path.exists(real):
            sys.exit(f"ERROR: dependency not resolvable on this machine: {ref} (from {loader_dir})")
        needed[ref] = base
        real_of[base] = real
        search_dirs.add(os.path.dirname(real))
        for sub in raw_deps(real):
            if sub.startswith(HOMEBREW_PREFIXES) or sub.startswith(("@rpath/", "@loader_path/")):
                if os.path.basename(sub) not in real_of:
                    to_visit.append((sub, os.path.dirname(real)))

    if not real_of:
        print("No Homebrew-linked dylibs found — nothing to do (already bundled?).")
        return

    print(f"Bundling {len(real_of)} dylibs into Contents/Frameworks/")

    # 2. Copy (dereferencing symlinks) + set each copy's own id.
    copied = {}  # basename -> bundled absolute path
    for base, real in real_of.items():
        dst = os.path.join(fw_dir, base)
        if not os.path.exists(dst):
            shutil.copy2(real, dst)
            os.chmod(dst, 0o755)
        copied[base] = dst
        subprocess.check_call(["install_name_tool", "-id", f"@rpath/{base}", dst],
                              stderr=subprocess.DEVNULL)

    # 3. Rewrite references in the main binary AND in every copied dylib —
    # every reference whose basename we bundled becomes @rpath/<basename>
    # (flat Frameworks layout), whatever form it was written in.
    targets = [main_bin] + list(copied.values())
    for t in targets:
        for dep in raw_deps(t):
            base = os.path.basename(dep)
            if base in copied and dep != f"@rpath/{base}":
                subprocess.check_call(
                    ["install_name_tool", "-change", dep, f"@rpath/{base}", t],
                    stderr=subprocess.DEVNULL)
    # rpath on the main binary (ignore 'duplicate' failure on re-runs)
    subprocess.call(
        ["install_name_tool", "-add_rpath", "@executable_path/../Frameworks", main_bin],
        stderr=subprocess.DEVNULL)

    # 4. Ad-hoc re-sign, innermost first: each dylib, then the main binary
    # (install_name_tool invalidated its signature too), then the bundle
    # itself as best-effort (fails harmlessly on incomplete test shells;
    # on a real .app it refreshes the seal).
    for t in copied.values():
        subprocess.check_call(["codesign", "-f", "-s", "-", t],
                              stderr=subprocess.DEVNULL)
    subprocess.check_call(["codesign", "-f", "-s", "-", main_bin],
                          stderr=subprocess.DEVNULL)
    subprocess.call(["codesign", "-f", "-s", "-", app], stderr=subprocess.DEVNULL)

    # Verify: no Homebrew references left anywhere, and every @rpath
    # reference resolves to a file we actually shipped in Frameworks.
    problems = []
    for t in targets:
        for d in raw_deps(t):
            if d.startswith(HOMEBREW_PREFIXES):
                problems.append(f"{os.path.basename(t)} still references {d}")
            elif d.startswith("@rpath/") and not os.path.exists(os.path.join(fw_dir, d.split("/", 1)[1])):
                # @rpath refs from SYSTEM frameworks aren't ours; only flag
                # basenames we were supposed to bundle.
                if os.path.basename(d) in copied or t != main_bin:
                    problems.append(f"{os.path.basename(t)} references unshipped {d}")
    if problems:
        sys.exit("ERROR:\n  " + "\n  ".join(problems))
    print(f"OK — {len(copied)} dylibs bundled, all references rewritten to @rpath, re-signed.")


if __name__ == "__main__":
    main()
