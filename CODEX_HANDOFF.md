# Consignes pour Codex — session Nemo du 26/07/2026

## Mise à jour Codex — audit performance et refactoring progressif (23/08/2026)

Branche : `codex/performance-refactor-handoff`, créée depuis
`claude/web-public-beta` (`48b3bb7`) en conservant les modifications locales
préexistantes. Ne pas attribuer à ce lot les diffs déjà présents dans
`.claude/launch.json`, `geometry-wasm/src/engine.rs`,
`src/js/shader-effects-library.js`, `src/wasm/geometry_wasm_bg.wasm`,
`src/.DS_Store` et les fichiers non suivis historiques.

### Lot 1 — optimisations sans changement fonctionnel

- `playback-cache.js` ne réaffecte plus `canvas.width/height` à chaque cache
  hit lorsque la taille est inchangée. Une affectation recréait le backing
  store complet à chaque frame.
- Le prototype `labs/timeline-zoom.js` n'est plus chargé par `index.html` :
  sa version promue `timeline-zoom.js` est la seule active. Cela supprime un
  second listener `Ctrl/Cmd+wheel` et évite un double zoom si l'ancien Lab
  était activé.
- `npm test` exécute les tests Node de `tests/performance-regressions.test.cjs`.
  Ils verrouillent les deux invariants ci-dessus.

Validation du lot : `npm test` (2/2), `node --check` sur tous les scripts
(loader WASM vérifié en mode module), `git diff --check`, et compilation des
tests Rust/WASM avec `cargo test --manifest-path geometry-wasm/Cargo.toml
--target wasm32-unknown-unknown --no-run`. Le checkpoint suivant rend aussi
la validation native exécutable sans essayer d'ouvrir une surface Canvas.

### Checkpoint effets existants et garde-fou WGSL

Les modifications locales déjà présentes sur Film Grain, Watercolor,
Anamorphic Flare, Cinematic Lens Flare et la boîte document stable sous
zoom/pan ont été isolées dans un commit dédié avec leur binaire WASM. Elles
ne font pas partie du refactoring de timeline.

Le test `shader_library_validation.rs` a été remis en état : son wrapper
déclare maintenant `bbox_o`, `bbox_s`, `local_uv` et les 12 floats réels de
`Params`. `create_engine` est exporté uniquement en `wasm32`, ce qui permet
à `cargo test` natif de compiler le reste du moteur et d'exécuter Naga sans
changer le build navigateur. Résultat : 2 tests de coordonnées viewport et
la validation Naga de toute la bibliothèque WGSL passent réellement.

### Lot 2 — timeline : suppression d'un parcours quadratique

`renderKeyframeCellsInto` mémorise désormais la dernière vraie clé et son
état plein/vide pendant son parcours vers l'avant. L'ancien code rescannait
les frames précédentes pour chaque cellule tenue, soit O(frames²) sur une
longue piste vide ou avec une clé ancienne. Le résultat reste O(frames).

Le test Node compare les classes attendues sur clés vides/pleines,
interpolation et fins de span, puis instrumente 1 000 frames vides et refuse
plus de 2 000 accès indexés. `npm test` passe à 3/3.

### Lot 3 — calques d'effet : une copie GPU plein écran supprimée

Pour un calque d'effet, `apply_effect_stack_into` conserve le ping-pong des
passes intermédiaires mais écrit la dernière passe directement dans
l'accumulateur suivant. L'ancienne copie `effect-stack-to-accum-copy` et son
`queue.submit()` disparaissent. La pile ordinaire ne construit plus non plus
un `Vec` temporaire des effets activés à chaque rendu.

Validation : `cargo test` natif (3/3, dont toute la bibliothèque WGSL), build
des tests wasm32, `wasm-pack build --target web --release --out-dir
../src/wasm`, puis test visuel au port 1420 sur un projet 1920×1080 avec un
calque d'effet : Invert seul, puis Invert + Sepia. Les deux rendus sont
corrects et le journal navigateur ne contient aucune erreur ou alerte WebGPU.
Ne pas tenter un `cargo fmt` global : le crate préexistant n'est pas formaté
et cela réécrirait des milliers de lignes sans rapport.

### Lot 4 — undo : suppression des doubles write-back

Vingt chemins appelaient `saveAllLayerFrames()` juste avant `pushUndo()` ou
`pushUndoLayers()`, qui rappelait la même sauvegarde. Le point central accepte
maintenant `alreadySaved=true`; tous les chemins explicitement appariés le
passent, tandis que les centaines d'appels ordinaires conservent exactement
le comportement historique. Le garde `_scrubLiveActive` reste prioritaire.

Deux tests verrouillent la sémantique (sauvegarde implicite, réutilisation,
scrub) et interdisent le motif double dans `app.js`, `images.js` et
`timeline.js`. `npm test` passe à 5/5. L'ajout de calque a aussi été rejoué
dans l'UI sans erreur console. Le harnais navigateur n'a pas su transmettre
Meta avec Z (il activait l'outil Z) : aucune validation Cmd+Z n'est revendiquée
à partir de ce geste invalide ; le test automatisé du point central fait foi.

### Validation finale et reprise Claude

Commits, dans l'ordre :

1. `aec24d0` — cache de lecture + doublon Timeline Zoom + tests Node.
2. `afbda66` — checkpoint shaders/Watercolor + validation WGSL exécutable.
3. `981fd75` — rendu linéaire des spans de timeline.
4. `c094f78` — dernière passe d'un calque d'effet vers l'accumulateur.
5. `2b5dfbf` — réutilisation des sauvegardes lors du push undo.

Validation finale : `npm test` 5/5, syntaxe de tous les JS, `cargo test`
Rust 3/3, compilation des tests wasm32, build WASM release, `cargo check`
Tauri avec jetons factices de compilation, et `git diff --check`. Les quatre
warnings Tauri `objc/cargo-clippy` sont préexistants. `cargo check` régénère
temporairement `src-tauri/gen/schemas/capabilities.json`; cette sortie a été
restaurée et ne fait partie d'aucun commit.

Reste volontairement hors scope de ces lots sûrs : virtualisation DOM de la
timeline, délégation globale de ses événements, snapshots undo différentiels
et regroupement de toutes les passes GPU dans un encoder unique. Ces travaux
nécessitent des tests Tauri plus larges et ne doivent pas être fusionnés dans
une réécriture unique. Les modifications préexistantes `.claude/launch.json`,
`src/.DS_Store`, `STRATEGY.md`, `logo.ai` et `nemo_timeline.html` restent
intactes et non commitées.

## Mise à jour Codex — lot autonome des 37 feedbacks

Branche de travail : `codex/feedback-2026-07-26`, créée depuis le
checkpoint effets/références 3D `3e1ac3f`.

État final sur disque : **36 `resolved`, 1 `approved`**. Chaque entrée
résolue contient une résolution écrite par
`SMFeedback.resolveFeedback(id, résolution)`.

### Statut des 37 entrées

- `fb_ms1tehiq_654020` — resolved — fill/stroke liés et sélection
  reconstruite après undo.
- `fb_ms1vlpf0_905955` — resolved — fragments Fill/Stroke transformables.
- `fb_ms1vpt4p_863478` — resolved — double-clic vers le mode Fill/Stroke.
- `fb_ms1x5ia8_664634` — resolved — lasso par intersection partielle.
- `fb_ms1x7e77_122668` — resolved — rectangle par intersection partielle.
- `fb_ms1vi1xl_232881` — resolved — contrainte Shift du sommet/tangentes.
- `fb_ms1vjpou_209227` — resolved — transformations numériques du sommet.
- `fb_ms1tjdje_722216` — resolved — dessin multi-frame sur le calque actif.
- `fb_ms1xcho4_585303` — resolved — lissage du Fill Brush amélioré.
- `fb_ms1vfy1t_383628` — resolved — Shadow Brush = ligne ouverte sans fill.
- `fb_ms1y7vrq_279926` — resolved — perspective dirigée vers les points de fuite
  depuis toute zone du canevas.
- `fb_ms1y2xti_95012` — **approved** — action trail rejoué, aucun défaut
  déterministe reproductible ; aucune correction inventée.
- `fb_ms1oe3u2_398396` — resolved — menu brosses scrollable, modes automatiques,
  aperçus homogènes.
- `fb_ms1omuqk_817075` — resolved — outils flottants verticaux et champs scrub.
- `fb_ms1oobzf_56220` — resolved — commandes inférieures alignées.
- `fb_ms1oqgb0_677984` — resolved — preset appliqué à la sélection.
- `fb_ms1otm5q_664430` — resolved — Vector Sculpt actif et rayon Alt-glisser.
- `fb_ms1vsj19_93848` — resolved — fermeture du color picker hors pipette.
- `fb_ms1x1bro_705738` — resolved — barre bleue d’insertion des sections.
- `fb_ms268dz5_757433` — resolved — Effects unifié et gradient sous Fill.
- `fb_ms1v44no_472123` — resolved — personnalisation persistante de la toolbar.
- `fb_ms1v8ooc_624201` — resolved — frame courante en contour.
- `fb_ms1vadtn_146111` — resolved — Ghost All Keyframes réparé.
- `fb_ms1vcq19_118835` — resolved — drag de la boîte globale depuis sa surface.
- `fb_ms1yix2g_673688` — resolved — sélection/distribution Motion unifiée.
- `fb_ms20xt2w_106453` — resolved — synchronisation canevas → calque Motion.
- `fb_ms20zv2m_756220` — resolved — motion path dans la bonne chaîne de matrices.
- `fb_ms1vwagy_692629` — resolved — composant suivi en continu pendant le drag.
- `fb_ms1w5zw1_352345` — resolved — déplacement/rotation/échelle du composant.
- `fb_ms1wgb2g_595332` — resolved — clés externes de composant en Animation 2D.
- `fb_ms1wilej_600565` — resolved — scrub sans reconstruction complète du symbole.
- `fb_ms1xzbzg_436210` — resolved — gradient transformé avec l’objet/Motion/parents.
- `fb_ms1y18rx_682296` — resolved — gradient préservé par eraser/booléens.
- `fb_ms1zg8ow_519376` — resolved — export à la frame demandée, sans overlays.
- `fb_ms1zivh0_685815` — resolved — clés d’effets préservées au round-trip.
- `fb_ms2115id_203564` — resolved — unités d’effets synchronisées au zoom courant.
- `fb_ms1zkf6t_157482` — resolved — garde de fermeture réentrante et permissions
  Tauri `close`/`destroy`.

### Scénarios de validation importants

- Harnais navigateur au port réel `1420`, mutations uniquement par événements
  pointeur/contrôles visibles ; le harnais temporaire a été retiré du produit.
- Menu Brush : `bitmapBrushOn:true/vectorBrush:false` après choix bitmap, puis
  l’inverse après choix vectoriel ; zone de brosses `clientHeight 335`,
  `scrollHeight 1070`.
- Shadow Brush : sorties ouvertes, `fill:null`, stroke visible, channel
  `shadow`, aucune forme de remplissage liée.
- Composants : F6 crée une clé externe, choix de frame interne stocké dans
  `componentFrame`, F7 tient le blank sur les frames suivantes.
- Effets : deux clés de Blur (frames 0 et 9) créées par l’UI ; export/import
  retourne `equal:true` et les mêmes valeurs.
- Fermeture Tauri native : sur document modifié, `Annuler` conserve le PID ;
  un second essai avec `Quitter sans sauvegarder` termine le processus.

### Vérifications

- `node --check` sur tous les JavaScript modifiés : OK.
- `git diff --check` : OK.
- `cargo test --tests --target wasm32-unknown-unknown --no-run` : OK.
- `cargo check` Tauri : OK ; quatre warnings `objc`/`cargo-clippy` préexistants.

Lis ce fichier en entier avant de faire quoi que ce soit. Il existe parce
qu'une session Claude Code travaille en parallèle sur ce repo et doit
pouvoir reprendre demain sans collision.

## 0. Action immédiate — change de branche AVANT de commiter quoi que ce soit

Le dossier est actuellement sur `claude/vandijk-batch2`, qui porte une PR
ouverte (#204) d'une autre session. Tes modifications non commitées sont
en ce moment posées dessus. **Avant tout commit :**

```bash
git checkout -b codex/<sujet-court>
```

(`git checkout -b` préserve les fichiers modifiés non commités — rien
n'est perdu.) Choisis un nom de branche qui décrit ce que tu fais
(ex. `codex/shader-effects-library`, `codex/feedback-2026-07-26`).

**Ne commite jamais sur `claude/vandijk-batch2` ni sur `main`.** Vérifie
avec `git branch --show-current` avant chaque commit si tu as un doute.

## 1. Lis `CLAUDE.md` à la racine du repo, en entier

C'est le document de conventions du projet, écrit à partir de bugs
réels déjà rencontrés. Les sections qui s'appliquent le plus souvent :

- **§1** — un nouveau champ/type de donnée doit être vérifié dans TOUS
  les lecteurs de `layer.children` (liste exacte dans le fichier), pas
  seulement celui où tu l'introduis. C'est la source du plus grand
  nombre de bugs de ce projet.
- **§3** — `render()` et `render_to_pixels()` (engine.rs) doivent rester
  synchronisées à chaque modif touchant le rendu.
- **§9** — règles Git : jamais de commit direct sur `main`, toujours une
  branche dédiée, PR à la fin, push régulier (pas de commits locaux qui
  s'accumulent sans être poussés).
- **§10** — tout nouveau champ numérique doit avoir la classe CSS
  `scrub` pour être glissable (convention de toute l'app).

Si tu implémentes des effets WGSL, lis aussi `docs/WGSL_EFFECTS_GUIDE.md`
— écrit aujourd'hui, sourcé ligne par ligne dans le code réel.

## 2. Méthode de test — piloter l'app, pas seulement lire le code

**Ne conclus jamais qu'un correctif marche en lisant le diff.** Lance le
serveur de dev (`.claude/launch.json`, config `nemo-motion-v7`, port
9522) ou l'app Tauri compilée, et vérifie le comportement réel à
l'écran ou via de vrais événements DOM.

Si tu construis un harnais de test synthétique (dispatch d'événements
`pointerdown`/`pointermove`/`pointerup` en JS pour simuler un geste) :

- **N'émets QUE les événements `pointer*`.** Émettre `pointer*` **et**
  `mouse*` pour le même geste fait tourner le geste dans deux pipelines
  à la fois (les bridges de dessin écoutent `pointer*` en capture,
  Paper.js écoute `mouse*` via son système Tool) — ça double
  silencieusement chaque trait et produit des faux résultats.
- **Calibre le harnais sur un cas dont tu connais la réponse** avant de
  t'en servir pour quoi que ce soit (ex. : un trait au pinceau doit
  donner exactement 2 enfants au calque ; un Cmd+Z doit revenir à 0).
  Un résultat de test ne vaut rien tant que cette calibration n'est pas
  passée.
- Session d'aujourd'hui, pour référence : ce piège exact a produit deux
  faux bugs « confirmés » (un résidu après annulation, un crash de
  l'outil Sélection) qui n'existaient pas — corrigés en recalibrant le
  harnais avant de conclure quoi que ce soit.

## 3. Scope — les 7 feedback du jour, si c'est ta tâche

Le panneau Feedback (outil Comment → « Enregistrer comme feedback »)
contient 7 entrées créées aujourd'hui, statut `approved` (à traiter),
réparties sur 2 projets. Fichiers sur disque :
`~/Library/Application Support/com.strokemotion.app/feedback/<projectKey>/*.json`
— champ texte = `note`.

**Projet `testc-1c2ksbz` (5 entrées, panneau flottant Labs) :**
1. Le scroll ne marche pas sur l'ensemble des brosses (panneau flottant
   bitmap/vector). La case à cocher « bitmap » ne devrait pas exister —
   cliquer une brosse doit basculer vector/bitmap automatiquement.
   Aperçus bitmap doivent avoir le même rendu que vector (blanc sur
   fond foncé, en ligne).
2. Quand plusieurs outils sont actifs dans le panneau flottant, leurs
   options doivent s'empiler **verticalement** (pas horizontalement),
   avec une icône grisée à gauche de chaque ligne d'outil. Les valeurs
   doivent être draggables (classe `scrub`, voir CLAUDE.md §10).
3. Les boutons du bas doivent être alignés horizontalement avec BG,
   avec un séparateur entre BG et les icônes.
4. Avec Select + Brush actifs dans le panneau flottant, il faut pouvoir
   changer la brosse **de la sélection courante** — impossible
   aujourd'hui depuis ce menu.
5. Le sculpt vectoriel doit s'afficher directement quand Select est
   actif (pas besoin de maintenir W), montrer un cercle sous la souris
   comme la gomme, et se redimensionner en Alt+glisser gauche/droite.

**Projet `untitled-autosave` (2 entrées) :**
6. Si le fill est attaché au stroke, les sélectionner/déplacer doit
   affecter les deux. Après un Cmd+Z sur un déplacement, la bounding
   box de transformation reste à l'ancienne position pendant que
   l'objet est revenu à sa place — elle doit toujours suivre le ou les
   objets sélectionnés.
7. Après une suite d'actions (nouvelle clé, nouveau calque, clic sur
   « dessin multi-frames »), le stroke n'apparaît plus alors que
   l'option est active partout ailleurs quand on dessine au pinceau.

**Priorise 6 et 7** (bugs francs, testables en pilotant l'app) avant le
lot #1-5 (UX du panneau flottant, plus long).

## 3bis. Méthode de correction — un cycle par item, pas de correction à l'aveugle

Pour chaque item :

1. **Reproduis-le en pilotant l'app** avant de lire une ligne de code —
   le symptôme exact tel que décrit dans la note. Si tu ne reproduis
   pas, dis-le et documente ce que tu observes à la place plutôt que de
   deviner une cause.
2. **Cherche la cause réelle**, pas le symptôme le plus proche. Les
   pointeurs ci-dessous sont des pistes de départ trouvées par une
   recherche rapide dans le code — **pas des diagnostics vérifiés**,
   confirme-les toi-même avant de corriger.
3. **Corrige, puis reproduis le scénario EXACT de la note** pour
   confirmer que le symptôme a disparu — pas juste que le code compile.
4. **Vérifie l'absence de régression adjacente** — si tu touches un
   point de passage partagé (ex. `renderArcs`, `getEffectiveStrokes`),
   relis CLAUDE.md §1 : est-ce que d'autres lecteurs du même état sont
   affectés par ton changement ?
5. Marque résolu (§4) seulement après l'étape 3.

**Pistes de départ (non vérifiées) :**

- **#1-5 (panneau flottant Labs)** — tout vit dans
  `src/js/labs/labs-float-panel.js`. C'est un panneau horizontal
  construit dynamiquement (`.labs-float-btn` par outil actif) ; le
  scroll cassé (#1) et l'empilement horizontal au lieu de vertical (#2)
  sont probablement liés au même conteneur flex/overflow. Le picker de
  brosse par sélection (#4) touche aussi `brush-menu-bridge.js`. Le
  sculpt vectoriel (#5) touche `vector-sculpt.js` (labs) et son
  intégration au raccourci `W`.
- **#6 (bounding box figée après Cmd+Z)** — `undo()`
  (`src/js/tweens.js:4289`) appelle déjà `renderArcs()` après restauration,
  donc le cas simple semble couvert. Piste probable : pendant un DRAG
  actif, `select-bridge.js` passe un cache de boîte
  (`arcDragCache`, ligne ~1043) à `renderArcs()` — si ce cache survit
  après un Cmd+Z qui interrompt/suit un drag, la boîte affichée resterait
  l'ancienne. À vérifier en pilotant : déplacer une forme, PUIS annuler,
  PUIS regarder si `renderArcs()` est rappelé sans le cache de drag.
- **#7 (stroke disparu après dessin multi-frames)** — `onStroke` dans
  `src/js/labs/multiframe-draw.js:20` appelle `getEffectiveStrokes` pour
  figer le contenu hérité des frames non-keyframe avant d'y ajouter le
  nouveau trait. Le commentaire du fichier (lignes ~30-40) documente déjà
  un bug corrigé de cascade sur ce même mécanisme — la régression décrite
  vient peut-être d'un nouveau calque qui n'a pas encore de frame 0
  correctement initialisée au moment où `onStroke` tourne. À creuser en
  reproduisant EXACTEMENT la séquence de la note (nouvelle clé → nouveau
  calque → clic dessin multi-frames → dessiner).

## 4. Marquer un feedback comme résolu

Une fois un point corrigé et vérifié en pilotant l'app :
```js
SMFeedback.resolveFeedback(id, "texte de résolution")
```
Ça écrit `status:'resolved'` + `resolution` dans le fichier JSON
correspondant — c'est ce qui permet de fermer la boucle et de savoir
demain ce qui a été traité sans relire tous les commits.

## 5. Pour que la session Claude de demain puisse reprendre proprement

- **Commits atomiques et précis** — un message qui dit CE QUI a changé
  et POURQUOI (symptôme observé, cause trouvée), pas juste « fix bug ».
  Regarde les derniers commits de `claude/vandijk-batch2` pour le
  format attendu (`git log --oneline -10`).
- **Pousse ta branche dès qu'un morceau cohérent est fini** —
  `git push -u origin codex/<nom>`. Ne laisse pas de travail seulement
  local.
- **Une PR à la fin**, même petite — ça donne un point d'entrée clair
  (`gh pr list`) plutôt que de devoir deviner quelle branche regarder.
- **Ne touche pas aux fichiers hors de ton scope.** Si tu remarques
  autre chose à corriger en cours de route, note-le plutôt que de le
  corriger en même temps (mélange les diffs, complique la revue).
- **N'exécute jamais `npm run build` en même temps qu'une autre session
  y touche** — ça peut écrire dans `src/wasm/*` pendant qu'un autre
  process compile. Vérifie qu'aucun build n'est en cours
  (`ps aux | grep -i tauri` ou demande avant de lancer).

## Mise à jour Codex — frontières Rust/JS et Motion (2026-08-23)

Branche : `codex/rust-js-batching`, créée depuis
`codex/performance-refactor-handoff`. Commits de ce lot :

1. `b22118f` — sorties `Float64Array` du StrokeModeler Rust, anciennes
   méthodes JSON conservées et fallback JS inchangé.
2. `bb26baa` — consommation directe des triplets par `draw-bridge.js`, sans
   reconstruire un objet `{x,y,p}` pour chaque point du stylet.
3. `f8ff1b0` — recherche binaire du segment de clés dans `evalTrack` et
   `rawValueAtFrame`; interpolation, courbes spatiales et holds inchangés.
4. `0014abe` — coalescence rAF des reconstructions complètes de timeline
   pendant les drags Motion (clé, groupe, connecteur et boîte skew/space/
   liquify), avec flush synchrone du dernier état au relâchement.
5. `d7eb344` — recherche binaire d'une clé Motion exacte (`keyAt`), utilisée
   notamment par la création, le déplacement et les collisions de clés.
6. `56ef184` — coalescence rAF commune des événements `input` et `change`
   des champs numériques scrubbables, avec flush synchrone de la dernière
   valeur au relâchement et conservation d'une seule entrée d'undo.
7. `6df6e98` — recherche binaire de la clé la plus proche utilisée par
   `nearestKey()` dans les expressions Motion; en cas d'égalité la clé
   précédente continue de gagner comme avec l'ancien parcours linéaire.
8. `94aa598` — les trois scans exacts qui restaient locaux (collision de
   nudge, collage et déplacement unifié) passent eux aussi par `keyAt`; un
   test interdit de réintroduire ce motif linéaire dans `motion.js`.

Mesures isolées sur le WASM release : 500 traits × 122 événements,
sortie compacte **12,30 ms** contre JSON+parse **44,39 ms** (3,61×), même
nombre de triplets. Recherche Motion : 200 000 requêtes dans 4 096 clés,
**11,29 ms** contre **443,54 ms** pour le parcours linéaire (39,30×), mêmes
indices sur toutes les requêtes.

Mesures complémentaires sur 200 000 requêtes / 4 096 clés : recherche de clé
exacte **12,00 ms** contre **236,00 ms** (19,67×), mêmes clés présentes ou
absentes; `nearestKey()` d'expression **11,02 ms** contre **584,67 ms**
(53,03×), mêmes indices, bornes et égalités. Le test de scrub envoie 40
`pointermove` avant une frame et vérifie un seul `input` + un seul `change`;
il vérifie aussi qu'un relâchement avant la frame applique immédiatement la
dernière valeur, annule la callback en attente et ne pousse qu'un undo.

Un batch `align_pairs` a été implémenté et mesuré, puis entièrement retiré :
sorties identiques mais aucune accélération (22,16 ms batch contre 22,37 ms
unitaire sur 48 paires fermées; 12,91 contre 12,76 ms sur 160 paires
ouvertes). Le calcul géométrique domine la frontière, donc ne pas réintroduire
ce batch sans changer le format de données ou l'algorithme.

Garde-fous : les méthodes JSON `down/move/up` existent toujours, le nouvel
adaptateur choisit l'API compacte une fois par geste, et une ancienne version
du WASM continue donc à fonctionner. Tests ajoutés pour la parité
JSON/compacte, le fallback historique, la consommation par triplets, la
complexité logarithmique, les bornes, l'interpolation linéaire et les holds.
Le test de drag envoie 40 demandes avant une frame et vérifie qu'une seule
reconstruction est planifiée; il vérifie aussi que le relâchement annule la
frame en attente, rend exactement une fois, puis qu'un second flush est neutre.
La suite Node contient désormais 12 tests de régression et passe intégralement;
la syntaxe de tous les scripts JS passe également (`geometry-wasm-loader.js`
est contrôlé en mode ES module).

Le navigateur automatisé Codex ne permet pas une validation UI fiable de ce
lot : son instrumentation fait échouer Paper.js au chargement avec
`TypeError: this.setItem is not a function`, avant l'initialisation de Nemo.
Cette erreur n'est pas revendiquée comme un bug produit. La validation finale
s'appuie sur les tests JS, Rust natifs, compilation wasm32 et exécution directe
du module WASM release. Les fichiers utilisateur préexistants
`.claude/launch.json`, `src/.DS_Store`, `STRATEGY.md`, `logo.ai` et
`nemo_timeline.html` restent intacts et hors commits.
