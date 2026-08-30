# Nemo — état fonctionnel

**Réécrit le 2026-08-29.** La version précédente datait des tout premiers mois du projet et
était devenue trompeuse : elle listait encore comme manquants les dossiers de calques, la
caméra, le rig, les dégradés, l'audio, l'import PSD/vidéo, les commentaires et *tout le moteur
de rendu Rust* — tous livrés depuis. Un document d'état faux est pire que pas de document :
il a servi de base à des décisions de scope pendant des mois.

Légende : ✅ Fait · 🟡 Partiel · ❌ Manquant

> **Analyse des écarts du mode Motion face à After Effects et Cavalry :** elle vit dans un
> document séparé (audit du 2026-08-29), pas ici. Ce fichier-ci couvre l'app entière ; celui-là
> creuse Motion en profondeur avec un ordre de priorité. Ne pas dupliquer les deux.

---

## 1. Moteur de dessin vectoriel

| Item | État | Détail |
|---|---|---|
| Pinceau vectoriel (Bézier + lissage/stabilisateur) | ✅ | `tools.js` |
| Pinceau de pression (tablette) | ✅ | Pointer Events + canal natif macOS, courbe de pression éditable |
| Pinceau calligraphique | ✅ | `draw-bridge.js`, presets dédiés |
| Pinceau texturé | ✅ | Presets vectoriels + Bitmap Brush (dabs tamponnés, profil de pression réel) |
| Gomme vectorielle (découpe précise) | ✅ | Découpe booléenne réelle via WASM (`erase_at_point`) |
| Sélection directe (ancres + poignées) | ✅ | Outil Sous-sélection (A) |
| Lasso vectoriel | ✅ | `select-bridge.js` |
| Plume point par point | ✅ | `pen-bridge.js` |
| Conversion de points (anguleux/lisse) | ❌ | Les poignées s'éditent, pas de bascule de type de point |
| Opérations booléennes | ✅ | Union/soustraction/intersection + groupes de combinaison non destructifs |
| Remplissages : uni / dégradé / motifs | 🟡 | Uni ✅, dégradé ✅ (`gradient-bridge.js`, + dégradé le long du trait), motifs ❌ |
| Contours : épaisseur/pointillés/caps | ✅ | |
| Palettes personnalisées | ✅ | Multi-palettes, swatches, gestionnaire de couleurs |
| Pipette | ✅ | |
| Sculpt de vecteurs / warp | ✅ | Pousse-vecteurs + lissage, avec subdivision automatique |
| Texte vectoriel | ✅ | Vraies courbes de glyphes, éditable en place, Google Fonts en ligne |

## 2. Timeline et séquençage

| Item | État | Détail |
|---|---|---|
| Calques multiples | ✅ | |
| Dossiers de calques | ✅ | Vrai type de calque : parentage, repli, effets scopés au sous-arbre |
| Calques Null / Effet / Guide | ✅ | |
| Keyframe / inbetween / image vide | ✅ | |
| Curseur, raccourcis, boucle in/out | ✅ | |
| FPS réglable | ✅ | |
| Insertion/suppression frames/keyframes | ✅ | F5/F6/F7 + clic droit |
| Conversion Keyframe↔Frame, Tween→Keyframes | ✅ | |
| Duplication frame/plage, drag & drop | ✅ | |
| Barres in/out par calque | ✅ | Trim, décalage, déplacement groupé |
| Zoom horizontal de timeline | ✅ | Scrollbar maison (zoom aux bords, pan au corps) |
| Marqueurs (comp + calque) | ✅ | |
| Calques « shy » | ✅ | |
| Feuille d'exposition (X-sheet) | 🟡 | Prototype Labs, pas un panneau de production |

## 3. Interpolation

| Item | État | Détail |
|---|---|---|
| Shape tween (morphing) | ✅ | Moteur avancé : proximité/courbure/lignes de force, réattribution manuelle |
| Motion tween (transform d'objet entier) | ✅ | C'est le mode Motion — keyframes de transform sur calque et par forme |
| Trajectoire éditable sur le canvas | ❌ | Les arcs existent en Animation 2D, pas de motion path manipulable en Motion |
| Éditeur de courbes | ✅ | Modèle à N points de passage, partagé tween/motion/caméra/pression |
| Éditeur de graphe (valeur + vitesse) | ✅ | `motion-graph.js` |
| Easing par paire de keyframes | ✅ | |
| Rebond / élastique | 🟡 | Overshoot ✅, pas de presets rebond/élastique nommés |
| Onion skin | ✅ | Teinté/contour, plage réglable, Ghost-All |
| Inbetweening auto + preview live | ✅ | |
| Harmonisation des frames de tween | ✅ | Édition d'une frame intermédiaire propagée intelligemment |

## 4. Animation avancée

| Item | État | Détail |
|---|---|---|
| Rotoscopie (référence vidéo/image) | ✅ | `reference-bridge.js` |
| Rigging (os, IK, poids) | ✅ | Outil Rig : os au Pen, rotation par tangente, IK, poids par vertex |
| Parentage de calques | ✅ | Pickwhip, uid stable, **blend entre deux parents** (au-delà d'AE) |
| Parent in Time | ✅ | Décalages entrée/sortie liables par expression |
| Caméra virtuelle | ✅ | Keyframée, courbes d'ease, propre à chaque Component |
| Calques 3D | ✅ | Projection par vertex façon Grease Pencil (l'épaisseur ne s'aplatit pas) |
| Lumières 3D | ❌ | Sans objet tant que la 3D reste de la projection vectorielle |
| Duplicateur mograph | ✅ | Grille/radial/chemin, stagger, effectors sur n'importe quelle propriété, 3D |
| Expressions | 🟡 | Moteur réel (globales, pickwhip, erreurs) — **vocabulaire en cours d'enrichissement** |
| Contrôles d'expression (slider, etc.) | ❌ | Le manque n°1 face à AE — brique du rig réutilisable |
| Animateur de texte | 🟡 | Par lettre/mot/ligne, presets, stagger — pas les sélecteurs de plage d'AE |
| Repères, règles, zones de sécurité | ✅ | `rulers-bridge.js`, guides monde persistés |
| Symétrie / perspective | ✅ | Miroir, radial, points de fuite |

## 5. Compositing

| Item | État | Détail |
|---|---|---|
| Modes de fusion | ✅ | |
| Caches (track mattes) | ✅ | Alpha, Alpha inv., Luma, Luma inv. — parité AE |
| Masques vectoriels | 🟡 | Add/Subtract + contour progressif ; pas les sélecteurs de masque d'AE |
| Effets empilables par calque | ✅ | Réordonnables, activables |
| Effets par forme | ✅ | |
| Effets shader WGSL personnalisés | ✅ | |
| Calques d'effet (adjustment) | ✅ | |
| Flou de mouvement | ✅ | Global et par calque |
| Components (precomp) | 🟡 | Entrée par double-clic, caméra propre, time remap — pas de « collapse transformations » |

## 6. Import / Export

| Item | État | Détail |
|---|---|---|
| Import SVG | ✅ | Vrais vecteurs éditables (transforms imbriqués aplatis, trous fusionnés) |
| Import Figma | ✅ | Frames, texte vectoriel éditable, images ; polices via Google Fonts |
| Import PSD | ✅ | `psd-import-bridge.js` |
| Import images / séquences | ✅ | |
| Import vidéo | ✅ | Décodeur natif indexé + média optimisé |
| Import audio | ✅ | Pistes multiples, waveform, décalage/volume/mute |
| Import AI / EPS | ❌ | |
| Export MP4 / séquences PNG | ✅ | H.264 via VideoToolbox (sans lib brevetée) |
| Export ProRes / WebM | ✅ | |
| Export Lottie / SVG animé | ✅ | |
| Export Rive / AE | ✅ | |
| Export OCA | 🟡 | Prototype Labs |
| Spritesheets | ❌ | |
| Render Manager (file de rendus) | ✅ | N rendus, réglages par élément, index dynamique |
| Format projet JSON | ✅ | Auto-suffisant par défaut, **ou médias liés** (réglage projet) |

## 7. Technique

| Item | État | Détail |
|---|---|---|
| Moteur de rendu Rust/WebGPU | ✅ | vello, sans état, JSON→JSON (`geometry-wasm/`) |
| Calculs lourds en Rust | ✅ | Résample, matching de tween, booléennes, gomme (avec repli JS) |
| Géométrie retenue côté moteur | ✅ | `register_path`/`pathRef` — jusqu'à 7× sur la sérialisation de scène |
| Cache d'images borné (LRU) | ✅ | Budget mémoire réglable, éviction pilotée par JS |
| Rendu incrémental / réutilisation | ✅ | `loadFrame` ne reconstruit plus ce qui n'a pas changé (10×) |
| Encodage vidéo FFmpeg | ✅ | Sidecar LGPL (sans GPL ni brevets x264/x265) |
| Web Workers | ❌ | Tout sur le thread principal (le GPU absorbe la charge) |
| Auto-save | ✅ | 30 s + historique de projet |
| Undo/redo | ✅ | Snapshot complet, garde anti-flood sur les drags |
| Compression du fichier projet | ❌ | JSON brut ; les médias liés répondent au même besoin autrement |
| Build web | ✅ | Garde WebGPU, autosave IndexedDB, Worker feedback |

## 8. Interface & collaboration

| Item | État | Détail |
|---|---|---|
| Panneaux détachables | ✅ | |
| Panneau Médias & Presets | ✅ | Catalogue d'imports, presets d'animation, compositions ouvertes |
| Raccourcis personnalisables | ❌ | Raccourcis fixes |
| Préréglages d'interface | ❌ | |
| Mode aperçu (sans guides/onion) | ❌ | |
| Qualité de preview réglable (draft) | ❌ | |
| Zoom tactile (pinch) | ❌ | |
| Thème clair | ❌ | Sombre uniquement, par choix |
| i18n | ✅ | FR / EN / JA / ES |
| Commentaires & annotations | ✅ | Outil Comment, avatars, feedback beta-testeurs vers GitHub |
| Sync / revue d'équipe | ✅ | Dossier de sync, fusion de snapshots, ghosts de révision |
| Tutoriel interactif | ✅ | 25 modules, 4 langues |
| API de scripting | ✅ | `nemo.*`, panneaux et plugins HTML |
| Labs (prototypes) | ✅ | 35 prototypes, éteints par défaut, un seul hook en prod |

---

## Les vrais trous, par ordre d'impact

1. **Contrôles d'expression** (slider/case/couleur posés sur un calque) — sans eux, pas de rig
   réutilisable. Le chantier expressions en cours est le préalable.
2. **Opérateurs de chemin** (offset, zigzag, twist, arrondi, fusion) — pur vectoriel, sans
   toucher au moteur ; c'est ce qui rend les shape layers d'AE et de Cavalry expressifs.
3. **Motion path éditable sur le canvas** — débloque au passage les roving keyframes.
4. **Conversion de points anguleux/lisse** — petite lacune de l'outil Plume, très visible à l'usage.
5. **Raccourcis personnalisables** — attendu de tout logiciel pro à ce stade de maturité.

Hors classement, à trancher côté produit plutôt qu'à estimer : graphe nodal (paradigme
Cavalry), données externes CSV/JSON (dataviz), lumières 3D.
