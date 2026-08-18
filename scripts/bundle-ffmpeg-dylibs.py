#!/usr/bin/env python3
"""Make a built Nemo.app self-contained w.r.t. the ffmpeg shared libraries.

WHY THIS EXISTS AGAIN (2026-08-18): after the GPL/patent audit
(THIRD_PARTY_NOTICES.md), the bundled ffmpeg sidecar was rebuilt from source
WITHOUT --enable-gpl and without libx264/libx265/libvvenc/libkvazaar/
libvidstab (GPL and/or H.264/H.265/H.266 patent exposure) — see
scripts/rebuild-ffmpeg-lgpl.sh for the exact build. The previous binary was
statically linked (zero dylib deps, this script was a no-op — see CLAUDE.md
§7's note from 2026-07). The new one links dynamically against Homebrew's
libvpx/libaom/libsvtav1/libopus/etc. (all permissive/royalty-free, but real
shared libraries this time), so this script is needed again to make a built
.app run on a machine without Homebrew installed:

  1. BFS-collect every /opt/homebrew dylib EVERY executable in Contents/MacOS
     (the app's own binary AND the ffmpeg sidecar — a build that only fixed
     the main binary would ship an app that launches fine but crashes the
     instant Export is used, since ffmpeg is a separate process, not a
     linked library of the main binary) transitively needs.
  2. Copy them into Contents/Frameworks/.
  3. install_name_tool: point every reference at @rpath/<name>, give each
     copied dylib a matching @rpath id, and add an rpath entry on EACH
     executable (@executable_path/../Frameworks — correct for both, since
     Tauri places the sidecar in Contents/MacOS/ alongside the main binary,
     so @executable_path resolves the same way from either).
  4. Ad-hoc re-sign everything deepest-first (install_name_tool invalidates
     code signatures; unsigned dylibs won't load on arm64 macOS at all).

USAGE (after `npm run build`):
  python3 scripts/bundle-ffmpeg-dylibs.py \
      "src-tauri/target/release/bundle/macos/Nemo.app"

Then package/publish that .app as usual (publish-update.sh). Idempotent —
running it twice is safe (already-@rpath references are left alone).
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
                if not f.startswith(".") and os.access(os.path.join(macos_dir, f), os.X_OK)]
    # Every executable in Contents/MacOS gets scanned — the ffmpeg sidecar
    # is its OWN process (spawned, not linked), so its Homebrew dependencies
    # are entirely invisible to a scan that only follows the main binary's
    # load commands. Missing this was the bug that made an earlier version
    # of this script report "nothing to do" against the new dynamically-
    # linked ffmpeg build (the main Nemo binary itself is still static,
    # zero Homebrew deps — only the ffmpeg sidecar needs this).
    exes = binaries

    # 1. BFS over the Homebrew dependency graph. Two edge kinds:
    #    - absolute /opt/homebrew/... references (the common case), and
    #    - @rpath/@loader_path SIBLING references between Homebrew dylibs
    #      (libwebp → @rpath/libsharpyuv...), resolved against the
    #      referencing lib's own real directory + every dir seen so far.
    needed = {}     # reference as written -> basename
    real_of = {}    # basename -> real file path to copy
    search_dirs = set()
    to_visit = []
    for exe in exes:
        to_visit += [(d, os.path.dirname(os.path.realpath(exe))) for d in deps_of(exe)]
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

    # 3. Rewrite references in every executable AND in every copied dylib —
    # every reference whose basename we bundled becomes @rpath/<basename>
    # (flat Frameworks layout), whatever form it was written in.
    targets = list(exes) + list(copied.values())
    for t in targets:
        for dep in raw_deps(t):
            base = os.path.basename(dep)
            if base in copied and dep != f"@rpath/{base}":
                subprocess.check_call(
                    ["install_name_tool", "-change", dep, f"@rpath/{base}", t],
                    stderr=subprocess.DEVNULL)
    # rpath on EVERY executable (ignore 'duplicate' failure on re-runs) —
    # not just main_bin: the ffmpeg sidecar runs as its own process and
    # resolves its own @executable_path independently, from Contents/MacOS/
    # same as the main binary, so it needs the same rpath entry.
    for exe in exes:
        subprocess.call(
            ["install_name_tool", "-add_rpath", "@executable_path/../Frameworks", exe],
            stderr=subprocess.DEVNULL)

    # 4. Ad-hoc re-sign, innermost first: each dylib, then every executable
    # (install_name_tool invalidated their signatures too), then the bundle
    # itself as best-effort (fails harmlessly on incomplete test shells;
    # on a real .app it refreshes the seal).
    for t in copied.values():
        subprocess.check_call(["codesign", "-f", "-s", "-", t],
                              stderr=subprocess.DEVNULL)
    for exe in exes:
        subprocess.check_call(["codesign", "-f", "-s", "-", exe],
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
                if os.path.basename(d) in copied or t in exes:
                    problems.append(f"{os.path.basename(t)} references unshipped {d}")
    if problems:
        sys.exit("ERROR:\n  " + "\n  ".join(problems))
    print(f"OK — {len(copied)} dylibs bundled, all references rewritten to @rpath, re-signed.")


if __name__ == "__main__":
    main()
