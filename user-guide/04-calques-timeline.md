# 4. Calques et timeline

La bande **Scene** en bas de l'écran est la timeline : la liste des calques à gauche, la grille
de frames à droite.

## Calques

En bas du panneau "Layers" :

| Bouton | Rôle |
|---|---|
| **+** (New layer) | Ajoute un calque vide |
| Caméra | Ajoute un **calque caméra** — voir [Caméra, audio et médias](08-camera-audio-medias.md) |
| Piste audio | Importe un fichier audio (mp3/ogg/wav) |
| Corbeille | Supprime le calque actif |
| Dupliquer | Duplique le calque actif |
| ◈ (Convert to component) | Convertit le calque en Component — voir [Components et StoryBoard](07-storyboard-composants.md) |

Chaque calque a une pastille de couleur, un œil (visibilité), un cadenas (verrouillage), et un
bouton **S** (Solo — isoler ce calque).

## Frames et keyframes

Nemo distingue trois types de contenu de frame, avec des raccourcis directs sur la timeline :

| Raccourci | Action |
|---|---|
| **F5** | Insère une frame simple (répète le dessin précédent) |
| **F6** | Insère une keyframe (un dessin explicite, point d'ancrage pour l'interpolation) |
| **F7** | Insère une frame vide |
| **D** | Duplique la frame (ou la plage sélectionnée) |
| **T** | Lance le tween (interpolation automatique) entre deux keyframes — voir [chapitre 5](05-interpolation.md) |
| **F** | Flip (retournement) |
| **X** | Miroir |
| **Entrée** | Lecture / Stop |

Un clic-droit sur la timeline donne accès aux mêmes opérations, plus la conversion
Keyframe→Frame et Tween→Keyframes (matérialiser les frames générées automatiquement en
keyframes indépendantes, éditables une par une).

Les frames et plages se glissent-déposent directement sur la timeline pour les réorganiser.

## Lecture

- **Aller à la première/dernière frame**, **frame précédente/suivante**, **Play/Stop (Entrée)**.
- **FPS** et **Frames** (durée totale) réglables directement dans la barre de lecture.
- **Boucle** — relit en boucle la zone de travail (in/out) ; clic-droit pour un aller-retour
  (ping-pong) plutôt qu'une boucle simple.
- **Cycle** — répète N fois la plage de frames sélectionnée (pratique pour un cycle de marche).

## Ghost / vue de révision

- **Ghost all keyframes** — affiche toutes les keyframes du calque actif superposées, comme un
  feuilletage complet plutôt que juste avant/après.
- **Ghost select** — sélectionne le contenu de toutes les keyframes pour les transformer
  ensemble.
- **Vue de révision** — bascule cycliquement Tout / Mes traits / Corrections — utile en contexte
  d'équipe pour distinguer qui a dessiné quoi.
