# Nemo — Guide utilisateur

Documentation d'aide utilisateur pour **Nemo**, l'outil d'animation vectorielle frame-by-frame
avec inbetweening automatique.

> **Statut de ce guide (juillet 2026)** : première passe, à faire évoluer au fil des versions.
> Chaque page reflète l'état réel du logiciel (voir [`docs/FUNCTIONAL_GAPS.md`](../docs/FUNCTIONAL_GAPS.md)
> pour la matrice complète fonctionnalité par fonctionnalité). Les captures d'écran sont en
> cours de complétion — voir la note en bas de page. Aucun GIF pour l'instant : à ajouter dans
> une prochaine itération (capture d'écran animée à faire manuellement, l'environnement qui a
> écrit cette première version n'a pas accès à l'enregistrement d'écran).

## Sommaire

1. [Démarrage — créer, ouvrir, reprendre un projet](01-demarrage.md)
2. [L'interface — StoryBoard, Animation 2D, Motion](02-interface.md)
3. [Dessiner — pinceau, plume, formes, gomme, remplissage](03-dessin.md)
4. [Calques et timeline](04-calques-timeline.md)
5. [Interpolation automatique (inbetweening) et onion skin](05-interpolation.md)
6. [Mode Motion — animer comme dans After Effects](06-motion.md)
7. [Components et StoryBoard — montage nodal](07-storyboard-composants.md)
8. [Caméra, audio et bibliothèque de médias](08-camera-audio-medias.md)
9. [Exporter et importer un projet](09-export-import.md)
10. [Raccourcis clavier](10-raccourcis.md)
11. [Ce qui n'est pas encore disponible](11-limitations-connues.md)

## À qui s'adresse ce guide

Aux animateur·rice·s et illustrateur·rice·s qui utilisent Nemo au quotidien — pas aux
développeur·se·s du logiciel (pour ça, voir [`CLAUDE.md`](../CLAUDE.md) à la racine du
projet, qui documente l'architecture technique et les pièges déjà rencontrés).

## Convention de ce guide

- Les raccourcis clavier sont donnés entre parenthèses, ex. **Pinceau (B)**.
- Les captures d'écran vivent dans [`assets/`](assets/), nommées `NN-nom-descriptif.jpg`.
- Ce qui n'existe pas encore dans Nemo est explicitement marqué **❌ Pas encore disponible**
  plutôt que passé sous silence — pour éviter de chercher un bouton qui n'existe pas.
