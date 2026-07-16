# 5. Interpolation automatique et onion skin

## Tween (inbetweening automatique)

Posez deux keyframes (**F6**) avec un dessin différent, placez le curseur entre les deux, et
appuyez sur **T** (ou le bouton Tween du panneau de droite). Nemo génère automatiquement les
frames intermédiaires en faisant correspondre chaque trait de la keyframe de départ à son
équivalent sur la keyframe d'arrivée (par proximité, courbure et lignes de force), puis en
interpolant leur forme.

Options du panneau Tween :

- **Skip manually-edited frames** — les frames affichées en vert dans la timeline ont été
  corrigées à la main après un tween ; cochée (par défaut), l'option préserve ces corrections
  quand vous relancez un tween sur le même calque.
- Sélectionner une ou plusieurs keyframes sur la timeline avant de lancer Tween ne régénère que
  leur portion ; sans sélection, tout le calque est retweené.
- **Réattribuer un inbetween…** — quand l'appariement automatique se trompe (ex. il fait
  correspondre une bouche à un nez), placez le curseur sur une keyframe qui a un inbetween vers
  la suivante, cliquez l'élément de départ puis son équivalent réel sur la keyframe suivante :
  ça force la correspondance au lieu de la détection automatique.

## Éditeur de courbes (easing)

Le tween n'est pas forcément linéaire — un éditeur de courbes permet de régler l'accélération
entre deux keyframes : linéaire, ease in/out, bézier personnalisée, avec un effet "over"
(dépassement/rebond).

## Onion skin

Les boutons de la timeline (à côté des contrôles de lecture) :

| Bouton | Rôle |
|---|---|
| **Onion skin on/off (O)** | Affiche en transparence les frames voisines de la frame courante |
| **Onion skin outlines only** | N'affiche que les contours des frames voisines (moins de bruit visuel) |
| **Modify onion markers** | Ouvre les réglages détaillés (plage, mode teinté/contour) |

Deux petits marqueurs (**onion marker start/end**) apparaissent sur la timeline et suivent le
curseur — ils délimitent la plage de frames visibles en fantôme, réglable à la volée en les
faisant glisser.

## Cycle (walk cycles)

Le bouton **Cycle** répète N fois la plage de frames sélectionnée sur la timeline — pratique pour
prévisualiser un cycle de marche ou tout autre mouvement en boucle sans dupliquer les frames pour
de vrai.
