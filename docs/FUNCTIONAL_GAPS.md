# StrokeMotion — État fonctionnel vs cahier des charges

Dernière mise à jour : voir historique git. Légende : ✅ Fait · 🟡 Partiel · ❌ Manquant

## 1. Moteur de dessin vectoriel (Brush Engine)

| Item | État | Détail |
|---|---|---|
| Pinceau vectoriel standard (Bézier + lissage) | ✅ | `tools.js` — draw tool, `state.smoothing`/`stabilizer` |
| Pinceau de pression (tablette) | ✅ | Pointer Events + `webkitForce` + canal natif macOS (`src-tauri/src/lib.rs`), P.min/P.max, inversion |
| Pinceau calligraphique (angle configurable) | ❌ | — |
| Pinceau texturé | ❌ | — |
| Gomme vectorielle (découpe précise de chemin) | 🟡 | Supprime des traits entiers, ne découpe pas au point de clic |
| Sélection directe (points d'ancrage + poignées) | ✅ | Outil Sous-sélection (A), multi-point marquee |
| Lasso vectoriel | ❌ | Seule la marquee rectangulaire existe |
| Plume point par point | ✅ | `_pen` dans `tools.js` |
| Conversion de points (anguleux/lisse/symétrique) | ❌ | — |
| Fusion de chemins (union/soustraction/intersection/exclusion) | ❌ | Paper.js le supporte, rien n'est exposé en UI |
| Remplissages : uni / dégradé / motifs | 🟡 | Uni ✅, dégradé ❌, motifs ❌ |
| Contours : épaisseur/pointillés/caps custom | ✅ | Panneau Stroke & Fill |
| Palette personnalisée (swatches RVB/HSV/CMJN) | ❌ | Color picker natif seulement |
| Pipette | ✅ | Outil Eyedropper |

## 2. Timeline et séquençage

| Item | État | Détail |
|---|---|---|
| Calques multiples, repli/dépli | 🟡 | Multi-calques ✅, pas de dossiers/repli |
| Dossiers de calques | ❌ | — |
| Keyframe / inbetween / image vide | ✅ | |
| "Pose" (keyframe marquée spécial) | ❌ | — |
| Curseur, raccourcis, boucle in/out | ✅ | |
| Vitesse de lecture (FPS réglable) | ✅ | |
| Insertion/suppression frames/keyframes/blank | ✅ | F5/F6/F7 + clic-droit |
| Conversion Keyframe→Frame, Tween→Keyframes | ✅ | |
| Duplication frame/plage | ✅ | |
| Drag & drop frames/plages | ✅ | |

## 3. Interpolation automatique

| Item | État | Détail |
|---|---|---|
| Motion tween (position/rotation/échelle/inclinaison, objet entier) | ❌ | Le moteur ne fait que du morphing de forme, pas de tween de transformation pure |
| Trajectoires éditables (motion path) | ❌ | — |
| Shape tween (morphing de formes) | ✅ | Moteur avancé : proximité/courbure/lignes de force, `tweens.js` |
| Classic tween (symboles/instances) | 🟡 | Composants jouent leur mini-timeline, pas de tween de transformation dédié |
| Éditeur de courbes | ✅ | `ui.js` curve editor |
| Linéaire / ease in-out / bézier custom | ✅ | |
| Rebond, élastique | 🟡 | "Over" (overshoot) seulement |
| Graphique de vitesse (accélération) | ❌ | — |
| Onion skin complet | ✅ | Modes tinted/outline, plage réglable, marqueurs qui suivent le curseur |
| Inbetweening auto + ajustement + preview live | ✅ | Matching géométrique + lignes de force + découpage N:1 |

## 4. Outils d'animation avancés

| Item | État |
|---|---|
| Rotoscopie (import vidéo, calque de référence) | ❌ |
| Rigging (pivots, hiérarchie parent/enfant, sliders) | ❌ |
| Caméra virtuelle (zoom/pan/rotation de scène) | ❌ (zoom/pan de vue d'édition seulement) |
| Guides de mise en page (règles, grilles) | ❌ |
| Zones de sécurité | ❌ |

## 5. Interface et workflow

| Item | État | Détail |
|---|---|---|
| Panneaux détachables | ✅ | Drag header → flottant, clampé à la fenêtre |
| Préréglages d'interface | ❌ | — |
| Raccourcis personnalisables | ❌ | Raccourcis fixes |
| Bibliothèque de symboles | 🟡 | Composants + mini-timeline ✅, pas de panneau bibliothèque dédié |
| Instances avec overrides | 🟡 | Vitesse/mode de lecture par instance, pas d'override couleur/forme |
| Import SVG/PNG séquences/spritesheets | 🟡 | Export seulement |
| Preview qualité ajustable (draft) | ❌ | — |
| Mode aperçu sans guides/onion | ❌ | — |
| Export rapide GIF/MP4 review | ✅ | |

## 6. Import/Export

| Item | État |
|---|---|
| Import SVG/AI/EPS | ❌ |
| Import PNG/JPG/PSD | ❌ |
| Import vidéo/audio | ❌ |
| Export MP4, séquences PNG/JPG | ✅ |
| Export WebM, MOV alpha | ❌ (ProRes existe) |
| SVG animé, Lottie | ✅ |
| Spritesheets | ❌ |
| JSON projet | ✅ |

## 7. Spécificités techniques (Tauri)

| Item | État | Détail |
|---|---|---|
| Frontend Canvas | ✅ | Paper.js |
| Rendu vectoriel Rust (skia-safe/resvg) | ❌ | Tout le rendu est encore JS |
| Calculs lourds en Rust | ❌ | Seul le moniteur de pression tablette est natif |
| Encodage vidéo FFmpeg | ✅ | Sidecar ffmpeg |
| Rendu incrémental | ❌ | — |
| Web Workers | ❌ | Tout sur le thread principal |
| Cache vectoriel | ❌ | — |
| GPU/WebGL preview | ❌ | (pipeline GPU/WGSL existe pour shaders custom, usage différent) |
| Format projet JSON/SQLite | 🟡 | JSON seulement |
| Auto-save | ✅ | Toutes les 30s |
| Undo/redo illimité structurel | ✅ | Snapshot complet du projet, corrigé cette session |
| Compression | ❌ | — |

## 8. Complémentaire

| Item | État |
|---|---|
| Timeline audio, markers, sync labiale | ❌ |
| Commentaires/annotations | ❌ |
| Partage de bibliothèque entre projets | ❌ |
| Support tablette latence <10ms | ✅ |
| Zoom tactile pinch/rotation 2 doigts | ❌ |
| Mode sombre | ✅ (par défaut, pas de bascule clair) |

---

## Priorisation

- **Phase 1 (MVP)** : quasi complète — manque mode draft/aperçu et dossiers de calques.
- **Phase 2 (Interpolation)** : shape tween très avancé (au-delà du brief), mais **motion tween classique (transformer un objet entier, trajectoire éditable) absent** — trou le plus structurant.
- **Phase 3 (Pro)** : quasiment tout manque (rotoscopie, caméra, motion graphs avancés, audio) sauf Lottie/SVG.
- **Phase 4 (Polish)** : rien fait, prévu en dernier.

## En cours

- Optimisation des outils existants de la section 1 (gomme vectorielle précise, conversion de points, lasso, boolean ops, dégradés) — voir commits suivants.
