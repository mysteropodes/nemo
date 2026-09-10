#!/bin/sh
# R14 Lane H — clean-room MCP attach against an INSTALLED Nemo package.
#
# Purpose: show that the nemo-mcp executable bundled inside an installed Nemo
# desktop package resolves and runs with nothing else on its resolution path —
# no source checkout, no Cargo, no Node, no Python, no separately downloaded
# MCP binary.
#
# Method: the script re-executes itself under `env -i` with an EMPTY PATH
# directory, so no toolchain is resolvable by name, then drives the bundled
# executable over stdio with newline-delimited JSON-RPC. Only the installed
# app bundle and base macOS (/bin/sh, /bin/cat, /bin/sleep, /bin/mkdir,
# /usr/bin/env) are used.
#
# HOME is deliberately preserved: nemo-mcp/src/registry.rs resolves the
# per-user discovery registry from data_local_dir(), i.e. under $HOME, when
# neither NEMO_MCP_REGISTRY nor NEMO_TAURI_DATA_DIR is set. Both overrides are
# dropped by `env -i`, which is the honest configuration — a normally
# installed client must look where a normally installed app registers.
#
# Usage:
#   clean-attach.sh <app-bundle> <out-dir> <requests-file> [hold-seconds]
#
# <requests-file> is one JSON-RPC message per line. The pipeline holds the
# server's stdin open for [hold-seconds] after the last request so replies can
# arrive, then closes it, which is how an MCP stdio session ends.

set -eu

if [ "${NEMO_R14_CLEANROOM:-}" != "1" ]; then
    EMPTY_BIN="/tmp/nemo-r14-emptybin"
    /bin/rm -rf "$EMPTY_BIN"
    /bin/mkdir -p "$EMPTY_BIN"
    exec /usr/bin/env -i \
        NEMO_R14_CLEANROOM=1 \
        HOME="$HOME" \
        PATH="$EMPTY_BIN" \
        /bin/sh "$0" "$@"
fi

# Absolutise every path up front: the MCP session below runs from / so that
# the working directory is not the source checkout, which would leave a
# relative path resolving somewhere unintended.
abspath() {
    case "$1" in
        /*) echo "$1" ;;
        *)  echo "$(pwd)/$1" ;;
    esac
}

APP=$(abspath "$1")
OUT=$(abspath "$2")
REQ=$(abspath "$3")
HOLD="${4:-6}"
BIN="$APP/Contents/MacOS/nemo-mcp"

/bin/mkdir -p "$OUT"

# Leave the source checkout BEFORE anything is recorded. Every path above is
# already absolute, so nothing below depends on the old working directory.
# This has to happen before environment.txt is written, not just before the
# MCP session: otherwise the receipt reports the checkout as its cwd and reads
# as though the session ran from inside the source tree, which is the very
# thing this transcript exists to disprove.
cd /

# ---- record the resolution path we are actually running under --------------
{
    echo "# clean-room environment (captured from inside the scrubbed shell)"
    echo "cwd=$(pwd)"
    echo "server=$BIN"
    echo
    echo "## full environment after env -i"
    /usr/bin/env
    echo
    echo "## PATH contents (must be empty)"
    echo "PATH=$PATH"
    /bin/ls -A "$PATH" 2>&1 || true
    echo "(no output above means the single PATH entry is an empty directory)"
    echo
    echo "## toolchain resolvability by name"
    for tool in node npm npx cargo rustc python3 python git cc clang make; do
        if resolved=$(command -v "$tool" 2>/dev/null); then
            echo "$tool -> $resolved"
        else
            echo "$tool -> <<NOT RESOLVABLE>>"
        fi
    done
    echo
    echo "## registry overrides (both must be unset)"
    echo "NEMO_MCP_REGISTRY=${NEMO_MCP_REGISTRY:-<<unset>>}"
    echo "NEMO_TAURI_DATA_DIR=${NEMO_TAURI_DATA_DIR:-<<unset>>}"
} > "$OUT/environment.txt" 2>&1

# ---- record what the server binary itself depends on -----------------------
{
    echo "# installed server binary"
    /bin/ls -l "$BIN"
    echo
    echo "## shasum -a 256"
    /usr/bin/shasum -a 256 "$BIN"
    echo
    echo "## otool -L (dynamic dependencies)"
    /usr/bin/otool -L "$BIN"
} > "$OUT/binary.txt" 2>&1

# ---- drive the MCP session -------------------------------------------------
set +e
{ /bin/cat "$REQ"; /bin/sleep "$HOLD"; } | "$BIN" \
    > "$OUT/stdout.jsonl" 2> "$OUT/stderr.log"
STATUS=$?
set -e
echo "$STATUS" > "$OUT/exit-status.txt"

/bin/cp "$REQ" "$OUT/requests.jsonl"
exit 0
