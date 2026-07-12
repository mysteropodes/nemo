# Feature scouting — Umoupen / Callipeg / Autodesk SketchBook vs Nemo

Branche: `experimental/feature-scouting` (jamais mergée sans validation).
Audit du 2026-07-12, basé sur les pages publiques de 4 apps (Umoupen, Callipeg,\nAutodesk SketchBook, TVPaint) + grep du code Nemo.

## Déjà présent chez Nemo (pas la peine de reprototyper)
- Stabilisation de trait, gap-closing sur le fill, onion skin, pistes audio,
  brush wobble/texture, tween de forme + position, Ghost-All, courbes d'easing,
  export Rive/AE/vidéo, calque caméra avec keyframes bézier.

## Manque et candidats à prototyper (triés par rapport effort/valeur)

### 1. Outil Symétrie / Miroir (SketchBook) — ★ candidat #1
Dessiner avec réflexion en temps réel sur un axe X/Y ou en radial (mandala,
jusqu'à 16 secteurs). Gros ROI pour la symétrie de personnage. Isolable : un
wrapper au moment du commit de trait, n'importe pas dans le moteur de fill/tween.
**Prototype fait dans cette branche** (voir plus bas).

### 2. French Curve / guide ellipse-perspective (SketchBook)
Gabarits de courbes savantes + ellipse qui s'aligne sur des lignes de fuite.
Utile pour les BG/perspective. Risque UI moyen (nouvel outil + poignées).

### 3. Predictive Stroke (SketchBook)
Reconnaît et corrige à la volée cercle/rectangle/ligne dessinés à main levée.
Isolable (post-traitement du trait au commit), mais heuristique de reco à régler.

### 4. Calques masque (Umoupen/Callipeg)
Un calque qui clippe les calques en dessous par son alpha. Nemo a des
blend modes mais pas de vrai clipping-mask. Touche le rendu Rust (engine.rs) —
plus risqué, à faire en dernier.

### 5. Storyboard mode (Umoupen)
Vue grille de vignettes + pistes de légendes (dialogue/note/beat) + export PDF.
Gros morceau (nouveau mode de projet entier), pas un "prototype d'un soir".

### 6. Interval Assistant / Cycle tool (Callipeg)
Génère automatiquement un cycle de marche/course en répétant une plage de
frames N fois avec option miroir. Nemo a déjà un bouton "Cycle" (repete N fois)
— à vérifier s'il manque le miroir automatique en fin de cycle.

### 7. Référence 3D (Umoupen)
Import OBJ/glTF comme calque de référence rotatif. Gros morceau (rendu 3D),
hors scope pour un prototype léger.

### 8. Effets non-destructifs empilables (Umoupen)
Motion blur, bloom, filtres shader, réordonnables/togglables sans détruire le
dessin. Recoupe avec le pipeline shader WGSL déjà en place (voir CLAUDE.md) —
faisable en Labs comme post-process optionnel par calque.

### 9. Marking-menu radial ("Lagoon", SketchBook)
Clic-maintenu ouvre un menu circulaire avec les outils les plus utilisés
autour du curseur. Pure UI/UX, faible risque, gain de vitesse à l'usage tablette.

## Ce qui est prototypé (tous testés en live, tous off par défaut)

Console : `SMLabs.list()` / `SMLabs.enable('nom')` / `SMLabs.disable('nom')`.
Un fichier + un commit par prototype sous `src/js/labs/` ; seul point de
contact avec le code de prod : UN hook gardé dans `draw-bridge.js`
(commitStroke) + les balises `<script>` dans index.html.

| Prototype | Source | Fichier | Vérifié |
|---|---|---|---|
| `symmetry` (miroir vertical) | SketchBook | symmetry-mirror.js | trait + miroir, 1 undo retire les 2 |
| `radial-symmetry` (mandala 2-16 secteurs) | SketchBook | symmetry-mirror.js | 6 copies en couronne, 1 undo retire tout |
| `predictive-stroke` (ligne/ellipse/rectangle parfaits) | SketchBook | predictive-stroke.js | cercle→ellipse 4 seg, ligne→2 pts, rect→4 coins ; chaîné avec symmetry, le miroir reflète la forme corrigée |
| `multiframe-draw` (trait tamponné sur les frames sélectionnées) | Umoupen | multiframe-draw.js | 1 copie exacte par frame cible, promotion keyframe propre, 1 undo global |
| `lagoon-menu` (menu radial d'outils sur Q) | SketchBook | lagoon-menu.js | ouverture au curseur, clic = switch d'outil, inerte dans les champs texte |
| `pingpong-cycle` (cycle aller-retour A B C B A) | Callipeg | pingpong-cycle.js | action `SMLabs.pingpongCycle(n)` sur une plage sélectionnée ; séquence exacte, 1 undo |
| `move-to-layer` (déplacer la sélection vers un autre calque) | Umoupen | move-to-layer.js | action `SMLabs.moveSelectionToLayer(idx)` ; re-parentage Paper + promotion keyframe cible, companions inclus, 1 undo restaure |
| `canvas-grid` (grille monde superposée) | Umoupen/SketchBook | canvas-grid.js | suit le pan en restant verrouillée au monde, pas réglable (`SMLabs.setGridStep`), teardown propre au disable |
| `timeline-markers` (marqueurs nommés/colorés) | Callipeg | timeline-markers.js | drapeaux colorés + tooltip sur le header de frames, survivent au re-render (MutationObserver), stockés hors fichier projet |
| `flip-roll` (rouleau d'animateur, maintenir R) | TVPaint Flip panel | flip-roll.js | ±2 frames défilées autour de la pose (vitesse/portée réglables), retour exact au relâcher, coupé si focus perdu |
| `retime-exposure` (ones→twos→threes) | TVPaint X-sheet | retime-exposure.js | 4 clés sur ones re-calées sur twos (0/2/4/6) avec tenues entre, 1 undo restaure |
| `interval-assistant` (breakdowns éasés) | Callipeg | interval-assistant.js | spacing chart in-out : breakdowns serrés aux extrémités (frames 4 et 8 entre 3 et 9), pré-remplis du contenu tenu, 1 undo |
| `xsheet` (feuille d'exposition flottante) | TVPaint/Umoupen | xsheet-panel.js | frames × calques, ●/◆/│, clic = saut de frame, refresh auto sur rebuild timeline, teardown propre |
| `out-of-pegs` (décaler les fantômes onion) | TVPaint light table | out-of-pegs.js | fantôme prev décalé de +200px vérifié numériquement, survit à la navigation de frames, remis sur pegs au disable (wrapper runtime autour de renderOS, retiré au disable) |
| `command-palette` (Cmd/Ctrl+K) | Umoupen command search | command-palette.js | filtre fuzzy ('gomme'→1 résultat), Entrée exécute + ferme, toggles Labs inclus dans la liste |
| `mirror-check` (coup de miroir, maintenir M) | classique animateur | mirror-check.js | scaleX(-1) CSS le temps de l'appui, relâcher OU pointerdown remet droit (contrôle seulement, jamais dessiner en miroir) |
| `auto-actions` (macros de commandes) | Clip Studio Paint | auto-actions.js | macroStart/Stop/Play — enregistré 4 étapes (outil+frame+keyframe), rejouées exactement ; bug clé-flag/store trouvé et corrigé |
| `timelapse` (enregistrement de session .webm) | Clip Studio Paint | timelapse.js | captureStream direct du canvas WebGPU = vide (652 o) → composite 2D périodique, vraies frames (5,4 Ko/3 s), download auto |
| `vector-trim` (gomme vectorielle aux intersections) | Clip Studio Paint | vector-trim.js | branche au-delà d'un croisement supprimée (2306→1153), span central entre 2 croisements isolé (1 trait→2 bouts), autres traits intacts, 1 undo |
| `view-filter` (contrôle des valeurs) | Clip Studio Paint | view-filter.js | grayscale/contrast/dim en CSS display-only — on peut dessiner pendant (contrairement au miroir) |
| `speed-lines` (lignes de vitesse manga) | Clip Studio Paint | speed-lines.js | 24 traits radiaux générés avec jitter, couleur/épaisseur courantes, 1 undo retire l'éclat entier |
| `reference-fill` (ink & paint multi-calques) | CSP reference layer | reference-fill.js | fill sur le calque couleur avec les murs du calque lineart (clones jetables, pattern des closing strokes) ; tag isFillTempClose hérité par le résultat trouvé/corrigé |
| `pose-library` (substitution de dessins) | Toon Boom | pose-library.js | savePose sur sélection, stampPose sur n'importe quelle frame avec offset, strokeIds frais par tampon |
| `boil-effect` (ligne bouillante) | Moho vector noise | boil-effect.js | N keyframes-variantes à jitter SEEDÉ par frame (déterministe), points seuls déplacés (les poignées survivent), 1 undo |
| `follow-path` (bake le long d'un trait) | Moho | follow-path.js | sélection bakée en 8 keyframes le long d'une trajectoire éasée (x −220→7041), guide exclu du bake, 1 undo |
| `vector-sculpt` (pousse-vecteurs W / lissage W+Shift) | Toon Boom contour + Smooth Editor, Umoupen warp | vector-sculpt.js | subdivision auto sous la brosse (ligne 2 pts → poussée de 365px), lissage zigzag 2936→3 de rugosité (extrémités épinglées), interception document-capture sans trait parasite, 1 undo par geste |
| `lipsync-assistant` (bouches par amplitude audio) | Moho/Toon Boom (version amplitude-only) | lipsync-assistant.js | WAV synthétique 4 quarts (silence/faible/fort/silence) → bouches fermée/mi/ouverte posées exactement sur les 48 frames correspondantes, aucune dépendance nouvelle (réutilise l'AudioBuffer déjà décodé par SMAudio) |
| `clip-mask-bake` (masque de découpe BAKÉ) | Umoupen/CSP mask layer (version bakée, pas live — voir #1) | clip-mask-bake.js | intersection booléenne exacte (cercle 502796px² ∩ carré 160000px² = 160000px² pile), pipeline WASM/Paper existant réutilisé, calque masque supprimé, 1 undo restaure les 2 calques |

Bugs réels trouvés/corrigés pendant les protos (documentés dans les
commits) : RDP dégénéré sur boucle fermée, cascade de promotion keyframe
dans le tampon multi-frames, anchors simplifiés qui coupent les coins.

## Volontairement NON prototypé — pourquoi, et par où commencer si adopté

Chaque entrée ci-dessous touche le moteur Rust, le format de fichier, ou
exige un vrai chantier UI — un prototype Labs "sans risque" y est
impossible ou mensonger (il donnerait une fausse idée du coût réel).
Classées par rapport valeur/effort estimé.

### 1. Calques masque / clipping (Umoupen, CSP, Toon Boom stencil)
**Quoi** : un calque dont l'alpha découpe les calques en dessous (ou un
clipping-mask par calque adjacent façon CSP).
**Pourquoi pas en Labs** : le compositing des calques se fait dans
`engine.rs` (`render()` + `render_to_pixels()`, les DEUX copies à garder
identiques — CLAUDE.md §3). Un masque exige un vrai groupe de clip au
niveau vello, plus la sérialisation scène (`buildSceneJson` →
`ItemIn`/`LayerIn` Rust), plus la persistance (nouveau champ calque dans
`exportJSON`), plus l'UI calques.
**Par où commencer** : vello supporte `push_layer` avec clip — ajouter un
champ `clipMask: bool` au calque, l'appliquer dans les deux fonctions de
rendu, et brancher le fallback Paper.js (Group.clipMask) pour le mode
sans moteur. Estimation : 2-3 jours avec tests export inclus.
**Valeur** : haute (demandé par tous les workflows peinture).

### 2. Effets non-destructifs par calque (Umoupen : motion blur, bloom, filtres shader)
**Quoi** : pile d'effets réordonnables/désactivables par calque, rendus à
la volée sans modifier les traits.
**Pourquoi pas en Labs** : c'est du post-process GPU par calque — donc
`engine.rs` (rendu offscreen par calque + passe shader) ou le pipeline
WGSL déjà documenté dans CLAUDE.md (section shaders Rive). Un fake CSS ne
tiendrait ni à l'export ni au zoom.
**Par où commencer** : le pattern post-process validé côté Rive (CLAUDE.md
« Architecture Post-process sur Artboard ») est transposable : rendre le
calque dans une texture, passe WGSL, composite. Commencer par UN effet
(blur gaussien) derrière un champ `effects:[]` par calque. Estimation :
3-5 jours pour l'infra + 1er effet ; chaque effet suivant est petit.
**Valeur** : moyenne-haute (différenciant visuel fort).

### 3. Mode Storyboard (Umoupen : grille de vignettes, légendes, export PDF)
**Quoi** : un mode projet entier — vue grille des plans, pistes de
légendes (dialogue/note/beat), timing par plan, export PDF de board.
**Pourquoi pas en Labs** : ce n'est pas une feature, c'est une deuxième
application au-dessus du même document (nouveau mode d'UI, nouveau modèle
de données plan/séquence, nouvel export). Un prototype console n'aurait
aucun sens d'usage.
**Par où commencer** : trancher d'abord si Nemo veut être un outil de
board OU s'intégrer à un board externe (le pont Kitsu existe déjà — les
plans peuvent venir de Kitsu, ce qui est plus dans l'ADN prod du projet).
Si intégré : une vue « grille de projets/plans Kitsu » est un chantier UI
pur de ~1 semaine. Si natif : compter plusieurs semaines.
**Valeur** : haute pour la cible prod, mais gros pari produit.

### 4. Référence 3D (Umoupen OBJ/glTF, CSP 3D poser)
**Quoi** : importer un modèle 3D comme calque de référence orientable.
**Pourquoi pas en Labs** : il faut un rendu 3D (chargeur OBJ/glTF +
caméra + rastérisation) que ni Paper ni le moteur vello 2D ne fournissent.
**Par où commencer** : la voie la moins chère est un canvas WebGL séparé
(three.js) affiché comme calque de référence NON exporté, hors moteur —
c'est isolable, mais three.js est une dépendance lourde pour un seul
usage. Alternative zéro-code : importer des rendus PNG du modèle via la
référence roto existante (SMReference), qui couvre déjà 80 % de l'usage.
Estimation : 3-4 jours (three.js) / 0 jour (workaround roto).
**Valeur** : moyenne — le workaround existant en couvre l'essentiel.

### 5. French curve / guide ellipse avec snap (SketchBook)
**Quoi** : gabarits de courbes posés sur le canvas, le trait s'y colle.
(Le guide de PERSPECTIVE existe déjà dans Nemo — panneau « Guide de
perspective » ; le manque est le gabarit courbe/ellipse aimanté.)
**Pourquoi pas en Labs** : le snap doit intercepter le trait PENDANT le
drag (pas au commit comme predictive-stroke) — donc modifier
draw-bridge.js au cœur de sa boucle pointermove, la zone la plus
sensible en perf du codebase (§5).
**Par où commencer** : un hook optionnel `SMLabs.snapSample(pt)` appelé
dans la boucle d'échantillonnage (même pattern que le hook commitStroke,
une ligne gardée), le gabarit lui-même en overlay type canvas-grid.
Estimation : 2 jours, dont la moitié en tests perf stylet.
**Valeur** : moyenne (public illustration plus qu'anim).

### 6. Bones / smart bones / déformeurs (Moho), node view (Toon Boom)
**Quoi** : rigging squelettal, déformations, contrôleurs.
**Pourquoi pas en Labs — ni jamais en natif ?** : c'est un moteur
d'animation entier. La position déjà prise par le projet (export Rive
MCP, RiveBar) est la bonne réponse : Nemo dessine, **Rive rigge**. En
réimplémentant Moho dans Nemo on perdrait sur les deux tableaux.
**Par où commencer si le besoin se précise** : enrichir l'export Rive
(les bones existent dans l'object model Rive, cf. mémoire « weight
encoding / tendon transforms ») plutôt qu'un rig natif.
**Valeur** : haute, mais à capturer via Rive, pas en interne.

### 7. Brosses animées / moteur de brosses bitmap (TVPaint, CSP)
**Quoi** : pointes de brosse bitmap, brosses multi-images, mélange humide.
**Pourquoi pas en Labs** : Nemo est 100 % vectoriel par choix (StrokeMotion
« Paper.js = source de vérité ») ; une brosse bitmap casse ce contrat de
bout en bout (données, tween, export Rive). La diversité de brosses
VECTORIELLES est déjà un chantier queued (mémoire « brush diversity »).
**Valeur** : à réévaluer seulement après le chantier brush diversity.

### 8. Screentones / trames (CSP)
**Quoi** : remplissages en trames de points/lignes façon manga.
**Pourquoi pas en Labs** : en vectoriel naïf c'est des milliers de petits
Paths par aplat (budget scène §5 explosé) ; la vraie implémentation est un
motif de fill côté moteur (pattern fill vello) + export.
**Par où commencer** : `PaintIn` Rust avec un champ pattern ; ou attendre
les effets par calque (#2) dont c'est un cas particulier.
**Valeur** : basse pour la cible actuelle (anim > manga).

### 9. Auto lip-sync (Moho, Toon Boom)
**Quoi** : générer les bouches depuis la piste audio.
**Pourquoi pas (encore)** : l'analyse audio (amplitude/phonèmes) est un
chantier en soi. MAIS les deux briques Labs nécessaires existent
désormais : `pose-library` (kit de bouches) + la piste audio SMAudio.
**Par où commencer** : un Labs `lipsync-assistant` qui lit l'amplitude
RMS par frame (SMAudio a déjà les waveforms) et tamponne
bouche-ouverte/mi-ouverte/fermée via stampPose — PAS de phonèmes, juste
l'amplitude. C'est le seul de cette liste qui pourrait finalement passer
en Labs (~1 jour). Gardé hors scope aujourd'hui uniquement par prudence
sur la lecture des buffers SMAudio.
**Valeur** : haute en démo, moyenne en prod (l'amplitude seule est grossière).

### 10. Interchange X-sheet (XDTS/OCA — Callipeg, TVPaint, pipeline japonais)
**Quoi** : import/export du timing au format feuille d'expo standard.
**Pourquoi pas en Labs** : purement data-level DONC faisable — mais sans
un vrai cas d'usage (studio en face qui consomme le XDTS), c'est du
format-guessing. À prototyper le jour où un partenaire pipeline le
demande, contre ses fichiers réels.
**Valeur** : nulle sans partenaire, haute avec.

## Comment décider / trier plus tard
Chaque candidat prototypé peut être adopté indépendamment : `git
cherry-pick <commit>` sur `main`, puis décision UI (bouton Réglages,
raccourci, intégration à l'outil). Pour les non-prototypés ci-dessus,
l'ordre de valeur suggéré : masques (#1) → effets par calque (#2) →
lip-sync amplitude (#9) → French curve (#5), le reste sur demande.
