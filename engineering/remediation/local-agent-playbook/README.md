# Nemo local-agent coordination / Coordination des agents locaux

Prepared 7 September 2026 for Ilya and Mysteropodes. Proposed instructions; no assignments or GitHub changes were made by preparing these documents.

Préparé le 7 septembre 2026 pour Ilya et Mysteropodes. Instructions proposées ; leur préparation n’a attribué aucune tâche ni modifié GitHub.

## Documents

- [Full English playbook](PLAYBOOK.en.md)
- [Guide complet en français](PLAYBOOK.fr.md)

Both editions contain the current-work review, concrete task packets, ownership rules, sprint workflow, handoff templates, two-board update procedure, oversight rhythm and a copyable agent instruction. Commands and GitHub field values are identical across languages.

Les deux éditions contiennent l’analyse des travaux actuels, les missions concrètes, les règles de responsabilité, le déroulement des sprints, les modèles de transmission, la procédure de mise à jour des deux tableaux, le rythme de supervision et une instruction à copier pour les agents. Les commandes et valeurs de champs GitHub sont identiques entre les éditions.

## Suggested first allocation / Première répartition proposée

| Packet / Mission | Focus / Travail | Start condition / Condition de départ |
|---|---|---|
| F — retained / existante | Fizz: MCP payload contract / contrat du payload MCP | Continue #1002 with its owner / poursuivre #1002 avec son responsable |
| H — retained / existante | Honey: installed-package evidence / preuves du paquet installé | Continue #1001; coordinate corrected F binary / poursuivre #1001 ; coordonner le binaire corrigé de F |
| M1 | Concurrent desktop isolation / isolation des instances de bureau | Runnable identified package and resource slot / paquet identifié exécutable et créneau réservé |
| M2 | Native Rust boundaries / frontières du Rust natif | Agreed subset and integration owner / sous-ensemble et responsable d’intégration convenus |
| M3 | Missing opacity consumer tests / tests manquants des consommateurs d’opacité | Stable application/fixture contract / contrat application/jeux de test stable |
| M4 | Comparable performance measurements / mesures de performance comparables | Identified fixtures and reserved machine / jeux de test identifiés et machine réservée |

Start with the available capacity, not all six lanes automatically. Section 3 is the joint kickoff; section 4 contains each packet; sections 6–8 are the agent workflow; section 10 is the instruction to copy. Ilya retains core/integration ownership; Mysteropodes controls his agreed local lane pool through successive sprints.

Démarrer selon la capacité disponible, sans lancer automatiquement les six volets. La section 3 décrit le lancement commun ; la section 4 détaille chaque mission ; les sections 6 à 8 donnent le processus des agents ; la section 10 fournit l’instruction à copier. Ilya conserve la responsabilité du cœur et de l’intégration ; Mysteropodes dirige les volets locaux convenus au fil des sprints.

## First oversight action / Première action de supervision

Reconcile **R14/#910**: it is closed in GitHub while both remediation boards still show In progress / Needs validation, and later client-path findings remain under correction in #1002. Preserve the existing F/H work. The leads choose whether to reopen #910 or track equivalent outstanding acceptance in an explicitly linked open issue.

Rapprocher l’état de **R14/#910** : le ticket est fermé sur GitHub, alors que les deux tableaux indiquent encore In progress / Needs validation et que les défauts découverts ensuite avec les clients restent en cours de correction dans #1002. Préserver F/H. Les responsables décident de rouvrir #910 ou de suivre les mêmes validations restantes dans un ticket ouvert explicitement lié.

## Review identity / Référence de l’analyse

- Reviewed main / main examiné: `66ece0641708122eb8447e85ad8dd7e3402aaf6c`.
- #1001: `a99d718685f0515e2e5e949dd6a076eb46feda38`; #1002: `8c1dd9bdcc3b82d65fe963c8d7d95d30c48fb8c5`.
- Repository source, 43 remediation issue records, relevant comments/PRs, 58 origin Project items and 220 mirror items inspected; these identities and PR states rechecked after drafting.
- Code du dépôt, 43 tickets de remédiation, commentaires/PR pertinents, 58 éléments du Project d’origine et 220 éléments du miroir examinés ; ces références et états des PR ont été revérifiés après rédaction.
- Review scope: source and coordination evidence. Product suites and reported client/package behavior were not independently rerun for this documentation task.
- Périmètre de l’analyse : code et preuves de coordination. Les suites produit et les comportements clients/paquets rapportés n’ont pas été réexécutés indépendamment pour cette tâche documentaire.

Keep this directory together in the repository so relative references continue to resolve. These documents are a dated review and operating proposal; live task status remains in GitHub.

Conserver ce répertoire dans le dépôt pour maintenir les références relatives. Ces documents sont une analyse datée et une proposition de fonctionnement ; l’état actif des tâches reste dans GitHub.
