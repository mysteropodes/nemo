#!/usr/bin/env python3
"""Abort the build if anything from private-labs/ has leaked into src/.

tauri.conf.json sets frontendDist to ../src, so EVERY file under src/ ends up
inside the shipped bundle whether or not index.html references it. The AE
bridge (private-labs/ae-bridge/) is deliberately kept out of the distributed
product for legal reasons — see that directory's README — and enable-dev.sh
copies it into src/ for local use, which is exactly the state that must never
reach a build.

This runs as tauri's beforeBuildCommand so the guard is structural rather than
a habit: forgetting to run disable-dev.sh fails the build instead of silently
shipping the thing we removed on purpose.
"""
import sys, pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / 'src'

# Filenames that belong to private-labs and must never exist under src/.
BANNED_FILES = ['aescript-host.js', 'aescript-ui.js', 'aeext-host.js']
# Symbols that would betray a copy hiding under a different filename.
# Les noms de fichiers comptent aussi : un <script src> orphelin laissé dans
# index.html ne casse pas la build mais prouve que le nettoyage est incomplet.
BANNED_MARKERS = ['__adobe_cep__', 'SMAEScript', 'SMAEExt', 'SMScriptUI'] + BANNED_FILES

found = []

for name in BANNED_FILES:
    for hit in SRC.rglob(name):
        found.append('fichier interdit : %s' % hit.relative_to(ROOT))

for path in SRC.rglob('*'):
    if not path.is_file() or path.suffix.lower() not in ('.js', '.html', '.css'):
        continue
    try:
        text = path.read_text(encoding='utf-8', errors='ignore')
    except OSError:
        continue
    for marker in BANNED_MARKERS:
        if marker in text:
            found.append('référence « %s » dans %s' % (marker, path.relative_to(ROOT)))

if found:
    print('\n[BUILD ARRÊTÉE] Du code private-labs se trouve dans src/ et partirait dans la build :\n', file=sys.stderr)
    for f in sorted(set(found)):
        print('  - %s' % f, file=sys.stderr)
    print('\nLance  ./private-labs/ae-bridge/disable-dev.sh  puis relance la build.\n', file=sys.stderr)
    sys.exit(1)

print('[ok] private-labs absent de src/ — la build peut continuer')
