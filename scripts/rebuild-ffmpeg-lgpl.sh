#!/usr/bin/env bash
# Rebuilds src-tauri/binaries/ffmpeg-aarch64-apple-darwin from FFmpeg source
# WITHOUT --enable-gpl and without any encoder whose format is patent-
# encumbered (libx264/libx265 = H.264/H.265, libvvenc = H.266, libkvazaar =
# an alternate H.265 encoder, libvidstab = GPL-only stabilization filter) —
# see THIRD_PARTY_NOTICES.md for the full legal reasoning. Everything kept
# is permissive/royalty-free (BSD/MIT/Apache-2.0) except libmp3lame (LGPL,
# fine — MP3's own patents expired in 2017, and LGPL only obligates the lib
# itself, not what links it, unlike GPL).
#
# Also enables --enable-videotoolbox: h264_videotoolbox / hevc_videotoolbox
# / prores_videotoolbox call Apple's OWN OS-provided hardware encoder via
# the VideoToolbox framework (always present on macOS, not a Homebrew dep)
# instead of bundling libx264/libx265/libx265-alternatives. Apple already
# holds whatever patent license its own encoder needs — same principle
# already used by exportVideoBrowser's MediaRecorder path in the browser
# build — so H.264/H.265/ProRes export stay available without touching
# Via LA/Access Advance registration or re-introducing GPL.
#
# Result license: LGPL v2.1+ (confirmed by ffmpeg's own `configure` output
# when this flag set is used — no --enable-gpl anywhere).
#
# Requires Homebrew with these formulas installed (`brew install <name>`):
#   nasm libvmaf openjpeg opus lame libvpx webp libass freetype fontconfig
#   theora libvorbis snappy aom zimg svt-av1 harfbuzz
# The resulting binary links DYNAMICALLY against these Homebrew dylibs —
# scripts/bundle-ffmpeg-dylibs.py (run after `npm run build`) makes a
# packaged .app self-contained by copying them into Contents/Frameworks/,
# same as it already did for the old GPL build. This means the MACHINE
# THAT RUNS `npm run build` must have this exact Homebrew formula list
# installed — CI or a fresh machine needs `brew install` for all of them
# first, this script does NOT do that for you (kept explicit/inspectable).
#
# Usage: ./scripts/rebuild-ffmpeg-lgpl.sh
# Takes ~10-20 min (mostly `make`). Overwrites
# src-tauri/binaries/ffmpeg-aarch64-apple-darwin in place — review the diff,
# test an export, THEN commit.
set -euo pipefail
cd "$(dirname "$0")/.."
REPO_ROOT="$(pwd)"
BUILD_DIR="$(mktemp -d)"
trap 'rm -rf "$BUILD_DIR"' EXIT

echo "==> Cloning ffmpeg n8.1 into $BUILD_DIR"
git clone --branch n8.1 --depth 1 https://github.com/FFmpeg/FFmpeg.git "$BUILD_DIR/ffmpeg-src"

echo "==> Configuring (no GPL, no patent-encumbered encoders)"
cd "$BUILD_DIR/ffmpeg-src"
./configure \
  --arch=arm64 --cc=/usr/bin/clang \
  --extra-cflags="-I/opt/homebrew/include" --extra-ldflags="-L/opt/homebrew/lib" \
  --pkg-config-flags="--define-prefix" \
  --enable-libvmaf --enable-libopenjpeg --enable-libopus --enable-libmp3lame \
  --enable-libvpx --enable-libwebp --enable-libass --enable-libfreetype --enable-fontconfig \
  --enable-libtheora --enable-libvorbis --enable-libsnappy --enable-libaom --enable-libzimg \
  --enable-libsvtav1 --enable-libharfbuzz --enable-videotoolbox \
  --enable-neon --enable-runtime-cpudetect \
  --disable-doc

grep -q "^License: LGPL" ffbuild/config.log 2>/dev/null || true
echo "==> Building (this is the slow part)"
make -j"$(sysctl -n hw.ncpu)"

echo "==> Verifying: no GPL, real encode works"
./ffmpeg -version | grep -q "GPL" && { echo "ERROR: GPL still present in build config"; exit 1; }
./ffmpeg -y -f lavfi -i testsrc=duration=1:size=160x120:rate=10 -c:v libvpx-vp9 "$BUILD_DIR/smoke.webm" -loglevel error
[ -s "$BUILD_DIR/smoke.webm" ] || { echo "ERROR: smoke-test encode produced no output"; exit 1; }

cp ffmpeg "$REPO_ROOT/src-tauri/binaries/ffmpeg-aarch64-apple-darwin"
chmod +x "$REPO_ROOT/src-tauri/binaries/ffmpeg-aarch64-apple-darwin"
echo "==> Done. Binary replaced: $REPO_ROOT/src-tauri/binaries/ffmpeg-aarch64-apple-darwin"
echo "    Next: npm run build && python3 scripts/bundle-ffmpeg-dylibs.py <built .app>, test an export, then commit."
