# Pont After Effects — hors build, usage local

Exécute des scripts `.jsx` et des extensions CEP `.zxp` d'After Effects contre
le document Nemo. **Volontairement exclu du produit distribué** (décision
2026-07-25) : c'était le point le plus exposé juridiquement de l'app, et le
risque vient de la **distribution**, pas de l'usage personnel.

## Utiliser

```bash
./private-labs/ae-bridge/enable-dev.sh     # copie dans src/ + injecte les <script>
# ... session de dev ...
./private-labs/ae-bridge/disable-dev.sh    # retire tout de src/
```

Puis dans la console : `SMAEScript.openFile()`, `SMAEExt.openFile()`,
`SMAEScript.supported()`.

## Le garde-fou

`src/` **est** ce que Tauri empaquette (`frontendDist: ../src`) : un fichier
oublié là partirait dans la build même sans être référencé.

`scripts/assert-no-private-labs.py` tourne en `beforeBuildCommand` et **arrête
la build** si un de ces fichiers — ou un simple `SMAEScript` / `__adobe_cep__`
dans n'importe quel fichier de `src/` — est encore présent. Oublier
`disable-dev.sh` casse la build au lieu de livrer en silence.

## Contenu

| fichier | rôle |
|---|---|
| `aescript-host.js` | DOM AE : `app.project`, `CompItem`, `Layer`, `Property`, easing |
| `aescript-ui.js` | ScriptUI → DOM réel (palettes) |
| `aeext-host.js` | Extensions CEP `.zxp` : lecteur zip, manifeste, `CSInterface` |
| `ae-bridge.css` | styles des palettes et panneaux |

## Points juridiques ouverts — à traiter AVANT toute réintégration

1. **Constantes numériques du SDK non vérifiées.** `KeyframeInterpolationType`,
   `PropertyValueType`, `BlendingMode` ont été écrites de mémoire par un
   modèle : ni exactitude ni provenance garanties. À vérifier contre le
   *scripting guide public* d'Adobe et à sourcer en commentaire.
2. **`appName: "AEFT"`** (`aeext-host.js`) déclare à du code tiers « je suis
   After Effects ». Nécessaire à l'interop, mais c'est de l'endossement
   d'identité produit. Idem `matchName = 'ADBE ' + …`.
3. **Signature `.zxp` non vérifiée** — le zip est lu, la signature ignorée.
4. **Aucune traçabilité de dérivation** : il n'existe pas de trace montrant que
   cette API vient de spécifications publiées. C'est la défense standard en
   cas de litige, et elle manque.
5. **Ne jamais accepter la licence du SDK After Effects.** Ce qui échappe au
   droit d'auteur peut être capturé par un contrat qu'on a signé.
6. **Ne jamais embarquer de script ou d'extension tiers**, et n'en faire aucun
   argument commercial : le risque le plus probable n'est pas Adobe mais un
   éditeur d'outil dont la licence dit « pour usage avec After Effects ».
