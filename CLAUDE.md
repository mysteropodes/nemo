# StrokeMotion — guidelines pour éviter les bugs déjà rencontrés

Tauri v2, hybride Paper.js (modèle de document, source de vérité) + Rust/vello WebGPU
(`geometry-wasm/`, moteur de rendu **sans état** JSON→JSON, bridgé via `src/js/engine-bridge.js`).

Ces guidelines viennent d'un audit complet (bugs, perf, cohérence Rust/JS) après plusieurs
tours de régressions sur l'éraseur/les booléennes/le brush preset — toutes causées par la
**même famille de bug**. Les lire avant de toucher au pipeline de scène/sauvegarde.

## 1. La famille de bug n°1 : un nouvel item/tag géré dans UN lecteur mais pas dans les autres

Chaque calque Paper.js (`userLayers[i].children`) est lu par PLUSIEURS boucles indépendantes,
historiquement toutes écrites avec l'hypothèse "chaque enfant est un `Path` simple". Dès qu'un
nouveau type d'item (`CompoundPath`) ou un nouveau tag `data.*` (`isVectorBrush`,
`isLinkedFillCompanion`, `isBrushTextureCopy`, `brushGroupId`, `fillSeed`/`fillWalls`,
`strokeId`, `preTextureOpacity`/`preTextureStroke`…) apparaît, **il faut vérifier qu'il est
géré ou exclu partout**, pas juste là où on vient de l'introduire.

**Ancres de brush-texture, deux modes de camouflage** (applyBrushTexture, tools.js) : une ancre
SANS fill est cachée par `opacity=0` (+ `preTextureOpacity` pour restaurer) ; une ancre AVEC
fill garde son opacité et se fait nuller SEULEMENT `strokeColor` (+ `preTextureStroke`) pour
que le fill reste visible pendant que les dabs remplacent le trait. Attention : `serP`/`desP`
ont un fallback historique `'#ffffff'`/`'#fff'` quand `strokeColor` est null — les ancres
texturées en sont explicitement exemptées (sinon un contour blanc ressuscite au save/reload).

**Les dabs ne participent JAMAIS au matching de tween** : `splitTweenables()` (tweens.js) les
filtre avant `autoMatch` (generateTweens ET renderArcs) — les laisser entrer faisait exploser
le Hongrois en O(n³) sur des centaines d'entrées et produisait des appariements
bouche↔nez. Sur les frames générées, les dabs sont RE-TAMPONNÉS le long de l'ancre interpolée
(`dabRecordsForTween`) avec un RNG SEEDÉ par paire (`seededRng`, tools.js) — un
`Math.random` frais par frame ferait bouillir la texture.

**Liste des consommateurs connus de `layer.children` à vérifier à chaque nouveau tag/type :**
- `buildSceneJson()` / `onionLayerItems()` / `buildGhostAllItems()` — engine-bridge.js (rendu)
- `saveActiveLayerFrame()` / `saveAllLayerFrames()` — app.js (**le plus critique** : un item
  exclu ici disparaît des DONNÉES persistées, pas juste de l'écran)
- Construction de `selectedPaths` — select-bridge.js (marquee, click), tools.js (clic sur
  composant), timeline.js (Ghost-All)
- `fillRegenerateLinked()` — tools.js
- `serP()` / `desP()` — app.js (sérialisation d'un item individuel)
- Matching de tween — tweens.js (`buildTP`, `strokeFeat`, `autoMatch`)
- `export.js` (rendu de frame pour export PNG/vidéo)

**CompoundPath en particulier** : tout résultat d'une opération booléenne Paper.js
(`.subtract()`, `.unite()`, `.intersect()`, `.exclude()`, ou le retour WASM de
`erase_at_point`/`booleanOp`) PEUT être une `CompoundPath` dès qu'il a plus d'un contour ou un
trou. `CompoundPath` n'est **pas** un `instanceof Path` (classes sœurs, pas parent/enfant).
Ne JAMAIS insérer un résultat booléen brut dans un calque — toujours passer par
`insertBooleanResult(layer, insertAt, result, fillColor, opacity)` (tools.js), qui éclate
automatiquement les îles en `Path` indépendants. Si une future fonction produit un résultat
booléen, utiliser ce helper, pas `layer.insertChild`/`addChild` direct.

**Une donnée "live" (référence d'objet, pas juste une valeur) ne survit jamais à un
save/reload.** `data.brushCompanions` (tableau de références `Path`), `data.linkedFill`
(référence `Path`) — ce genre de champ doit avoir un ID stable persistable en parallèle
(`data.brushGroupId`, pattern identique à `data.strokeId`), + une fonction de "relink" appelée
après toute reconstruction du calque (`relinkBrushCompanions()` dans `loadFrame()`, app.js).
Sans ça, le lien fonctionne en session mais se rompt silencieusement au prochain
save/undo/navigation de frame — bug très facile à rater car il "marche" au moment du test.

## 2. Couleurs avec alpha : `.dataset.hex8`, pas `.value`

Un `<input type="color">` natif tronque silencieusement tout hex 8 chiffres (`#rrggbbaa`) à 6
chiffres au moment où on lit/écrit `.value` — confirmé par test direct. Toute couleur avec
alpha doit transiter par `el.dataset.hex8` en plus de `.value` :
- Lecture : toujours `el.dataset.hex8 || el.value`, jamais `el.value` seul.
- Écriture : toujours les deux : `el.value = hex; el.dataset.hex8 = hex;`.
- Pour lire une couleur Paper.js (`path.fillColor`/`strokeColor`) vers du texte : utiliser
  `colorHex8(color)` (app.js), **jamais** `color.toCSS(true)` — ce dernier force TOUJOURS
  alpha=1 en interne dans Paper.js (bug/particularité de la lib, pas une erreur locale).

## 3. Rust : deux fonctions dupliquées doivent rester identiques

`render()` et `render_to_pixels()` (engine.rs) sont des copies quasi-identiques (l'une pour
l'écran, l'autre pour l'export) qui DOIVENT rester en phase — toute modif de l'une (couleur,
blend mode, dash, etc.) doit être répercutée dans l'autre immédiatement, pas "plus tard".
Même principe pour les paires JS/Rust dupliquées (résample, tween matching, erase) : la
version WASM est essayée en premier avec repli JS silencieux en cas d'erreur — un bug
seulement côté Rust peut donc ne JAMAIS se voir en test si le fallback JS masque la différence.

`Viewport::transform()` a UNE seule définition (`self.viewport.transform()`), utilisée par
`render()`, `render_to_pixels()`, `screen_to_world()`. Ne pas dupliquer cette formule ailleurs.

## 4. Méthodologie de test qui a fait perdre du temps cette session

- **Toujours vérifier qu'un objet de test est réellement inséré dans le calque**
  (`layer.children.indexOf(x)>=0` ou `x.parent===layer`) avant de faire confiance à un
  résultat de test. Plusieurs fonctions `buildXxxPath()` de ce codebase utilisent
  `{insert:false}` par convention — un objet jamais inséré donne des résultats de test
  trompeurs (faux positifs ET faux négatifs).
- **Un simple clic/nibble de bord ne reproduit pas les bugs liés aux `CompoundPath`** — il
  faut un geste qui perce vraiment le MILIEU d'une forme (trou) ou la sépare en plusieurs
  morceaux. Un test qui ne touche que le contour restera toujours un `Path` simple.
- **Préférer les vrais événements DOM déclenchés sur les vrais éléments UI** (`preview_click`
  sur le bouton réel, checkbox cochée par un vrai clic) plutôt que d'appeler l'état interne
  directement (`state.xxx = true` sans passer par le listener) — un raccourci JS peut sauter
  une logique associée au vrai listener et donner un faux résultat de test.
- **Cache navigateur** : `python3 -m http.server` ne force aucune invalidation de cache — un
  onglet ouvert depuis le début de session peut servir du JS obsolète même après un F5 normal.
  En cas de "ça ne marche toujours pas" qui contredit un test qui vient de passer : redémarrer
  le serveur preview sur un port jamais utilisé dans la session, PUIS seulement chercher un
  vrai bug de fond.

## 5. Performance du pipeline de rendu (résolu 2026-07, à préserver)

Quatre optimisations mesurées sur une scène de ~2600 items (15 strokes texturés × ~170 dabs) —
dessin au stylet 240Hz : 19fps → 60fps ; pan : 5fps → 60fps. **Ne pas les défaire :**

1. **`view.autoUpdate = false` quand le moteur Rust est actif** (engine-bridge `setEnabled`) —
   LE facteur dominant : Paper.js re-rasterisait son canvas INVISIBLE (caché sous le canvas
   Rust opaque) à chaque invalidation de vue (~200ms à 2600 items, à chaque move de pan).
   Le scene graph Paper reste vivant (hit-testing, bounds, exportSVG n'ont jamais eu besoin
   du raster). Restauré (`autoUpdate=true` + `view.update()`) si le moteur est désactivé.
2. **Coalescence rAF** de `renderWithOverlayItem` et `renderNow(true)` : un stylet tire
   120-240 events/s, l'écran n'affiche que 60 — seul le DERNIER état par frame est rendu.
   `resume()` annule tout rendu en attente (un overlay périmé ne doit pas repeindre par-dessus
   le trait fraîchement commité).
3. **Cache du socle de scène pendant un drag** (`overlayBasePrefix`, string pré-sérialisée) :
   pendant un drag intercepté le document est intouché par design, inutile de re-sérialiser
   2600 items par move. Clé (`_sceneVersion`, viewportKey) ; invalidé par `renderNow()` complet
   (la gomme mutte SANS bump de version !), `resume()`, resize. Les curseurs volatils
   (pression/gomme/pen) sont EXCLUS du cache (`buildSceneJson(skipVolatile)`) et ré-ajoutés
   frais à chaque move, sinon ils gèlent à leur position de début de drag.
4. **JSON de scène allégé** : coordonnées arrondies à 2 décimales (rendu seulement — la
   persistance via serP garde la précision pleine), champs de style par défaut omis (tous les
   champs `ItemIn` sont `#[serde(default)]` côté Rust, vérifié). 1215→901 Ko à 2600 items.

**`renderNow(true)` (viewportOnly) est un contrat d'appelant** : seul le viewport a changé
depuis le dernier rendu. Vrai pour pan/rotate (aucun item de scène ne dépend de center ou de
la rotation ; les poignées en `1/view.zoom` dépendent du ZOOM seul). JAMAIS l'inférer
automatiquement : select-bridge/eraser-bridge muttent la géométrie sans bump de version.

## 6. Feedback debug (`feedback-bridge.js`, 2026-07)

Système de commentaires "hors projet" pour le debug : outil Comment → bouton "Enregistrer
comme feedback" (au lieu de "Enregistrer" qui reste le commentaire d'équipe classique, IN-
projet). Jamais dans `exportJSON()` — ni `state.actionLog` (trail d'actions, ring buffer
alimenté par `pushUndoLayers()`) ni les entrées feedback ne transitent par le fichier projet.

**Où lire les entrées** : dossier `<appDataDir>/feedback/<projectKey>/*.json` (un fichier par
entrée, Tauri fs — `<projectKey>` = `SMProject.getProjectKey()`, même slug que le dossier
d'historique). En preview navigateur (pas de Tauri) : `localStorage['sm-feedback-<projectKey>']`.
Champ `status` : `'approved'` (à traiter) / `'pending'` (attend l'approbation de l'utilisateur
dans Réglages → Feedback avant de compter) / `'resolved'` (déjà traité — champ `resolution`
si renseigné). Une fois un point corrigé, écrire `status:'resolved'` + `resolution` dans le
fichier (via `SMFeedback.resolveFeedback(id, texte)`) pour fermer la boucle.

Transport multi-poste : réutilise le dossier de Sync équipe déjà existant
(`SMProject.getSyncFolder()`) — `submitFeedback()` publie AUSSI dans
`root/<profileId>/feedback/<id>.json` ; `SMFeedback.pullAllIncoming()` scanne les autres
profils et importe leurs entrées en LOCAL avec `status:'pending'` (jamais approuvées
automatiquement, même si le remote dit `'approved'` — seule l'approbation locale compte).
