# Remédiation de Nemo : deux responsables humains, des agents locaux

Édition française · 7 septembre 2026 · v1.0 · [Édition anglaise](PLAYBOOK.en.md)

**Objectif :** permettre à Ilya et Cyrill de diriger chacun leurs agents locaux en parallèle, avec des responsabilités stables, des transmissions utiles entre les sprints et une progression visible vers les mêmes objectifs de remédiation : une application modulaire, des commandes applicatives partagées et un serveur MCP Rust intégré à la distribution, une meilleure couverture de régression et un comportement reproductible dans le navigateur et l’application de bureau.

**Statut : proposition de règles de fonctionnement à adopter par les deux responsables.** L’analyse ci-dessous décrit les faits au moment de l’observation indiqué. Les répartitions proposées ne signifient pas que les agents ont reçu ces missions. Cette préparation n’a publié aucun message, modifié aucun ticket ni tableau, lancé aucun agent ni fusionné de code. Les affectations existantes restent en vigueur jusqu’à une transmission explicite par leurs responsables.

## 1. Ce que l’analyse établit

L’analyse a récupéré la branche `main` de GitHub au commit **`66ece0641708122eb8447e85ad8dd7e3402aaf6c`**, examiné l’implémentation et le guide du dépôt, les 43 tickets de remédiation, la liste des PR ouvertes, les commentaires récents pertinents et les listes complètes des éléments des deux Projects : 58 éléments dans le tableau d’origine et 220 dans la feuille de route. Elle a également examiné les PR #1001 et #1002. Il s’agit d’une analyse du code et de la coordination ; elle ne constitue ni une nouvelle exécution des suites de tests du produit, ni une reproduction indépendante des résultats d’exécution rapportés par les auteurs des PR.

| Travail | Éléments actuellement disponibles | Conséquence pour l’affectation |
|---|---|---|
| Référence initiale et lancement | F0/#888, R00/#895, BZ0/#896, R01/#897, R02/#898 et R04/#900 sont fermés. La référence reproductible a été intégrée par #983. | Réutiliser ces fondations. Ne pas recommencer l’outillage de référence ou le guide. La clôture historique de R04 ne prouve pas le bon fonctionnement multimédia de tous les paquets ultérieurs. |
| R03 : inventaire et jeux de test | Le code contient l’inventaire, le corpus déterministe et les tests navigateur. #899 reste Review / Needs validation dans les deux tableaux. | Compléter les preuves manquantes pour les consommateurs ; ne pas régénérer tous les jeux de test ni répéter un travail accepté sans modification pertinente du code. |
| R05 : frontières des modules | La découverte des fichiers, le comptage textuel indépendant du langage, la provenance et les contrôles de non-croissance ont été intégrés par #984. La tranche opacité possède des profils JS et MCP Rust. #901 reste Review. | Compléter une partie délimitée de la classification et du contrôle du Rust natif. La politique de dépendances de l’ensemble du code reste incomplète ; la couverture des tailles ne vaut pas couverture architecturale. |
| R06 : isolation | #991 a intégré l’isolation ; #993 a corrigé les effets de bord des imports refusés ; #996 a corrigé la troncature des comptes rendus JSON du CLI natif. #902 reste In progress / Needs validation. | Valider les critères restants d’exécution et de compilation simultanées sur le code actuel. Ne pas réimplémenter les corrections déjà intégrées. |
| R08 : extraction d’une fonction d’animation | L’extraction de la courbe utilisée en production et le choix du lanceur Node ont été intégrés par #986. #904 est Validate / Needs validation. | Réutiliser le noyau extrait ; terminer les validations restantes au lieu de produire une autre extraction. |
| Tranche R09/R11/R12/R13/R14 | #992 est fusionnée dans le `main` examiné. Les modules application/domain/bootstrap/adapters, les consommateurs de l’opacité et le MCP Rust compilé sont présents. #905/#907/#908/#909 restent In progress. | L’équipe d’Ilya prend en charge les évolutions suivantes des contrats centraux et de l’application, sous réserve des périmètres explicitement réservés à Cyrill. La remédiation globale reste ouverte. |
| Volet F de Cyrill | La PR en brouillon [#1002](https://github.com/mysteropodes/nemo/pull/1002), `8c1dd9bdcc3b82d65fe963c8d7d95d30c48fb8c5`, corrige la structure annoncée du payload et le nom du champ d’instance des requêtes. | Préserver le volet de Fizz et ses fichiers exacts. La PR indique explicitement que les preuves de validation restent à fournir. |
| Volet H de Cyrill | [#1001](https://github.com/mysteropodes/nemo/pull/1001), `a99d718685f0515e2e5e949dd6a076eb46feda38`, contient uniquement des preuves de packaging et de connexion depuis un environnement épuré. | Préserver le volet de Honey. Son auteur rapporte une connexion réussie sous macOS arm64 ainsi que des constats distincts sur FFmpeg et le cycle de vie du registre. Cela ne prouve ni le packaging sur toutes les plateformes ni la validation de l’export. |

**Une incohérence de suivi exige une décision humaine immédiate.** [#910](https://github.com/mysteropodes/nemo/issues/910) a été fermé comme terminé à 20:22:52 UTC avec le commit de fusion `66ece06`. Les deux tableaux de remédiation indiquent encore In progress / Needs validation. Des [constats ultérieurs avec Claude](https://github.com/mysteropodes/nemo/issues/910#issuecomment-5575427772) et [avec Codex](https://github.com/mysteropodes/nemo/issues/910#issuecomment-5575438157) rapportent un défaut réel du schéma annoncé. Main déclare encore `payload: Value` tout en refusant les payloads qui ne sont pas des objets. Ces éléments justifient de garder la validation des clients ouverte, même si le transport et le code applicatif sont fusionnés. Les responsables doivent explicitement rouvrir #910 ou relier un ticket de validation ouvert offrant la même couverture. Ne pas déclarer silencieusement ce jalon Done. Les vues enregistrées filtrées par `is:open` peuvent masquer cette validation inachevée.

L’ancien [compte rendu de fin de week-end](../../../40min-checkins/2026-09-07-weekend-final-handoff.md) reste une source utile, mais son indication selon laquelle #992 n’est pas fusionnée est dépassée. Plusieurs descriptions de tickets et passages du guide précèdent également l’installation des dépendances de test et les intégrations actuelles. Croiser le code, les comptes rendus récents et l’état actuel des PR ; ni un ancien paragraphe ni l’égalité des valeurs des tableaux ne prouvent leur actualité.

## 2. L’accord de fonctionnement

### Deux responsables, un seul programme de remédiation

1. **Ilya est responsable de la remédiation et de l’intégration.** Il ordonne les évolutions des contrats partagés, maintient l’ordre global des dépendances, décide des changements de périmètre entre équipes et accepte les jalons de phase.
2. **Cyrill dirige ses propres agents locaux.** Une fois l’ensemble des volets et leurs limites convenus, il peut affecter, examiner et poursuivre ces missions localement sans attendre Ilya à chaque étape.
3. Chaque tâche a **un responsable humain, un seul agent autorisé à modifier son périmètre à un instant donné et un relecteur nommé**. L’identifiant de l’agent ou de sa session figure dans le ticket, sans remplacer le responsable humain dans Assignees.
4. Les agents d’Ilya reçoivent leurs instructions d’Ilya ; ceux de Cyrill les reçoivent de Cyrill. Les demandes entre équipes passent par le ticket concerné et le responsable humain. Le travail courant ne nécessite aucune attribution de tâche à un agent distant.
5. Les deux équipes utilisent le même processus tickets/PR/Projects. Buzz peut transmettre un lien ou accueillir une discussion facultative ; une connexion Buzz, un minuteur, un accusé de réception du relais ou une réparation d’agent distant n’est pas un prérequis pour ces missions locales.
6. Le [guide existant](../README.md), la [fiche de mission](../templates/TASK_PACKET.md), le [compte rendu de transmission](../templates/HANDOFF_RECEIPT.md), `AGENTS.md`, `CONTRIBUTING.md` et les sections pertinentes de `CLAUDE.md` continuent de s’appliquer. Ces instructions adaptent la coordination à une exécution locale dirigée par les humains.

### Autonomie dans un périmètre convenu

Une attribution couvre la mission ou le jalon convenu, y compris ses différents sprints. Un point d’étape sert à communiquer et à préparer l’intégration ; il ne provoque ni interruption automatique ni perte de responsabilité. Les agents peuvent implémenter, tester et corriger leur mission en continu. Inscrire les nouvelles demandes dans la file du ticket existant ; ne pas changer de tâche sauf si le responsable humain modifie la priorité.

Un changement de périmètre est nécessaire pour toucher les fichiers d’un autre responsable, modifier un schéma ou l’autorité sur l’état, changer le raccordement partagé des builds/tests, modifier les critères d’acceptation ou effectuer une action externe non autorisée par la mission. Le débogage courant et les corrections dans le périmètre accepté ne nécessitent pas de confirmations répétées.

Un agent indisponible ou silencieux ne libère pas ses fichiers. Son responsable humain préserve les modifications non validées, les commits locaux, les branches non poussées, l’état d’exécution et la prochaine action avant de confirmer une transmission. Les anciennes opérations distantes au résultat indéterminé sont rapprochées une fois de leurs effets réels ; elles ne sont pas rejouées pour relancer le travail local.

## 3. Première mise en place commune : une courte séance de planification

1. **Actualiser les éléments disponibles.** Lire le `main` actuel, les dernières mises à jour de #910/#1001/#1002 et toutes les PR touchant les chemins proposés. Consigner le nouveau SHA complet de départ ; celui de ce document n’est pas un verrou permanent.
2. **Consigner les responsabilités actuelles.** Chaque humain liste les tâches actives, sessions d’agents, branches/commits, modifications non validées ou non poussées, fichiers historiques entiers réservés, ressources réservées et prochains points d’étape dans les tickets GitHub correspondants. Une file locale peut y renvoyer ; ne pas créer un autre backlog partagé.
3. **Résoudre l’incohérence R14.** Consigner si #910 est rouvert ou quel ticket ouvert porte désormais les validations restantes. Préserver les travaux F et H existants. Examiner le constat FFmpeg de #1001 comme un défaut distinct de packaging ; ne pas l’effacer au motif que #900 est fermé.
4. **Convenir de l’ensemble initial des missions.** Conserver F et H ; sélectionner les missions supplémentaires ci-dessous selon la capacité locale réelle. Un point de départ pratique est de limiter chaque humain à deux agents d’implémentation, plus un relecteur indépendant ou un volet de validation si les ressources le permettent. Traiter les relectures en attente avant d’ajouter des agents qui modifient le code.
5. **Découper les livrables indépendants en tickets enfants existants ou nouveaux.** Rechercher d’abord un ticket équivalent. Utiliser le parent R existant ; créer un enfant uniquement lorsque les responsables l’autorisent. Inscrire les vraies URL des enfants dans le parent et dans les deux Projects. Les libellés M1, etc. sont des identifiants de planification, pas des tickets GitHub déjà créés.
6. **Remplir et attribuer la mission une fois.** Nommer l’humain, l’agent qui modifie les fichiers, le relecteur, les chemins exacts, la base, les dépendances, les critères d’acceptation, les ressources, le prochain point d’étape et les droits de publication. Cyrill peut ensuite attribuer des sous-missions dans l’ensemble convenu ; dépasser ces limites revient à Ilya.
7. **Vérifier l’accès GitHub depuis chaque machine.** Utiliser le bon compte humain authentifié. Vérifier séparément la lecture du dépôt et des deux Projects ; un rôle dans le dépôt ne démontre pas à lui seul un droit d’écriture dans les Projects. Utiliser les accès déjà approuvés. Si un accès manque, l’humain s’en charge pendant que l’agent poursuit le travail local indépendant et prépare la mise à jour exacte.
8. **Choisir un responsable des mises à jour du tableau et son remplaçant.** Normalement, l’agent affecté à la tâche met à jour son élément dans les deux tableaux ; son humain le remplace si nécessaire. Ilya gère les mises à jour globales et des phases. Une seule personne ou un seul agent modifie un élément à la fois ; aucun second robot de surveillance ne le réécrit en parallèle.
9. **Fixer le rythme.** Proposition : sprints de travail de 60 à 90 minutes, courte mise à jour à la fin de chaque sprint, et revue commune de 10 à 15 minutes après deux sprints ou à un horaire convenu. Ces intervalles sont ajustables ; ce ne sont pas des échéances qui annulent le travail.

## 4. Ensemble de missions recommandé pour Cyrill

F et H sont des travaux à poursuivre. M1 à M4 sont des missions supplémentaires délimitées, à attribuer après clarification des responsabilités. Les nouveaux chemins indiqués sont des destinations proposées dont il faut vérifier la disponibilité avant attribution. Les preuves vont dans un répertoire propre à la mission ; les données brutes restent dans des répertoires de rapports isolés et ignorés par Git jusqu’à leur nettoyage.

### F — Terminer la correction du contrat client MCP déjà engagée dans #1002

**Humain :** Cyrill. **Volet existant :** Fizz / Lane F. **Priorité :** P0. **Objectif :** découverte et commandes fiables depuis les clients, via l’API applicative partagée. **Parent :** R14/#910 ou le ticket de validation explicitement choisi ; lien avec R09/#905.

**Fichiers réservés :** `nemo-mcp/src/contract.rs`, `nemo-mcp/src/server.rs`, `nemo-mcp/tests/stdio_contract.rs`, `engineering/application/transport-v1.schema.json`, `engineering/application/OPACITY_SLICE.md`. Il s’agit d’une exception explicite à la responsabilité d’Ilya sur les contrats partagés. Les autres agents les consultent en lecture seule jusqu’à leur libération.

1. Poursuivre la branche actuelle ; vérifier son dernier commit plutôt que créer une correction parallèle.
2. Vérifier que les schémas générés décrivent les clés et types acceptés du payload et que les requêtes mal formées reçoivent des erreurs utiles. Laisser au service applicatif l’autorité sur les plages de valeurs, les identités et l’historique.
3. Exécuter les contrôles Rust/stdio compilés et les contrôles de schéma pertinents, y compris l’ancien nom d’instance pris en charge et les cas d’entrées mal formées.
4. Depuis de nouvelles sessions des clients installés, demander à chacun de découvrir les capacités, de sélectionner la bonne instance et d’effectuer la même modification à partir du contrat annoncé. Conserver les payloads réellement émis et une relecture indépendante du document. Une requête de protocole construite manuellement ne remplace pas ce test client.
5. Coordonner le binaire corrigé avec H ; consigner le code et les octets installés. Répéter les cas concernés d’annulation/rétablissement, de nouvelle tentative, d’écriture obsolète, de reconnexion et d’annulation d’opération. Transmettre à Ilya les capacités applicatives manquantes avec un cas de reproduction délimité.
6. Livrer la PR, les preuves de test et une matrice d’acceptation par client. F reste Review/Validate jusqu’à l’acceptation des preuves prévues avec les clients installés ; le jalon R14 complet exige également les autres parcours spécifiés.

### H — Terminer les preuves sur le paquet installé déjà engagées dans #1001

**Humain :** Cyrill. **Volet existant :** Honey / Lane H. **Priorité :** P0. **Parent :** R14/#910 ; les points restants de packaging se rattachent à R21/#923.

**Chemins réservés :** `engineering/remediation/receipts/R14-packaging/**`. Le code, les scripts de compilation et la configuration restent hors de cette mission consacrée aux preuves.

1. Examiner le résumé actuel et l’interprétation corrigée des transcriptions déjà présents dans #1001.
2. Identifier l’application, le MCP, le système/l’architecture, le SHA source et les empreintes des artefacts ; distinguer un paquet de test non signé d’une version signée destinée à la distribution.
3. Vérifier le binaire installé et la connexion à l’instance voulue depuis un environnement de résolution épuré. Préserver les installations et projets de travail de l’utilisateur.
4. Lorsque la correction F est disponible, répéter uniquement les cas de connexion/client concernés sur ces octets installés. Coordonner les créneaux bureau/GPU avec M1.
5. Maintenir des constats séparés pour la dérive des dépendances FFmpeg, le comportement des enregistrements périmés du registre et les empreintes sensibles au chemin de compilation. Ne pas modifier silencieusement l’outil de packaging ni affaiblir les contrôles. F traite déjà le nom du champ de requête ; relier cette dépendance au lieu de dupliquer le travail.
6. Livrer un résumé des preuves accepté, les affirmations de support de plateformes encore non validées et les tâches résiduelles liées. Une connexion MCP réussie ne prouve pas le fonctionnement de l’export multimédia ni une distribution sur toutes les plateformes.

### M1 — Valider une tranche délimitée de l’isolation de plusieurs instances de bureau

**Humain :** Cyrill après attribution de ce sous-périmètre R06 par Ilya. **Priorité :** P0. **Parent :** R06/#902. **Taille suggérée :** deux sprints, à ajuster après le premier résultat reproductible. **Dépendance :** paquet identifié et exécutable, avec un créneau réservé pour le bureau ; il n’est pas nécessaire d’attendre la nouvelle sémantique des payloads MCP.

**Chemins modifiables proposés :** `tests/desktop/local-isolation-acceptance.test.cjs` et `engineering/remediation/receipts/M1-isolation/**`. Lire d’abord le dispositif de test existant et `engineering/runtime-isolation.md`. Le lanceur de production, le stockage, `project.js`, les sources Rust et le registre de tests restent en lecture seule.

1. Comparer les critères restants de #902 aux tests actuels et aux preuves du point 90 ; sélectionner les cas réellement manquants.
2. Démarrer deux instances appartenant aux tâches avec des racines de données, origines/ports et emplacements d’artefacts distincts via le lanceur documenté. Vérifier les identités du code et des instances en cours d’exécution.
3. Créer des documents de test jetables différents, sauvegarder/rouvrir et annuler séparément ; vérifier l’absence de contamination entre les états. Exécuter le cas requis de sorties de compilation isolées avec des racines de compilation réservées séparément.
4. Vérifier le refus d’arrêter une instance étrangère, l’arrêt d’une instance appartenant à la tâche, la relance avec données conservées et le comportement après arrêt du lanceur, sans toucher aux processus non concernés. Réutiliser les tests de régression existants lorsqu’ils couvrent déjà un cas.
5. Conserver les résultats et identités des artefacts. Si un défaut de production apparaît, transmettre un cas minimal à Ilya ; ne pas corriger le lanceur partagé dans cette mission. Continuer les autres validations indépendantes.
6. Livrer un test de régression permanent lorsqu’il manque et une matrice d’acceptation. Distinguer les contrôles de compilation/processus des véritables parcours de sauvegarde/réouverture dans l’interface. Cette mission ne clôt que ses critères R06 déclarés.

### M2 — Faire respecter un sous-ensemble examiné des frontières Rust natives

**Humain :** Cyrill après attribution de ce sous-périmètre R05 par Ilya. **Priorité :** P1. **Parent :** R05/#901. **Taille suggérée :** deux sprints. **Dépendance :** accord sur les racines Rust sélectionnées et sur l’unique responsable du raccordement du contrôleur.

**Chemins modifiables proposés :** `scripts/nemo/lib/boundaries-rust.cjs`, `tests/nemo-rust-boundaries.test.cjs`, `engineering/boundaries/profiles/native-rust.profile.json`, `engineering/remediation/receipts/M2-rust-boundaries/**`. Vérifier leur disponibilité. Le contrôleur existant, `ci.cjs`, les fichiers package/lock et le Rust de production restent en lecture seule. Exclure de cette nouvelle attribution les fichiers actifs du contrat MCP et `src-tauri/src/application_mcp.rs` ; conserver leur traitement par le profil existant.

1. Comparer la découverte actuelle et les profils adoptés aux fichiers Rust réellement suivis dans les racines attribuées, initialement `geometry-wasm/src/**` et le reste convenu de `src-tauri/src/**`.
2. Réutiliser le comptage indépendant du langage. Identifier précisément les fichiers dépourvus de limites adoptées ou de règles de dépendances ; ne pas présenter une racine volontairement exclue comme un oubli accidentel.
3. Proposer les responsables par chemin, les limites, les exceptions et le sens des dépendances pour ce sous-ensemble. Déclarer explicitement les analyses de graphe non prises en charge ; ne pas passer le Rust dans un analyseur lexical JS ni présenter des expressions régulières comme un graphe complet des dépendances Rust.
4. Ajouter le plus petit contrôle manquant réellement applicable, en s’appuyant sur Rust/Cargo et le contrat du contrôleur existant. Faire examiner les décisions architecturales par les responsables ; ne pas relever les plafonds historiques pour obtenir un résultat positif.
5. Prouver les cas valides et les échecs provoqués pertinents : fichier `.rs` surdimensionné dans le périmètre, fichier non classifié dans le périmètre, exception expirée et dépendance interdite lorsque le contrôle du graphe fait partie de la mission. Retirer ensuite les violations temporaires.
6. Fournir au responsable de l’intégration la modification exacte du point d’appel et les résultats attendus dans le circuit de contrôle existant. L’acceptation finale exige que ce contrôle passe par le point d’entrée local normal sur le commit combiné. Un helper jamais appelé ne constitue pas un contrôle livré.

### M3 — Ajouter les régressions navigateur manquantes autour de l’opacité intégrée

**Humain :** Cyrill. **Priorité :** P1. **Parents :** R03/#899 et R13/#909, avec un seul ticket enfant principal. **Taille suggérée :** un sprint de caractérisation puis un sprint d’implémentation. **Dépendance :** gestionnaires d’opacité fusionnés et contrat stable des jeux de test/consommateurs fourni par Ilya.

**Chemins modifiables proposés :** `tests/browser/local-opacity-consumers.spec.cjs` et `engineering/remediation/receipts/M3-opacity-consumers/**`. Le fichier existant `tests/browser/opacity-consumers.spec.cjs`, les jeux de test, le bootstrap, le code de production et la configuration Playwright restent en lecture seule.

1. Lire les tests existants et les comptes rendus R03. Construire une petite matrice des exigences couvertes et manquantes ; ne pas dupliquer les tests statiques, de sauvegarde ou de SVG déjà acceptés.
2. Choisir une famille manquante : contexte imbriqué/composant, comportement avec images clés/changement d’image, ou sélection/historique après sauvegarde/réouverture. Confirmer le comportement attendu avec le code et la documentation locale.
3. Manipuler les vrais contrôles d’interface et les actions File Open/Save dans des contextes navigateur séparés, avec le dispositif de test actuel. Vérifier des valeurs attendues indépendantes avant et après réouverture.
4. Pour les consommateurs visuels/d’export concernés, comparer la sortie réellement rendue/exportée à une attente déclarée. Préserver le jeu de test et inclure un cas pertinent de corruption volontaire ; ne jamais modifier une référence visuelle simplement pour faire passer le test.
5. Laisser les corrections applicatives à Ilya. Soumettre immédiatement les preuves d’échec et poursuivre les cas indépendants de la même mission.
6. Livrer des tests navigateur reconnus par le lanceur, les références exactes du code et des jeux de test, les empreintes des artefacts et les limites. Il s’agit de validation navigateur uniquement ; M1 et H couvrent d’autres garanties natives et d’installation.

### M4 — Mesurer une référence comparable pour la prochaine extraction

**Humain :** Cyrill. **Priorité :** P1. **Parents :** R19/#921, avec les apports de R03/#899 et de la tranche R12 implémentée. **Taille suggérée :** un sprint. **Qualification au démarrage :** préparation de mesures délimitée, pas validation complète de R19.

**Chemins modifiables :** uniquement `engineering/remediation/receipts/M4-performance/**` ; les sorties brutes utilisent un répertoire de rapports isolé et ignoré par Git. Le code de production, le corpus et le lanceur de benchmark restent en lecture seule.

1. Lire `tests/bench/README.md` et les manifestes des charges existantes. Sélectionner des jeux d’évaluation/copie/mémoire aux octets identiques et pertinents pour la prochaine extraction prévue.
2. Consigner le code, les empreintes des jeux de test, la catégorie de machine, le backend, les versions d’exécution, le protocole à chaud/à froid et la charge concurrente. Réserver la machine pour des mesures temporelles comparables ; ne pas concurrencer H/M1 pour les mesures GPU.
3. Exécuter la commande de benchmark existante sur la référence et le candidat convenus, en conservant les échantillons bruts. Ne changer qu’une variable de comparaison à la fois et vérifier l’équivalence des sorties.
4. Ne rapporter que les statistiques réellement étayées par les échantillons conservés. Le dispositif CPU actuel fournit médiane/p90 et statistiques associées avec peu d’échantillons par défaut ; cela ne constitue pas un budget produit p95/p99 établi. Si davantage d’échantillons ou d’instrumentation sont nécessaires, les spécifier pour le responsable.
5. Proposer des budgets avec la variance observée et leur justification, pour adoption humaine. Les mesures GPU/export indisponibles restent explicitement not-run ou blocked.
6. Livrer une comparaison concise et la prochaine mesure exacte. Terminer cette préparation ne clôt pas le programme de performance et d’endurance R19.

### Mettre en file ensuite, après les décisions concernées

| Mission | Raison de l’attente | Prochain travail utile |
|---|---|---|
| R10/#906 : faisabilité OpenFX native | Le port image/paramètres R09 n’est pas entièrement adopté ; l’ancienne demande à Pollen doit recevoir une disposition clarifiée. | Après accord sur le port, autoriser une preuve jetable de chargement/description/rendu CPU float. La retirer ou l’intégrer explicitement ; ne pas inventer un second backend produit. |
| R18.1/#915 : extraction identité/codec du document | Les frontières partagées de persistance/historique exigent encore un contrat exact et une réservation de fichiers entiers. | Ilya sélectionne une fonction pure indépendante et ses consommateurs ; Cyrill peut alors prendre cet enfant en charge de bout en bout. |
| R18.2/#916 : prochaine extraction d’animation | Nécessite l’acceptation de l’extraction actuelle et une API convenue. | Choisir une responsabilité pure restante ; préserver l’autorité courbe/opacité et éviter des réécritures concurrentes de `motion.js`. |
| R20/#922 et R21/#923 complet | Les prérequis étendus de migration, de médias et de plateformes restent ouverts. | Attribuer des sous-cas de panne/plateforme lorsque leurs entrées précises existent ; ne pas déclarer les programmes complets Ready. |

## 5. Les volets d’Ilya et les limites de modification

L’équipe d’Ilya se concentre sur le candidat combiné, les contrats d’état et de commandes R09/R11, les diagnostics/rejeux R12, les corrections d’autorité et de consommateurs R13, l’intégration R07 des véritables contrôles locaux et l’acceptation finale sur les différentes surfaces. Elle examine les constats M1/M3 et raccorde les contrôles M2 acceptés. Elle prend également en charge le rapprochement des PR obsolètes avec le contenu déjà fusionné ; une ancienne PR ouverte n’est pas automatiquement une fusion sûre ni une attribution active.

**Une seule personne ou un seul agent modifie à la fois les fichiers partagés :** `package.json`, lockfiles, `src/index.html`, configuration/registre de tests partagés, `scripts/nemo/ci.cjs`, générateurs de schémas partagés, bootstrap/configuration de compilation natifs, inventaire généré et manifestes de politique. Réserver les monolithes historiques entiers tels que `app.js`, `motion.js`, `project.js`, `tweens.js` et `engine-bridge.js` lorsqu’ils doivent être modifiés. Deux plages de lignes distinctes ne constituent pas une isolation durable.

La mission F est une exception explicitement réservée pour ses cinq fichiers actuels. H produit uniquement des preuves. M1/M3 utilisent des fichiers de test distincts ; M2 possède ses nouveaux fichiers de contrôle/profil ; M4 se limite aux mesures. Le responsable de l’intégration applique une seule fois les changements de raccordement partagé. Les répertoires de preuves sont propres à chaque mission ; les agents n’ajoutent jamais leurs résultats simultanément dans un unique fichier de compte rendu partagé.

## 6. Processus de sprint de chaque agent

### Démarrer ou reprendre

1. Lire le ticket attribué, la fiche actuelle, la dernière transmission et les PR liées avant toute exploration. Vérifier la prochaine action et les tests déjà exécutés.
2. Lire les règles d’entrée du dépôt, le chapitre pertinent de remédiation, la documentation locale du module/nœud et son implémentation. Ne pas conseiller des réglages de nœud de mémoire.
3. Vérifier origin, branche, HEAD complet, modifications non validées et branches/PR ouvertes connexes. Récupérer le main actuel ; préserver le checkout et l’index existants. Commencer les modifications dans une branche/un worktree dédié, avec le préfixe `codex/` ou celui établi par l’équipe pour l’agent.
4. Vérifier les chevauchements par rapport aux attributions actuelles des deux équipes. Consigner les fichiers entiers et les ressources précisément réservés. Un statut Project n’est pas un verrou du système de fichiers.
5. Accuser réception une fois de la mission, avec résultat attendu, périmètre, relecteur et prochain point d’étape ; passer l’élément en In progress selon la section 8.
6. Configurer des racines d’exécution, profil/origine navigateur, ports, données applicatives, caches, sorties de compilation et chemins de rapports distincts avec les outils d’isolation documentés. Vérifier l’identité du code exécuté avant les assertions d’interface. Sérialiser les interactions avec le bureau physique et les benchmarks sur la machine de référence/le GPU.

### Travailler sans interruption inutile

7. Caractériser le comportement demandé et reproduire un vrai défaut avant de le modifier. Réutiliser les preuves déjà acceptées si le code, les jeux de test et l’environnement applicable restent équivalents, en justifiant cette équivalence.
8. Effectuer une modification cohérente ; conserver une seule autorité d’écriture applicative et une façade de compatibilité si nécessaire. Toute modification persistante vérifie les consommateurs applicables : sauvegarde/chargement, annulation/rétablissement, sélection, animation, rendu/export et ponts natifs.
9. Exécuter tôt les contrôles ciblés. Avant relecture, exécuter les points d’entrée normaux de vérification locale pertinents sur le candidat. `npm run verify` utilise par défaut un profil rapide ; il ne prouve pas tous les tests navigateur/natifs/MCP. Examiner les comptes rendus et pas seulement le code de sortie : `doctor` termine toujours avec zéro.
10. Employer `pass`, `fail`, `blocked` ou `not-run` avec une raison. Les commandes existantes comprennent `npm run check`, `npm test`, `npm run test:integration`, `npm run test:browser`, `npm run test:desktop` et `npm run bench` ; les choisir selon le périmètre réel. Pour le MCP, inclure les tests Cargo applicables de la crate. Lire d’abord la documentation actuelle des commandes.
11. Compiler et valider localement. Ne pas activer, déclencher, relancer ou ajouter de builds GitHub Actions automatiques sans demande humaine explicite pour cette exécution hébergée précise. Préserver les protections de revue. Une instruction normale de tâche/PR/fusion ne fournit pas cette autorisation.
12. Pour un défaut hors périmètre, inscrire dans le ticket un cas de reproduction délimité et le responsable proposé. Continuer le travail indépendant dans le périmètre. En cas de blocage réel, préserver l’état et passer uniquement cette mission en Blocked. Commencer une autre mission Ready déjà attribuée seulement avec un périmètre/worktree distinct ; ne jamais remplacer silencieusement la tâche bloquée.

### Terminer un sprint, mettre en pause ou terminer la mission

13. Vérifier les chemins modifiés et le diff indexé ; créer des commits cohérents lorsque l’attribution l’autorise. Ne pas indexer de changements étrangers ni modifier manuellement les sorties générées. Préserver explicitement les travaux inachevés plutôt que déclarer le candidat propre.
14. Publier le court compte rendu de sprint ci-dessous dans un seul commentaire du ticket, avec les liens vers les preuves. Actualiser la PR liée lorsque le comportement à examiner ou sa validation change. Ne pas ouvrir une nouvelle PR de rapport à chaque point d’étape.
15. Actualiser les deux éléments Project et les relire. Déclarer honnêtement une synchronisation partielle si une seule mise à jour a réussi. L’agent/humain actuel prend en charge sa réparation.
16. Consigner la prochaine action exacte, les tests restants, les chemins/ressources réservés et le maintien, la proposition de transfert ou la libération de responsabilité. Notifier l’humain seulement pour une décision, un blocage, un résultat prêt à examiner ou un changement significatif.
17. Après achèvement/transmission, arrêter uniquement les processus appartenant à la tâche et libérer ses ressources. Préserver les commits publiés, les preuves nécessaires et les modifications non validées/non poussées. Supprimer les worktrees jetables seulement après préservation et libération par le responsable ; expliquer pourquoi tout checkout conservé reste nécessaire.

## 7. Des notes de sprint et de transmission courtes et durables

**Emplacement canonique :** les commentaires du ticket GitHub de la tâche. Le corps du ticket conserve la fiche/les critères actuels et un lien vers la dernière note de sprint ; la PR contient le diff et les preuves finales de revue. `reports/` est ignoré par Git et propre à la machine : un simple lien vers un rapport local n’est donc pas une transmission partagée. Placer un résumé nettoyé et les preuves nécessaires dans le répertoire de comptes rendus autorisé pour la tâche ou dans un emplacement partagé approuvé. Exclure des éléments partagés les identifiants de connexion, jetons, projets utilisateur et chemins absolus privés.

Utiliser un identifiant unique et un commentaire ajouté à l’historique par sprint. Corriger les erreurs par une correction explicite renvoyant à la note originale, sans réécrire les résultats historiques. Les comptes rendus finaux plus longs utilisent le [modèle de transmission existant](../templates/HANDOFF_RECEIPT.md).

```text
NEMO-SPRINT <ticket>-<volet>-<date-heure-UTC>-<séquence>
Humain / session d’agent / relecteur :
Objectif et résultat du sprint : <une phrase sur le comportement>
État : In progress | Review | Validate | Blocked
Base / SHA candidat / modifications non validées :
PR et chemins modifiés relatifs au dépôt :
Acceptation : <critères validés>/<total déclaré> ; nommer les critères restants
Contrôles : commande -> pass/fail/blocked/not-run -> URL de preuve + limite
Artefacts : identité source/build, jeu de test/graine, plateforme/backend, empreintes
Blocage ou décision : <quoi, humain responsable, action requise ; ou aucun>
Prochaine action exacte : <fichier/test/commande et résultat attendu>
Responsabilité : conservée | transfert demandé à <nom> | libérée par <humain>
Ressources : réservations actives ou nettoyage confirmé
Tableaux : Status/Validation prévus ; relecture des deux éléments ou réparation en attente
Prochain point d’étape : <heure UTC ou jalon convenu>
```

Pour remplacer un agent, le responsable humain confirme la transmission. Le successeur vérifie la branche/le commit/l’état non validé préservés, lit le dernier compte rendu et confirme son périmètre avant toute modification. Reprendre à l’action indiquée. Répéter un test coûteux uniquement si le code/l’environnement a changé, si le compte rendu n’est pas fiable ou si un critère restant l’exige. Un délai dépassé ne doit pas transférer automatiquement la responsabilité.

## 8. Mise à jour du tableau : partie obligatoire de la mission

### Utiliser les deux Projects existants

| Rôle | Project | Champ de statut |
|---|---|---|
| Tableau d’origine de la remédiation | [Nemo Foundation Remediation — ivg-design/8](https://github.com/users/ivg-design/projects/8) | **Status** |
| Miroir visible depuis le dépôt | [Nemo Feature Roadmap — mysteropodes/2](https://github.com/users/mysteropodes/projects/2) | **Remediation status** |

Ne pas modifier les champs ordinaires `Status`, `Board Status` ou `Category` de la feuille de route pour suivre la remédiation. Identifier le même ticket par son URL dans chaque tableau ; les identifiants des éléments, champs et options diffèrent entre tableaux. Conserver des valeurs sémantiquement identiques pour Priority, Area, Goal, Size, Phase, Program, Kind, Work package, Validation, Surface, References, les dates et le responsable de validation lorsque ces champs font partie de la mise à jour autorisée. Les valeurs existantes de Program sont Core, Editor, Platform et Collaboration ; utiliser Assignees pour distinguer les deux équipes humaines.

### Transitions d’état et responsable de mise à jour

| Événement | Status | Validation | Qui le consigne |
|---|---|---|---|
| Prévu, périmètre incomplet | Inbox | Planned | Humain ou responsable du suivi |
| Fiche complète et prérequis disponibles | Ready | Planned | Humain qui attribue/responsable du suivi |
| L’agent confirme et commence | In progress | Needs validation | Agent affecté |
| Un cas de reproduction reste nécessaire | In progress ou Blocked, avec raison | Needs reproduction | Agent affecté |
| Candidat concret et contrôles appropriés disponibles | Review | Needs validation | Agent affecté |
| Revue/intégration achevée mais validation d’exécution restante | Validate | Needs validation | Relecteur/responsable de l’intégration |
| Une dépendance/entrée manquante empêche la progression | Blocked | Conserver la valeur conforme aux faits | Agent affecté ; nommer le responsable du blocage |
| Tous les critères déclarés sont validés sur les octets intégrés | Done | Accepted | Responsable humain de l’acceptation ou responsable du suivi explicitement autorisé |

Si une modification fusionnée nécessite encore une validation avec les clients installés ou sur le bureau, utiliser Validate. Ne pas fermer automatiquement le ticket avec `Fixes`/`Closes` dans une PR partielle. Garder le parent ouvert jusqu’à acceptation de chaque enfant requis ou exception approuvée. `Paused` et `handoff` sont des dispositions de compte rendu, pas de nouvelles options de statut du tableau. Une tâche en pause dont les dépendances sont satisfaites peut revenir à Ready après libération par son humain ; elle ne doit pas paraître disponible alors que son ancien agent conserve la responsabilité.

### Séquence de mise à jour sûre, dans l’interface ou avec le CLI

1. Confirmer que la mission autorise les commentaires de ticket/PR et les mises à jour des Projects. L’attribution recommandée en section 10 fournit cette autorisation lorsque l’humain la donne réellement. Ne pas redemander confirmation pour chaque mise à jour courante dans ce cadre.
2. Lire le corps actuel du ticket, les derniers commentaires, les deux éléments, les définitions de champs et les valeurs pertinentes immédiatement avant toute écriture. Conserver `updatedAt`, une empreinte du corps du ticket et les valeurs anciennes→nouvelles dans un relevé local ignoré par Git.
3. Faire la correspondance par URL canonique complète du ticket, avec exactement un élément dans chaque tableau. Vérifier la pagination et l’exhaustivité. Un élément manquant n’autorise pas la création d’un doublon de ticket ; ajouter le ticket existant uniquement dans le cadre autorisé pour le tableau.
4. Vérifier qu’aucun autre responsable ne modifie cet élément. Publier une seule fois le compte rendu de sprint à identifiant unique ; consulter les commentaires existants si une publication précédente a expiré sans résultat clair. Relire après le commentaire pour intégrer le changement attendu d’horodatage.
5. Relire les champs immédiatement avant chaque modification. Si un humain ou un autre responsable a changé les valeurs concernées, rapprocher les changements au lieu de les écraser. Ne modifier que les champs prévus, en préservant les autres références, dates, périmètres et cases d’acceptation. Éviter de remplacer tout le corps du ticket ; si nécessaire, comparer une empreinte fraîche et ne modifier que la section attribuée.
6. Écrire les valeurs d’origine puis les valeurs correspondantes du miroir. Actualiser `Validation` et les références selon le besoin ; modifier seulement Status peut masquer une validation manquante.
7. Relire les deux éléments et le ticket. Vérifier les valeurs prévues, l’identité du ticket et les champs préservés. Ne déclarer la mise à jour terminée qu’après cette relecture.
8. Si le second tableau échoue, conserver la première écriture réussie. Consigner `BOARD-SYNC-PENDING` avec URL du ticket, tableau, valeurs anciennes/nouvelles exactes, identifiant du compte rendu et responsable. Avant toute nouvelle tentative, examiner les effets réels ; appliquer uniquement les changements encore absents. Ne pas annuler des modifications humaines plus récentes ni rejouer toute la séquence à l’aveugle.

Il s’agit de contrôles coopératifs, pas d’une transaction atomique ni d’un verrou compare-and-swap. La règle d’un seul responsable de mise à jour est essentielle. Si le compte ou le réseau empêche l’accès, enregistrer la proposition exacte et la transmettre à l’humain ; poursuivre le travail local indépendant. Ne jamais affirmer que le tableau a été mis à jour si ce n’est pas le cas.

### Exemples CLI vérifiés

Utiliser le CLI `gh` installé et son aide locale. Les commandes de lecture ci-dessous servent à la vérification préalable ; choisir une limite supérieure au total actuel et vérifier l’exhaustivité. Les totaux étaient de 58 et 220 lors de cette analyse ; ils évolueront.

```bash
gh api user --jq .login
gh issue view "$ISSUE" --repo mysteropodes/nemo \
  --json number,url,state,body,updatedAt,assignees,comments
gh project view 8 --owner ivg-design --format json
gh project field-list 8 --owner ivg-design --format json
gh project item-list 8 --owner ivg-design --limit 500 --format json
gh project view 2 --owner mysteropodes --format json
gh project field-list 2 --owner mysteropodes --format json
gh project item-list 2 --owner mysteropodes --limit 500 --format json
```

Définir `ISSUE` avec le vrai numéro du ticket attribué. À partir de JSON fraîchement lu, retrouver l’identifiant du Project, l’élément dont `content.url` correspond au ticket, l’identifiant du champ nommé et celui de l’option correspondant à la valeur voulue. Exiger exactement une correspondance ; ne jamais réutiliser les identifiants d’un autre Project ou d’un ancien compte rendu. Enregistrer un nouvel instantané immédiatement avant application.

Les commandes suivantes sont des **modèles de modification pour une mission attribuée**, pas des commandes à lancer lors d’une simple lecture de ce guide. Chaque variable doit être résolue et vérifiée ; `gh project item-edit` modifie un champ par invocation.

```bash
gh issue comment "$ISSUE" --repo mysteropodes/nemo --body-file "$RECEIPT_FILE"

gh project item-edit --project-id "$ORIGIN_PROJECT_ID" \
  --id "$ORIGIN_ITEM_ID" --field-id "$ORIGIN_STATUS_FIELD_ID" \
  --single-select-option-id "$ORIGIN_STATUS_OPTION_ID"

gh project item-edit --project-id "$MIRROR_PROJECT_ID" \
  --id "$MIRROR_ITEM_ID" --field-id "$MIRROR_REMEDIATION_FIELD_ID" \
  --single-select-option-id "$MIRROR_STATUS_OPTION_ID"
```

Répéter l’opération vérifiée champ par champ pour `Validation` si nécessaire. Pour un texte tel que References, utiliser `--text` avec le contenu frais préservé et le nouveau lien unique ; pour les dates, utiliser `--date YYYY-MM-DD`. Écrire les commentaires multilignes dans un fichier et utiliser `--body-file`. Relancer ensuite les lectures et comparer les deux éléments. Un code de sortie CLI positif ne suffit pas. Voir la [référence officielle item-edit](https://cli.github.com/manual/gh_project_item-edit) et le [guide de l’API Projects](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project/using-the-api-to-manage-projects).

## 9. Supervision humaine sans interruption permanente des agents

Utiliser les Projects existants et ajouter des vues enregistrées uniquement si les responsables les jugent utiles. Les deux humains peuvent examiner les deux équipes avec les mêmes colonnes : ticket, Assignees, statut de remédiation, Priority, Program, Work package, Validation, Validation owner, Target date, Updated et References.

- **Travail actif :** grouper par responsable humain dans Assignees, puis examiner In progress / Review / Validate. Inclure tous les états de tickets pendant la correction de l’incohérence #910.
- **Revue et intégration :** Review / Validate, avec accès à la PR candidate et au dernier compte rendu.
- **Blocages :** Blocked, avec responsable du blocage et prochaine action dans le ticket.
- **Travail suivant :** Ready, trié par priorité et dépendances concrètes satisfaites.
- **Historique d’acceptation :** Done / Accepted avec références du code/des artefacts intégrés ; garder l’historique fermé accessible.

À chaque point commun, Ilya et Cyrill examinent les changements depuis le point précédent, pas les journaux de conversation complets. Chaque humain ajoute un court bilan dans un commentaire du ticket de phase concerné, avec des liens vers les comptes rendus enfants :

```text
Équipe / point d’étape :
Accepté depuis le dernier point : <ticket, comportement, SHA/artefact intégré>
Prêt à examiner : <ticket, PR, relecteur, prochaine revue>
En cours : <ticket, critères validés, prochain critère>
Bloqué : <ticket, dépendance, humain responsable, décision nécessaire>
Changements partagés suivants : <fichier/contrat, responsable actuel, ordre proposé>
Dérive de tableau ou compte rendu manquant : <élément exact et responsable de réparation>
Prochaines missions attribuées : <identifiants et agents, ou attendre la fin des revues>
```

Mesurer **les critères acceptés et le comportement intégré**, l’attente de revue, l’ancienneté des blocages et les validations manquantes. Ne pas inventer de pourcentages d’avancement ni classer la production selon les messages d’agents, les jetons, les commits ou les lignes modifiées. Les dates prévisionnelles restent des prévisions tant qu’un humain ne s’engage pas sur une nouvelle date ; ne jamais les repousser automatiquement.

L’intégration est séquentielle même lorsque l’implémentation est parallèle :

1. Le relecteur vérifie indépendamment le candidat et ses preuves, en consignant son SHA exact.
2. Le responsable de l’intégration vérifie le main actuel et les changements conflictuels, combine les missions cohérentes et applique une seule fois le raccordement partagé.
3. Exécuter les contrôles applicables sur le candidat combiné. Des branches précédemment vertes ne prouvent pas leur composition. Recompiler/réinstaller et répéter les validations d’exécution concernées lorsque les octets pertinents changent.
4. Publier/fusionner uniquement selon l’autorité réellement donnée et les protections normales de PR. Les builds hébergés restent soumis à leur restriction distincte.
5. Consigner le SHA intégré, le résultat d’acceptation et les limites restantes. Passer à Validate ou Done selon les vrais critères ; actualiser les deux tableaux et la progression des parents.

## 10. Instruction à copier pour l’agent local de l’un ou l’autre humain

Remplir les valeurs entre crochets avant de donner cette instruction. La ligne de publication est une proposition d’autorisation explicite pour une mission normale d’implémentation ; l’humain peut la restreindre. Une mission de relecture ou de collecte de preuves doit le préciser.

```text
Tu travailles à la remédiation des fondations de Nemo sous la direction de
[Ilya / Cyrill]. Ta tâche est [URL du ticket + identifiant de mission].
Ton responsable humain est [nom] ; ton relecteur est [nom]. Lis la fiche
attribuée, le dernier compte rendu de sprint, les PR pertinentes, AGENTS.md,
CONTRIBUTING.md et la documentation applicable de remédiation/des modules.

Résultat attendu : [résultat observable]. À préserver : [invariants].
Chemins modifiables : [fichiers/répertoires exacts relatifs au dépôt].
Limites partagées/en lecture seule : [chemins et responsables].
Base : [SHA complet] ; branche/worktree : [identité].
Dépendances satisfaites : [preuves]. Jalons restants : [critères].
Ressources : [racines isolées données/build/rapports et réservations bureau/GPU].
Point d’étape : [heure ou jalon].

Autorité pour cette mission : implémenter et valider localement, créer des
commits limités au périmètre, pousser cette branche, ouvrir/actualiser sa PR,
publier ses comptes rendus de sprint dans le ticket/la PR et actualiser ses
éléments existants de remédiation dans les deux Projects selon le guide.
Cela n’autorise ni fusion, version publiée, déploiement, clôture de ticket,
nouveau ticket, message sans rapport, changement de compte ni exécution
hébergée d’Actions. Si une autre action a déjà été explicitement autorisée,
conserve cette autorité.

Travaille en continu dans ton périmètre. N’envoie pas d’instructions aux agents
de l’autre humain et ne reprends pas leurs fichiers. Mets les nouvelles
demandes en file sans abandonner cette tâche. Si un contrat/chemin partagé
doit changer, transmets la proposition et le cas de reproduction minimaux au
responsable humain, puis continue le travail indépendant dans ton périmètre.

À chaque fin de sprint, avant une pause/réinitialisation de contexte et lorsque
le travail est prêt à examiner : préserve branche/commit et modifications non
validées ; publie un compte rendu NEMO-SPRINT ; actualise Status à l’origine et
Remediation status dans le miroir, ainsi que Validation si applicable ; relis
les deux ; consigne la prochaine action exacte, la responsabilité et le
nettoyage. Déclare honnêtement les échecs de synchronisation. Ne répète pas un
travail déjà prouvé sur des octets équivalents et ne déclare pas une fusion
acceptée tant que les contrôles d’exécution requis restent à effectuer.

Rends : résultat, candidat/PR, contrôles ciblés et preuves, limites,
acceptation restante, prochaine action exacte et état vérifié des tableaux.
```

## 11. Références et maintenance

- [Architecture actuelle/cible](../01_CURRENT_AND_TARGET.md), [séquence de remédiation](../02_REMEDIATION_PLAN.md), [contrat de travail parallèle](../07_GITHUB_PROJECT_AND_PARALLEL_WORK.md).
- [Correspondance des Projects et sens des états](../../project-management/README.md), [politique d’exécution locale de CI](../../ci/README.md), [isolation des environnements](../../runtime-isolation.md).
- [Tranche opacité et autorité](../../application/OPACITY_SLICE.md), [profil source adopté R05](../../boundaries/profiles/app-surfaces.md), [code au main examiné](https://github.com/mysteropodes/nemo/tree/66ece0641708122eb8447e85ad8dd7e3402aaf6c).
- [Intégration application/MCP fusionnée #992](https://github.com/mysteropodes/nemo/pull/992), [preuves de packaging #1001](https://github.com/mysteropodes/nemo/pull/1001), [correction du payload #1002](https://github.com/mysteropodes/nemo/pull/1002).

Maintenir les deux éditions ensemble. Préserver les numéros de tickets, chemins, commandes, noms de schémas et valeurs exactes des champs/options GitHub lors de la traduction. L’état du travail appartient aux tickets et Projects actifs, pas à des réécritures répétées de cette analyse datée. Réviser le protocole uniquement lorsque les deux responsables modifient leur accord de fonctionnement.
