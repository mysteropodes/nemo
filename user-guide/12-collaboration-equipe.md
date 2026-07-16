# 12. Travailler à plusieurs

Nemo propose plusieurs façons de collaborer sur un même projet, toutes **asynchrones** (pas de
temps réel façon Google Docs) : un dossier partagé, un système de correction/révision inspiré des
studios traditionnels, et un modèle de contacts pour du futur partage direct. Tout ça se règle
dans **Réglages → Collaboration** (voir aussi [Paramètres de l'app](13-parametres-app.md)).

## Profil

Dans **Réglages → Général**, chacun renseigne un **nom** et une **couleur** qui identifient ses
traits — utile dès qu'un superviseur ou un autre animateur corrige votre travail : ses
modifications restent visuellement distinctes des vôtres. Un **rôle** (Animateur / Superviseur /
Producteur) complète le profil.

## Sync équipe (dossier partagé)

Dans **Réglages → Collaboration**, choisissez un dossier partagé quelconque (kDrive, Google
Drive monté, S3 synchronisé…) :

1. **Choisir…** — sélectionne le dossier partagé.
2. **Publier** — écrit un instantané (snapshot) de votre travail dans ce dossier, sous votre
   profil.
3. **Vérifier les mises à jour** — repère les instantanés publiés par les autres profils et les
   liste, prêts à être récupérés.

Les **nouveaux traits** des autres sont fusionnés automatiquement. Un même trait modifié **des
deux côtés** devient une **correction** à traiter comme un calque de révision — voir plus bas.

> Nécessite l'application desktop (pas disponible en preview navigateur).

## Vue de révision et corrections

Le bouton **Vue de révision** de la timeline fait défiler cycliquement trois affichages : **Tout**
/ **Mes traits** / **Corrections** — pour voir isolément ce que vous avez dessiné ou ce qu'un
autre profil a modifié après coup.

Avec l'outil **Sélection Fond/Trait (M)**, cliquer une correction fait apparaître deux boutons :

- **Accepter** — garde la correction, supprime l'original.
- **Rejeter** — annule la correction, restaure l'original.

Ce mécanisme fonctionne pour les corrections reçues via le dossier partagé, mais aussi comme
outil de révision interne au sein d'une même session.

## Contacts (P2P)

Dans **Réglages → Collaboration**, une section **Contacts** permet d'échanger un code avec un
collaborateur distant (copier le vôtre, coller le sien) pour l'ajouter à votre liste de contacts.

> ⚠️ **Modèle uniquement, pas encore de transport réseau réel.** La gestion des contacts est
> prête, mais la synchronisation P2P en temps réel (travailler ensemble sur le même fichier sans
> passer par un dossier partagé) n'est pas encore branchée — c'est une phase séparée, pas encore
> livrée.

## Feedback

L'outil **Commentaire (C)** a un bouton **"Enregistrer comme feedback"** (à côté de
l'enregistrement classique de commentaire d'équipe) : une note technique liée à un contexte de
travail réel, stockée **hors du fichier projet** — elle ne l'alourdit jamais et ne se perd jamais
en changeant de projet.

Dans **Réglages → Feedback** :

- **Récupérer le feedback de l'équipe** — importe les notes des autres profils via le dossier
  partagé ; elles restent en attente jusqu'à approbation manuelle (les vôtres sont approuvées
  automatiquement).
- **Feedback beta-testeurs (GitHub)** — pour les versions distribuées : chaque note envoyée
  devient une Issue publique sur un dépôt GitHub dédié. Le triage (voir/résoudre) nécessite un
  token GitHub personnel, saisi une fois et gardé uniquement sur votre machine.
