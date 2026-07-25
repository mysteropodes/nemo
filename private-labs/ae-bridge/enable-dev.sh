#!/usr/bin/env bash
# Copie le pont AE dans src/ pour une session de dev locale UNIQUEMENT.
#
# src/ est ce que Tauri empaquette (frontendDist: ../src) — donc tant que ces
# fichiers y sont, une build les embarquerait. C'est exactement ce que
# scripts/assert-no-private-labs.py empêche : la build s'arrêtera tant que
# disable-dev.sh n'aura pas été passé.
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="$HERE/../../src"

cp "$HERE"/aescript-ui.js "$HERE"/aescript-host.js "$HERE"/aeext-host.js "$SRC/js/"
cp "$HERE"/ae-bridge.css "$SRC/css/ae-bridge.css"

# Injecté juste avant </body> pour ne pas dépendre d'un ancrage qui bouge.
python3 - "$SRC/index.html" <<'PY'
import sys
p=sys.argv[1]; s=open(p,encoding='utf-8').read()
if 'ae-bridge-dev' in s:
    print('  (déjà activé)'); raise SystemExit
tag = ('<!-- ae-bridge-dev : DEV LOCAL UNIQUEMENT, retiré par disable-dev.sh -->\n'
       '<link rel="stylesheet" href="css/ae-bridge.css">\n'
       '<script src="js/aescript-ui.js"></script>\n'
       '<script src="js/aescript-host.js"></script>\n'
       '<script src="js/aeext-host.js"></script>\n')
s=s.replace('</body>', tag+'</body>',1)
open(p,'w',encoding='utf-8').write(s)
print('  index.html : scripts injectés')
PY

echo ""
echo "Pont AE actif en dev."
echo "  Console :  SMAEScript.openFile()   ouvrir un .jsx"
echo "             SMAEExt.openFile()      ouvrir un .zxp"
echo "             SMAEScript.supported()  surface couverte"
echo ""
echo "IMPORTANT : lance ./disable-dev.sh avant toute build."
