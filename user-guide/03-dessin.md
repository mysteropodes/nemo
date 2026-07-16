# 3. Dessiner

## Outils de sélection

| Outil | Raccourci | Rôle |
|---|---|---|
| Sélection | **V** | Sélectionner/déplacer/redimensionner/pivoter des formes entières |
| Sous-sélection | **A** | Éditer les points d'ancrage et tangentes d'un trait (comme l'outil Plume blanche d'Illustrator) |
| Sélection Fond/Trait | **M** | Cliquer un fond ou un contour pour le sélectionner indépendamment du reste de la forme |

## Outils de dessin

| Outil | Raccourci | Rôle |
|---|---|---|
| Pinceau | **B** | Trait vectoriel lissé, sensible à la pression de la tablette |
| Plume | **P** | Points par points, clic pour un angle, glisser pour une courbe |
| Pinceau de remplissage | **N** | Peint une forme pleine à main levée, sensible à la pression |
| Ligne | **U** | Segment droit |
| Rectangle | **R** | — |
| Ellipse | **L** | — |
| Texte | — | Clic sur le canevas pour poser un bloc de texte (rendu comme une image) |
| Gomme | **E** | Efface le trait sous le curseur |
| Pot de peinture | **G** | Clic dans une zone fermée pour la remplir |
| Pipette | **I** | Prélève la couleur sous le curseur |
| Commentaire | **C** | Épingle une note sur le canevas (feedback interne à l'équipe — voir plus bas) |
| Main | **H** ou barre d'espace | Déplacer la vue |
| Zoom | **Z** | Clic pour zoomer, **Alt**+clic pour dézoomer |
| Rotation du canevas | — | Glisser pour faire pivoter la vue autour de son centre ; fonctionne aussi en **Alt**+glisser depuis n'importe quel autre outil |
| Perspective | — | Place des points de fuite ; une fois activé, l'outil Ligne s'aimante dessus, quel que soit l'outil actif |

**X** échange les couleurs de trait et de fond.

## Panneau "Draw — Options"

Le panneau de droite change selon l'outil actif. Pour le Pinceau/la Plume/les formes :

- **Fond** — couleur et opacité, avec un bouton pour propager la couleur choisie à toutes ses
  occurrences sur toutes les frames.
- **Trait** — couleur, épaisseur (pilote aussi la taille des tampons du Bitmap Brush), style
  (plein/tireté/pointillé), cap (rond/carré/plat), jointure (rond/onglet/biseau), limite de
  l'onglet.
- **Fill in front of Stroke / Stroke in front of Fill** — ordre d'empilement fond/contour.

## Textures de pinceau

Deux systèmes de texture coexistent, à ne pas confondre :

- **Textures vectorielles** (presets de brush) — plusieurs copies décalées et semi-transparentes
  du même trait, superposées pour simuler un grain sans jamais devenir une image. Reste 100%
  vectoriel, donc réexportable proprement (Rive, SVG…).
- **Bitmap Brush** (case à cocher dans les options du Pinceau/Plume) — remplace le rendu par un
  tamponnage bitmap réel (vrai grain image). La géométrie captée par l'outil de dessin ne change
  pas ; seule la sortie devient une image. Les presets vectoriels au-dessus restent disponibles
  en parallèle (utiles pour un export qui a besoin de vrais traits vectoriels, comme Rive).
- **Import .abr tip…** — importer une pointe de pinceau Photoshop (`.abr`) comme tampon bitmap.

## Mode Shadow

Une case à cocher "Shadow" (Pinceau/Plume/Formes) tague ce que vous dessinez comme de l'ombre —
utile pour la technique de layout façon anime japonais (remplissages posés d'abord, puis lignes
de délimitation d'ombre approximatives par-dessus). Séparer Trait/Fond/Ombre plus tard route
automatiquement tout ce qui a été dessiné avec cette option activée vers son propre calque
Ombre au lieu de Trait/Fond.

## Opérations booléennes

Dans le panneau de sélection, une fois plusieurs formes sélectionnées : **Union**, **Soustraction**,
Intersection, Exclusion — pour fusionner ou découper des formes entre elles.

## Import bitmap

- **Import Image(s)…** — importer une ou plusieurs images. Une séquence numérotée
  (`frame001.png`, `frame002.png`…) est détectée automatiquement et devient un calque animé, une
  image par frame.
- **Import Video…** — une vidéo est décodée en une image par frame à la cadence du projet et
  déposée sur un nouveau calque animé, exactement comme une séquence d'images.

## Grille, aides au dessin

- **Grille** — affiche une grille de repère sur le canevas.
- **Miroir vertical** / **Miroir radial (mandala)** — dessine en symétrie miroir ou en rosace.
- **Trait prédictif** — lissage/prédiction de trait pendant le dessin.
- **Dessin multi-frames** — dessiner en même temps sur plusieurs frames.
- **Rouleau d'animateur** (maintenir **R**) / **Miroir de contrôle** (maintenir **M**) — aides
  spécifiques à l'animation traditionnelle.
- **Menu radial d'outils** (maintenir **Q**) — accès rapide aux outils sans quitter le canevas.

## Commentaires (feedback d'équipe)

L'outil **Commentaire (C)** épingle une note sur le canevas, visible par toute l'équipe qui
partage le projet (revue de plan classique — distinct du système de feedback de debug destiné
aux développeurs).
