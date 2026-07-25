#!/usr/bin/env bash
# Retire le pont AE de src/ — à passer avant toute build.
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="$HERE/../../src"

rm -f "$SRC/js/aescript-ui.js" "$SRC/js/aescript-host.js" "$SRC/js/aeext-host.js" "$SRC/css/ae-bridge.css"
python3 "$HERE/strip-index.py" "$SRC/index.html"

echo "Pont AE retiré de src/. La build peut repartir."
