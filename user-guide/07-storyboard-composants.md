# 7. Components et StoryBoard

## Components

Un **Component** est un dessin/animation réutilisable, avec sa propre mini-timeline —
l'équivalent d'un symbole/instance dans d'autres outils (Symbol Flash, Precomp After Effects).

**Comment un calque devient un Component :**

- Manuellement — bouton **◈ (Convert layer to component)** en bas du panneau Calques.
- Automatiquement en **Motion**, dès qu'une propriété de niveau calque reçoit sa première clé
  (voir [chapitre 6](06-motion.md)).
- Automatiquement en **StoryBoard**, dès qu'un calque est connecté à un bloc "Edit module" du
  montage (voir plus bas).

**Décomposer en calque** (bouton dans le panneau Component) fait l'inverse : ça "cuit"
l'animation du Component dans un calque normal, perdant la réutilisabilité mais redonnant un
accès direct au dessin brut.

## Instances

Une fois un Component créé, chaque copie posée sur la scène est une **instance** :

- **Offset** — la frame de la timeline principale à laquelle la timeline propre de cette
  instance démarre (permet de décaler le jeu d'une copie par rapport à une autre).
- Vitesse et mode de lecture réglables indépendamment par instance.
- Une caméra ajoutée à l'intérieur d'un Component a sa propre timeline caméra, qui voyage avec
  l'instance partout où elle est utilisée.

## StoryBoard

Le mode **StoryBoard** (onglet en haut) est un montage nodal : il enchaîne des **instances de
Components** en une séquence, avec le son.

> **StoryBoard ne manipule que des Components.** Un calque plat (jamais converti) ne peut pas
> être placé directement dans un montage StoryBoard — il doit d'abord devenir un Component (soit
> manuellement, soit automatiquement en le connectant à un bloc "Edit module" du graphe).

Utile pour :

- Construire l'enchaînement des plans d'une séquence à partir de Components déjà animés.
- Ajouter du son synchronisé au montage.
- Visualiser un aperçu en direct de chaque Component en survolant sa carte (l'aperçu revient à
  la vignette statique dès que le curseur quitte la carte, pour ne pas tout recalculer en
  continu).

## Vue d'ensemble : comment les 3 modes communiquent

- **Animation 2D** — dessin brut, calque plat.
- **Motion** — dès qu'une propriété de calque est animée, le calque devient un Component ;
  Motion anime ce Component (position/rotation/échelle/opacité, et bientôt plus finement par
  sous-élément).
- **StoryBoard** — assemble des instances de ces Components en séquence.

Un même Component est donc lu et modifié de façon cohérente par les trois vues — l'animer en
Motion, le voir jouer en aperçu StoryBoard, le retoucher au trait en Animation 2D en double-
cliquant dessus.
