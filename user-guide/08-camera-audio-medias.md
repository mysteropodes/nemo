# 8. Caméra, audio, référence et médiathèque

## Calque caméra

Le bouton **Calque caméra** (panneau Calques) ajoute un calque spécial dont les clés pilotent le
cadrage (zoom/pan) de la scène entière, interpolées avec des courbes de Bézier — comme dans
TVPaint ou Callipeg. La caméra a son propre éditeur de courbe d'accélération, accessible via
**Modifier la courbe d'accélération…**, pointé directement sur le segment qui encadre la frame
courante.

À l'intérieur d'un Component, la caméra a sa propre timeline : le mouvement voyage avec
l'instance partout où elle est placée (voir [Components et StoryBoard](07-storyboard-composants.md)).

> Ceci est une caméra 2D (zoom/pan/rotation de la vue) — pas une caméra 3D avec perspective/profondeur.

## Référence vidéo (rotoscopie)

Le bouton **Importer…** du panneau Référence permet d'importer une vidéo, une séquence
d'images (sélection multiple), ou une image seule comme calque de référence pour de la
rotoscopie — dessiner par-dessus une source filmée. Un réglage **Offset** décale la frame de la
timeline à laquelle la référence démarre, pour la resynchroniser avec l'animation en cours.

## Audio

Le bouton **piste audio** (panneau Calques, icône note de musique) importe un fichier mp3/ogg/
wav. La piste apparaît en waveform sous la timeline et se lit en synchronisation avec la
lecture de l'animation.

> ❌ **Pas encore disponible** : marqueurs audio dédiés, synchronisation labiale automatique.

## Médiathèque

Le panneau "Médias" (à droite) liste tous les fichiers importés dans le projet (images et
vidéos) avec leur vignette, sous forme de catalogue consultable — pratique pour retrouver un
asset déjà importé sans rouvrir le sélecteur de fichiers. La médiathèque n'importe rien
elle-même : elle référence ce qui a déjà été déposé via **Import Image(s)…** ou
**Import Video…** (voir [Dessiner](03-dessin.md)).

> ❌ **Pas encore disponible** : partage de bibliothèque entre projets.
