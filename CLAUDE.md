# Nemo — guidelines pour éviter les bugs déjà rencontrés

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
`strokeId`, `preTextureOpacity`/`preTextureStroke`, `isDuplicatorCopy`/`dupIndex`…) apparaît,
**il faut vérifier qu'il est géré ou exclu partout**, pas juste là où on vient de l'introduire.
(`isDuplicatorCopy`/`dupIndex` : copies synthétiques du duplicateur mograph — le calque est
verrouillé de force et exclu de saveActiveLayerFrame/saveAllLayerFrames, voir
`applyLayerDuplicator`/`getEffectiveStrokesRendered` dans app.js.)

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

### 5bis. Deuxième passe perf (2026-07-28) — lecture / scrub / drag

Point de départ : « la lecture dans motion de cet élément est saccadée » sur un projet
réel. Mesuré : 24 fps visés rendus à 20 fps, avec un intervalle rAF de **47,7 ms** (l'écran
repeignait à 21 Hz). Après correction : rAF 16,1 ms, scrub 60 fps. **Deux familles de bug**,
à reconnaître avant d'en réintroduire une :

**(a) Un accès « gratuit » qui ne l'est pas, placé AVANT le garde bon marché.**
`raster.canvas` (Paper.js) n'est pas un accesseur mais un **constructeur** : il alloue un
canvas et y fait un `drawImage` au premier accès, **par objet Raster** — et `loadFrame`
reconstruit tous les Raster à chaque frame. `registerRasterIfNeeded` le lisait avant
`registeredImageIds[id]`, donc N rasters = N allocations de canvas par frame pour des pixels
déjà sur le GPU (2000 items : 24 fps rendus à 4,4 fps). Même forme dans select-bridge
(`computeHandles()` avant le garde `e.altKey`). **Toujours tester le cache/garde le moins
cher en premier**, et se méfier de `.canvas`, `.bounds`, `.rasterize()`, `.getImageData()`.

**(b) Reconstruire tout un DOM/arbre parce que la frame a bougé.**
`renderTimeline()` refait 4800 nœuds (27,7 ms à 40 calques) ; sa seule sortie dépendante de
la frame est la classe `.cur` + la position du playhead, ce que `updatePlayhead()` produit
en 0,1 ms. D'où le **contrat `frameOnly`** : `goToFrame` le passe à `updateUI`, qui appelle
`updatePlayhead()` au lieu de `renderTimeline()`, et le transmet à `renderLayerList` (qui
l'honore en Animation 2D seulement — en Motion les lignes affichent la VALEUR à la tête de
lecture). C'est le découpage que la lecture utilisait déjà (`startPlay` → `updatePlayhead`
seul, `stopPlay` → `updateUI` complet). **Un appelant qui a changé le CONTENU ne doit jamais
passer `frameOnly`.** Vérifié par diff DOM normalisé (ordre des classes trié) : identique à
un rebuild complet sur 28/28 cas.

**Invalider un cache, c'est aussi choisir QUAND.** Le cache de `symbolUnionBounds` a d'abord
été purgé depuis `saveAllLayerFrames` — or `goToFrame` l'appelle à **chaque avance de
frame**, donc la lecture recalculait l'union de 120 frames par tick (75 % du temps mur).
Gardé par `state.activeSymbolId` : le contenu d'un symbole ne change que quand on est
DEDANS. `_sceneVersion` est un mauvais critère ici — select-bridge l'incrémente à chaque
move de drag.

**Ne pas dupliquer le matcher.** `renderArcs` appelait `updateReassignBadge` qui recalculait
`computeArcMatchState()` (hongrois en O(n³)) de son côté : deux fois par tick de scrub
(13,1 ms à 60 traits, moitié pure duplication). L'état se calcule une fois et se passe.

**Les drags bruts doivent avoir un garde ou un verrou rAF** : barre in/out (garde de delta,
100 mousemoves → 6 rebuilds), poignée de zoom timeline (verrou rAF, 40 → 1 + flush au
relâchement).

**Clonage : partager les charges lourdes, mais mesurer.** `src` (base64) et
`bitmapPressureProfile` sont écrits une fois puis seulement lus — `_HEAVY_STROKE_FIELDS`
(app.js, exporté) les liste, `cloneStrokeForTransform` les partage par référence. Contre-
intuitif et vérifié : pour l'undo, un cloneur JS par objet et un couple replacer/reviver
sont **plus lents** qu'un `JSON.parse(JSON.stringify())` complet (14,1 et 26,5 ms contre
16,1) — un seul stringify natif de tout l'arbre bat des milliers de petites copies JS même
en déplaçant plus d'octets. Ce qui gagne : **détacher** les chaînes lourdes, cloner en
natif, rattacher (10,7 ms), avec restauration en `finally`.

**Mesurer : le rAF est gelé quand l'onglet est caché.** Tout chiffre basé sur
`requestAnimationFrame` n'est valable que si `document.visibilityState === 'visible'` —
sinon la sonde ne récolte aucun échantillon et un calcul naïf sort un NaN d'apparence
plausible. Et le scrub mesuré en boucle serrée sous-estime le coût réel : le rendu moteur
est coalescé en rAF (§5.2), donc **c'est l'intervalle rAF en lecture qui est le chiffre
honnête**, pas la durée synchrone de `goToFrame`.

Repères après cette passe (scène synthétique, régime établi, Animation 2D) :
vecteurs 500/frame scrub 60 fps · vecteurs 2000/frame 31 fps · images 1000/frame 60 fps ·
séquence 200/frame 60 fps · bitmap 2000/frame 61 fps ; lecture 23,2-23,4 fps sur 24 partout.
Les vecteurs restent ~2× plus chers que les rasters par item (chaque segment est
re-sérialisé dans le JSON de scène, un raster c'est six nombres et un id).

### 5ter. Géométrie retenue côté moteur (2026-07-28)

Le pattern `register_image`/`imageId` étendu aux paths. Avant : les segments d'un item
étaient re-sérialisés dans le JSON de scène, re-parsés par serde et reconstruits en
`BezPath` à **chaque rendu**, même géométrie inchangée. Après : JS enregistre le path une
fois (`register_path`) et n'envoie plus qu'un `pathRef`.

Mesuré (sérialisation de scène isolée, `SMEngineBridge.timeSceneBuild`) :
2000 vecteurs 5,2 ms/1652 Ko → **1,5 ms/300 Ko** ; 4000 vecteurs 15,0 ms/6156 Ko →
**2,8 ms/606 Ko**. Scrub bout-en-bout (médiane d'intervalle rAF) : 36,2 → 27,7 ms à 2000,
96,3 → 57,7 ms à 4000.

**L'invariant unique dont tout dépend : l'identité d'objet d'un dict de stroke stocké EST
son identité de géométrie.** `desP` tamponne le dict source sur `path.data.__engineSrcDict`
(app.js) ; engine-bridge mappe dict → clé moteur. Trois conséquences à ne jamais oublier :

1. **Seuls les calques dont `getEffectiveStrokes` renvoie le tableau STOCKÉ sont éligibles.**
   `symbolId` (un composant avec caméra/symMatrix passe par `cloneStrokeForTransform`, qui
   fabrique des dicts NEUFS à chaque appel), `montageId` et `lfsGroup` synthétisent — pour
   eux l'identité ne veut rien dire. Mesuré avant le garde : le store grimpait 0 → 25 → 50
   → 75 sur des passes de scrub identiques. Le garde est explicite (`layerRetainable`) et
   `_registerCap` (250k) est un filet dur pour qu'une future branche synthétisante dégrade
   en « pas de gain » plutôt qu'en fuite mémoire.
   ⚠️ Une heuristique « n'enregistrer qu'un dict vu deux fois » a été essayée et NE SUFFIT
   PAS : « vu deux fois » signifie « la scène a été construite deux fois pendant que ce dict
   vivait », ce qu'un second rendu entre deux `loadFrame` satisfait trivialement.

2. **La mutation live doit invalider le stamp.** Hook sur `_changed`, avec le masque de bits
   **découvert par auto-test** et non codé en dur — et deux pièges vérifiés empiriquement :
   `Path` a son PROPRE `_changed` (le système de classes de Paper capture `base` par
   référence directe, donc patcher `Item.prototype` seul n'intercepte rien : zéro callback
   mesuré), et un item **non inséré** ne déclenche aucun `_changed` du tout — c'est ce qui
   avait produit un masque nul silencieux au premier essai. `CompoundPath`/`Raster` n'ont
   pas de `_changed` propre et héritent d'`Item` : les DEUX prototypes doivent être hookés.
   Si l'auto-test échoue, la fonctionnalité reste OFF (le rendu garde son comportement).

3. **Le gain n'existe que si `serP()` est sauté.** Première version : garder serP+roundSegs
   inconditionnels et n'économiser que les octets JSON → **36 → 33 fps, soit pire**, le
   lookup n'étant que du coût ajouté. C'est le parcours du path Paper qui domine, pas la
   taille du JSON.

**Vérification** : `setRetainedPathsEnabled(false)` est un vrai interrupteur (aussi filet en
prod). Rendu prouvé **identique octet pour octet** (PNG via `render_to_pixels`) ON vs OFF sur
7 frames, après gomme, après undo/redo, et avec un CompoundPath en donut réel. Attention en
testant : `renderFrameToPixelsPNG` appelle `loadFrame`, qui reconstruit les items et les
RE-TAMPONNE — un A/B naïf compare donc refs contre refs et passe toujours.

**v2 — Motion inclus (`pathTransform`).** Toute la chaîne élément → calque → parents est une
composition d'affines autour d'un pivot : elle est repliée en UNE matrice 2×3 envoyée à côté
du ref (`affineFromMotion`/`affineMul`, engine-bridge), que le moteur compose avec sa
view transform. Une forme ANIMÉE réutilise donc son path enregistré au lieu de re-sérialiser
chaque coordonnée. Mesuré : 2000 vect + Motion 7,33 ms/1751 Ko → **1,82 ms/573 Ko** (4,0×) ;
4000 vect + Motion 26,68 ms/6349 Ko → **3,71 ms/1149 Ko** (7,2×). Coût réel par tick de scrub
(boucle déterministe, cf. ci-dessous) : 2000 sans Motion 22,8 → 17,1 ms ; 4000 avec Motion
83,2 → 59,0 ms.

⚠️ **`affineFromMotion` DOIT rester le miroir exact de `transformSegments` (motion.js)** —
mise à l'échelle dans le repère local du pivot, rotation autour du pivot, translation en
dernier. Si les deux divergent, l'image ne change que pour les formes animées, donc
silencieusement : c'est un pixel-A/B qui l'attrape, pas une relecture (§3).

**Quatre exclusions, chacune parce qu'elle changerait l'image** : offsets par vertex (seule
pièce non affine), **échelle non uniforme** n'importe où dans la chaîne (le chemin inline
multiplie la largeur de trait par `(|sx|+|sy|)/2` alors que le moteur trace À TRAVERS
l'affine — ça ne coïncide que si `sx == sy`, sinon c'est une plume elliptique), fill en
dégradé (ses ancres sont pré-transformées inline), et l'overlay `currentFrameOutline` (sa
largeur de trait est une constante écran qui ne doit pas suivre l'affine). Avec
`pathTransform`, JS envoie donc la largeur de trait **non multipliée**.

⚠️ **L'enregistrement lit `segsBefore`, la géométrie AVANT transformation** — un path stocké
doit vivre dans l'espace propre de la forme, jamais posé. Le premier jet de la v2
enregistrait la géométrie posée : un calque en permanence animé ne présentant jamais de frame
non transformée, il ne pouvait jamais amorcer son entrée de store.

**Identité de rendu, chiffrée** : v1 (non transformé) reste **identique au bit près**. v2
avec `pathTransform` diffère au pire de **63 px sur 2 073 600 (0,003 %), écart de canal max
1/255** — artefact d'ORDRE D'ARRONDI (inline arrondit après transformation, le retenu arrondit
avant puis applique une affine exacte), pas une erreur de maths : le transport de points est
exact à 1e-15, vérifié point à point contre `transformSegments`. Une forme mal placée
donnerait des milliers de pixels à 255.

**Mesurer la perf ici : les sondes fps basées sur rAF ne sont PAS fiables** — la boucle de
sonde et le tick rAF du moteur s'aliasent, et le même réglage a donné des résultats
contradictoires d'un run à l'autre (37→36 fps puis 28→39 fps). Utiliser
`SMEngineBridge.timeSceneBuild(n)` pour la sérialisation isolée, et une boucle synchrone
`goToFrame + timeSceneBuild(1)` pour le coût par tick. Le reste du coût est `loadFrame`/`desP`
qui reconstruit les objets Paper — non touché par ce chantier, c'est le mur suivant.

### 5quater. `loadFrame` ne reconstruit plus ce qui n'a pas changé (2026-07-28)

Après la géométrie retenue, le mur suivant était `loadFrame` lui-même : **32,6 ms sur 37,0 ms
par frame (88 %) étaient `desP`**, à reconstruire 4000 objets Paper. Les alternatives de
construction plafonnent à 1,4× (mesuré : `add()` par segment 7,33 µs/path, constructeur avec
tableau de Segments 6,33, tableaux bruts 5,33) — pas de quoi changer de catégorie.

Le vrai levier : `getEffectiveStrokes` renvoie **le tableau STOCKÉ** (`f.strokes`, ou celui de
la keyframe héritée). Donc toute frame qui MAINTIENT sur une keyframe rend le **même objet
tableau** — et `loadFrame` reconstruisait des calques dont le contenu était identique.
Mesuré : 83 % des couples calque-frame sur un projet en maintien de 6.

`_canReuseMaterialized(lyr, strokes)` (app.js) saute `removeChildren` + reconstruction quand
**les trois** conditions tiennent :
1. `lyr._matStrokes === strokes` — test d'IDENTITÉ, et c'est le point : tout écrivain
   REMPLACE le tableau (`f.strokes = strokes`), il ne le mute pas en place, donc une frame
   modifiée présente toujours un objet différent. Les branches composant/montage/lfs
   synthétisent un tableau neuf à chaque appel : elles ne matchent jamais et continuent de
   reconstruire, inchangées.
2. `!lyr._smGeomDirty` — drapeau posé par le MÊME hook `_changed` que la géométrie retenue
   (§5ter), sur `this._parent`. Sans lui, un sculpt non sauvegardé sur une frame non-keyframe
   survivrait au rechargement, alors que `loadFrame` doit l'annuler. Vérifié en pilotant :
   mutation → drapeau à true → `loadFrame` reconstruit → mutation annulée.
3. `lyr.children.length === strokes.length` — paranoïa. `desP`/`desR` émettent exactement un
   item par stroke ; toute divergence force une reconstruction plutôt que de faire confiance
   à la seule identité (c'est ce qui rattrape un trait fraîchement dessiné, ajouté au calque
   sans que `_matStrokes` bouge).

Le garde global `window.__smGeomDirtyHookInstalled` : si le hook n'est pas posé (moteur
désactivé, auto-test échoué), **aucune réutilisation** — sans signal de saleté il n'y a aucun
moyen de savoir si les items live ont été édités.

Mesuré : `loadFrame` 35,1 → **3,35 ms** à 4000 traits (10,5×) ; 8,3 → **0,64 ms** à 2000
(12,8×). Coût par tick de scrub bout-en-bout : 42,4 → **13,7 ms** à 4000 (3,1×), 13,8 →
**5,8 ms** à 2000, 16,2 → **6,4 ms** à 2000+Motion.

⚠️ **Pire cas honnête : une keyframe sur CHAQUE frame de chaque calque ne gagne rien**
(8,1 → 8,06 ms, mesuré) — mais ne régresse pas non plus. L'animation traditionnelle est
pleine de maintiens (« on twos »/« on threes »), c'est là que ça paie.

**Vérification** : rendu identique (PNG) entre réutilisation et reconstruction forcée sur
8 frames en Animation 2D, 6 en Motion, 5 sur le projet réel ; trait réellement dessiné au
geste puis aller-retour de frame (stocké et enfants cohérents, visible à l'écran) ; undo/redo ;
masquage/réaffichage de calque ; gomme.

### 5quinquies. Le store d'images du moteur est borné (2026-07-28)

Le premier mur du côté FOOTAGE de l'app (vidéo/images/séquences), et ce n'est pas une
lenteur : c'est une limite dure. `register_image` insérait sans jamais rien retirer — son
propre commentaire l'assumait, « cached for the engine's whole lifetime ». Correct pour un
document de dessin avec quelques rasters importés, intenable pour du métrage.

**Mesuré** : 120 frames distinctes en 640×360 = **105,5 Mo** décodés, soit 5 secondes à
24 fps. À 1920×1080 c'est ~950 Mo pour les mêmes 5 s, ~11 Go la minute. Rien n'en libérait
un octet.

**L'éviction est pilotée par JS, pas par le moteur** — même raison que pour les paths
retenus (§5ter) : ce côté-ci sait ce que la scène en cours de construction référence
réellement, et il peut toujours re-téléverser (les pixels reviennent du canvas du Raster
Paper, ou du push par frame des ponts vidéo/référence). Une LRU côté moteur devrait deviner,
et une mauvaise devinette fait disparaître une image de l'image sans autre signal qu'un
warning.

Politique : LRU par dernière UTILISATION (dernière émission dans une scène), jusqu'à un
budget d'octets (**384 Mo par défaut**, `setImageBudgetBytes`). `_imgUsedThisBuild` est
ouvert au début de `buildSceneJson` et fermé au retour : **rien de ce que la frame courante
dessine n'est jamais candidat**. Côté Rust : `retire_images`, `image_store_bytes`,
`image_store_size`.

⚠️ Deux bugs à moi, tous deux trouvés par la mesure et pas par la relecture :
- `Math.max(1, n | 0)` — **la coercition bitwise déborde à 2³¹**, donc un budget de 4 Go
  atterrissait sur 1 octet et vidait tout le store dès la première frame. `Math.floor`.
- Une image fraîchement téléversée doit compter comme **utilisée**, pas seulement comme
  enregistrée : sans l'ajouter à `_imgUsedThisBuild`, elle devenait candidate à l'éviction
  à l'instant même où elle arrivait (le store finissait à 0 en dessinant quand même, via
  re-upload permanent).

**Vérification** : sous un budget de 8 Mo forçant une éviction continue (223 évictions), les
frames rendues sont **identiques au pixel près** à la référence en budget large, sur 6 frames.
Budget 32 Mo : 36 images retenues, 31,6 Mo, sous budget, ça dessine. Aucune régression côté
dessin — 2000 rasters partageant une source ne tiennent qu'UNE entrée (3,1 ms/frame, zéro
éviction), et le projet réel de l'utilisateur n'évince rien.

**Le budget n'est pas un réglage « perf » mais un réglage MACHINE** — c'est l'équivalent du
cache RAM d'After Effects. Il devra être exposé dans les Réglages quand le côté footage
sortira, avec une valeur par défaut dérivée de la VRAM disponible plutôt que la constante
actuelle.

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

**Transport beta-testeurs (2026-07)** : repo public dédié
[`mysteropodes/strokemotion-feedback`](https://github.com/mysteropodes/strokemotion-feedback)
(aucun code de l'app dedans) — une Issue GitHub par feedback, créée depuis Rust
(`submit_feedback_issue` dans `src-tauri/src/lib.rs`, **jamais** depuis JS) avec un token
compilé à la build via `env!("STROKEMOTION_FEEDBACK_TOKEN")` (même pattern que
`STROKEMOTION_UPDATER_TOKEN`) — fine-grained PAT scopé À CE SEUL REPO, permissions
"Issues: write" + "Contents: write" (élargi en 2026-07 pour les captures d'écran jointes,
voir ci-dessous — décision utilisateur explicite, toujours zéro exposition de code) : un
token extrait du binaire peut au pire spammer des issues ou écrire n'importe quel fichier
dans CE repo précis, jamais toucher au code de l'app. Le label `pending` est posé
automatiquement à la création.

**Capture d'écran jointe (2026-07)** : l'outil Commentaire a une zone de drop
(`#comment-shot-drop`) — glisser-déposer, clic-pour-parcourir, ou Cmd+V. Gardée en data URL
localement (`entry.screenshotDataUrl`) ; à la publication GitHub, `uploadScreenshotIfAny()`
(feedback-bridge.js) l'envoie à `upload_feedback_attachment` (Rust) qui la committe dans
`attachments/<id>.<ext>` via l'API Contents, puis référence l'URL
`raw.githubusercontent.com` résultante en Markdown dans le corps de l'issue — GFM ne rend
PAS les data URIs, le fichier doit réellement exister dans le repo pour s'afficher inline.

Lecture des issues : publique, aucune auth requise (`SMFeedback.fetchGithubIssues()`).
Triage (approuver/résoudre/éditer) : nécessite le token PERSONNEL de Cyril, saisi dans
Réglages → "Feedback beta-testeurs (GitHub)" et gardé en `localStorage` sur sa machine
uniquement — jamais embarqué dans le build distribué. Une copie de l'app livrée à un
beta-testeur a ce même panneau de triage dans le code, mais il est inutilisable sans le
token perso de Cyril.

**Build web (2026-08) : la publication GitHub passe par un Worker, pas par Rust.**
Sur desktop, `submit_feedback_issue`/`upload_feedback_attachment` tournent en Rust — un
navigateur n'a pas ce backend pour cacher le token. `worker-feedback/` (racine du repo) est
un Worker Cloudflare séparé (secret `GITHUB_FEEDBACK_TOKEN` propre, jamais dans le Worker du
site statique `nemo-editor`) qui joue exactement le même rôle de frontière de confiance.
`feedback-bridge.js` branche sur `tauriOk()` : Tauri → `invoke()`, sinon → `fetch()` vers ce
Worker (`FEEDBACK_WORKER_URL`, à mettre à jour après le premier déploiement). **Piège déjà
tombé une fois** : le premier jet du build web gardait le vieux garde-fou `if (tauriOk())`
autour de tout l'appel de publication — le feedback s'enregistrait bien en local
(`localStorage`) et semblait "envoyé", mais ne partait jamais vers GitHub, silencieusement.
Voir `worker-feedback/README.md` pour le setup (secret Worker à poser une fois via
`wrangler secret put`, PAS un secret GitHub Actions).

## 7. Avant chaque build : synchroniser le numéro de version partout

Trois fichiers portent le numéro de version et doivent rester identiques à chaque bump :
`package.json`, `src-tauri/tauri.conf.json`. L'affichage à l'écran (titre de fenêtre, barre
de statut en bas, Réglages → "Mises à jour de l'app") est lu dynamiquement via
`window.__TAURI__.app.getVersion()` (`updater-bridge.js`'s `showVersion()`) — donc PAS besoin
de toucher `index.html` pour ça, c'est déjà la source unique de vérité côté affichage runtime.
Seul le fallback statique (`<title>` et `#status-text` dans `index.html`, visible un instant
avant que `getVersion()` résolve, et seule valeur affichée en preview navigateur sans Tauri)
doit être bumpé à la main en même temps que les deux fichiers de config — sinon le premier
flash à l'ouverture (et tout le preview navigateur) montre encore l'ancien numéro.

Checklist avant `npm run build` :
1. Bump `version` dans `package.json` ET `src-tauri/tauri.conf.json` (même valeur).
2. Bump le fallback statique dans `src/index.html` (`<title>` + `#status-text`).
3. **Étape dylibs REDEVENUE nécessaire (2026-08-18, rebuild LGPL)** :
   `scripts/bundle-ffmpeg-dylibs.py` avait un commentaire "n'a plus rien à faire" écrit quand
   le binaire ffmpeg embarqué était encore l'ancien build GPL, **statiquement lié** (zéro
   dépendance Homebrew, confirmé par `otool -L`). Ce n'est plus vrai : suite à l'audit licence
   du 2026-08-17 ([THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)), le binaire a été
   **recompilé sans `--enable-gpl`** et sans `libx264`/`libx265`/`libvvenc`/`libkvazaar`/
   `libvidstab` (GPL et/ou brevets H.264/H.265/H.266) — reproductible via
   `scripts/rebuild-ffmpeg-lgpl.sh`. `ffmpeg -version` confirme désormais **License: LGPL
   version 2.1 or later**, plus aucune trace de `--enable-gpl`. Contrepartie : ce nouveau
   binaire est **lié dynamiquement** contre les dylibs Homebrew (libvpx/libaom/libsvtav1/
   libopus/libwebp/libass/freetype/fontconfig/libtheora/libvorbis/libmp3lame/libsnappy/libzimg/
   libharfbuzz/libopenjpeg/libvmaf — toutes permissives ou LGPL, voir THIRD_PARTY_NOTICES.md).
   `bundle-ffmpeg-dylibs.py` a donc été corrigé pour scanner TOUS les exécutables de
   `Contents/MacOS/` (le binaire principal ET le sidecar ffmpeg — l'ancienne version ne
   scannait que le binaire principal, ce qui aurait silencieusement laissé le sidecar ffmpeg
   sans ses dylibs, crash à l'export sur une machine sans Homebrew) et à exécuter **après
   chaque `npm run build`**, obligatoire de nouveau, pas optionnel. La machine qui build (pas
   celle qui reçoit l'app) doit avoir les formules Homebrew listées en tête de
   `rebuild-ffmpeg-lgpl.sh` installées.
   ⚠️ **H.264/H.265 restaurés SANS libx264/libx265, via VideoToolbox** (même 2026-08-18) :
   `--enable-videotoolbox` (framework système Apple, pas une dépendance Homebrew, pas GPL)
   expose `h264_videotoolbox`/`hevc_videotoolbox`/`prores_videotoolbox` — l'encodeur matériel
   OS d'Apple, piloté via leur API plutôt qu'embarqué comme lib tierce. Apple a déjà la
   licence brevet nécessaire pour SON encodeur ; on ne redistribue rien de nous-mêmes — même
   principe que `exportVideoBrowser` (MediaRecorder du navigateur) côté web. Aucune inscription
   Via LA (AVC) ni Access Advance (HEVC) nécessaire pour ce chemin. `exportMP4ToPath`
   (`src/js/export.js`) utilise désormais `h264_videotoolbox` (plus `libx264` — qualité via
   `-q:v`, VideoToolbox n'a pas d'équivalent `-crf`). `prores_videotoolbox` existe aussi
   (alternative plus propre à `prores_ks`, pas encore branchée).
   ProRes (`prores_ks`, encodeur natif ffmpeg, pas une lib externe) reste inchangé pour
   l'instant — réimplémentation clean-room de RDD-36 (spec SMPTE publiée), aucun pool de
   brevets public connu contrairement à AVC/HEVC, risque plus faible mais pas fermé ;
   `prores_videotoolbox` (ci-dessus) le fermerait si besoin un jour.
4. Si c'est un vrai changement fonctionnel (pas juste un patch de bug) : lancer
   `./scripts/publish-update.sh "notes"` après la build pour que les installs existantes le
   voient — voir §6 pour le détail des tokens nécessaires.

## 8. Logique globale StoryBoard / Animation 2D / Motion (2026-07, validée avec l'utilisateur)

Les 3 modes ne sont PAS trois éditeurs indépendants — ce sont trois vues d'un même document,
reliées par des règles précises. Toute nouvelle fonctionnalité touchant l'un des trois doit se
demander comment elle traverse les deux autres. Règles confirmées explicitement par
l'utilisateur (à ne pas re-décider différemment sans lui reposer la question) :

**StoryBoard ne manipule QUE des Components.** Dès qu'un calque (un "module") est connecté à un
bloc "Edit module" dans le graphe StoryBoard, il devient automatiquement un Component — même
règle d'auto-conversion que côté Motion (ci-dessous), déclenchée par un point d'entrée différent.
Un calque plat (non-Component) ne peut pas être placé/séquencé directement dans un montage
StoryBoard.

**Animation 2D ↔ Motion, niveau CALQUE : la conversion en Component est MANUELLE (revu 2026-08,
feedback "évite de faire automatiquement des composant dans motion, ça doit être manuelle").**
Keyer une propriété de niveau calque (Position/Anchor/Rotation/Scale/Opacity, stopwatch OU drag
canvas) sur un calque plat ne convertit plus rien tout seul — `toggleAnimated`/`setValue`
(motion.js) ont perdu leur appel à l'ancien `maybeAutoConvertToComponent` (supprimé). Le rendu
d'un calque plat animé reste correct sans conversion (`buildSceneJson` applique déjà `motionMat`
à N'IMPORTE QUEL calque, `symbolId` ou non — la conversion n'a jamais été une exigence de rendu,
seulement un raccourci UX + le prérequis StoryBoard ci-dessus). Conversion toujours possible à la
main (menu calque/contexte → `convertLayerToComponent`/`convertLayersToComponent`, timeline.js) ;
un pivot Rotation/Scale sur un calque à plusieurs formes non converties pivote autour du centre
des bounds du calque entier (peut sembler "faux" si les formes sont dispersées — c'est le prix
du contrôle manuel, pas un bug).

**Motion, niveau SHAPE : double-clic sur un calque Component = entrer dedans comme un precomp
After Effects (construit 2026-07-17).** Décision re-tranchée avec l'utilisateur après un premier
essai imbriqué ("Layer 1 > Éléments > Forme N avec son propre Transform") jugé pas assez proche
du besoin réel — capture à l'appui, ce qui est voulu est le résultat PLAT de "Release to Layers"
(`splitLayerIntoElements`) : un vrai calque séparé par forme, nommé `"<Layer> — Forme N"`, chacun
avec sa vraie barre de présence sur la timeline (icônes eye/lock/solo normales, PAS de sous-lignes
imbriquées). `enterComponentLayer` (motion.js) : appelle `enterSymbol` (vrai onglet + "Scene" pour
revenir, zéro état parallèle), saute à la frame interne résolue (`resolveSymbolFrameIdx`, pour ne
pas atterrir sur une frame 0 vide du symbole), puis auto-éclate SILENCIEUSEMENT
(`splitLayerIntoElementsCore(li,{silent:true})`, app.js) chaque calque du symbole qui contient
encore 2+ formes — idempotent, un calque déjà éclaté n'a plus qu'un seul élément. À partir de là,
le rendu Motion normal (accordéon, barres réelles) affiche directement le résultat voulu, sans
code de montage spécifique. Prérequis découvert en construisant ceci : `elementMotionAt`
(motion.js) forçait `null` pour tout calque Component — levé (getEffectiveStrokes applique
maintenant l'animation par-forme en plus du placement de l'instance), sinon animer une forme
individuellement n'aurait aujourd'hui aucun effet visuel.

**Propriétés étendues par forme (fill/stroke/brush/path) : toujours pas construites.** Une fois
éclatée en calque séparé, chaque "Forme N" n'a que les 5 propriétés de base
(Position/Anchor/Scale/Rotation/Opacité) comme n'importe quel calque normal — pas encore
`path`/fill/stroke/brush en plus. Reste un chantier futur si le besoin se confirme, pas un
sous-produit de ce qui vient d'être construit.

**Propriétés étendues (fill/stroke/brush/path) : opt-in, cachées par défaut, activées dans le
panel de droite — même convention que les propriétés optionnelles de Rive.** Ce ne sont PAS des
propriétés qui apparaissent automatiquement dans la liste Motion ; l'utilisateur les active
explicitement quand il en a besoin, exactement comme Rive expose ses propriétés additionnelles.

**Le path est keyable au niveau du VERTEX, dans un menu déroulant, et ça peut se brancher sur le
moteur de tween existant.** Keyer `path` ne veut pas dire un blob de géométrie opaque : chaque
vertex du shape a ses propres coordonnées keyables, listées dans un sous-menu déroulant sous
l'entrée `path` du shape. Le mécanisme d'interpolation entre deux clés de path peut/doit
réutiliser le moteur de tween d'Animation 2D (`tweens.js`, `interpStroke`/`resamplePairFeatureAware`)
plutôt que d'inventer un système de morph séparé — cohérent avec le principe déjà appliqué au
raccord Motion↔tween (le raster Bitmap Brush d'une frame tween, `recordForTween`, suit déjà
cette logique de réutilisation).

**Non tranché — à reposer explicitement avant de construire** : un calque Animation 2D devrait
pouvoir contenir PLUSIEURS Components à la fois (pas juste 1 layer = 1 component en 1:1 comme
aujourd'hui). Ça demande de faire d'une instance de Component un item DANS `ld.frames[i].strokes`
(un troisième type de stroke aux côtés de `Path`/`isRaster`, avec son propre `symbolId` +
placement), pas un flag `ld.symbolId` au niveau du calque entier — donc ça traverse tous les
consommateurs de item-type déjà listés en §1 (saveActiveLayerFrame, getEffectiveStrokes,
buildSceneJson, select-bridge, Motion element-holders). Chantier à part entière, pas un
sous-produit du reste de cette section.

**Component "lu par les 3 parties" (construit 2026-07)** : `state.symbols[symId]` est LE
Component partagé par StoryBoard et Animation2D/Motion — trois lacunes concrètes comblées :
1. **Fusion multi-calques préserve Motion** : `convertLayersToComponent` (app.js) copie
   désormais `motion`/`motionStatic`/`elementMotion` de chaque calque source dans le
   `symLayer` correspondant (avant : silencieusement perdus, famille de bug n°1 du §1).
   `convertLayerToComponent` (cas 1 calque) n'avait pas ce problème — l'`ld` original reste
   l'instance et garde son `.motion` propre (transform de l'instance, type precomp AE).
2. **Caméra par-Component** : `state.cameraKeys` était 100% global. `enterSymbol`/
   `exitToScene` (app.js) swappent maintenant `state.cameraKeys` vers/depuis
   `sym.cameraKeys` exactement comme ils swappent déjà `state.layers`/`userLayers` — édition
   caméra à l'intérieur d'un Component = timeline caméra propre à ce Component, zéro
   changement dans camera.js (il lit/écrit déjà `state.cameraKeys` en direct). Le rendu
   compose ce Component-camera dans `getEffectiveStrokes` (app.js, branche `ld.symbolId`) via
   `SMCamera.cameraMatrixAtFrame(sym.cameraKeys, ii, canvasW, canvasH)` + `applyMatrixToStrokeData`
   (même helper que `symMatrix`) — donc le mouvement de caméra d'un Component voyage avec
   l'instance partout où elle est placée (Animation2D, export, StoryBoard).
   ⚠️ Piège vérifié empiriquement : `Matrix.rotate/scale/translate` (objet Matrix nu) compose
   en **ajoutant** chaque opération, alors que `Item.rotate/scale/translate` (Layer/Path)
   compose en **préfixant** — mêmes appels dans le même ordre donnent des résultats DIFFÉRENTS
   entre les deux. `cameraMatrixAtFrame` doit appeler translate/scale/rotate dans l'ordre
   INVERSE de la chaîne `applyToExportLayer` pour produire la matrice équivalente — confirmé
   par test direct dans le Browser pane, pas par lecture de la doc Paper.js.
3. **Aperçu StoryBoard live** : `thumbDataUrl` accepte un 3e paramètre `bypassCache` ;
   `storyboard.js` a une boucle rAF (`startLivePreview`/`stopLivePreview`) limitée à LA carte
   survolée (jamais toutes à la fois — coût déjà rejeté par le commentaire historique de
   `thumbDataUrl`), qui revient à la vignette statique au `mouseleave`.

## 9. Collaboration Git (2026-07) — repo `mysteropodes/nemo`

Depuis l'arrivée d'un collaborateur (pencilpark), ce dossier n'est plus le seul endroit où le
code vit — `origin` pointe vers un vrai repo GitHub (public depuis 2026-08-26), et il faut éviter de s'écraser
mutuellement. Règles à suivre **sans qu'on ait besoin de le redemander** :

- **Jamais de commit direct sur `main`.** Toujours une branche dédiée par tâche/session :
  `git checkout -b claude/<sujet-court>` pour le travail fait avec Claude (préfixe qui
  identifie la provenance dans l'historique), branches sans préfixe particulier pour le
  travail humain direct. Une fois la tâche terminée, ouvrir une Pull Request vers `main`
  plutôt que de merger en local en douce.
- **`git pull` avant de commencer à toucher au code**, à chaque nouvelle session — le dossier
  peut avoir bougé depuis la dernière fois (pencilpark, ou une session précédente).
- **Pousser la branche dès qu'un morceau cohérent est fini**, ne pas laisser des commits
  locaux non poussés s'accumuler sur plusieurs sessions — plus l'écart avec `main` grandit,
  plus les conflits de merge sont douloureux à résoudre.
- **Ne jamais toucher à `main` en écriture directe** même pour un "petit" fix — même une
  correction d'une ligne passe par une branche + PR, pour rester cohérent et laisser une
  trace revue.
- Ce dossier reste synchronisé par kDrive/OneDrive en tâche de fond (usage personnel de
  Cyril) — **ce n'est PAS le mécanisme de partage avec pencilpark**, qui clone sa propre
  copie du repo GitHub ailleurs. Ne jamais partager ce dossier OneDrive directement avec un
  collaborateur pour du travail simultané (sync cloud + git en parallèle sur le même dossier
  risque de corrompre l'historique).
- Secrets (`TAURI_SIGNING_PRIVATE_KEY`, `STROKEMOTION_PUBLISH_TOKEN`,
  `STROKEMOTION_UPDATER_TOKEN`, `STROKEMOTION_FEEDBACK_TOKEN`) restent strictement
  personnels à Cyril — jamais committés (déjà couvert par `.gitignore` pour les clés de
  signature), jamais partagés même avec un collaborateur de confiance. Pour du dev normal,
  des valeurs placeholder (`STROKEMOTION_FEEDBACK_TOKEN=dev-placeholder
  STROKEMOTION_UPDATER_TOKEN=dev-placeholder`) suffisent à compiler et lancer `npm run dev`.
- **Avant de partir en investigation sur un bug rapporté (surtout Motion/canvas), vérifier
  les branches sœurs AVANT de diagnostiquer soi-même** — quand plusieurs sessions Claude
  travaillent en parallèle dans des worktrees séparés (ex. `nemo` sur `claude/web-public-beta`
  vs `nemo-motion` sur `claude/trim-and-motion-anchor-fixes`), le SEUL canal entre elles est
  Git, de façon asynchrone : rien n'informe une session que l'autre a déjà committé un fix
  tant qu'un `git fetch` explicite n'est pas fait. Incident du 2026-08-22 : un bug de
  keyframes Position en Motion a été diagnostiqué et corrigé en profondeur (deux causes
  distinctes trouvées par du reverse-engineering en direct) — alors qu'un fix pour une
  troisième cause du MÊME bug (`29f4e7e`, feedback #41) était déjà poussé sur
  `claude/trim-and-motion-anchor-fixes` depuis la VEILLE, avant même le début de
  l'investigation. `git fetch` + `git log <branche-sœur> --oneline` (ou une recherche de
  mots-clés du bug rapporté) est un coût de quelques secondes contre potentiellement une
  heure de diagnostic redondant — à faire SYSTÉMATIQUEMENT en tout début d'investigation
  d'un bug, pas seulement juste avant un déploiement.

## 10. Tout champ numérique doit supporter le "scrub" (glisser pour changer la valeur)

Convention établie : cliquer-glisser horizontalement sur un champ numérique change sa valeur
(vs. clic simple = curseur texte pour taper), avec Shift = pas ×0.1 et Alt = pas ×10. Mécanisme
unique, délégué au niveau `document` (`ui.js`, écoute `pointerdown`/`pointermove`/`pointerup`
globale, pas un binding par élément) — **le seul déclencheur est la classe littérale `scrub`
sur l'`<input>`** (`class="pi scrub"` en général, `data-step="…"` optionnel, retombe sur `1`
sinon). Comme la détection est déléguée, un champ créé dynamiquement plus tard (popover, ligne
de tableau) devient scrubbable dès qu'on lui donne cette classe — aucun ré-enregistrement
nécessaire.

**Tout nouveau champ numérique (`type="number"`) doit avoir la classe `scrub`**, que ce soit
statique dans `index.html` ou créé en JS (`input.className = 'pi scrub'`). Oubli trouvé et
corrigé 2026-07 (feedback : "on peut pas slide en draguant les value... fait ça pour toute
option où l'on a des values à changer") sur `#p-opacity`, `#p-stroke-alpha`, les champs RGBA
du color-picker, l'opacité par swatch du color-manager, le stop de dégradé, `#comp-singleframe`/
`#comp-speed`, `#exp-w`/`#exp-h`, `#text-size`, `#tl-fps`/`#tl-total` — tous avaient un `class="pi"`
(ou rien) sans le token `scrub`, donc silencieusement non-scrubbables malgré un `data-step` par
ailleurs correct. Un `type="range"` natif (slider) n'a pas besoin de cette classe — c'est un
mécanisme d'interaction différent, déjà équivalent en pratique (glisser le curseur).

## 11. Motion — état après la session du 2026-07-25 (AE-lite)

Motion a reçu une grosse passe de features After Effects. À lire avant d'y
retoucher, surtout pour les invariants qui ne se voient pas dans le code.

**L'invariant d'alignement panneau/grille est la contrainte n°1.** `#layer-list`
(gauche) et `#frame-grid` (droite) rendent DEUX listes de lignes qui doivent
rester ligne-à-ligne identiques. Toute propriété ajoutée doit passer par
`propsFor(holder)` (motion.js) — le seul endroit qui décide de la liste — sinon
les deux côtés divergent silencieusement. Même règle pour toute ligne
supplémentaire (éditeur d'expression, etc.) : si un côté l'insère, l'autre doit
réserver la même hauteur.

**Scroll.** `renderTimeline`/`renderLayerList` vident leur conteneur, ce qui
remet le scroll à 0 — ils prennent donc un snapshot avant et le restaurent
après (`_tlScrollSnapshot`/`_tlScrollRestore`). La synchro verticale des deux
panneaux est un miroir **1:1** (layer-scroll-sync.js) : ne jamais y remettre
un décalage « pour compenser le header », c'était le bug. Les plages de scroll
sont égalisées par une cale en fin de contenu. Le `#playhead` est épinglé au
viewport (`syncPlayheadToViewport`) parce qu'il est `position:absolute` dans le
conteneur scrollable.

**Bas de timeline.** `#fg-col` garde libre la bande de 40 px du bas :
`#layer-ctrls` à gauche, `#tlzoom-scrollbar` (timeline-zoom.js) à droite. Ne pas
réintroduire de barre de scroll native sur `#fg-wrap` — timeline-zoom la masque
volontairement, il n'y en a qu'une.

**Un seul geste, un seul rectangle.** Le lasso de motion.js est en phase de
CAPTURE sur `#fg-wrap` : il fait suivre à `SMLayerInOut.marqueeSelect` (barres
in/out) et layer-inout fait suivre à `SMMotion.marqueeSelect` (clés). Les deux
sens sont nécessaires — sans ça, un des deux types de sélection meurt.

**Boîte de sélection de clés** : n'apparaît qu'à partir de 2 pistes de
propriété (demande explicite). Bords haut/bas = skew, bords gauche/droite =
space, remplissage = déplacement, Cmd+glisser = liquify. Un seul moteur
(`startSkewDrag`/`onDragMove`), un facteur par clé selon le mode.

**Déplacement des clés avec les in/out** — ordre de priorité, à ne pas
inverser : sélection explicite > verrou permanent `ld.keyLock` > défaut par
poignée. Alt a UN sens à la fois selon qu'il y a une sélection ou non.

**Ajouts persistés** (à répercuter dans `exportJSON` ET l'import) : `markers`
(comp + calque), `shy`, `keyLock`, `timeRemap`, `motionBlur`, `bpm*`,
`shyEnabled`, `motionBlur*`.

**Time Remap** passe par `resolveSymbolFrameIdx` (app.js), point de passage
unique de tous les lecteurs d'un composant. **Motion blur** est un
post-traitement dans `buildSceneJson` : il réutilise les items déjà construits
et leur applique la matrice DELTA — ne jamais dupliquer la boucle de
construction (CLAUDE.md §3).

## 12. Image mesh — déformer une image importée (2026-08-30, PR1)

`image-mesh.js` (+ `draw_image_mesh` dans engine.rs). Demande de Cyril :
déformer/animer une image importée via un maillage éditable, AVEC un système de
masque, les deux **intégrés au calque image** et pas en calques séparés. Règle
confirmée explicitement : **le contour du masque EST la frontière du maillage**
— on triangule À L'INTÉRIEUR du contour. C'est ce qui fait UNE fonctionnalité et
pas deux : il n'existe aucun chemin de code « masque d'image » séparé, le
contour est directement la silhouette que le moteur découpe.

**Le maillage vit dans `state.imageMeshes`, PAS sur le stroke.** Le dict d'un
raster est réécrit tel quel dans CHAQUE frame du calque (boucle d'import de
images.js) — poser la topologie dessus la dupliquerait une fois par frame.
Mesuré sur 24 frames avec un maillage 8×8 : projet 133 855 octets (store) contre
311 268 (par frame), soit **+177 413 octets, ×2,33**, pour une entrée de
maillage de 7 723 octets. Le stroke ne porte que `meshId` (17 octets/frame).
Même catégorie et même emplacement de persistance que `symbols`/`trackRoles`.

⚠️ **`meshId` doit être écrit sur TOUTES les frames, pas seulement la
courante** (`SMImageMesh.propagate`). Trouvé en mesurant, pas en relisant :
`saveActiveLayerFrame` ne re-sérialise que la frame courante, donc taguer le
Raster live ne taguait qu'UNE frame — un scrub d'une frame et l'image
redevenait plate et non masquée, sans la moindre erreur. Famille de bug n°1 du
§1 exactement.

**Espace normalisé.** Tout un maillage (contour, sommets de repos, offsets) est
en coordonnées 0..1 sur le rect d'affichage du raster, JAMAIS en monde :
`buildSceneJson` résout déjà toute la chaîne Motion/parent/3D en UN rect final
par item, et `scenePayload` mappe le maillage à travers CE rect — donc le
maillage suit déplacement/rotation/échelle/Motion sans deuxième copie des maths
de transformation (§3). `verts` = repos, `offsets` = pose statique ; la version
animée (clés par sommet, PR3) se superpose via l'argument `poseAt`.

**Rendu : une affine par triangle, pas de nouveau pipeline GPU.** vello ne sait
placer une image que sous une affine — mais une affine est déterminée
EXACTEMENT par trois paires de points, donc un mapping triangle→triangle n'est
pas une approximation. Chaque triangle est un `scene.fill` avec l'image comme
BRUSH (c'est littéralement ce que fait `draw_image` en interne, avec un rect au
lieu d'un triangle). Un seul `push_layer` externe sur le contour déformé porte
le masque ET l'opacité — l'opacité là plutôt que par triangle, sinon chaque
couture se composite deux fois. Coutures : chaque triangle de destination est
dilaté d'un demi-pixel ÉCRAN autour de son centroïde (le clip seulement, jamais
l'affine de l'image), sinon l'antialiasing laisse une grille visible à ~25 % de
fond entre triangles voisins.

**Chiffré** : maillage au repos contre image simple = **52 px différents sur
2 073 600 (0,0025 %), écart de canal max 1/255** — même ordre que le
`pathTransform` du §5ter, donc la reconstruction par morceaux est exacte.
`render()` et `render_to_pixels()` d'accord sur 11 échantillons monde sur 12
(le 12e tombe sur une frontière de damier, artefact de ma calibration écran).

⚠️ **`exportNeedsEngine` (export.js) doit inclure `exportHasImageMesh`** —
même piège que 3D/Motion Blur/Order : le repli Paper.js dessine un Raster comme
son rect, sans notion de maillage ni de masque, donc l'export sortirait l'image
parfaitement plate et non masquée en ayant l'air correct à l'écran.

**Limite v1 assumée** : triangulation Delaunay NON contrainte (Delaunator,
ISC, vendorisé) + filtre par centroïde-dans-le-contour. Exact pour un contour
convexe ; un contour très concave peut perdre un éclat de couverture près de la
concavité. Les arêtes longues du contour sont densifiées pour limiter ça.

**Pas encore fait** : l'éditeur on-canvas (PR2) et les clés Motion par sommet
(PR3, via le patron `vtx*`/`applyPathVertexOffsets` de motion.js). Une entrée
de maillage orpheline (image supprimée) n'est pas ramassée — sans conséquence
visible, mais elle force l'export par le moteur via `exportHasImageMesh`.
