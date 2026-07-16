# 1. Démarrage

## Écran d'accueil

Au lancement, Nemo propose trois façons de commencer (capture à ajouter — voir la note en bas
du [sommaire](README.md)) :

- **New Project** — choisir la taille du canevas et le nombre d'images par seconde (FPS), puis
  cliquer **Create**.
- **Open Project…** — parcourir le disque pour ouvrir un fichier `.json` de projet Nemo existant.
- **Resume Last Session** — n'apparaît que s'il existe une sauvegarde automatique récente (voir
  plus bas) ; restaure l'état exact où vous vous étiez arrêté·e.
- **Ouvrir depuis Kitsu…** — pour les studios connectés à une production Kitsu (voir
  [Kitsu](#ouvrir-depuis-kitsu) plus bas).
- **Recent Projects** — liste des derniers fichiers ouverts, cliquables directement.

### Nouveau projet

Dans le panneau **New Project** :

| Champ | Description |
|---|---|
| Name | Nom du projet |
| Preset | Formats courants : 1920×1080 (Full HD), 1280×720 (HD), 1080×1080 (carré), 1080×1920 (portrait), ou **Custom…** pour une taille libre |
| FPS | 12 / 24 / 25 / 30 / 60 images par seconde |

Un nouveau projet démarre avec **1 calque** et **5 secondes** de timeline (à la cadence choisie).

## Sauvegarde automatique

Nemo sauvegarde automatiquement votre travail **toutes les 30 secondes** dans un cache local.
Si l'application se ferme de façon inattendue, l'écran d'accueil affiche la carte
**Resume Last Session** au prochain lancement — cliquer dessus restaure l'état exact (calques,
frames, dessins) sans rien perdre.

Cette sauvegarde automatique est un filet de sécurité, **pas un remplacement** de l'enregistrement
manuel (**⌘S**) : pensez à sauvegarder votre fichier `.json` régulièrement, surtout avant de
fermer le projet ou de changer de version.

## Ouvrir depuis Kitsu

Pour les studios utilisant [Kitsu](https://kitsu.cg-wire.com/) comme outil de suivi de
production : **Ouvrir depuis Kitsu…** permet de se connecter à une instance de production et de
récupérer une entité (plan, asset) directement, sans passer par un fichier local. La synchronisation
est **asynchrone** (pas de collaboration temps réel) — un peu comme un "check-out" : vous
récupérez l'état au moment de l'ouverture, et vous republiez votre travail en sauvegardant.

## Onglets multiples

Le bouton **Nouveau projet (nouvel onglet)** dans la barre d'outils du haut ouvre un second
projet vierge dans un nouvel onglet, sans fermer celui en cours — pratique pour travailler sur
plusieurs plans en parallèle sans changer de fenêtre.
