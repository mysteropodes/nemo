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
3. **Étape dylibs devenue INUTILE depuis le décodeur v2 (pipe ffmpeg, 2026-07)** :
   `scripts/bundle-ffmpeg-dylibs.py` existe toujours mais n'a plus rien à faire — le moteur
   vidéo natif (`src-tauri/src/video_decode.rs`) ne lie plus aucune lib ffmpeg directement
   dans le binaire Rust (plus de crate `video-rs`/`ffmpeg-sys-next`). Il pilote désormais le
   binaire CLI ffmpeg **déjà embarqué** en sous-processus (pipe stdout, résolu au runtime via
   `current_exe().parent().join("ffmpeg")`) — ce binaire est **statiquement lié** (confirmé via
   `otool -L` : uniquement des frameworks système, zéro dépendance Homebrew), donc aucun dylib
   à embarquer, aucun crash au lancement. Vérifié : `otool -L target/release/nemo | grep
   homebrew` → 0 résultat.
   ⚠️ **Licence, nuance importante** : le binaire ffmpeg embarqué reste GPL (`ffmpeg -version`
   confirme `--enable-gpl --enable-libx264 --enable-libx265`). Le piper en sous-processus est
   de la "simple agrégation" (le pattern standard de tout logiciel de montage commercial qui
   embarque ffmpeg), nettement plus sain juridiquement que le linkage direct qu'on avait avant
   — mais ça ne fait pas disparaître la dépendance GPL en soi. Avant toute vente, il faudra
   toujours une build ffmpeg custom LGPL-only décode-seul si on veut être totalement propre.
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

**Animation 2D ↔ Motion, niveau CALQUE : le déclencheur de conversion est la clé, pas le geste.**
Un calque plat et sa vue Motion sont la MÊME chose tant qu'aucune propriété de NIVEAU CALQUE
n'a de clé. Dès la première clé posée sur une propriété du calque (Position/Anchor/Rotation/
Scale/Opacity — que ce soit via le stopwatch OU un drag direct sur le canvas, les deux doivent
converger vers la même conversion, cf. `maybeAutoConvertToComponent` dans motion.js), le calque
(s'il contient 2+ éléments sélectionnables) devient un Component. Une clé posée sur une propriété
d'un SEUL élément (pas le calque entier) NE déclenche PAS cette conversion — c'est la distinction
layer-holder vs element-holder déjà dans `state.layers.indexOf(ld)` (motion.js).

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

## 9. Collaboration Git (2026-07) — repo privé `mysteropodes/nemo`

Depuis l'arrivée d'un collaborateur (pencilpark), ce dossier n'est plus le seul endroit où le
code vit — `origin` pointe vers un vrai repo GitHub privé, et il faut éviter de s'écraser
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
