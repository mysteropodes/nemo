# 9. Exporter et importer

## Formats d'export

Le panneau Export (menu principal) propose :

| Format | Détail |
|---|---|
| Séquence PNG | Une image par frame |
| Séquence TIFF | Une image par frame |
| GIF | Animation GIF |
| MP4 (H.264) | Vidéo, via le sidecar ffmpeg embarqué |
| ProRes (.mov) | Vidéo qualité montage |
| Lottie JSON | Animation "cuite" — chaque pose déjà résolue par le moteur de tween est écrite comme keyframe à timing linéaire, donc n'importe quel lecteur Lottie reproduit exactement ce qu'affiche Nemo |
| Rive (via MCP) | Nécessite Rive Editor ouvert en local avec le serveur MCP actif (port 9791) |
| Caméra → After Effects (.jsx) | Génère un script à lancer dans After Effects (File > Scripts > Run Script File…) — nécessite un calque caméra actif |

Réglages communs :

- **Plage** — zone de travail (in/out) ou toute la timeline.
- **Échelle** — le contenu étant vectoriel, l'agrandissement reste net à n'importe quelle échelle
  (0.5x à 4x, ou une taille personnalisée en pixels).
- **Fond transparent (alpha)** — remplace la couleur de fond du document par de la transparence ;
  sans effet sur les formats qui ne supportent pas l'alpha (MP4, GIF).

> Les exports vidéo/image bitmap (PNG/TIFF/GIF/MP4/ProRes) nécessitent l'application Tauri
> (accès disque + ffmpeg). Lottie JSON et la séquence SVG fonctionnent aussi en preview
> navigateur pur.

## Sauvegarde du projet

**⌘S** (Save) écrit dans le fichier actuel, ou ouvre **Save As…** s'il n'y en a pas encore. Le
format de projet est du **JSON**. Voir aussi [Démarrage — sauvegarde automatique](01-demarrage.md#sauvegarde-automatique).

## Publier vers Kitsu

**Publier vers Kitsu** (panneau Export, pour les projets ouverts depuis Kitsu) rend un MP4,
l'envoie sur la tâche du shot correspondant et passe son statut en révision — un aller-retour
complet sans quitter Nemo.

## Import

- **Import Image(s)…** / **Import Video…** — voir [Dessiner](03-dessin.md#import-bitmap).
- **Importer…** (référence) — vidéo/séquence/image comme calque de rotoscopie, voir
  [Caméra, audio et médias](08-camera-audio-medias.md#référence-vidéo-rotoscopie).
- **Import .abr tip…** — pointe de pinceau Photoshop, voir
  [Dessiner](03-dessin.md#textures-de-pinceau).
- **Open Project…** — fichier `.json` de projet Nemo, voir [Démarrage](01-demarrage.md).

> ❌ **Pas encore disponible** : import SVG/AI/EPS, import PSD, import de spritesheets.
