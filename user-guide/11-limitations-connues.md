# 11. Ce qui n'est pas encore disponible

Nemo évolue vite — cette page liste des manques confirmés au moment de la rédaction
(juillet 2026). Le développement va plus vite que cette documentation ne peut le suivre : si
vous cherchez une fonctionnalité listée ici et qu'elle semble pourtant exister dans
l'interface, faites confiance à l'interface, pas à cette page, et signalez l'écart pour qu'on
la corrige.

## Confirmé absent

- **Motion tween classique dédié** — animer un objet entier (position/rotation/échelle) par
  trajectoire éditable indépendamment du morphing de forme. Le moteur actuel excelle au
  morphing de forme (shape tween) ; le tween de transformation pure passe par le mode
  [Motion](06-motion.md), pas par un outil de trajectoire dédié sur le canevas.
- **Dossiers de calques** — pas de repli/groupement hiérarchique de calques dans le panneau
  Calques, seulement une liste plate.
- **Rigging** — pas de pivots/hiérarchie parent-enfant/sliders pour de l'animation à la
  marionnette.
- **Préréglages d'interface** et **raccourcis clavier personnalisables**.
- **Import SVG/AI/EPS**, **import PSD**, **spritesheets** (en import — l'export spritesheet
  n'existe pas non plus).
- **Partage de bibliothèque de médias entre projets**.
- **Zoom tactile pinch/rotation à deux doigts**.

## Partiel

- **Instances de Component** — vitesse et offset de lecture réglables par instance, mais pas
  encore d'override de couleur ou de forme.
- **Motion, niveau sous-élément** — entrer dans un calque affiche une sous-ligne par élément,
  mais seules les 5 propriétés de base (position/ancrage/rotation/échelle/opacité) y sont
  exposées pour l'instant, pas encore le path/fill/stroke/brush par sous-élément décrits comme
  prochaine étape dans la feuille de route interne.

## À vérifier avant de documenter plus en détail

Certaines fonctionnalités listées comme absentes dans une note technique plus ancienne
(`docs/FUNCTIONAL_GAPS.md`) existent en réalité déjà dans l'interface au moment de la
rédaction — entre autres : opérations booléennes (union/soustraction), import d'images et de
vidéos, référence vidéo pour la rotoscopie, zones de sécurité (safety zones). Cette page a été
corrigée en conséquence, mais si un doute persiste sur une fonctionnalité précise, le plus
fiable est de l'essayer directement dans Nemo plutôt que de se fier à un document de suivi
interne qui peut être en retard sur le code.
