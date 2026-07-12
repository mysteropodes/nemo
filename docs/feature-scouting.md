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

Bugs réels trouvés/corrigés pendant les protos (documentés dans les
commits) : RDP dégénéré sur boucle fermée, cascade de promotion keyframe
dans le tampon multi-frames, anchors simplifiés qui coupent les coins.

## Comment décider / trier plus tard
Chaque candidat ci-dessus peut être prototypé indépendamment dans cette même
branche, un fichier/commit par feature, sans jamais toucher aux fichiers de
prod tant que tu n'as pas dit "prends celle-là". Pour intégrer une feature
choisie sur `main` : `git cherry-pick <commit>` puis retirer le flag Labs si tu
veux qu'elle soit active par défaut.
