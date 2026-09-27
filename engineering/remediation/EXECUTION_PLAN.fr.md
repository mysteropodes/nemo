# Nemo — liste de contrôle pour l’exécution de la remédiation

Stratégie approuvée : **7 septembre 2026** ; amendement sur le moteur natif approuvé le **20 septembre 2026** ; séparation native approuvée le **22 septembre 2026**. Responsables humains : **Ilya** (`ivg-design`) et **Cyrill** (`mysteropodes`). [Version anglaise](EXECUTION_PLAN.en.md).

## Avant-propos — Ilya

Après avoir examiné notre progression et le temps consacré au projet pendant le week-end, j’ai estimé qu’il nous fallait une marche à suivre plus claire pour mener la remédiation à son terme. En particulier, « l’état de référence » avait été interprété différemment : je voulais documenter Nemo tel qu’il fonctionne actuellement, défauts compris, alors que certaines tâches se sont orientées vers la correction de ces défauts avant la restructuration du code.

Aucun de ces efforts n’a été perdu. Les corrections, les tests, les investigations et le travail d’intégration nous donnent un point de départ plus solide. Ce plan révisé s’appuie sur ces acquis tout en recentrant le travail sur la modularisation, la protection contre les régressions et l’extensibilité. Des tâches plus petites, des responsabilités claires, des agents locaux et un seul tableau de progression partagé devraient nous aider à avancer en consacrant moins de temps à la coordination.

Ce document est le plan opérationnel unique de la remédiation en cours. Il remplace l’ordre d’exécution, les prévisions, les validations globales bloquantes, les hypothèses sur les agents distants et les exigences de compte rendu de l’ancien plan R00–R22 et des guides d’agents locaux. Les documents d’architecture et de code existants restent des références ; en cas de conflit, le présent document régit le périmètre et le fonctionnement. Les issues GitHub consignent les prises en charge et les transmissions en cours ; cette liste définit les résultats attendus. Le [journal horaire partagé de progression #1062](https://github.com/mysteropodes/nemo/issues/1062) est l’unique destination centrale des comptes rendus ; ne créez pas de registres de sprint concurrents ni de PR de compte rendu.

## 0. Séparation native actuellement applicable — 22 septembre 2026

**Cet amendement est la règle opérationnelle actuelle.** Il remplace tout texte transitoire
plus bas qui exige une libération du document natif vers legacy, un ancien writer dormant
et passant, ou une parité complète avant la déconnexion de ce writer. Les implémentations
et reçus déjà acceptés restent des preuves, pas une obligation de faire fonctionner deux
moteurs d’édition ensemble. Mettre à jour les issues concernées et Project #2 avant de
reprendre un résultat modifié ; ne pas compter le travail de coexistence annulé comme
une fonctionnalité finale livrée.

- Préserver le `main` opérationnel au SHA de séparation
  `1065e9d8d9c48e66d9b44bb4de446e8f4e887040` pendant la remédiation sur la branche
  d’intégration commune et protégée `codex/native-remediation`. Ce SHA identifie le code,
  pas un artefact opérationnel installé et vérifié indépendamment. Les branches de tâche
  ciblent l’intégration par PR revues ; seule une PR finale entièrement acceptée promeut
  le résultat vers `main`. Une correction opérationnelle autorisée séparément peut rester
  sur `main`, puis être évaluée une fois pour l’intégration sans fusionner sans cesse les
  deux lignes.
- Distinguer trois références : la base fonctionnelle convenue d’origine et ses défauts
  caractérisés ; le code/build opérationnel préservé pour récupération ; et le code de
  remédiation accepté avec ses travaux inachevés sous responsabilité. P03 et les issues
  liées rapprochent les workflows de référence du recensement des sources figé. Un nombre
  de paquets ou d’issues fermées n’est pas un pourcentage de parité fonctionnelle.
- Rust est l’unique autorité du runtime de remédiation pour documents/révisions persistants,
  commandes/historique, évaluation, cycle de vie médias/ressources/GPU, production du
  viewport et export. JavaScript reste la couche UI/scripts/présentation ; Paper.js peut
  fournir géométrie, hit-test ou présentation justifiés, jamais un second document
  modifiable ou évaluateur d’autorité. UI, SDK, MCP et scripts applicables utilisent l’API
  commune de commandes/requêtes/jobs. Les capacités navigateur couvertes emploient
  l’adaptateur WASM/hôte déclaré, non une ancienne autorité JavaScript indépendante.
- Une opération non migrée peut être visiblement indisponible sur la branche d’intégration
  et doit être refusée avant toute mutation du document ; elle ne peut se replier sur
  l’ancien moteur modifiable. Déconnecter ou retirer le code runtime remplacé à mesure que
  sa responsabilité migre, même avant la parité finale de cette fonctionnalité. Ses
  fixtures et son obligation en attente restent visibles. À la promotion finale,
  **100 % des fonctionnalités de référence convenues et des surfaces applicables** doivent
  être acceptées ou disposer d’une décision précise sur un défaut déjà présent dans la
  base. Cela comprend l’import des anciens projets, les expressions et la sémantique
  publique des scripts/plugins ; une indisponibilité temporaire ne ferme jamais une tâche.
- Préserver les issues, propriétaires, modules natifs acceptés, modules UI/domaine extraits,
  fixtures, diagnostics et tests. Requalifier N19B pour fermeture/réentrée natives ;
  N19C/N19D pour refus ou déconnexion des anciennes entrées ; N19G pour intention de
  sélection pure, commandes natives des opérations implémentées et refus des gestes non
  migrés ; N19E pour remplacement natif attendu ; N19F pour synchronisation révision/UI ;
  N20 pour le premier workflow d’édition continûment natif ; N21 pour l’acceptation réelle
  bureau/navigateur sans aller-retour legacy. L’issue propriétaire consigne précisément
  les critères conservés, remplacés et encore ouverts avant la reprise de son writer.

La suite utilise les tâches bornées existantes : **A** préserver `main`, isoler l’app et
les données de développement et connecter un premier shell/viewport natif ; **B** achever
document/édition/historique/persistance natifs ; **C** animation et édition directe ;
**D** familles de scènes complexes ; **E** médias/sortie et tous les consommateurs
UI/API/navigateur ; **F** clôture structurelle, parité à dénominateur fixe, acceptation
installée/client et promotion finale protégée. Les contrôles du renderer/viewport,
de sauvegarde/historique et d’export pertinent commencent dès la première fonctionnalité
utilisable, pas seulement à la fin de la vague E. Paralléliser les propriétaires
indépendants après le contrat commun commande/révision ; garder un writer par fichier
entier et une seule prévision intégrée.
Réutiliser l’isolation existante des données par tâche et les outils de build natif pour
l’application de développement ; vérifier identité et chemins réels plutôt que créer un
second système d’isolation. La [prévision du travail restant du 22 septembre](https://github.com/mysteropodes/nemo/issues/1062#issuecomment-5785104824)
est une allocation de planification datée, pas un pourcentage de fonctionnalités livrées
ni un remplacement du dénominateur fixe des workflows de référence.

Pour chaque test modifié, conserver les oracles indépendants de comportement, adapter les
assertions couplées à l’ancienne structure d’appels, retirer les tests portant uniquement
sur le transfert d’autorité supprimé et garder visiblement en attente tout comportement
requis temporairement indisponible. Un sous-ensemble de développement réussi n’est pas
une acceptation complète. Chaque branche de tâche acceptée s’intègre progressivement
dans la branche protégée avec ses contrôles réels ; la promotion finale exige la matrice
complète de parité, les preuves à partir d’un clone propre, installé, visuel, navigateur
et client, ainsi que la revue humaine normale.

### Pivot antérieur vers le moteur natif et admission ordonnée — 20 septembre 2026

Le pivot du 20 septembre conservait l’interface Tauri/JavaScript tout en visant un moteur Rust natif comme autorité unique sur les révisions du document, l’application des commandes et de l’historique, l’évaluation et l’ordonnancement, le cycle de vie des médias/ressources/GPU, la production des images du viewport et les jobs d’export. Il préparait Paper.js comme adaptateur de compatibilité pour l’édition, la sélection et le hit-test à la frontière de l’interface et conservait le navigateur/WASM comme adaptateur déclaré séparément. Le contrat du 22 septembre ci-dessus supprime l’éditeur de compatibilité modifiable du runtime de remédiation ; ce paragraphe rappelle l’admission antérieure et n’exige pas un repli actuel.

**La phase 0 est le réalignement de Project #2 et du plan ; elle précède l’implémentation du moteur.** Exécuter le travail admis dans cet ordre de dépendances :

- [x] **Le tableau d’abord :** créer [R23 / #1327](https://github.com/mysteropodes/nemo/issues/1327) et la file bornée N00–N05 dans le [Project #2](https://github.com/users/mysteropodes/projects/2) canonique, avec les relations natives de sous-issues et de blocage, tous les champs et un README de projet mis à jour. La relecture courante du 20 septembre confirme R23 `In progress`, avec N00–N05 `Already there` / `Done` / `Accepted` ; les états admis de N06–N21 sont consignés ci-dessous.
- [x] **[N00 / #1329](https://github.com/mysteropodes/nemo/issues/1329) :** publier cet amendement anglais/français synchronisé et les règles actives d’entrée dans le dépôt. Aucun travail sur le code source du moteur ne commence avant la revue, la fusion et l’acceptation de N00.
- [x] **[N01 / #1328](https://github.com/mysteropodes/nemo/issues/1328) :** après N00, rétablir la baseline de la file de remédiation en direct sur le main actuel et cartographier chaque tâche conservée, remplacée ou nouvellement bloquante sans effacer les acceptations ni les responsabilités existantes.
- [x] **[N02 / #1331](https://github.com/mysteropodes/nemo/issues/1331) :** après N00, figer les contrats du moteur natif, de révision, de commande/requête/job, de transition et d’adaptateur, avec deux exemples concrets de compatibilité/échec et des frontières d’autorité explicites.
- [x] **[N03 / #1330](https://github.com/mysteropodes/nemo/issues/1330)** et **[N04 / #1332](https://github.com/mysteropodes/nemo/issues/1332) :** après N02, démontrer indépendamment l’évaluation Rust headless et un viewport Tauri natif sur des jeux d’essai représentatifs. Consigner honnêtement les échecs mesurés et les résultats non pris en charge ; une simple compilation ne vaut pas acceptation de faisabilité.
- [x] **[N05 / #1333](https://github.com/mysteropodes/nemo/issues/1333) :** seulement après les deux jalons de faisabilité, inscrire la file bornée d’implémentation de production avec fichiers, responsables, SHA prédécesseurs, fixtures de parité, matrices de consommateurs, conditions de retrait et commandes de validation locale exacts.

La migration de production progresse par responsabilités caractérisées via les mêmes commandes, requêtes et jobs applicatifs versionnés qu’utilisent l’interface et le MCP Rust livré. Commencer par la fixture d’opacité acceptée et déplacer derrière les contrats figés la propriété des révisions persistantes, l’évaluation immuable, l’ordonnancement et les ressources natifs, la présentation bureau et l’export à révision fixe. Sur la branche séparée, un ancien writer obsolète peut être déconnecté ou retiré avant la parité finale de son remplacement ; marquer alors la fonctionnalité manquante en attente et refuser son opération sans repli. Les preuves applicables de sauvegarde/chargement, annulation/rétablissement, sélection, animation, rendu, export, pont natif, navigateur et application bureau installée restent requises pour l’acceptation finale sur des candidats identifiés.

Cet amendement préserve le travail de remédiation accepté et les responsabilités existantes. En particulier, [P03 / #1005](https://github.com/mysteropodes/nemo/issues/1005), [P16 / #1018](https://github.com/mysteropodes/nemo/issues/1018), [#1316](https://github.com/mysteropodes/nemo/issues/1316) et sa [PR #1323](https://github.com/mysteropodes/nemo/pull/1323) actuelle ne sont ni réassignés ni fermés silencieusement. R23 est un parent de suivi ; seules les tâches exécutables nommées et leurs relations natives de blocage conditionnent le travail.

## 1. Résultat attendu, point de départ et limites

- [ ] Terminer avec chaque responsabilité convenue du code écrit manuellement derrière une frontière de module publique claire, un seul propriétaire de l’état, des tests de non-régression adaptés, des règles de taille et de dépendances appliquées automatiquement et un contrat de capacité de fonctionnalité relié au MCP Rust livré avec le projet.
- [ ] Préserver exactement le comportement observé, y compris les défauts existants documentés. Un état de référence identifie le SHA du code, les jeux d’essai et leurs empreintes, l’environnement et les observations de réussite, d’échec, de blocage ou de non-exécution. Il n’exige **pas** de réparer toutes les fonctionnalités.
- [ ] Réparer un défaut au cours d’une extraction uniquement si cette extraction l’a introduit, ou s’il empêche d’extraire le module retenu ou de le valider de manière pertinente. Consigner les défauts sans rapport comme dette produit avec un scénario de reproduction circonscrit ; poursuivre la remédiation.
- [ ] Préserver la compatibilité des projets, l’identité, l’historique et les consommateurs concernés : sauvegarde/chargement, sélection, animation, rendu/export et interfaces natives. Les défauts connus doivent avoir des signatures exactes ; une dérogation générique pour échec connu ne peut pas masquer une nouvelle régression.
- [ ] Garder hors de ce programme les nouveaux effets OpenFX, une implémentation complète d’OCIO/EXR/OTIO, une nouvelle infrastructure de transport Buzz, les améliorations générales de performances et les ajouts de fonctionnalités produit. Définir leurs interfaces, leur disponibilité et leurs contrats de données lorsque nécessaire ; ne pas élargir le produit pour clôturer la remédiation. L’éditeur d’instructions d’espace de travail Buzz autorisé séparément est une amélioration actuelle de la coordination, pas un critère de clôture structurelle de Nemo.
- [ ] Traiter les catalogues générés, tiers et de données selon une politique précise de provenance et d’intégrité. La logique de contrôle écrite manuellement ne peut pas échapper aux contrôles de modularité en étant étiquetée comme catalogue. Le nombre de lignes sert la cohérence des API ; découper arbitrairement des fichiers ne constitue pas une acceptation.

L’audit du code a porté sur `66ece0641708122eb8447e85ad8dd7e3402aaf6c`, et non sur l’ancienne copie locale de main. Réutiliser l’état de référence F0 fidèle aux observations, l’extraction des courbes déjà fusionnée, l’analyseur de projet, le service applicatif d’opacité, les tests de consommateurs existants et le transport MCP Rust. Dans la source auditée de l’instantané antérieur ci-dessous, P05 avait ajouté le registre déterministe des capacités applicatives ; P06 publiait le descripteur d’opacité enregistré complet par la découverte applicative ; P07 devait encore terminer sa consommation par le MCP Rust. Le profil JavaScript de l’application contenait 152 modules et en classait encore 140 comme `App JS unclassified (pre-R01)`. Il s’agit de constats sur le code, pas de résultats de tests nouvellement exécutés ; N01 porte le rétablissement de la baseline sur le main actuel.

### Réconciliation antérieure — 12 septembre 2026

Cet instantané a été vérifié sur `origin/main` `1cad07768b884cf44c0380e2236c2d3133051cf8`, les issues et PR élémentaires en direct et Project #2. Vingt-huit tâches finies de la liste initiale possédaient une acceptation terminale/Done et étaient fermées : P01, P02, C01–C08, F01, F02, P14, P15, P04, P05, P06, P11, P13, T01–T04, D02, P20, P22, A01 et P28. Leurs cases de résultat sont cochées plus bas. Les listes opérationnelles récurrentes des sections 1, 3, 5–7 et 10 ainsi que tous les jalons finaux de la section 9 restaient ouverts.

Cinq tâches supplémentaires de recensement issues de C08 étaient aussi acceptées et fermées : [C09 / #1091](https://github.com/mysteropodes/nemo/issues/1091), [C13 / #1092](https://github.com/mysteropodes/nemo/issues/1092), [C15 / #1093](https://github.com/mysteropodes/nemo/issues/1093), [C17 / #1094](https://github.com/mysteropodes/nemo/issues/1094) et [C18 / #1095](https://github.com/mysteropodes/nemo/issues/1095). [T09 / #1105](https://github.com/mysteropodes/nemo/issues/1105) était fermée par une décision hors code relue : le décodage natif restait indisponible sur ce poste ; aucune correction produit ni réussite du décodage natif n’était revendiquée. P03 restait ouverte jusqu’à la consolidation du recensement accepté, le gel de l’empreinte de l’ensemble de sources et l’inscription de toute la file d’extraction bornée.

D01 restait prise en charge dans son worktree dédié. P06 était acceptée et fusionnée par la [PR #1112](https://github.com/mysteropodes/nemo/pull/1112) sous `1cad07768b884cf44c0380e2236c2d3133051cf8` : le candidat exact `9489214` avait passé la revue technique indépendante ainsi que les deux contrôles réels Chrome de consommateur et de geste, sans nouvelle tentative. La transmission de P07 était réconciliée ; la [PR #1113](https://github.com/mysteropodes/nemo/pull/1113) restait à `5aa8026ef412f70f4caf1e391b6447657d5cd12a` avec des modifications demandées. T05 conservait sa réservation ; son prédécesseur P06 était accepté. À cet instantané, la file était réconciliation du plan → P07 pendant que D01 progressait séparément ; la section 0 et les issues en direct régissent désormais la priorité actuelle.

Le champ de feuille de route `Status` de Project #2 indiquait `Already there` pour les tâches fermées, mais son champ `Validation` ne constituait pas un décompte d’acceptation fiable : seule P01 indiquait `Accepted`, la plupart des tâches fermées indiquaient encore `Planned`, et F01/F02 indiquaient encore `Needs validation`. Les cases cochées plus bas reposent sur la preuve terminale d’acceptation dans l’issue/PR et sur la présence dans le main de référence, pas seulement sur la fermeture ou l’état du tableau.

Les issues de départ ci-dessous sont des tâches d’entrée concrètes, sans prétendre que leur nombre couvre tous les monolithes. C01–C08 et les tâches supplémentaires C09/C13/C15/C17/C18 cartographient l’ensemble de sources fixé ; P03 inscrit les tâches élémentaires restantes, chacune limitée à une responsabilité. Cette cartographie vient en premier, et chaque responsabilité non couverte doit recevoir sa propre tâche avant toute prévision de fin. Aucune famille ultérieure ne peut être dissimulée derrière une issue de plusieurs jours intitulée « tout migrer ».

## 2. Deux équipes humaines, trois postes chacune

| Poste | Ilya | Cyrill |
|---|---|---|
| **O — orchestrateur** | `gpt-6-astra`, **high**. Responsable des contrats partagés, des règles sur le code et la couverture, de l’ordre d’intégration, de la validation combinée et du tableau canonique. | `opus`, **high**. Responsable de la validation technique, des fusions et de la clôture au tableau de son équipe. Revue aux jalons ; responsable des prises en charge de l’équipe, des transmissions Fizz/Honey existantes, de l’acceptation Claude/native et des décisions de quota. |
| **D1 — délégué** | Préférer `gpt-6-sol`, **medium** pour une implémentation bornée ; augmenter l’effort si l’autorité/l’historique est ambigu. | `sonnet`, **medium**. Tests ciblés, fonctions utilitaires pures, analyseurs/adaptateurs natifs et enregistrement des fonctionnalités. |
| **D2 — délégué** | Préférer `gpt-6-luna`, **low/medium** pour revue, tests et inventaire ciblés ; utiliser Sol ou Astra si la complexité l’exige. | `sonnet`, **medium**. Couverture et rapports, tests navigateur isolés, préférences/Labs et interface de diagnostic. |

Ces réglages de session sont des préférences, pas des garanties de quota ni des capacités rigides des postes. Utiliser `gpt-6-astra` quand la tâche le justifie. Vérifier le modèle et l’effort réels lors de la première prise en charge ; choisir un modèle disponible et capable si la préférence ne l’est pas, puis consigner la substitution. Aucun choix de modèle ne crée un quatrième poste.

Pour chaque agent lancé dans la tâche Codex actuelle, inclure le modèle et l’effort de raisonnement choisis dans le nom local de sa tâche. Ne pas renommer les agents Buzz ; Buzz affiche leur modèle séparément.

Cyrill commence avec **un seul** délégué Sonnet et n’active le second que s’il dispose d’un travail indépendant et d’un quota suffisant. Opus reçoit un dossier d’issue compact et un compte rendu de jalon, sans interrogation continue ni relecture répétée de tout le dépôt. Éviter l’effort maximal automatique et les variantes à contexte étendu. Si les quotas deviennent faibles, pousser le point de reprise et libérer ou conserver explicitement la prise en charge ; Ilya ne peut reprendre une tâche qu’après cette transmission. Les humains assignés aux issues restent responsables même lorsque les agents changent.

Exemples de lancement depuis des copies de travail déjà allouées ; ces commandes créent des sessions, pas des branches :

```sh
codex --model gpt-6-astra -c 'model_reasoning_effort="high"'
codex --model gpt-6-sol -c 'model_reasoning_effort="medium"'
codex --model gpt-6-luna -c 'model_reasoning_effort="low"'
claude --model opus --effort high
claude --model sonnet --effort medium
```

Pour les sessions dans l’application, utiliser ses réglages de modèle et d’effort. Les alias Claude se résolvent selon le compte et le fournisseur ; consigner la version réelle de la session. Vérifier la disponibilité actuelle des modèles dans l’application plutôt que de l’inférer des anciennes prises en charge. Les niveaux d’effort des différents fournisseurs ne sont pas des mesures équivalentes.

### Compétences portables : appliquer directement ces procédures

Aucune installation de compétence personnelle n’est nécessaire. La compétence locale `nemo-a2a` n’est pas suivie dans la version de main auditée ; Cyrill ne doit donc pas en dépendre. Des enveloppes de compétences d’agent facultatives peuvent pointer vers ce document ; elles ne doivent pas le copier puis diverger. Utiliser des agents locaux pour l’implémentation. Buzz fournit la conversation partagée, les prompts planifiés rédigés manuellement et les liens vers le journal central de progression ; cela n’autorise pas à lancer des workers distants ni à dupliquer les prises en charge locales.

| Compétence nommée dans une issue | Méthode étape par étape | Références suivies dans le dépôt |
|---|---|---|
| `orchestration` | Vérifier responsable et dépendances → allouer un seul rédacteur → examiner les preuves → intégrer → relire le tableau → libérer les ressources. | `AGENTS.md`, `CONTRIBUTING.md`, ce plan. |
| `inventory` | Énumérer les entrées suivies → identifier responsabilité, état et consommateurs → répartir les chemins exacts → assigner un propriétaire principal → inscrire de petites tâches élémentaires. | Fichiers d’inventaire et de profils existants, code concerné. |
| `architecture` | Examiner l’autorité existante → écrire deux exemples concrets de contrat → identifier les cas de compatibilité et d’échec → approuver des interfaces étroites. | `CLAUDE.md`, architecture actuelle/cible, code applicatif. |
| `extraction` | Caractériser → définir l’API publique → déplacer une responsabilité → rediriger les véritables appelants → retirer l’ancien propriétaire des écritures → vérifier les consommateurs. | Section pertinente de `CLAUDE.md`, documentation locale du module/nœud, politique de modularité. |
| `capabilities` | Déclarer le schéma typé et le gestionnaire de la fonctionnalité → valider l’enregistrement → exposer via l’API partagée et le MCP → tester la découverte, le routage et la disponibilité réels. | Tranche applicative d’opacité, code MCP et tests stdio. |
| `validation` | Utiliser des résultats attendus indépendants → exécuter sur la surface requise → fournir couverture et trace → exercer un contrôle négatif → classer les limites. | Guide de test, README des jeux d’essai et du bureau, `scripts/nemo/README.md`. |
| `diagnostics` | Corréler les commandes existantes → borner la capture → inspecter/exporter via le service partagé → rejouer un jeu d’essai synthétique isolé. | Tests existants de trace/rejeu d’opacité et tâches de diagnostic ci-dessous. |

Avant de donner des instructions sur les réglages d’un nœud, lire sa documentation locale et son implémentation. Ne lire que les contrats et le code utiles à la tâche en cours ; ne pas refaire un audit complet à chaque reprise.

## 3. Taille des tâches, prises en charge et exécution quotidienne

**Une issue exécutable = un résultat observable = un rédacteur = une PR cohérente.** Viser 30 à 90 minutes d’implémentation, plus au maximum 30 minutes de revue. C’est une limite de périmètre, pas la promesse qu’une compilation finira dans un délai chronométré. Si une tâche exige manifestement plus, la scinder avant de commencer. Une validation longue peut continuer sur un candidat figé pendant qu’une autre tâche Ready sans chevauchement progresse.

Utiliser les sous-issues natives sous R03/R05/etc. pour mesurer l’avancement. Ces anciennes issues sont des parents de suivi, pas des missions de délégué ni des prérequis globaux. Seules les dépendances explicitement nommées entre tâches élémentaires bloquent le travail. Les travaux futurs non commencés sont Inbox ; Blocked signifie un obstacle réel, pas l’ordre normal de la file. Ne pas passer à Done simplement parce qu’un créneau s’est terminé.

### Démarrage et points de contrôle de l’orchestrateur

- [ ] Lire ce plan, la vue filtrée du tableau de l’équipe et la dernière transmission de chaque tâche active. Récupérer la branche protégée d’intégration de remédiation et vérifier le `main` préservé, examiner les PR et les worktrees, préserver les prises en charge actives. Le silence ne libère jamais une prise en charge.
- [ ] Réserver au maximum deux worktrees de tâches en écriture par machine, en plus de la copie principale. Les revues en lecture seule, mises à jour du tableau et notes ne créent pas de worktree. L’orchestrateur effectue les intégrations en série dans une copie de tâche propre et disponible ; ne pas ajouter une quatrième copie uniquement pour l’intégration.
- [ ] Allouer uniquement des tâches Ready dont les prédécesseurs sont intégrés/acceptés et dont les **fichiers entiers** ne se chevauchent pas. Un partage par fonctions dans `motion.js`, `timeline.js`, `tweens.js`, `tools.js` ou `engine-bridge.js` n’autorise pas plusieurs rédacteurs simultanés.
- [ ] Laisser à l’orchestrateur d’Ilya le câblage partagé de `package.json`, du fichier de verrouillage, du bootstrap/index, des profils, du manifeste généré et de `scripts/nemo/ci.cjs`. Les délégués fournissent des modifications ciblées dans leur branche de tâche ; l’orchestrateur intègre ces portions partagées en série.
- [ ] Examiner l’issue modifiée au démarrage du délégué, en cas de blocage, lorsque la revue devient possible et en fin de sprint. Pendant le travail actif, un rappel horaire déclenche un compte rendu central de chaque orchestrateur. Lire les derniers changements et l’état du tableau, pas tout l’historique ; ne pas interrompre l’appel d’outil d’un délégué uniquement pour obtenir un compte rendu. Une équipe en pause publie un seul avis de pause/transmission et suspend sa minuterie.
- [ ] Examiner un blocage lors du point de contrôle courant si disponible : résoudre la question de frontière, fournir une dépendance ou isoler une petite tâche de diagnostic. Ne pas laisser un délégué passer un second sprint à réparer des défauts produit sans rapport.
- [ ] Avant chaque intégration, vérifier le SHA exact du candidat, le diff circonscrit, les critères d’acceptation et les preuves d’exécution pertinentes. Exécuter les contrôles du candidat combiné après les changements réels d’intégration ; des branches isolées précédemment vertes ne prouvent pas le résultat combiné.
- [ ] À chaque point horaire et en fin de session, réconcilier les tâches acceptées, actives ou bloquées et la progression des sous-issues des parents. Ajouter le compte rendu structuré au journal partagé de progression et le relier depuis la conversation Buzz désignée. Conserver les preuves détaillées dans l’issue ou la PR responsable. Aucune PR de point de contrôle ni dossier de rapports en double.

### Délégué : démarrage → travail → transmission

- [ ] Lire la tâche et son dernier compte rendu. Confirmer l’humain assigné, le modèle et l’effort, les SHA réels des prédécesseurs et l’existence d’un seul rédacteur actuel. Examiner le code concerné et les tests existants avant toute création.
- [ ] Consigner dans l’issue le SHA de base, la branche de tâche, les chemins proposés, le résultat observable attendu et les commandes de validation ciblées nommées. Les nouveaux chemins de cette liste sont des cibles proposées, pas l’affirmation que les fichiers existent déjà.
- [ ] Réutiliser la même branche pour ce résultat d’un sprint à l’autre. Créer une branche et un worktree uniquement lorsque la tâche a besoin d’un poste d’écriture ; utiliser `codex/<task-id>-<topic>` pour Codex ou le préfixe existant de l’équipe. Réutiliser une PR dont on est responsable plutôt que créer une PR de compte rendu ou de session.
- [ ] Établir le jeu minimal de caractérisation avant de modifier. Conserver des attentes indépendantes et les échecs existants exacts. Ne pas régénérer une référence attendue à partir de la nouvelle implémentation pour faire disparaître un échec.
- [ ] Extraire une responsabilité derrière une API publique. Conserver une seule autorité pour l’état persistant. Rediriger les véritables consommateurs et retirer l’ancien propriétaire des écritures ou la façade remplacée dans le même résultat. Une extraction de fonction utilitaire renvoie à la capacité de sa fonctionnalité ; chaque fonction interne n’a pas besoin d’un outil MCP.
- [ ] Exécuter les tests de la tâche et les contrôles applicables de couverture, taille/dépendances et schéma/enregistrement. Élargir les tests uniquement pour un risque restant concret. Distinguer les résultats navigateur, natifs, d’installation empaquetée et de client réel.
- [ ] Créer un commit cohérent et révisable portant l’ID de tâche et un résultat concret. Faire un commit avant une transmission ou une interruption si les modifications suivies sont significatives ; un point de reprise WIP est permis, mais doit nommer les contrôles échoués/non exécutés et ne pas être présenté comme prêt à fusionner.
- [ ] Pousser la **même branche** lorsqu’elle est prête pour revue et avant de terminer un sprint ou de transférer la responsabilité avec du travail commité. Les modifications locales ordinaires n’exigent pas des poussées répétées. Garder journaux et produits de compilation hors du code ; joindre des rapports expurgés ou des références d’artefacts reproductibles à l’issue ou à la PR existante.
- [ ] Mettre à jour l’issue et le tableau au démarrage, lors d’un blocage significatif, à la préparation de la revue et en fin de sprint. Ne jamais abandonner silencieusement sa responsabilité, réassigner un autre flux, fusionner son propre travail non relu ni clôturer la validation d’intégration.
- [ ] En cas d’interruption, rédiger la transmission compacte ci-dessous. Le successeur la lit, vérifie l’état réel du code et du dépôt distant puis reprend à l’action suivante ; il ne relance pas toute la mission initiale depuis zéro.
- [ ] Ne jamais écrire « je reprends »/« je continue » sans vérifier l’état réel (`git status`/`reflog` du worktree, `gh pr list`) et le citer dans le même message ; une intention non vérifiée ne vaut pas un compte rendu.
- [ ] « Terminé » ou « en cours » exige un commit poussé ou une PR existante à citer ; sans cette preuve, l’état réel est « non commencé », même annoncé la veille.
- [ ] Avant de reprendre la tâche laissée par un autre rédacteur : le signaler d’abord dans le journal/la conversation partagée, pour éviter une reprise en double si l’autre agit entretemps.
- [ ] Diviser puis déléguer (A2A ou second rédacteur) toute tâche croisée bloquée ou manifestement surdimensionnée plutôt que d’attendre en silence ou de la laisser inactive.

Modèle de transmission, à utiliser dans un commentaire d’issue ou dans sa section de transmission actuelle :

```text
Task / human / lane / actual model-effort:
Base → candidate SHA / branch / PR:
Outcome reached; acceptance checks passed / remaining:
Exact commands + result; coverage/report/trace links:
Known baseline failure / new regression / unavailable environment:
Next concrete action; blocking issue if any:
Whole-file/runtime claim retained or released; pending unpushed work:
```

Ne pas créer de commit uniquement pour mettre à jour ce modèle. Une note sans nouveau code appartient à l’issue. Publier les comptes rendus planifiés de coordination uniquement dans le journal partagé et la conversation Buzz désignés ; les messages directs et les nouveaux travaux d’agents distants exigent une autorisation distincte.

## 4. Tests, couverture et débogage intégré

Conserver **Node `node:test` + `node:assert`**, **Playwright** et les **tests Cargo**. Ajouter **c8** pour la couverture JavaScript et **cargo-llvm-cov** pour Rust. Nemo a déjà retenu Node dans [la comparaison consignée des exécuteurs](../animation/ADR-001-curve-runner.md) ; ne pas refaire l’essai ni migrer la suite vers Vitest pendant cette remédiation. Ne reconsidérer l’exécuteur que lorsqu’un besoin réel TS/ESM/composants le justifie.

| Couche | Outil et objectif | Rapport / acceptation requis |
|---|---|---|
| JS pur et contrats applicatifs | `node:test`, avec import des modules de production ; bootstrap VM restreint uniquement pour les consommateurs de scripts hérités. | Compte rendu de test lisible par machine ; invariants indépendants d’identité, d’historique, de codec, d’horloge et d’échec. |
| Couverture du JS migré | c8 avec inclusion explicite des sources et `--all`. | HTML pour les humains, LCOV pour les outils, JSON pour les agents ; minimum provisoire par module de 90 % des lignes / 80 % des branches pour le code domaine/applicatif migré. Les fichiers non importés restent dans le dénominateur. |
| Parcours navigateur réels | Playwright sur l’application livrée, ses véritables contrôles et les consommateurs pertinents de sauvegarde/rendu/export. | Rapport HTML/JUnit, captures des échecs et traces `retain-on-failure` ; les nouvelles tentatives ne doivent pas masquer le premier échec. |
| Unités/contrats Rust | Tests Cargo ; tests stdio MCP existants. Cas générés ou fondés sur des propriétés uniquement s’ils exercent des invariants pertinents. | Compte rendu de test ; HTML/LCOV de cargo-llvm-cov avec état initial mesuré et seuil empêchant toute régression. Adopter progressivement la couverture par crate. |
| Architecture | Étendre le vérificateur existant aux imports réels, cycles, variables globales, modules Rust, budgets et à l’exhaustivité des capacités/schémas. | La validation locale normale doit rejeter des dépendances interdites délibérées, des fichiers trop grands, un enregistrement absent et des schémas périmés. |
| Acceptation produit/native | Harnais de test bureau existant et véritables sessions Codex/Claude installées. | Octets exacts de l’application/MCP, versions des clients, résultats d’opérations observés et limites de plateforme ; une réussite navigateur ou de compilation ne suffit pas. |

La couverture mesure le code exécuté, pas sa justesse. Pour l’historique, la persistance et la validation des requêtes, les scénarios d’échec indiqués sont obligatoires même au-dessus du seuil de pourcentage. Le code hérité reçoit un état de référence observé plutôt qu’un objectif global immédiat. Chaque exception nomme son chemin, son responsable, le scénario non couvert, les preuves alternatives et sa condition de suppression. Garder les dénominateurs JavaScript et Rust distincts dans les rapports ; ne pas présenter un pourcentage combiné trompeur.

T01–T04 ajoutent les producteurs de rapports. Le compte rendu normal de validation doit relier rapports de tests, couverture par module, résultats des frontières et comparaison à la référence au même SHA candidat. P32 le vérifie depuis un clone propre. Ne pas introduire de tableau de bord ou de service hébergé comme prérequis. Les rapports restent locaux ou dans des artefacts de revue expurgés, conformément à la politique actuelle de CI locale.

Les commandes d’entrée actuelles comprennent `npm run doctor`, `npm run check`, `npm test`, `npm run test:browser`, `npm run test:desktop` et `npm run verify` ; lire `scripts/nemo/README.md` et l’aide du job retenu avant de choisir les options. `doctor` peut sortir avec le code zéro tout en signalant des capacités bloquées, et `verify` utilise par défaut un profil rapide : examiner les résultats structurés des jobs. Les tests MCP exigent la véritable commande Cargo de leur crate, sans supposer qu’un script racine couvre toutes les crates. Les commandes de couverture et de diagnostic proposées dans T01–T08 ne sont pas installées du seul fait de la publication de ce plan.

La couche de débogage prolonge les premières traces et le rejeu d’opacité existants. T05 extrait des diagnostics structurés bornés ; T06 corrèle les spans Rust ; T07 ajoute un paquet de reproduction synthétique ; T08 ajoute un panneau mince et l’inspection MCP. Inclure build/source, instance, document, révision, commande/requête et, si nécessaire, ID de job, erreurs structurées, état sélectionné et mesures temporelles échantillonnées. Garder le traçage détaillé facultatif, borné et séparé de l’état modifiable. La sortie native de `tracing` ne doit jamais contaminer stdout du MCP.

Un paquet de reproduction contient un jeu d’essai synthétique versionné, des empreintes, une horloge/graine, une séquence de commandes et les versions pertinentes. Rejouer dans un **document isolé** ; ne jamais rejouer silencieusement des écritures dans le projet actif de l’utilisateur. Exclure par défaut secrets, chemins privés et ressources utilisateur complètes. Aucun outil arbitraire d’évaluation de code ou de shell ne fait partie du débogueur d’exécution. Playwright Trace Viewer fournit les preuves navigateur ; le service de diagnostic applicatif fournit les preuves d’état produit et de commandes.

Références principales des outils : [exécuteur de tests Node](https://nodejs.org/api/test.html), [c8](https://github.com/bcoe/c8), [Playwright Trace Viewer](https://playwright.dev/docs/trace-viewer), [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov), [Rust tracing](https://docs.rs/tracing/latest/tracing/). Épingler des versions compatibles lors de l’implémentation de la tâche correspondante ; ne pas mettre à niveau toute la chaîne d’outils par anticipation.

## 5. Contrat de capacités et de croissance

- [ ] L’interface, le SDK/les scripts, les tests et le MCP appellent les mêmes services applicatifs versionnés. Le MCP Rust est un adaptateur ; il ne devient ni un second document modifiable ni un dépôt de logique fonctionnelle.
- [ ] Chaque fonctionnalité possède une déclaration lisible par machine : ID/version stables, schéma d’entrée/sortie, unités/limites, effets, disponibilité, gestionnaire public, exemples et liens vers les jeux d’essai. Les commandes précisent révision/nouvelle tentative/annulation ; les jobs précisent progression/annulation/états terminaux ; les ressources précisent identité/durée de vie/libération.
- [ ] La compilation ou le lancement découvre les déclarations de confiance de façon déterministe, les valide et génère catalogue, schéma et documentation. Ajouter une fonctionnalité ne nécessite pas de modifier manuellement un sélecteur central d’opérations MCP. Une déclaration absente, dupliquée ou périmée fait échouer le contrôle normal.
- [ ] Découvrir d’abord des résumés compacts ; charger les contrats détaillés à la demande. Les médias et géométries volumineux circulent via des handles ou des références d’artefacts. Les agents peuvent inspecter des images ou états ciblés ; ne pas exposer des milliers d’outils au niveau des widgets ni sérialiser des buffers entiers dans les prompts.
- [ ] Appliquer la politique de modification et d’accès dans les gestionnaires applicatifs. Les descriptions d’outils expliquent les règles sans les faire respecter. Utiliser des instances, documents et révisions explicites, rejeter les écritures périmées et réconcilier les requêtes interrompues avant de réessayer.
- [ ] Séparer les versions du protocole et des schémas de fonctionnalités, avec des règles explicites de compatibilité et de dépréciation. Exécuter la migration Rust approuvée derrière ces contrats, par tranches verticales bornées ; ne pas tenter une réécriture d’un seul coup. Un framework universel de plugins et un ordonnanceur distribué restent hors périmètre.
- [ ] Une fonctionnalité existante défaillante annonce précisément sa disponibilité ou son échec. Sur la branche de remédiation, une opération non migrée peut être indisponible mais ne peut pas invoquer un ancien writer retiré. Un descripteur seul ne prouve pas un comportement opérationnel et ne ferme pas une fonctionnalité requise ; les noyaux internes sont associés à une capacité fonctionnelle utile au lieu de devenir des outils publics sans intérêt.

## 6. Règles du tableau et supervision humaine

Le tableau faisant autorité est celui de **Cyrill : [Nemo Feature Roadmap — Remediation execution](https://github.com/users/mysteropodes/projects/2/views/1)**. Utiliser la [vue des tâches assignées à Ilya](https://github.com/users/mysteropodes/projects/2/views/7), la [vue des tâches assignées à Cyrill](https://github.com/users/mysteropodes/projects/2/views/8) et la [progression des parents](https://github.com/users/mysteropodes/projects/2/views/9) de ce même projet. L’[ancien tableau d’Ilya](https://github.com/users/ivg-design/projects/8) est un instantané de référence historique ; il a été créé lorsque l’accès au tableau de Cyrill était indisponible. Les agents ne maintiennent pas de second tableau et n’exigent pas de concordance avec cet instantané.

Chaque tâche élémentaire a exactement un assigné GitHub : Ilya=`ivg-design`, Cyrill=`mysteropodes`. `Work package` affiche l’ID de tâche, l’humain/le flux et la compétence. Les champs natifs Parent issue / Sub-issues progress affichent les agrégats ; les liens natifs blocked-by identifient les véritables prédécesseurs. Les labels distinguent `remediation:leaf`, `remediation:parent` et `remediation:deferred` ; `stream:O/D1/D2` identifie le flux. Les vues par personne filtrent sur l’assigné. Le tableau d’exécution inclut les tâches Done afin que le travail terminé reste visible.

| Transition | Qui l’inscrit | Preuves requises |
|---|---|---|
| Inbox → Ready | Orchestrateur de l’humain | Périmètre exact et circonscrit, décision sur les prédécesseurs, acceptation indépendante, responsable et créneau de fichiers/exécution disponible. |
| Ready → In progress | Délégué ou orchestrateur ayant pris la tâche | Branche/base/modèle réels et commandes ciblées consignés ; aucun rédacteur concurrent. |
| In progress → Blocked | Agent actuellement responsable | Décision, environnement ou dépendance manquante nommés, dernier candidat et plus petite action suivante. |
| In progress → Review | Agent actuellement responsable | Candidat poussé, diff cohérent, liens vers contrôles/rapports et transmission complète. |
| Review → Validate | Orchestrateur | Revue de code réussie ; preuves d’intégration ou d’exécution restant à fournir explicitement nommées. |
| Review/Validate → Done | Orchestrateur | Candidat intégré accepté exact, tous les contrôles applicables, traitement des défauts connus, issue fermée et responsabilité de branche/exécution libérée ou préservée. |

- [ ] Juste avant toute écriture, récupérer le corps de l’issue et sa prise en charge actuelle ainsi que l’ID d’élément et les champs en direct de Project #2. Préserver les changements faits par un autre responsable ; ne modifier que les champs visés.
- [ ] Mettre à jour **Project #2 `Remediation status`** et `Validation`. Pour ces nouvelles tâches de remédiation, appliquer aussi la correspondance des champs de feuille de route `Status` / `Board Status` : Inbox→Planned later ; Ready→To do ; In progress/Review/Validate→In progress ; Blocked→Needs work ; Done→Already there. Ne pas toucher aux éléments de feuille de route sans rapport ni à l’instantané historique Project #8.
- [ ] Renseigner chaque classification applicable : Priority, Area, Goal, Size, Phase, Program, Kind, Work package, Planning window, Risk, Validation, Surface, Estimate days, Validation owner et References ; définir Triage date et Schedule basis. Un jour estimé représente huit heures de travail actif ; 0.125–0.25 jour désigne une tâche bornée d’une à deux heures, pas un délai calendaire de livraison. Les travaux non pris en charge, ordonnés par dépendances, sont Forecast / Unscheduled. Définir Start date lors de la prise en charge réelle et Target date uniquement lorsqu’un humain s’engage sur un calendrier ; Exception expiry ne s’applique qu’à une véritable exception. Les dates vides non applicables ne doivent pas devenir des engagements fictifs.
- [ ] Relire l’élément principal et l’issue. Si une mise à jour de champ ou une fermeture d’issue ne réussit que partiellement, consigner cet état partiel et le réconcilier avant la transition suivante. Ne pas supposer qu’un appel API réussi a mis à jour tous les champs visés.
- [ ] Définir `Validation owner` sur l’orchestrateur de l’humain assigné : `Ilya/O (ivg-design)` ou `Cyrill/O (mysteropodes)`. Cet orchestrateur relit, valide, fusionne et clôture les tâches de son équipe et met à jour les parents/dépendances ; Ilya coordonne l’ordre d’intégration partagé. Une validation interéquipes exige un accord explicite pour la tâche concernée ; ce n’est pas la règle par défaut. Préserver la responsabilité historique des acceptations terminées.
- [ ] Les délégués ne mettent à jour que leur propre tâche. Les orchestrateurs gèrent l’intégration de leur équipe, la réassignation, les résumés des parents, la libération des dépendances et la clôture finale. Un transfert de responsabilité humaine modifie ensemble l’assigné de l’issue, le responsable de validation, l’allocation du plan et la transmission actuelle.
- [ ] Conserver l’avancement en direct dans les issues. Les cases cochées et allocations peuvent être réconciliées lors de la prochaine modification ordinaire du plan ou du code ; ne pas créer une PR par mise à jour de statut. L’état actuel de l’issue prime sur un instantané statique des cases à cocher.

Les agents peuvent utiliser les outils GitHub authentifiés ou `gh` ; examiner les champs actuels plutôt que coller des ID d’élément périmés. Commandes de découverte :

```sh
gh issue view <issue-number> --repo mysteropodes/nemo --json body,assignees,state,url
gh project field-list 2 --owner mysteropodes --limit 100 --format json
gh project item-list 2 --owner mysteropodes --limit 500 --format json
```

Utiliser les ID de projet/élément/champ/option retournés avec `gh project item-edit --id <item-id> --project-id <project-id> --field-id <field-id> --single-select-option-id <option-id>`, puis répéter la lecture. Ces paramètres doivent être résolus en direct. Utiliser `--body-file` pour le texte multiligne d’issue ou de PR ; ne jamais interpoler du texte non fiable dans des commandes shell.

## 7. Intégration et nettoyage des branches

**Politique de fusion approuvée par Ilya et Cyrill le 9 septembre 2026 :** chaque équipe de collaborateurs prend en charge sa revue technique et sa validation locale. La nouvelle branche d’intégration protégée conserve les exigences GitHub normales de revue et de fusion ; sa première PR d’extension de politique exige une approbation GitHub ordinaire d’un autre compte éligible, car la politique actuellement déployée ne couvre que `main`. Cette approbation n’est pas une seconde revue technique par l’autre équipe. Après acceptation et vérification de l’extension sur la branche d’intégration protégée, une PR éligible peut demander l’accusé au SHA exact par un `workflow_dispatch` explicite, limité aux métadonnées, avec `--ref codex/native-remediation -f pull_request=<NUMÉRO_PR>`. GitHub ne déclenche pas automatiquement `pull_request_target` sur cette branche non principale ; vérifier la référence, la base de PR et l’approbation obtenue avant de s’y fier.

- [ ] Conserver une seule branche et une seule PR de tâche d’un sprint à l’autre. Un orchestrateur peut intégrer plusieurs tâches relues successivement, mais chacune conserve son acceptation et sa responsabilité propres. Ne pas regrouper des changements sans rapport uniquement pour réduire le nombre de PR.
- [ ] Publier le candidat dans la PR existante de la tâche. L’orchestrateur de l’équipe relit le SHA exact et le diff délimité, vérifie les preuves locales et d’exécution requises, puis consigne dans cette PR le relecteur, le candidat, les commandes/résultats, les défauts connus de la baseline et la décision d’acceptation. Les agents partageant le compte GitHub de l’auteur peuvent effectuer cette revue technique ; ne pas tenter une auto-approbation GitHub ni inventer une autre identité de relecteur.
- [ ] Pour une PR dont l’auteur est un **collaborateur actuel du dépôt disposant d’un accès write, maintain ou admin**, l’équipe responsable ne peut fusionner qu’après sa revue technique, sa validation et l’approbation GitHub requise au SHA actuel. Le workflow automatique `Collaborator PR policy`, limité aux métadonnées, s’applique aux PR vers `main`. Avant la fusion et la vérification de son extension d’intégration, obtenir une approbation ordinaire d’un autre compte éligible pour une PR d’intégration. Ensuite, déclencher uniquement la politique manuellement depuis la référence d’intégration protégée pour le numéro exact de PR ; ne pas supposer d’événement d’intégration automatique. Un accusé de politique n’est ni une revue de code, ni un résultat de test, ni une instruction de fusion. Les accès read/triage, l’appartenance au tableau et l’inscription Buzz ne suffisent pas.
- [ ] Pour une **PR d’un contributeur externe**, obtenir une revue GitHub favorable d’un collaborateur du dépôt après revue et validation locale du candidat actuel. Le workflow de politique n’approuve pas ces PR, même si un collaborateur y pousse des commits ou clique sur Merge. Conserver l’exigence d’une revue et d’approbation du dernier push, l’annulation des approbations périmées et la résolution des conversations.
- [ ] Résoudre les demandes de modifications et les conversations ouvertes. Coordonner avec l’autre équipe en cas de responsabilités qui se chevauchent, de contrats partagés, de conflits ou de décision produit réelle ; ce sont des besoins précis de coordination, pas une condition systématique de fusion. L’acceptation des prédécesseurs nommés reste nécessaire.
- [ ] Juste avant de fusionner, relire le SHA distant et l’état des revues ; si le SHA a changé, refaire la revue et la validation concernées. Utiliser la fusion GitHub normale, puis consigner le SHA intégré, effectuer les contrôles d’intégration applicables et mettre à jour la tâche. Une PR ouverte ou un accusé de politique ne suffit pas pour passer à Done.
- [ ] Si le workflow de politique échoue sur une PR dont il couvre effectivement la base, inspecter l’exécution et ne relancer que la politique de métadonnées depuis cette base de confiance. Ne jamais lancer la politique actuellement déployée, réservée à `main`, pour approuver une PR d’intégration ni ajouter une revue technique systématique entre équipes pour compenser une politique défaillante. Résoudre un problème d’accès/API avec un administrateur du dépôt ; ne jamais fabriquer une approbation.
- [ ] Aucun push direct sur `main` ou `codex/native-remediation` protégés, ni contournement habituel des protections. Builds, tests, versions et déploiements restent locaux sauf demande humaine explicite pour l’exécution hébergée précise. Le workflow de politique limité aux métadonnées est la seule exception automatique Actions approuvée ; il n’exécute aucun code de PR et ne fusionne rien. Garder les quatre workflows produit désactivés. Les rappels de coordination restent en pause hors exécution active.
- [ ] Après fusion et acceptation, vérifier que le travail est bien contenu dans le dépôt distant, que l’état suivi/non suivi est propre et qu’aucun processus détenu ne reste actif avant de supprimer branche et worktree de tâche. Ne pas retirer la copie de travail d’un autre flux. Une branche incomplète encore utile reste sous responsabilité ou est archivée avec une procédure de restauration testée.
- [ ] Préserver la branche distante protégée `archive`. Les bundles archivés possèdent des manifestes nom-original→SHA, des sommes de contrôle et une vérification de restauration dans un dépôt vide. Ne jamais supprimer une branche uniquement à cause de son âge ou de l’absence de PR. Les têtes/bases de PR ouvertes, versions, worktrees et responsabilités non résolues restent protégés du nettoyage.
- [ ] Les nouvelles branches ne doivent pas s’accumuler après clôture. L’orchestrateur effectue le nettoyage dans le cadre de la transmission et du passage à Done ; l’humain ne doit pas hériter d’une corvée de nettoyage de worktrees après chaque session.

## 8. Liste ordonnée d’exécution

Les dépendances déterminent ce qui est prêt, pas l’ordre numérique des ID. Le pivot du 20 septembre remplace l’ancien exemple de première allocation : mettre d’abord à jour le tableau canonique et le plan, puis franchir les jalons de contrats et de faisabilité avant d’inscrire l’implémentation du moteur. Les tâches indépendantes déjà prises en charge peuvent continuer uniquement si leurs responsables et leurs fichiers ne chevauchent pas le travail du pivot.

1. N00 installe et valide Project #2 ainsi que cet amendement du plan.
2. Après l’acceptation de N00, N01 réconcilie la file en direct pendant que N02 fige les contrats d’autorité native et de transition.
3. Après l’acceptation de N02, N03 et N04 exécutent les jalons indépendants de faisabilité pour l’évaluateur headless et le viewport natif.
4. Après les deux jalons, N05 admet N06–N21 sous les parents de famille R18.1–R18.6, coordonnés par R23 ; aucune issue générale de moteur ne constitue une mission d’écriture.
5. Les tâches d’implémentation migrent les responsabilités caractérisées directement vers l’autorité native. Sur la branche séparée, elles peuvent déconnecter l’ancien writer obsolète avant la parité finale ; déclarer toute opération manquante indisponible et garder son obligation de parité ouverte. La promotion finale attend tous les contrôles applicables nommés de consommateurs et de surfaces installées.

C08 a réparti la couverture restante dans les tâches de recensement supplémentaires acceptées C09, C13, C15, C17 et C18. P03 doit consolider C01–C08 ainsi que C09, C13, C15, C17 et C18, puis créer les petites tâches d’extraction restantes et les rattacher aux mêmes parents de famille. Réutiliser le protocole de tâche ci-dessous : symboles/responsable/dépendances exacts, trois contrôles observables, exclusions de défauts connus, mêmes labels d’assigné/flux et liens natifs de parent/blocage. Étendre cette liste dans la même modification ordinaire que celle qui adopte le recensement. L’empreinte de l’ensemble de sources figé et les contrôles d’absence d’éléments non cartographiés empêchent de déclarer silencieusement la remédiation terminée après ces seules tâches de départ.

<!-- generated-task-index -->

Le tableau initial contenait **59 tâches élémentaires : 33 sous la responsabilité d’Ilya et 26 sous celle de Cyrill**. Ces nombres historiques décrivent la file initiale ; le registre admis actuel figure ci-dessous.

### N — première tranche native d’opacité admise (N05, 20 septembre 2026)

N00–N05 disposent de reçus d’acceptation terminaux : fusion N00 `2bd3da7f7e7811b51150bcf95dc669182d2c8f25`, réconciliation des métadonnées/Project N01, fusion N02 `c489e5fbb1822f6a816db3a4d8e901d67382b7a2`, fusion N03 `b37b9cb6a4eb391a248a8dba98cf3f676afbbfe9`, fusion N04 `3084dbf9aeb7010b4ecd24eafee57c106f57c32f` et fusion N05 `cf73104ad896ea2b7c90d5115cc77416d6c6a13b`. Les reçus des issues conservent les candidats, contrôles et limites exacts. N03 est une preuve headless caractérisée qui rejette une opacité modifiée ; N04 est une preuve de viewport macOS. Aucune ne vaut acceptation d’une migration de production.

À l’instantané d’acceptation de N05, le registre des issues contenait **171 tâches élémentaires = 143 acceptées + 28 ouvertes (83.63%)**. Ce sont des résultats d’issues acceptés, préparation et validation comprises ; la première tranche native comptait séparément **0/16 acceptations runtime**. La revue de N16 a ensuite admis N18A/#1367 comme dix-septième tâche runtime. Le préflight de N20 a ensuite prouvé l’absence des lectures d’autorité pour sérialisation/évaluation et d’un contrat terminal de libération/réentrée native ; N19A/#1373 et N19B/#1374 ont donc été admises comme dix-huitième et dix-neuvième tâches runtime avant N20. La revue ultérieure de l’atteignabilité des callbacks a admis N19C/#1377, N19D/#1378 et N19E/#1379 comme tâches runtime vingt à vingt-deux : N19C installe la barrière dormante d’édition legacy avant mutation, N19D étend cette barrière de façon sérialisée aux mutations de sélection et de repli après N19C, et N19E fait indépendamment attendre à des appelants externes de remplacement de projet leur résultat terminal après N19B. L’état courant des issues et du Project #2 prévaut sur ces comptes historiques. Les dossiers du recensement P03 ne constituent pas ce dénominateur ; ni leur nombre ni cette première tranche ne définissent le dénominateur complet de la migration du moteur. La revue du candidat exact de N20 a admis N19F/#1384 comme vingt-troisième tâche runtime pour imposer l’acquittement avant le succès réseau du MCP embarqué. La découverte ultérieure du flux pointeur de N21 a admis N19G/#1393 après N19D comme vingt-quatrième tâche runtime afin de préserver l’intention de sélection du canevas détenue par le natif grâce à une libération différée.

Les vingt-quatre tâches natives admises avaient été attribuées à **Ilya (`ivg-design`)**, avec **Ilya/O comme Validation owner**. La préférence actuelle des délégués est GPT-6 Sol/Luna, Astra lorsque justifié ; consigner les réglages réels dans chaque nom d’agent local et chaque prise en charge. L’équipe responsable nomme un relecteur indépendant avant toute écriture. À l’instantané N05, N06 conservait un empêchement sur des fichiers partagés et N07–N21 restaient planifiées ; les tâches suivantes ont été admises à partir de découvertes précises. Le tableau ci-dessous conserve ces ID d’issues et dépendances initiales, sans affirmer que tous les anciens critères de coexistence restent actuels. N’avancer chaque tâche révisée qu’à partir de son issue en direct, des preuves d’acceptation de ses prédécesseurs et des champs de Project #2 ; consigner les SHA de fusion réels, jamais prospectifs. L’ordre numérique ne définit pas l’état prêt.

| Tâche | Résultat observable | Famille / poste | Prédécesseurs effectifs |
|---|---|---|---|
| [N06 / #1338](https://github.com/mysteropodes/nemo/issues/1338) | Squelette du crate et interfaces de modules | R18.1 / O | N05, #1316 / PR #1323 |
| [N07 / #1339](https://github.com/mysteropodes/nemo/issues/1339) | Admission et aller-retour des documents d’opacité | R18.1 / D1 | N06 |
| [N08 / #1340](https://github.com/mysteropodes/nemo/issues/1340) | Autorité sur les révisions et commandes d’opacité | R18.1 / D1 | N07 |
| [N09 / #1341](https://github.com/mysteropodes/nemo/issues/1341) | Autorité sur les transactions et l’historique | R18.1 / D1 | N08 |
| [N10 / #1342](https://github.com/mysteropodes/nemo/issues/1342) | Évaluer des entrées immuables image/contexte | R18.2 / D2 | N08, N09 |
| [N11 / #1343](https://github.com/mysteropodes/nemo/issues/1343) | Ordonnancer les images et libérer les baux de ressources | R18.3 / D2 | N10 |
| [N12 / #1344](https://github.com/mysteropodes/nemo/issues/1344) | Partager un compositeur natif aperçu/export | R18.3 / D2 | N11 |
| [N13 / #1345](https://github.com/mysteropodes/nemo/issues/1345) | Fournir l’hôte du viewport et l’adaptateur d’entrée | R18.4 / D2 | N12 |
| [N14 / #1346](https://github.com/mysteropodes/nemo/issues/1346) | Exporter des images PNG à révision figée | R18.5 / D1 | N09, N12, N13 |
| [N15 / #1347](https://github.com/mysteropodes/nemo/issues/1347) | Exposer le dispatcher/protocole v2 commun | R18.6 / D1 | N09, N14 |
| [N16 / #1348](https://github.com/mysteropodes/nemo/issues/1348) | Relier les transports hôtes Tauri/MCP sans bascule | R18.6 / O | N13, N15 |
| [N17 / #1349](https://github.com/mysteropodes/nemo/issues/1349) | Préparer les adaptateurs d’intention d’édition/sélection | R18.4 / D1 | N09, N10, N15, N16 |
| [N18 / #1350](https://github.com/mysteropodes/nemo/issues/1350) | Préparer les adaptateurs consommateurs d’aperçu/export | R18.3 / D2 | N12, N13, N14, N15, N16, N17 |
| [N18A / #1367](https://github.com/mysteropodes/nemo/issues/1367) | Préparer le bootstrap de l’application native et les ports de l’hôte bureau | R18.6 / O | N16, N17, N18 |
| [N19 / #1351](https://github.com/mysteropodes/nemo/issues/1351) | Appliquer les frontières natives et la validation locale | R18.3 / O | N13, N15, N16, N17, N18, N18A |
| [N19A / #1373](https://github.com/mysteropodes/nemo/issues/1373) | Exposer la sérialisation native figée par révision et les lectures de sélection évaluées | R18.1 / O | N19 |
| [N19B / #1374](https://github.com/mysteropodes/nemo/issues/1374) | Préserver fermeture, nettoyage et réentrée natifs ; aucun transfert vers legacy | R18.6 / O | N19A |
| [N19C / #1377](https://github.com/mysteropodes/nemo/issues/1377) | Refuser ou déconnecter les anciennes entrées directes d’édition avant mutation | R18.4 / D2 | N19B |
| [N19D / #1378](https://github.com/mysteropodes/nemo/issues/1378) | Refuser ou déconnecter les anciennes mutations de sélection/repli | R18.4 / D2 | N19C |
| [N19G / #1393](https://github.com/mysteropodes/nemo/issues/1393) | Intention pure de sélection canevas ; opération native implémentée ou indisponibilité sûre | R18.4 / D2 | N19D |
| [N19E / #1379](https://github.com/mysteropodes/nemo/issues/1379) | Attendre le remplacement natif du projet avant de poursuivre l’appelant | R18.1 / D1 | N19B |
| [N19F / #1384](https://github.com/mysteropodes/nemo/issues/1384) | Synchroniser les révisions MCP embarquées avant le succès réseau | R18.6 / O | N16, N18A, N19A, N19B |
| [N20 / #1352](https://github.com/mysteropodes/nemo/issues/1352) | Maintenir le premier workflow d’opacité continûment natif | R18.1 / O | N17, N18, N18A, N19, N19A, N19B, N19C, N19D, N19E, N19F, N19G |
| [N21 / #1353](https://github.com/mysteropodes/nemo/issues/1353) | Accepter les workflows natifs installés et navigateur déclaré sans aller-retour legacy | R18.4 / O | N20 |

**N19F / #1384 préserve la synchronisation des révisions MCP embarquées directes.** Son abonné accepté de la webview principale est lié à l’instance, au document et à la génération du cycle de vie. Un abonné absent/périmé refuse les avancées externes avant commit ; une avancée validée émet exactement `{instanceId, documentId, lifecycleGeneration, fromRevision, toRevision, requestId}` et le succès réseau attend un acquittement exact après blocage des lectures périmées et synchronisation de `toRevision`. Lectures, échecs, annulation, travail non validé et relecture de reçu n’émettent rien. Une avancée en attente bloque les suivantes UI/externes tout en permettant les lectures immuables ; échec d’écoute ou délai dépassé conserve une barrière indéterminée jusqu’à fermeture/réentrée natives, sans réexécution. Déconnexion, fermeture native et réservation réussie de réinstallation drainent les anciens abonnés/attentes. Les sources privées du protocole conservent leurs limites MCP acceptées. N20 doit consommer ce callback et prouver la parité écriture MCP directe→UI/persistance/historique ; aucun transfert vers l’ancien propriétaire n’est requis.

**Les prises en charge des fichiers entiers passent par des transmissions explicitement sérialisées.** Chaque issue liée contient sa liste exacte de chemins nouveaux/existants, sa matrice de consommateurs, ses contrôles et sa condition de bascule. N06 exige à la fois N05 acceptée et la transmission terminale de propriété de #1316/PR #1323. L’enregistrement des sources Rust suit **N06→N07→N08→N09→N10→N11→N12→N13→N14→N15→N16**, puis la transmission sérialisée **N18A→N19→N19A→N19B→N19C→N19D→N19G→N20** après acceptation de N17/N18 ; N19E avance en parallèle après N19B et rejoint N20 seulement après sa propre acceptation. N19F avance après N19B acceptée et la transmission des fichiers partagés, et constitue un prédécesseur accepté obligatoire de N20. Celui des applications/adaptateurs JS suit **N13→N15→N17→N18→N18A→N19A→N19C→N19D→N19G**, avec les dépendances intermédiaires du tableau. Les écritures partagées de provenance app-JS et d’inventaire généré se sérialisent **N19E→N19C** : N19E les rafraîchit d’abord pour ses sources d’appelants acceptées, puis les libère pour la source de garde de N19C. Un prédécesseur doit libérer sa prise en charge du fichier entier avant le démarrage du writer suivant ; des fichiers de modules différents n’autorisent pas des modifications concurrentes des profils partagés.

N06 possède seule `native-engine/Cargo.toml`, `Cargo.lock`, `src/lib.rs` et `README.md` : elle fige dépendances, exports de modules désactivés par feature et cibles de test exigeant leurs features, sans comportement runtime factice. N07–N15 possèdent leurs modules d’implémentation distincts et tests ciblés sans réécrire le squelette de N06. En outre, **chaque candidat produisant des sources Rust N06–N15** réserve et met à jour `engineering/boundaries/profiles/rust.profile.json`, `engineering/boundaries/profiles/rust.baseline.json`, `engineering/boundaries/profiles/rust.coverage.json` et `tests/nemo-boundaries-rust.test.cjs` selon la découverte fraîche des sources, après transmission par son prédécesseur. L’enregistrement accompagne le candidat source ; il n’est pas repoussé à N19.

**N13, N15, N17 et N18** réservent de même `engineering/boundaries/profiles/app-js.profile.json`, `engineering/boundaries/profiles/app-js.baseline.json`, `engineering/boundaries/profiles/app-js.coverage.json`, `engineering/boundaries/profiles/app-js.edges.json` et `scripts/nemo/boundaries-application.test.cjs` pour chaque nouvel enregistrement d’adaptateur, avec transmission sérialisée des fichiers entiers. N16 possède le câblage existant des manifestes, verrous, hôtes, wire/schémas et tests de transport Tauri/MCP, ainsi que `src/js/adapters/application-mcp.js`, et met à jour `engineering/boundaries/profiles/mcp-rust.profile.json` et `engineering/boundaries/profiles/nemo-desktop.edges.json` pour ces hôtes. Elle relie les transports sans admettre l’autorité native sur le document. N17 ajoute les adaptateurs `native-opacity-editor.js` / `native-opacity-selection.js` et leur test ; N18 ajoute `native-opacity-preview.js` / `native-opacity-export.js` et leur test.

**N18A possède le bootstrap dormant et sélectionnable en production de l’application native ainsi que les ports concrets de l’hôte bureau découverts par la revue de N16.** Elle peut ajouter un accesseur natif étroit aux snapshots, conserver exactement une application native optionnelle et un viewport de fenêtre principale, et faire atteindre cette même application par l’interface comme par le MCP embarqué authentifié. Son viewport et son compositeur d’export partagent un seul contexte GPU exact. N18A met à jour le profil/la couverture des sources Rust et les arêtes bureau avec des listes d’autorisation native-engine propres à chaque module, tout en laissant inchangée la baseline Rust figée. Elle ne doit ni activer le bootstrap, ni admettre un document de production, ni créer une seconde autorité modifiable, ni revendiquer la preuve installée ; N20 possède l’invocation/la bascule et N21 l’acceptation installée.

**N19 possède uniquement** `engineering/boundaries/profiles/native-engine.edges.json`, `engineering/boundaries/profiles/rust.profile.json`, `tests/native-engine-enforcement.test.cjs`, `scripts/nemo/ci.cjs`, `scripts/nemo/ci.test.cjs`, `scripts/nemo/lib/jobs.cjs`, `engineering/inventory/baseline-manifest.json`, les trois artefacts d’inventaire générés `engineering/inventory/surfaces.json`, `engineering/inventory/surfaces.csv` et `engineering/inventory/SURFACES.md`, `package.json` et les paragraphes N19 synchronisés des plans anglais/français pour l’enregistrement des jobs normaux et les contrôles négatifs après libération des fichiers partagés par N18A. Son exception étroite pour le profil répartit les chemins `native-engine/src` déjà enregistrés afin que l’analyseur Rust adopté puisse appliquer la direction interne. Les artefacts d’inventaire générés remettent en synchronisation les surfaces source courantes déjà acceptées après que la validation normale de N19 a révélé des sorties périmées ; N19 ne modifie elle-même aucune source de l’inventaire. N19 ne peut modifier ni l’ensemble des chemins enregistrés, ni les budgets de taille, ni la baseline figée, ni la politique/les exclusions de couverture, ni les tests de garde des prédécesseurs, ni les sources de production. N19 ne reçoit pas de transmission directe de #1316 ; ce préalable est hérité via N06.

**N19A possède l’interface de lecture manquante découverte pendant le préflight de N20.** Elle déclare `query.document.serialize` et `query.document.evaluate` dans la capacité v2 appartenant à la fonctionnalité, implémente dans l’application native des lectures immuables figées par révision, puis les transporte par le dispatcher d’application générique existant, l’adaptateur JavaScript et le MCP Rust embarqué. Le résultat sérialisé conserve les identifiants stables et les pistes à images clés ; le résultat évalué porte l’identité exacte document/snapshot/révision/contexte/image et satisfait la projection de sélection préparée. N19A reste indisponible en production et n’ajoute ni writer, ni fallback, ni activation au démarrage. Ses fichiers exacts et contrôles négatifs sont figés dans #1373.

**N19B préserve la sûreté du cycle de vie natif.** Sa commande terminale acceptée, contrôlée par identité, arrête les nouveaux dispatchs, réconcilie les transactions/export/aperçu en cours, détruit viewport et ressources, retire l’unique application native installée, conserve un reçu explicite et autorise un bootstrap natif ultérieur. Un échec de nettoyage n’autorise jamais une seconde autorité. Sur la branche séparée, cela sert à fermer/remplacer/réentrer côté natif, jamais à transférer vers un document legacy modifiable. La source et les contrôles de course/panne acceptés restent dans #1374.

**N19C préserve la couverture des entrées directes de mutation.** Sa garde acceptée et ses tests des callbacks dessin/remplissage/pen/forme/gomme identifient les anciennes entrées avant mutation Paper/document. Sur la branche séparée, déconnecter ces writers ou faire refuser par leurs entrées les opérations indisponibles avant mutation ; ne pas installer leur ancien contrôleur de libération vers legacy ni considérer le mode dormant passant comme une acceptation. Conserver l’enregistrement véridique des sources, les contrôles négatifs de frontière et les limites de non-croissance jusqu’au retrait du writer. Une opération implémentée utilise l’API applicative native ; une opération non implémentée reste visiblement indisponible. L’issue #1377 conserve sa preuve historique acceptée et les obligations exactes de déconnexion restantes.

**N19D préserve la couverture des entrées de sélection et de repli.** Ses contrôles directs/programmatiques acceptés identifient les chemins de sélection, sous-sélection, callbacks Paper de repli et panneau Shapes dans #1378. Sur la branche séparée, ces chemins envoient une commande implémentée au propriétaire natif ou refusent l’opération indisponible avant mutation ; ils ne libèrent jamais vers un ancien propriétaire modifiable et ne rejouent pas un geste différé. Préserver la sérialisation des fichiers entiers `tools.js`/`select-bridge.js` et la prise en charge distincte PR #4/P25.

**N19G possède la première interface de sélection du canevas native seule.** Ne réutiliser du prototype arrêté et non accepté que la précédence pure des hits Motion/Select et l’intention transitoire scalaire du pointeur qui servent le résultat final. Une sonde ne peut pousser l’undo, sauvegarder, allouer une baseline de geste, retenir des références Paper mutables ni modifier document/sélection. Sélection par clic, resélection, désélection et Shift conservent identité/révision natives ; seuil de mouvement, annulation, perte de capture et identité périmée d’outil/image/document ne doivent pas lancer une action obsolète. Pour le premier workflow d’opacité, une action de sélection/édition implémentée atteint sa commande native ; une géométrie, transformation, marquee/lasso ou autre action glissée non migrée est indisponible et s’arrête avant mutation par un ancien writer. N19G n’implémente **pas** toute la future famille native de dessin/transformation, ne demande pas de libération vers legacy et ne rejoue pas un geste. W05/W10 et leurs tâches propriétaires conservent ces obligations plus larges. Le périmètre révisé et la transmission du writer appartiennent à [#1393](https://github.com/mysteropodes/nemo/issues/1393) ; le prototype arrêté n’est pas un candidat accepté.

**N19E préserve la sûreté des appelants externes du remplacement de projet.** Ses appelants Kitsu/tutoriel acceptés attendent le résultat terminal avant toute mutation ultérieure de layer/frame/setup, rendu, entrée dans l’éditeur ou continuation de succès. Sur la branche séparée, ils attendent le remplacement natif et s’arrêtent après rejet, échec ou résultat indéterminé ; ils ne reprennent pas un ancien projet modifiable. Conserver la provenance acceptée et la transmission des fichiers partagés de #1379, mais ne pas compter la compatibilité avec l’ancien retour synchrone comme acceptation native finale.

N20 réutilise son implémentation native d’opacité acceptée, ses ports de lecture, son cycle de vie, sa synchronisation MCP et la correction de sélection de ligne présentes sur `main`, mais son résultat rouvert est le **premier workflow d’édition continûment natif** sur la branche d’intégration. Elle doit maintenir un propriétaire natif unique pendant la sélection Motion et les vraies modifications d’opacité ; un contenu non pris en charge ou une action indisponible est refusé sans mutation ni transfert vers un propriétaire legacy. Elle peut déconnecter tôt les anciens writers, sans retirer du contenu pour fabriquer une projection faussement prise en charge ni revendiquer la parité finale d’une fonctionnalité à partir d’un sous-ensemble de développement. L’interface étroite de sélection/refus N19G précède l’acceptation N20 concernée ; N21 possède séparément les preuves réelles bureau installé et navigateur déclaré. Les fichiers restants exacts et les critères natifs/tests appartiennent à #1352 ; la migration plus large est répartie dans les paquets déjà possédés, pas cachée dans ce premier workflow.

**Oracle indépendant et consommateurs.** Préserver `tests/animation/fixtures/curve-workflow.json`, SHA-256 `dceb05d13576a4dda0eb1a1a9d8c0184e8617e9a3a2662150ee54f4badedf08d`, et les attentes indépendantes actuellement vérifiées dans `tests/browser/opacity-consumers.spec.cjs` : l’opacité statique 25 survit à la sauvegarde/réouverture avec un `layerUid` stable ; les images clés 0/10/20 s’évaluent à 20/50/80. La sortie 320×180 comporte 400 pixels colorés et les bornes `[20,60,40,80]`, `[84,60,104,80]`, `[148,60,168,80]` ; rouge/alpha valent 255 et vert/bleu valent `255 × (1-opacity/100)` à un octet près. Le rendu/export ne modifie pas les calques stockés. N09/N19A/N20 doivent en plus figer et vérifier l’oracle de résultat manquant selon lequel 40→60→annulation rend l’opacité d’autorité à 40 ; le test navigateur actuel vérifie la longueur des piles d’historique, pas cette valeur résultante. Figer toutes les valeurs attendues indépendantes avant l’implémentation ; ne pas les régénérer depuis le candidat.

N07–N19A ont fourni les interfaces natives acceptées de sauvegarde/chargement, identité, commandes/historique, évaluation, ordonnancement/ressources, viewport/export, transport UI/MCP et lecture ; N19 a fourni de vrais contrôles négatifs de frontière. Sur la branche séparée, N19B fournit fermeture/réentrée natives sûres, N19C/N19D protègent ou déconnectent les anciennes mutations directes/programmatiques, N19G fournit intention pure de sélection et comportement sûr natif ou indisponible, N19E arrête la continuation de l’appelant après échec du remplacement natif et N19F synchronise les révisions MCP directes avec l’UI. N20/N21 établissent uniquement le premier workflow natif d’opacité et ses preuves réelles bureau installé/navigateur ; ils ne ferment pas les autres familles de référence. Révisions périmées, répétitions au corps modifié, remplacement, contenu non pris en charge, générations obsolètes et échecs d’annulation/nettoyage restent des contrôles négatifs explicites. Les paquets plus larges cartographiés par P03 portent leur propre parité sauvegarde/chargement, historique, sélection, animation, rendu et export jusqu’à l’acceptation finale.

**Activation et validation.** Le `main` opérationnel conserve son propriétaire observé ; ce n’est pas une preuve pour le runtime de remédiation séparé. Sur `codex/native-remediation`, un document admis a un propriétaire natif unique et aucun autre writer JavaScript/Paper. N20 n’admet que le sous-ensemble caractérisé sans perte de contenu ; un contenu non pris en charge échoue sûrement, laissant le document natif précédent inchangé ou l’ouverture tentée indisponible, sans transfert vers un propriétaire legacy. Les anciennes entrées N19C/N19D/N19G sont déconnectées ou refusées avant mutation, tandis que les opérations implémentées utilisent les commandes natives. N19B peut fermer/remplacer le propriétaire natif et permettre une réentrée native sûre, jamais une édition de repli. Les autres familles requises restent visiblement en attente de migration, pas Done. Chaque tâche modifiée exécute ses contrôles ciblés Cargo/Node, les contrôles normaux d’enregistrement/frontières/taille et les consommateurs réels concernés ; les jalons intégrés utilisent les validations locales plus larges. N21 consigne séparément l’exécutable installé identifié et le résultat du navigateur réel pour le premier workflow. Tests unitaires, hôte de fixture, compilation, captures ou fusion seuls ne démontrent pas l’acceptation installée. Aucun build produit hébergé n’est autorisé.

**Graphe final et travail préservé.** N21 → [P32/#1034](https://github.com/mysteropodes/nemo/issues/1034) → [P33/#1035](https://github.com/mysteropodes/nemo/issues/1035) → [R22/#930](https://github.com/mysteropodes/nemo/issues/930). P32 conserve ses prédécesseurs existants, dont P16 et sa question d’acceptation contrôlée par son propriétaire. P33 conserve P03/P08/P32 et exige toutes les tâches natives admises ; R22 reste un suivi et une acceptation structurelle humaine, pas une tâche d’écriture. [R23/#1327](https://github.com/mysteropodes/nemo/issues/1327) attend N21 ainsi que N00–N05. **P03 reste l’autorité de recensement/admission des sources et n’est pas bloquée par l’exécution des tâches qu’elle admet.** Elle continue à classer les familles restantes et peut ajouter des tâches bornées sans prétendre qu’elles sont implémentées. P24/#1026 et toutes les autres prises en charge/périmètres existants restent inchangés ; N10 est une tâche d’évaluateur distincte. Aucune clôture de parent de suivi ne crée un verrou global d’extraction.

### A — établir les faits et préserver le travail en cours

| Ilya — étapes sous sa responsabilité | Cyrill — étapes sous sa responsabilité |
|---|---|
| [P01 / #1003](https://github.com/mysteropodes/nemo/issues/1003) · **O** · Mettre en place la liste approuvée, les issues élémentaires assignées et les vues du tableau | [C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039) · **D1** · Cartographier les outils de l’éditeur et la présentation en dossiers d’extraction circonscrits |
| [P02 / #1004](https://github.com/mysteropodes/nemo/issues/1004) · **O** · Adopter un manifeste unique de comparaison de l’état actuel | [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040) · **D1** · Cartographier les médias, l’import, l’export et les services natifs en dossiers d’extraction circonscrits |
| [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036) · **O** · Cartographier le document et l’historique en dossiers d’extraction circonscrits | [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041) · **D2** · Cartographier les préférences, Labs et l’initialisation en dossiers d’extraction circonscrits |
| [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037) · **D1** · Cartographier l’animation et le temps en dossiers d’extraction circonscrits | [C08 / #1043](https://github.com/mysteropodes/nemo/issues/1043) · **D2** · Cartographier l’exhaustivité et les racines suivies restantes en dossiers d’extraction circonscrits |
| [C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038) · **D2** · Cartographier le rendu et la propriété des ressources en dossiers d’extraction circonscrits | [F01 / #1047](https://github.com/mysteropodes/nemo/issues/1047) · **O** · Examiner les preuves existantes de Honey sur le paquet installé |
| [C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042) · **O** · Cartographier l’application, le MCP et les diagnostics en dossiers d’extraction circonscrits | [F02 / #1048](https://github.com/mysteropodes/nemo/issues/1048) · **D1** · Terminer l’acceptation existante de Fizz sur le schéma des payloads |
| [P03 / #1005](https://github.com/mysteropodes/nemo/issues/1005) · **O** · Valider le recensement combiné du code et inscrire toute la file d’extraction | [P14 / #1016](https://github.com/mysteropodes/nemo/issues/1016) · **D2** · Terminer l’acceptation de l’isolation native entre deux instances |
| — | [P15 / #1017](https://github.com/mysteropodes/nemo/issues/1017) · **D1** · Ajouter une régression sur un véritable geste du curseur d’opacité |

- [ ] **[P01 / #1003](https://github.com/mysteropodes/nemo/issues/1003) — Mettre en place la liste approuvée, les issues élémentaires assignées et les vues du tableau**

  Responsable **Ilya/O** · compétence `orchestration` · `gpt-6-astra` / **high**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `engineering/remediation/EXECUTION_PLAN.en.md`; `engineering/remediation/EXECUTION_PLAN.fr.md`; `AGENTS.md`; `liens d’entrée du manuel`; `issues GitHub de remédiation, vues du Project 2 principal et bascule depuis l’ancien Project 8`.

  1. Les listes anglaise et française contiennent les mêmes ID, assignés humains, modèles, compétences de flux, périmètres et tests de clôture.
  2. Chaque issue exécutable possède un assigné humain, un flux, des dépendances vérifiées et un parent ; les vues du tableau en direct séparent tâches élémentaires et parents.
  3. Les responsabilités des PR actives existantes sont préservées ; les résumés périmés de R03/R05/R06/R14 sont réconciliés ; le plan publié et le tableau sont relus.

  Limite : Aucune issue n’est marquée Done du seul fait de la fusion d’une PR. Aucun écrasement du champ Status de la feuille de route avant une décision sur le tableau.

- [ ] **[P02 / #1004](https://github.com/mysteropodes/nemo/issues/1004) — Adopter un manifeste unique de comparaison de l’état actuel**

  Responsable **Ilya/O** · compétence `orchestration` · `gpt-6-astra` / **high**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `engineering/inventory/BASELINE.md`; `sortie existante de comptes rendus ignorée par Git`.

  1. Identifier le code retenu, les modifications locales non commitées, les empreintes des jeux d’essai et l’environnement applicable. Réutiliser les comptes rendus antérieurs équivalents plutôt que relancer toutes les suites.
  2. Les cas connus en échec, bloqués ou non exécutés restent explicites, avec cas et preuves exacts ; la comparaison distingue nouvelle régression, échec connu inchangé et incompatibilité d’environnement.
  3. Publier un manifeste de comparaison lisible par machine avec des références immuables aux jeux d’essai et au code ; signaler une référence manquante ou discordante au lieu de la réutiliser silencieusement.

  Limite : Aucun prérequis de réparation fonctionnelle ni référence attendue modifiée silencieusement. Le F0 historique reste valide à sa propre révision.

- [ ] **[C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036) — Cartographier le document et l’historique en dossiers d’extraction circonscrits**

  Responsable **Ilya/O** · compétence `inventory` · `gpt-6-astra` / **high**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `src/js/project.js`; `src/js/project-document.js`; `src/js/project-entry.js`; `src/js/idb-store.js`; `src/js/asset-tree.js`; `src/js/history-panel.js`; `responsabilités de document/historique dans timeline.js et tweens.js`; `engineering/inventory/remediation-scope.json (partition soumise à l’orchestrateur)`.

  1. Cartographie en lecture seule pour « Document et historique » : nommer les responsabilités réelles, les propriétaires actuels des écritures, les points d’entrée publics et les consommateurs concernés au SHA retenu.
  2. Répartir en dossiers d’une seule responsabilité : symboles/chemins exacts, oracle de jeu d’essai indépendant, frontière de module attendue et un seul rédacteur par fichier entier. Aucun dossier ne nécessite plusieurs jours d’implémentation.
  3. Remettre la répartition JSON avec les ID proposés liés, les exclusions de défauts connus et la prise en compte de toutes les responsabilités restantes ; l’orchestrateur met à jour l’index partagé en série et crée les tâches. Les fichiers partagés ont un seul propriétaire principal malgré plusieurs familles de responsabilités.

  Limite : Aucune migration de code dans cette tâche de recensement. Pour C08, vérifier mécaniquement la différence d’ensembles et ne cartographier que les chemins non couverts ; ne pas refaire les recherches de C01-C07.

- [ ] **[C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037) — Cartographier l’animation et le temps en dossiers d’extraction circonscrits**

  Responsable **Ilya/D1** · compétence `inventory` · `gpt-6-sol` / **medium**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `src/js/motion.js`; `src/js/tweens.js`; `src/js/timeline.js`; `src/js/animation/**`; `src/js/domain/animation/**`; `src/js/expr-*.js`; `src/js/layer-inout.js`; `src/js/camera.js`; `src/js/text-animator*.js`; `src/js/markers.js`; `src/js/bpm-grid.js`; `engineering/inventory/remediation-scope.json (partition soumise à l’orchestrateur)`.

  1. Cartographie en lecture seule pour « Animation et temps » : nommer les responsabilités réelles, les propriétaires actuels des écritures, les points d’entrée publics et les consommateurs concernés au SHA retenu.
  2. Répartir en dossiers d’une seule responsabilité : symboles/chemins exacts, oracle de jeu d’essai indépendant, frontière de module attendue et un seul rédacteur par fichier entier. Aucun dossier ne nécessite plusieurs jours d’implémentation.
  3. Remettre la répartition JSON avec les ID proposés liés, les exclusions de défauts connus et la prise en compte de toutes les responsabilités restantes ; l’orchestrateur met à jour l’index partagé en série et crée les tâches. Les fichiers partagés ont un seul propriétaire principal malgré plusieurs familles de responsabilités.

  Limite : Aucune migration de code dans cette tâche de recensement. Pour C08, vérifier mécaniquement la différence d’ensembles et ne cartographier que les chemins non couverts ; ne pas refaire les recherches de C01-C07.

- [ ] **[C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038) — Cartographier le rendu et la propriété des ressources en dossiers d’extraction circonscrits**

  Responsable **Ilya/D2** · compétence `inventory` · `gpt-6-luna` / **medium**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `src/js/engine-bridge.js`; `src/js/render-manager.js`; `src/js/playback-cache.js`; `src/js/color-manager.js`; `src/js/path-fx.js`; `src/js/custom-effects.js`; `src/js/shader-effects-library.js`; `geometry-wasm/src/**`; `engineering/inventory/remediation-scope.json (partition soumise à l’orchestrateur)`.

  1. Cartographie en lecture seule pour « Rendu et propriété des ressources » : nommer les responsabilités réelles, les propriétaires actuels des écritures, les points d’entrée publics et les consommateurs concernés au SHA retenu.
  2. Répartir en dossiers d’une seule responsabilité : symboles/chemins exacts, oracle de jeu d’essai indépendant, frontière de module attendue et un seul rédacteur par fichier entier. Aucun dossier ne nécessite plusieurs jours d’implémentation.
  3. Remettre la répartition JSON avec les ID proposés liés, les exclusions de défauts connus et la prise en compte de toutes les responsabilités restantes ; l’orchestrateur met à jour l’index partagé en série et crée les tâches. Les fichiers partagés ont un seul propriétaire principal malgré plusieurs familles de responsabilités.

  Limite : Aucune migration de code dans cette tâche de recensement. Pour C08, vérifier mécaniquement la différence d’ensembles et ne cartographier que les chemins non couverts ; ne pas refaire les recherches de C01-C07.

- [ ] **[C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039) — Cartographier les outils de l’éditeur et la présentation en dossiers d’extraction circonscrits**

  Responsable **Cyrill/D1** · compétence `inventory` · `sonnet` / **medium**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `src/js/tools.js`; `src/js/select-bridge.js`; `src/js/*-bridge.js (liaisons de l’éditeur uniquement)`; `src/js/ui.js`; `src/js/*-panel.js (panneaux de l’éditeur)`; `modules de dessin, texte, rig, maillage et pinceau d’app-js.profile.json`; `engineering/inventory/remediation-scope.json (partition soumise à l’orchestrateur)`.

  1. Cartographie en lecture seule pour « Outils de l’éditeur et présentation » : nommer les responsabilités réelles, les propriétaires actuels des écritures, les points d’entrée publics et les consommateurs concernés au SHA retenu.
  2. Répartir en dossiers d’une seule responsabilité : symboles/chemins exacts, oracle de jeu d’essai indépendant, frontière de module attendue et un seul rédacteur par fichier entier. Aucun dossier ne nécessite plusieurs jours d’implémentation.
  3. Remettre la répartition JSON avec les ID proposés liés, les exclusions de défauts connus et la prise en compte de toutes les responsabilités restantes ; l’orchestrateur met à jour l’index partagé en série et crée les tâches. Les fichiers partagés ont un seul propriétaire principal malgré plusieurs familles de responsabilités.

  Limite : Aucune migration de code dans cette tâche de recensement. Pour C08, vérifier mécaniquement la différence d’ensembles et ne cartographier que les chemins non couverts ; ne pas refaire les recherches de C01-C07.

- [ ] **[C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040) — Cartographier les médias, l’import, l’export et les services natifs en dossiers d’extraction circonscrits**

  Responsable **Cyrill/D1** · compétence `inventory` · `sonnet` / **medium**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `src/js/export.js`; `src/js/*import*.js`; `src/js/*export*.js`; `src/js/native-video-bridge.js`; `src/js/linked-media.js`; `src/js/media-library.js`; `src-tauri/src/** (hors application_mcp.rs)`; `engineering/inventory/remediation-scope.json (partition soumise à l’orchestrateur)`.

  1. Cartographie en lecture seule pour « Médias, import, export et services natifs » : nommer les responsabilités réelles, les propriétaires actuels des écritures, les points d’entrée publics et les consommateurs concernés au SHA retenu.
  2. Répartir en dossiers d’une seule responsabilité : symboles/chemins exacts, oracle de jeu d’essai indépendant, frontière de module attendue et un seul rédacteur par fichier entier. Aucun dossier ne nécessite plusieurs jours d’implémentation.
  3. Remettre la répartition JSON avec les ID proposés liés, les exclusions de défauts connus et la prise en compte de toutes les responsabilités restantes ; l’orchestrateur met à jour l’index partagé en série et crée les tâches. Les fichiers partagés ont un seul propriétaire principal malgré plusieurs familles de responsabilités.

  Limite : Aucune migration de code dans cette tâche de recensement. Pour C08, vérifier mécaniquement la différence d’ensembles et ne cartographier que les chemins non couverts ; ne pas refaire les recherches de C01-C07.

- [ ] **[C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041) — Cartographier les préférences, Labs et l’initialisation en dossiers d’extraction circonscrits**

  Responsable **Cyrill/D2** · compétence `inventory` · `sonnet` / **medium**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `src/js/labs/**`; `src/js/app.js`; `src/js/i18n.js`; `src/index.html`; `src/css/**`; `responsabilités de préférences/raccourcis/chargement dans timeline.js`; `engineering/inventory/remediation-scope.json (partition soumise à l’orchestrateur)`.

  1. Cartographie en lecture seule pour « Préférences, Labs et initialisation » : nommer les responsabilités réelles, les propriétaires actuels des écritures, les points d’entrée publics et les consommateurs concernés au SHA retenu.
  2. Répartir en dossiers d’une seule responsabilité : symboles/chemins exacts, oracle de jeu d’essai indépendant, frontière de module attendue et un seul rédacteur par fichier entier. Aucun dossier ne nécessite plusieurs jours d’implémentation.
  3. Remettre la répartition JSON avec les ID proposés liés, les exclusions de défauts connus et la prise en compte de toutes les responsabilités restantes ; l’orchestrateur met à jour l’index partagé en série et crée les tâches. Les fichiers partagés ont un seul propriétaire principal malgré plusieurs familles de responsabilités.

  Limite : Aucune migration de code dans cette tâche de recensement. Pour C08, vérifier mécaniquement la différence d’ensembles et ne cartographier que les chemins non couverts ; ne pas refaire les recherches de C01-C07.

- [ ] **[C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042) — Cartographier l’application, le MCP et les diagnostics en dossiers d’extraction circonscrits**

  Responsable **Ilya/O** · compétence `inventory` · `gpt-6-astra` / **high**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `src/js/application/**`; `src/js/adapters/application-mcp.js`; `src/js/bootstrap/**`; `nemo-mcp/**`; `src-tauri/src/application_mcp.rs`; `engineering/inventory/remediation-scope.json (partition soumise à l’orchestrateur)`.

  1. Cartographie en lecture seule pour « Application, MCP et diagnostics » : nommer les responsabilités réelles, les propriétaires actuels des écritures, les points d’entrée publics et les consommateurs concernés au SHA retenu.
  2. Répartir en dossiers d’une seule responsabilité : symboles/chemins exacts, oracle de jeu d’essai indépendant, frontière de module attendue et un seul rédacteur par fichier entier. Aucun dossier ne nécessite plusieurs jours d’implémentation.
  3. Remettre la répartition JSON avec les ID proposés liés, les exclusions de défauts connus et la prise en compte de toutes les responsabilités restantes ; l’orchestrateur met à jour l’index partagé en série et crée les tâches. Les fichiers partagés ont un seul propriétaire principal malgré plusieurs familles de responsabilités.

  Limite : Aucune migration de code dans cette tâche de recensement. Pour C08, vérifier mécaniquement la différence d’ensembles et ne cartographier que les chemins non couverts ; ne pas refaire les recherches de C01-C07.

- [ ] **[C08 / #1043](https://github.com/mysteropodes/nemo/issues/1043) — Cartographier l’exhaustivité et les racines suivies restantes en dossiers d’extraction circonscrits**

  Responsable **Cyrill/D2** · compétence `inventory` · `sonnet` / **medium**. Prédécesseurs : [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036), [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037), [C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038), [C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039), [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041), [C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042).

  Périmètre : `tous les chemins suivis par Git non alloués par C01-C07`; `scripts/**`; `tests/**`; `racines restantes d’exécution/services/workers`; `provenance du code tiers/généré et des données`; `engineering/inventory/remediation-scope.json (partition soumise à l’orchestrateur)`.

  1. Cartographie en lecture seule pour « Exhaustivité et racines suivies restantes » : nommer les responsabilités réelles, les propriétaires actuels des écritures, les points d’entrée publics et les consommateurs concernés au SHA retenu.
  2. Répartir en dossiers d’une seule responsabilité : symboles/chemins exacts, oracle de jeu d’essai indépendant, frontière de module attendue et un seul rédacteur par fichier entier. Aucun dossier ne nécessite plusieurs jours d’implémentation.
  3. Remettre la répartition JSON avec les ID proposés liés, les exclusions de défauts connus et la prise en compte de toutes les responsabilités restantes ; l’orchestrateur met à jour l’index partagé en série et crée les tâches. Les fichiers partagés ont un seul propriétaire principal malgré plusieurs familles de responsabilités.

  Limite : Aucune migration de code dans cette tâche de recensement. Pour C08, vérifier mécaniquement la différence d’ensembles et ne cartographier que les chemins non couverts ; ne pas refaire les recherches de C01-C07.

- [ ] **[P03 / #1005](https://github.com/mysteropodes/nemo/issues/1005) — Valider le recensement combiné du code et inscrire toute la file d’extraction**

  Responsable **Ilya/O** · compétence `orchestration` · `gpt-6-astra` / **high**. Prédécesseurs : [P02 / #1004](https://github.com/mysteropodes/nemo/issues/1004), [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036), [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037), [C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038), [C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039), [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041), [C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042), [C08 / #1043](https://github.com/mysteropodes/nemo/issues/1043).

  Périmètre : `engineering/inventory/remediation-scope.json (nouvel index de recensement)`; `issues GitHub élémentaires et couverture figée du code/des consommateurs`.

  1. Fusionner les huit partitions du recensement à un même SHA ; chaque racine suivie de code d’exécution ou d’outillage écrit manuellement est classée exactement une fois, avec exclusions explicites du code tiers/généré et des données.
  2. Chaque responsabilité restante est couverte par une tâche exécutable liée, avec rédacteur du fichier entier, API publique, matrice de consommateurs et périmètre d’implémentation <=90 minutes ; scinder les unités plus grandes avant Ready.
  3. Figer les nombres et l’empreinte de l’ensemble de sources ; tout élargissement produit exige une décision humaine explicite. Toute source ou surface sans correspondance fait échouer le contrôle d’exhaustivité.

  Limite : Ne pas confondre les 902 lignes d’inventaire avec 902 capacités distinctes, ni classer chaque grande table de données comme monolithe de code.

  Admission découpée avant implémentation : [P03A / #1116](https://github.com/mysteropodes/nemo/issues/1116) fige l’index source et rejette les déclarations de complétude non prouvées ; [P03B / #1170](https://github.com/mysteropodes/nemo/issues/1170) le refige sur `main` courant avec les suppléments C19/C20 épinglés, enregistre la couverture par plages recalculée fichier par fichier (21 422 lignes de code en 171 plages n’appartiennent encore à aucun paquet — `app.js`, la fin de `timeline.js` et `motion.js` d’abord) et n’admet les dispositions relues qu’avec leurs preuves. [Contrat de l’index](../inventory/REMEDIATION_SCOPE.md). P03 reste ouvert pour P03C : réconcilier les cinq notes ouvertes et la dérive des plages, cartographier les monolithes non couverts en tranches de recensement bornées, puis découper les paquets en attente en feuilles de ≤90 minutes ou les dispositionner ; les références de paquets en attente ne sont pas des tâches Ready.

- [ ] **[F01 / #1047](https://github.com/mysteropodes/nemo/issues/1047) — Examiner les preuves existantes de Honey sur le paquet installé**

  Responsable **Cyrill/O** · compétence `validation` · `opus` / **high**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `PR #1001 engineering/remediation/receipts/R14-packaging/**`.

  1. Lire la dernière tête de la PR #1001 et le résumé corrigé ; vérifier les empreintes des artefacts et les preuves de connexion à l’installation.
  2. Identifier les preuves architecturales acceptées et les limites produit/plateforme restantes sans déclarer FFmpeg ou l’export réparés.
  3. Consigner une conclusion de revue circonscrite dans le flux existant et préserver la responsabilité de Honey jusqu’à une transmission explicite.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[F02 / #1048](https://github.com/mysteropodes/nemo/issues/1048) — Terminer l’acceptation existante de Fizz sur le schéma des payloads**

  Responsable **Cyrill/D1** · compétence `capabilities` · `sonnet` / **medium**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `PR #1002 nemo-mcp/src/contract.rs`; `PR #1002 nemo-mcp/src/server.rs`; `PR #1002 nemo-mcp/tests/stdio_contract.rs`; `PR #1002 engineering/application/transport-v1.schema.json`.

  1. Poursuivre le flux Fizz existant ; ne pas assigner un rédacteur en double ni remplacer sa branche.
  2. Le payload objet annoncé et l’orthographe de l’identité d’instance passent les contrôles de client réel et d’entrée malformée sur le candidat exact.
  3. La PR existante comprend la validation ciblée et une transmission explicite indiquant qu’elle est prête pour revue ; l’orchestrateur intègre avec les protections normales.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[P14 / #1016](https://github.com/mysteropodes/nemo/issues/1016) — Terminer l’acceptation de l’isolation native entre deux instances**

  Responsable **Cyrill/D2** · compétence `validation` · `sonnet` / **medium**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `tests/desktop/* ajouts d’acceptation R06`; `scripts/nemo/native.cjs et documentation d’exécution en lecture seule`.

  1. Deux instances identifiées disposent de racines différentes pour application, navigateur, fichiers temporaires, cache, build et rapports.
  2. Une tentative délibérée d’arrêt par un autre propriétaire est rejetée, et le comportement du créneau réservé bureau/GPU est observé.
  3. Consigner les octets réels du candidat et le nettoyage des processus ; une plateforme indisponible ne limite que la conclusion qui la concerne.

  Limite : Aucune réparation sans rapport de FFmpeg, d’export, de signature ou de prise en charge de plateforme ; un défaut du harnais reçoit son propre dossier restreint.

- [ ] **[P15 / #1017](https://github.com/mysteropodes/nemo/issues/1017) — Ajouter une régression sur un véritable geste du curseur d’opacité**

  Responsable **Cyrill/D1** · compétence `validation` · `sonnet` / **medium**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `nouveau tests/browser/opacity-ui-consumers.spec.cjs`; `tests/browser/opacity-consumers.spec.cjs existant en lecture seule`; `jeux d’opacité existants`.

  1. Piloter le véritable contrôle d’opacité sur un jeu d’essai fixe ; vérifier une valeur finale spécifiée indépendamment.
  2. Le geste crée une seule entrée d’annulation ; undo restaure la valeur initiale consignée et redo restaure la valeur modifiée.
  3. Les tests existants de commande directe, sauvegarde, réouverture et export restent intacts. Consigner précisément tout échec préexistant de l’interface, sans le réparer dans cette tâche de test.

  Limite : Ne pas reproduire les assertions existantes de sauvegarde/réouverture/SVG sous un second harnais, ni corriger silencieusement le produit dans un périmètre limité aux tests.

### B — contrats, registre et rapports assortis de contrôles

| Ilya — étapes sous sa responsabilité | Cyrill — étapes sous sa responsabilité |
|---|---|
| [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006) · **O** · Approuver le descripteur de capacité v1 avec deux exemples concrets | [P09 / #1011](https://github.com/mysteropodes/nemo/issues/1011) · **D2** · Intégrer les contrôles de dérive des capacités/schémas à la validation locale normale |
| [D01 / #1044](https://github.com/mysteropodes/nemo/issues/1044) · **O** · Spécifier l’identité du document et la compatibilité du temps et des coordonnées | [P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013) · **D1** · Couvrir la classification du code Rust et le plafonnement de sa taille |
| [D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045) · **O** · Spécifier les contrats de cycle de vie des transactions, jobs et ressources | [P13 / #1015](https://github.com/mysteropodes/nemo/issues/1015) · **D2** · Classer séparément initialisation, styles et données de l’interface et modules exécutables |
| [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007) · **D1** · Créer l’enregistrement déterministe des fonctionnalités et la validation des descripteurs | [T01 / #1050](https://github.com/mysteropodes/nemo/issues/1050) · **D2** · Mesurer et imposer la couverture JavaScript migrée avec c8 |
| [P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008) · **D1** · Déplacer les métadonnées et le routage de l’opacité dans son module fonctionnel | [T03 / #1052](https://github.com/mysteropodes/nemo/issues/1052) · **D2** · Conserver des traces et rapports utiles des échecs Playwright |
| [P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009) · **D2** · Faire consommer les contrats de fonctionnalités par la découverte et le routage MCP Rust | [T04 / #1053](https://github.com/mysteropodes/nemo/issues/1053) · **D1** · Générer la couverture Rust de la crate MCP existante |
| [P10 / #1012](https://github.com/mysteropodes/nemo/issues/1012) · **D2** · Faire respecter les dépendances applicatives par le contrôle normal | [P16 / #1018](https://github.com/mysteropodes/nemo/issues/1018) · **O** · Accepter le parcours d’opacité via Claude installé |
| [P12 / #1014](https://github.com/mysteropodes/nemo/issues/1014) · **D2** · Faire respecter la frontière de dépendances du module public du moteur géométrique | — |
| [B01 / #1046](https://github.com/mysteropodes/nemo/issues/1046) · **D2** · Faire respecter la frontière du module public applicatif/MCP natif | — |
| [T02 / #1051](https://github.com/mysteropodes/nemo/issues/1051) · **D2** · Distinguer les échecs exacts de référence des nouvelles régressions | — |
| [F03 / #1049](https://github.com/mysteropodes/nemo/issues/1049) · **O** · Accepter le parcours d’opacité via Codex installé | — |

- [ ] **[P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006) — Approuver le descripteur de capacité v1 avec deux exemples concrets**

  Responsable **Ilya/O** · compétence `capabilities` · `gpt-6-astra` / **high**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `engineering/application/capability-v1.schema.json (nouveau)`; `un jeu de descripteur d’opacité`; `un jeu existant de descripteur de job d’export`.

  1. Le schéma nomme ID/version stables, entrée/sortie typées, unités, effets, disponibilité, clé de gestionnaire, jeu d’essai et exemples.
  2. Les exemples d’opacité et de job d’export sont valides ; des cas d’échec sont spécifiés pour ID malformés/dupliqués et version non prise en charge.
  3. L’enregistrement est déterministe à partir de déclarations de fonctionnalités de confiance ; interface/API/MCP partagent les gestionnaires. Ne pas modifier les fichiers actifs de Fizz.

  Limite : Aucune place de marché générale de plugins, découverte d’exécutables arbitraires, nouveau DSL ou réécriture TypeScript/framework. Ne pas chevaucher les fichiers de contrat actifs de Fizz.

- [ ] **[D01 / #1044](https://github.com/mysteropodes/nemo/issues/1044) — Spécifier l’identité du document et la compatibilité du temps et des coordonnées**

  Responsable **Ilya/O** · compétence `architecture` · `gpt-6-astra` / **high**. Prédécesseurs : [P02 / #1004](https://github.com/mysteropodes/nemo/issues/1004).

  Périmètre : `engineering/application/value-contract-v1.schema.json (nouveau)`; `jeux existants de document/animation (en lecture seule)`.

  1. Définir ID stables, version de format du document, unités de temps/images et de coordonnées à partir des valeurs existantes ; nommer le propriétaire actuel de l’état.
  2. Un ancien projet de test effectue un aller-retour sans modifier ses valeurs sérialisées ; les migrations futures et champs inconnus ont une règle explicite de compatibilité.
  3. Documenter les métadonnées de précision/couleur/alpha aux frontières des adaptateurs ; aucune conversion du format de stockage ni nouveau pipeline couleur.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045) — Spécifier les contrats de cycle de vie des transactions, jobs et ressources**

  Responsable **Ilya/O** · compétence `architecture` · `gpt-6-astra` / **high**. Prédécesseurs : [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Périmètre : `engineering/application/operation-contract-v1.schema.json (nouveau)`; `code existant de requêtes/historique d’opacité (en lecture seule)`.

  1. Définir identité de requête, révision attendue, résultat de nouvelle tentative, begin/update/commit/cancel et états terminaux de job.
  2. Définir le propriétaire et la libération des handles de ressource ainsi que le comportement sur document périmé, avec exemples explicites ; une seule autorité de modification demeure.
  3. La compatibilité publique de schéma/version et les contrôles d’accès côté serveur sont explicites ; aucun conteneur générique de services ni miroir Rust modifiable.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007) — Créer l’enregistrement déterministe des fonctionnalités et la validation des descripteurs**

  Responsable **Ilya/D1** · compétence `capabilities` · `gpt-6-sol` / **medium**. Prédécesseurs : [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006), [D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045).

  Périmètre : `nouveau src/js/application/capability-registry.js`; `nouveau schéma de descripteur de fonctionnalité et tests du registre`.

  1. Les ID dupliqués, versions de schéma non prises en charge et gestionnaires/schémas manquants produisent des échecs explicites.
  2. Le registre produit une découverte déterministe à partir des enregistrements des modules ; les gestionnaires restent dans les modules propriétaires des fonctionnalités.
  3. Les données brutes opaques d’image/géométrie sont représentées par des handles, pas par du JSON de commande.

  Limite : Ne pas réparer les défauts fonctionnels préexistants sans rapport. Préserver exactement le comportement de référence et consigner les limites.

- [ ] **[P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008) — Déplacer les métadonnées et le routage de l’opacité dans son module fonctionnel**

  Responsable **Ilya/D1** · compétence `capabilities` · `gpt-6-sol` / **medium**. Prédécesseurs : [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007).

  Périmètre : `src/js/application/opacity-application.js`; `src/js/domain/animation/opacity.js`; `src/js/bootstrap/opacity-application.js`; `nouvel enregistrement de fonctionnalité d’opacité`; `tests/application-opacity*.cjs`.

  1. Les opérations actuelles d’opacité et leurs sémantiques d’identité/révision/nouvelle tentative/historique passent les contrôles indépendants existants via le registre.
  2. L’entrée publique de l’application atteint le gestionnaire enregistré ; aucun second propriétaire des écritures d’opacité ne subsiste.
  3. Le comportement actuel hors opacité et les défauts connus restent inchangés.

  Limite : Ne pas réparer les défauts fonctionnels préexistants sans rapport. Préserver exactement le comportement de référence et consigner les limites.

- [ ] **[P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009) — Faire consommer les contrats de fonctionnalités par la découverte et le routage MCP Rust**

  Responsable **Ilya/D2** · compétence `capabilities` · `gpt-6-luna` / **medium**. Prédécesseurs : [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007), [F02 / #1048](https://github.com/mysteropodes/nemo/issues/1048).

  Périmètre : `nemo-mcp/src/server.rs`; `nemo-mcp/src/contract.rs`; `nemo-mcp/src/schema.rs`; `src/js/adapters/application-mcp.js`; `tests de protocole MCP`.

  1. Le MCP annonce exactement le contrat d’entrée/sortie enregistré et route vers les gestionnaires applicatifs partagés.
  2. Une seule modification générique du transport suffit : ajouter ensuite une fonctionnalité ne nécessite aucune entrée manuelle dans un sélecteur d’opérations MCP.
  3. Un payload malformé, une capacité non prise en charge et une plateforme indisponible renvoient des erreurs typées permettant d’agir.

  Limite : Attendre la publication ou la décision sur Fizz #1002 ; ne pas dupliquer sa correction de payload. Préserver le transport pris en charge épinglé jusqu’à un dossier explicite de mise à niveau.

- [ ] **[P09 / #1011](https://github.com/mysteropodes/nemo/issues/1011) — Intégrer les contrôles de dérive des capacités/schémas à la validation locale normale**

  Responsable **Cyrill/D2** · compétence `validation` · `sonnet` / **medium**. Prédécesseurs : [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007), [P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009).

  Périmètre : `scripts/nemo/* fichiers de contrôle des capacités (nouveau)`; `tests/* contrôles de capacités (nouveau)`; `scripts/nemo/ci.cjs raccordement partagé par Ilya`.

  1. Une surface déclarée manquante, un schéma généré périmé, une capacité dupliquée et un jeu d’exemple manquant font chacun échouer le contrôle local nommé.
  2. Les contrôles négatifs exercent le vérificateur livré, pas un validateur réservé aux tests et inutilisé.
  3. Le compte rendu local identifie le catalogue généré et l’empreinte des sources ; l’omission d’un job de vérification ne peut pas produire une acceptation globale.

  Limite : Aucune exécution d’Actions hébergées ni nouvelle automatisation de CI fonctionnant en permanence.

- [ ] **[P10 / #1012](https://github.com/mysteropodes/nemo/issues/1012) — Faire respecter les dépendances applicatives par le contrôle normal**

  Responsable **Ilya/D2** · compétence `validation` · `gpt-6-luna` / **medium**. Prédécesseurs : [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Périmètre : `scripts/nemo/ci.cjs`; `scripts/nemo/lib/boundaries*.cjs`; `engineering/boundaries/profiles/app-js.profile.json`; `tests de contrôles négatifs de frontières`.

  1. La tranche migrée adoptée échoue aux contrôles de dépendance interdite, d’import privé, de variable globale implicite et de cycle via la même commande locale de frontières.
  2. Les relations entre scripts classiques non migrés sont classées honnêtement ; elles ne reçoivent pas une précision fictive du graphe.
  3. Présenter séparément les chemins analysés et ceux dont l’héritage reste non résolu, et maintenir les contrôles de non-croissance sur ce code sans prétendre couvrir toutes les dépendances.

  Limite : Ne pas attendre la totalité de R03 ni prétendre que la mesure de taille du texte prouve le sens des dépendances.

- [ ] **[P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013) — Couvrir la classification du code Rust et le plafonnement de sa taille**

  Responsable **Cyrill/D1** · compétence `validation` · `sonnet` / **medium**. Prédécesseurs : [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042).

  Périmètre : `engineering/boundaries/profiles/* profils Rust`; `scripts/nemo/lib/boundaries-size.cjs`; `nouveaux contrôles de couverture des sources Rust`.

  1. Découvrir indépendamment toutes les racines suivies de code Rust, y compris native, geometry et nemo-mcp ; chaque chemin est adopté ou explicitement exclu.
  2. Un fichier .rs trop grand dans le périmètre, un fichier non déclaré et un plafond hérité relevé font échouer la validation locale normale.
  3. Émettre le nombre de fichiers Rust découverts et la liste des chemins classés/exclus ; vérifier que le jeu de test ne modifie ni les plafonds hérités conservés ni les exclusions de code généré/tiers.

  Limite : Ce dossier démontre la découverte et la taille ; il ne prétend pas faire respecter le graphe de dépendances/modules Cargo.

- [ ] **[P12 / #1014](https://github.com/mysteropodes/nemo/issues/1014) — Faire respecter la frontière de dépendances du module public du moteur géométrique**

  Responsable **Ilya/D2** · compétence `validation` · `gpt-6-luna` / **medium**. Prédécesseurs : [P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Périmètre : `geometry-wasm/Cargo.toml`; `geometry-wasm/src/engine.rs`; `politique de dépendances Rust et tests du vérificateur`.

  1. Nommer les dépendances autorisées entre modules du moteur géométrique et l’interface exportée.
  2. Un import de production interdit fait échouer le contrôle local normal avec une analyse adaptée à Rust ; une dépendance autorisée passe.
  3. Les configurations de fonctionnalités Cargo déclarées sont exercées ou explicitement bloquées ; aucun analyseur lexical JS n’est utilisé pour Rust.

  Limite : Ne pas fournir du Rust à un analyseur lexical JavaScript ; les cas de graphe non pris en charge sont déclarés, pas acceptés silencieusement.

- [ ] **[B01 / #1046](https://github.com/mysteropodes/nemo/issues/1046) — Faire respecter la frontière du module public applicatif/MCP natif**

  Responsable **Ilya/D2** · compétence `validation` · `gpt-6-luna` / **medium**. Prédécesseurs : [P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013), [D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045).

  Périmètre : `src-tauri/src/application_mcp.rs`; `src-tauri/Cargo.toml`; `politique de frontières Rust natives et tests du vérificateur`.

  1. Déclarer l’interface applicative/MCP native et les dépendances autorisées.
  2. Un import privé franchissant la frontière fait échouer le contrôle local livré ; l’accès par l’interface approuvée passe.
  3. Les modules Rust non analysés restent des entrées explicites du recensement au lieu de recevoir un faux résultat positif de graphe.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[P13 / #1015](https://github.com/mysteropodes/nemo/issues/1015) — Classer séparément initialisation, styles et données de l’interface et modules exécutables**

  Responsable **Cyrill/D2** · compétence `validation` · `sonnet` / **medium**. Prédécesseurs : [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041), [C08 / #1043](https://github.com/mysteropodes/nemo/issues/1043).

  Périmètre : `src/index.html`; `src/css/style.css`; `src/js/i18n.js`; `src/js/shader-effects-library.js`; `politique des profils associés`.

  1. Séparer les catalogues générés/de données de la logique de contrôle écrite manuellement ; appliquer les limites cibles de manière adaptée.
  2. Un nouveau bloc d’initialisation ou exécutable non classé échoue ; le contenu des catalogues existants reste identique octet par octet lorsqu’il ne contient que des données.
  3. Consigner les affectations exactes aux profils et la provenance des exclusions ; le vérificateur normal rejette un fichier exécutable mal étiqueté comme données selon la règle adoptée.

  Limite : Aucun reformatage massif, réécriture des traductions ou découpage arbitraire des tables uniquement pour réduire le nombre de lignes. Les modifications exactes attendent le créneau du propriétaire du fichier entier.

- [ ] **[T01 / #1050](https://github.com/mysteropodes/nemo/issues/1050) — Mesurer et imposer la couverture JavaScript migrée avec c8**

  Responsable **Cyrill/D2** · compétence `validation` · `sonnet` / **medium**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `package.json`; `package-lock.json`; `nouvelle configuration c8`; `tests du job de couverture des courbes/opacité`.

  1. Conserver node:test ; c8 --all rapporte chaque module explicitement migré, y compris un module de contrôle négatif délibérément non importé.
  2. Générer des rapports HTML, LCOV et JSON ; imposer 90 % des lignes et 80 % des branches par module domaine/applicatif migré, avec exceptions exactes relues.
  3. Une exécution volontairement sous le seuil fait échouer le compte rendu ; les tests existants restent découvrables et les modules hérités ne sont pas tenus à tort d’atteindre le nouveau seuil.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[T02 / #1051](https://github.com/mysteropodes/nemo/issues/1051) — Distinguer les échecs exacts de référence des nouvelles régressions**

  Responsable **Ilya/D2** · compétence `validation` · `gpt-6-luna` / **medium**. Prédécesseurs : [P02 / #1004](https://github.com/mysteropodes/nemo/issues/1004).

  Périmètre : `scripts/nemo/lib/ comparateur de référence (nouveau)`; `manifeste de jeu d’essai/référence`; `tests de comparaison`.

  1. Comparer référence et candidat avec l’identité du code, du jeu d’essai et de l’environnement ; classer séparément une signature connue inchangée.
  2. Une signature modifiée, un nouveau cas en échec ou un cas requis manquant font échouer la comparaison ; un environnement d’exécution requis indisponible reste bloqué.
  3. Garder les échecs bruts des tests visibles et produire un résultat distinct de comparaison des régressions ; aucune autorisation globale d’échec ni réécriture des références attendues.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[T03 / #1052](https://github.com/mysteropodes/nemo/issues/1052) — Conserver des traces et rapports utiles des échecs Playwright**

  Responsable **Cyrill/D2** · compétence `validation` · `sonnet` / **medium**. Prédécesseurs : aucun ; réserver les fichiers et l’environnement d’exécution.

  Périmètre : `configuration Playwright navigateur`; `scripts/nemo adaptateur de rapport des jobs navigateur`; `contrôle négatif du harnais navigateur`.

  1. Un échec volontaire d’assertion d’interface produit une trace inspectable, un rapport HTML/JUnit et un compte rendu en échec.
  2. Utiliser retain-on-failure sans masquage par nouvelles tentatives automatiques ; les exécutions réussies suivent la politique de conservation bornée.
  3. Indiquer l’identité du code, du navigateur et de l’environnement d’exécution, et réutiliser l’isolation existante ; aucun contenu de véritable document utilisateur dans les artefacts de test.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[T04 / #1053](https://github.com/mysteropodes/nemo/issues/1053) — Générer la couverture Rust de la crate MCP existante**

  Responsable **Cyrill/D1** · compétence `validation` · `sonnet` / **medium**. Prédécesseurs : [C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042).

  Périmètre : `nemo-mcp/Cargo.toml (en lecture seule sauf nécessité)`; `enveloppe locale de job cargo-llvm-cov`; `tests de compte rendu de couverture Rust`.

  1. Exécuter les suites Cargo MCP existantes sous une version compatible et épinglée de cargo-llvm-cov ; produire des résumés HTML/LCOV pour le code de production.
  2. Consigner les modules non couverts et les pourcentages initiaux observés ; introduire un seuil de non-régression relu plutôt qu’inventer une cible globale déjà satisfaite.
  3. L’absence d’un outil ou d’une capacité de couverture est un blocage, et un test échoué reste un échec. L’adoption de la couverture native/géométrique constitue une tâche de recensement distincte.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[P16 / #1018](https://github.com/mysteropodes/nemo/issues/1018) — Accepter le parcours d’opacité via Claude installé**

  Responsable **Cyrill/O** · compétence `orchestration` · `opus` / **high**. Prédécesseurs : [F01 / #1047](https://github.com/mysteropodes/nemo/issues/1047), [F02 / #1048](https://github.com/mysteropodes/nemo/issues/1048).

  Périmètre : `harnais fizz/r14-parity existant et preuves PR1001/1002`.

  1. Utiliser le harnais de parité conservé et les octets identifiés de l’application/MCP installés via Claude avec son schéma annoncé.
  2. Découvrir, inspecter, modifier, annuler/rétablir et sauvegarder/réouvrir le jeu d’opacité fixe ; distinguer chaque résultat et tout défaut de référence.
  3. Consigner les versions d’exécution/client, les empreintes du candidat, la trace et les limites de plateforme restantes ; ne pas dupliquer l’implémentation de Fizz/Honey.

  Limite : Conserver les responsabilités actuelles de Fizz/Honey ; il s’agit de leur travail existant, pas d’une nouvelle mission en double. L’échec FFmpeg existant n’a pas à bloquer l’acceptation architecturale MCP/schéma sans rapport ; la conclusion d’export non étayée reste ouverte.

- [ ] **[F03 / #1049](https://github.com/mysteropodes/nemo/issues/1049) — Accepter le parcours d’opacité via Codex installé**

  Responsable **Ilya/O** · compétence `validation` · `gpt-6-astra` / **high**. Prédécesseurs : [F01 / #1047](https://github.com/mysteropodes/nemo/issues/1047), [F02 / #1048](https://github.com/mysteropodes/nemo/issues/1048).

  Périmètre : `fizz/r14-parity tests/mcp-parity/** existants (réutilisation)`; `compte rendu de l’application/MCP macOS installés`.

  1. Utiliser le harnais de parité publié sur les octets installés identifiés via Codex, en utilisant le schéma annoncé.
  2. Découvrir, inspecter, modifier, annuler/rétablir et sauvegarder/réouvrir le jeu fixe ; inclure les résultats de reconnexion et d’écriture périmée.
  3. Consigner l’identité du candidat/client et les cas exacts acceptés ou bloqués ; aucun oracle de payload écrit à la main ne remplace l’utilisabilité du client.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

### C — job d’export contrastant et diagnostics partagés

| Ilya — étapes sous sa responsabilité | Cyrill — étapes sous sa responsabilité |
|---|---|
| [P17 / #1019](https://github.com/mysteropodes/nemo/issues/1019) · **D1** · Extraire l’adaptateur existant d’export SVG d’une seule image | [P19 / #1021](https://github.com/mysteropodes/nemo/issues/1021) · **D1** · Relier l’interface d’export existante et le MCP au même job |
| [H01 / #1058](https://github.com/mysteropodes/nemo/issues/1058) · **D1** · Cartographier les entrées immuables de séquence SVG avant l’extraction du job | [P08 / #1010](https://github.com/mysteropodes/nemo/issues/1010) · **D1** · Vérifier l’enregistrement automatique de l’export depuis une nouvelle session d’agent |
| [P18 / #1020](https://github.com/mysteropodes/nemo/issues/1020) · **D1** · Ajouter un cycle de vie de job borné à cet exportateur | [T07 / #1056](https://github.com/mysteropodes/nemo/issues/1056) · **D1** · Effectuer l’aller-retour d’un paquet de reproduction synthétique isolé |
| [T05 / #1054](https://github.com/mysteropodes/nemo/issues/1054) · **D1** · Extraire les diagnostics applicatifs bornés à partir de l’opacité | [T08 / #1057](https://github.com/mysteropodes/nemo/issues/1057) · **D2** · Exposer l’inspecteur de diagnostic partagé dans l’interface et le MCP |
| [T06 / #1055](https://github.com/mysteropodes/nemo/issues/1055) · **D2** · Corréler une requête MCP Rust avec les diagnostics applicatifs | — |

- [ ] **[P17 / #1019](https://github.com/mysteropodes/nemo/issues/1019) — Extraire l’adaptateur existant d’export SVG d’une seule image**

  Responsable **Ilya/D1** · compétence `extraction` · `gpt-6-sol` / **medium**. Prédécesseurs : [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006), [P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008), [D01 / #1044](https://github.com/mysteropodes/nemo/issues/1044), [D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045).

  Périmètre : `src/js/export.js: exportFrameSVGString`; `nouveau src/js/adapters/export-svg-frame.js`.

  1. La fonction de production `exportFrameSVGString` délègue à l’adaptateur ; préambule XML, width/height/viewBox et basculement de visibilité restent équivalents pour le jeu existant.
  2. Les attentes SVG existantes d’opacité aux images 0/10/20 et les contrôles indépendants du document stocké avant/après passent par le code déplacé.
  3. Le nouveau module possède des interfaces explicites de construction d’image, de dimensions et de rectangle Paper, et les contrôles applicables de taille/dépendances ; aucune seconde implémentation du rendu.

  Limite : Périmètre : `src/js/export.js:352`, `exportFrameSVGString(frameIdx)`, avec `exportBuildFrame` existant fourni comme interface ; nouveau `src/js/adapters/export-svg-frame.js`. Ne pas extraire/réécrire le constructeur d’image de 320 lignes dans ce dossier. Oracle connu existant : `tests/browser/opacity-consumers.spec.cjs` appelle ce véritable chemin SVG et vérifie les pixels de sortie animés par clés ; cette preuve existe déjà, elle n’a pas été réexécutée ici. Aucune dépendance FFmpeg. Ilya D1 suggéré, après le contrat de fonctionnalité/export.

- [ ] **[H01 / #1058](https://github.com/mysteropodes/nemo/issues/1058) — Cartographier les entrées immuables de séquence SVG avant l’extraction du job**

  Responsable **Ilya/D1** · compétence `architecture` · `gpt-6-sol` / **high**. Prédécesseurs : [P17 / #1019](https://github.com/mysteropodes/nemo/issues/1019), [D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045).

  Périmètre : `src/js/export.js: exportSVGSequenceToDir / exportFrameRange / exportBuildFrame`; `src/js/render-manager.js appelants (en lecture seule)`.

  1. Énumérer chaque lecture d’état source nécessaire au jeu SVG retenu de trois images statiques/animées par clés, y compris contexte de scène/composant, caméra et dimensions ; identifier les lectures capturées et les lectures en direct.
  2. Spécifier un instantané ou une interface de session d’évaluation protégée empêchant le mélange de révisions du document entre les awaits ; rejeter explicitement le simple ajout d’une étiquette `revision` tout en lisant des variables globales modifiables. Spécifier les états start/status/cancel/artifact et le nettoyage des fichiers partiels.
  3. Nommer les propriétaires exacts du code et les cas de test d’un job immuable et du raccordement existant interface/MCP. Jusqu’à leur acceptation, P18/P19 restent Inbox ; le code actuel relit l’état global pour chaque image et ne possède aucun contrat d’annulation.

  Limite : Cartographie circonscrite uniquement. Produire dans l’issue le contrat exact et le dossier d’implémentation ; ne pas réparer le comportement connu de contexte/annulation ni lancer une refonte générale.

- [ ] **[P18 / #1020](https://github.com/mysteropodes/nemo/issues/1020) — Ajouter un cycle de vie de job borné à cet exportateur**

  Responsable **Ilya/D1** · compétence `extraction` · `gpt-6-sol` / **medium**. Prédécesseurs : [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [P17 / #1019](https://github.com/mysteropodes/nemo/issues/1019), [H01 / #1058](https://github.com/mysteropodes/nemo/issues/1058).

  Périmètre : `nouveau module applicatif de job d’export`; `src/js/export.js`; `tests de cycle de vie du job`.

  1. start/status/cancel possède une identité stable de job/requête, un cycle de vie terminal monotone et une progression bornée.
  2. Une nouvelle tentative ne crée pas d’artefacts validés en double ; les résultats tardifs ne peuvent pas se rattacher à un document remplacé ; l’annulation ne qualifie jamais de complète une sortie partielle.
  3. L’appelant de l’exportateur existant utilise cette frontière de job ; un jeu de trois images vérifie l’identité de l’artefact terminal et l’absence de lectures mélangeant plusieurs révisions via l’interface approuvée dans H01.

  Limite : Aucun ordonnanceur distribué général, nouvelle flotte de workers ou garantie de durabilité au-delà du contrat de job explicitement choisi.

- [ ] **[P19 / #1021](https://github.com/mysteropodes/nemo/issues/1021) — Relier l’interface d’export existante et le MCP au même job**

  Responsable **Cyrill/D1** · compétence `capabilities` · `sonnet` / **medium**. Prédécesseurs : [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [P18 / #1020](https://github.com/mysteropodes/nemo/issues/1020), [P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009).

  Périmètre : `enregistrement de fonctionnalité d’export`; `tests dédiés navigateur/MCP de job d’export`; `pont d’interface délimité par P03`.

  1. Une véritable interaction d’interface et un client MCP réel atteignent le même contrat de job et d’artefact.
  2. Progression, annulation et identité d’artefact terminal correspondent à l’API directe, avec disponibilité de plateforme déclarée.
  3. Désactiver le backend retenu dans un jeu d’essai : interface et MCP rapportent le même motif typé de disponibilité, et aucun job ni artefact partiel n’est marqué à tort comme réussi.

  Limite : Aucun buffer de pixels dans le JSON MCP. Les backends existants défaillants restent indisponibles avec un motif.

- [ ] **[P08 / #1010](https://github.com/mysteropodes/nemo/issues/1010) — Vérifier l’enregistrement automatique de l’export depuis une nouvelle session d’agent**

  Responsable **Cyrill/D1** · compétence `capabilities` · `sonnet` / **medium**. Prédécesseurs : [P19 / #1021](https://github.com/mysteropodes/nemo/issues/1021).

  Périmètre : `module fonctionnel et descripteur d’export de P19`; `tests dédiés d’acceptation de l’enregistrement des fonctionnalités`.

  1. Un nouvel agent charge la déclaration de fonctionnalité d’export via la génération et l’initialisation normales sans modifier le code MCP ni un sélecteur central d’opérations.
  2. L’API directe et le MCP exposent le même schéma/gestionnaire et les cas d’indisponibilité documentés.
  3. Retirer la déclaration fait échouer le contrôle standard d’exhaustivité ; la restaurer le fait passer. Cela accepte le second type de fonctionnalité sans créer de fonctionnalité produit artificielle.

  Limite : Ne pas inventer une nouvelle fonctionnalité produit uniquement pour prouver l’enregistrement. Utiliser uniquement la déclaration d’export et les chemins exacts acceptés dans P19 ; vérifier ce prédécesseur avant de commencer.

- [ ] **[T05 / #1054](https://github.com/mysteropodes/nemo/issues/1054) — Extraire les diagnostics applicatifs bornés à partir de l’opacité**

  Responsable **Ilya/D1** · compétence `diagnostics` · `gpt-6-sol` / **high**. Prédécesseurs : [P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008).

  Périmètre : `src/js/application/opacity-application.js section de trace/rejeu`; `nouvelle interface/service applicatif de diagnostic`; `tests/application-opacity-replay.test.cjs`.

  1. Réutiliser le comportement existant de trace/rejeu via un service versionné portant les ID de build, d’instance, de document, de révision et de requête.
  2. Les enregistrements bornés et détachés ne peuvent pas modifier l’application ; les tests existants de conservation, de nouvelle tentative et de document périmé passent.
  3. L’enregistrement détaillé est facultatif et borné ; aucun second propriétaire de l’état, aucune évaluation arbitraire de code ni capacité shell.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[T06 / #1055](https://github.com/mysteropodes/nemo/issues/1055) — Corréler une requête MCP Rust avec les diagnostics applicatifs**

  Responsable **Ilya/D2** · compétence `diagnostics` · `gpt-6-luna` / **high**. Prédécesseurs : [P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009), [T05 / #1054](https://github.com/mysteropodes/nemo/issues/1054).

  Périmètre : `nemo-mcp/src instrumentation du transport`; `nemo-mcp/Cargo.toml`; `tests stdio MCP`.

  1. Ajouter une dépendance Rust directe à tracing uniquement là où elle est utilisée ; le même ID de requête relie démarrage, résultat et échec structuré.
  2. L’annulation ou la terminaison clôt le span ; la destination stderr/journal est bornée et aucun diagnostic ne contamine stdout du protocole.
  3. Une sentinelle de jeu d’essai représentant secrets/chemins privés est exclue par défaut ; l’instrumentation ne change pas l’autorité des commandes.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[T07 / #1056](https://github.com/mysteropodes/nemo/issues/1056) — Effectuer l’aller-retour d’un paquet de reproduction synthétique isolé**

  Responsable **Cyrill/D1** · compétence `diagnostics` · `sonnet` / **medium**. Prédécesseurs : [T05 / #1054](https://github.com/mysteropodes/nemo/issues/1054).

  Périmètre : `codec de paquet de reproduction de diagnostic`; `jeu d’opacité synthétique`; `tests d’intégration de rejeu`.

  1. Exporter dans un paquet borné l’empreinte versionnée du jeu d’essai, la séquence de commandes, l’horloge/graine et les versions pertinentes.
  2. L’import et le rejeu dans un document isolé produisent l’empreinte d’état/historique spécifiée indépendamment.
  3. Les paquets malformés ou incompatibles en version sont rejetés ; le document utilisateur actif et la sentinelle de chemin privé restent intacts.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

- [ ] **[T08 / #1057](https://github.com/mysteropodes/nemo/issues/1057) — Exposer l’inspecteur de diagnostic partagé dans l’interface et le MCP**

  Responsable **Cyrill/D2** · compétence `diagnostics` · `sonnet` / **medium**. Prédécesseurs : [T05 / #1054](https://github.com/mysteropodes/nemo/issues/1054), [T07 / #1056](https://github.com/mysteropodes/nemo/issues/1056), [P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009).

  Périmètre : `nouvelle liaison de panneau Diagnostics`; `descripteur de fonctionnalité de diagnostic`; `tests de diagnostic interface/MCP`.

  1. Un panneau mince liste les opérations récentes corrélées, l’état sélectionné et les liens de rapports via l’API de diagnostic.
  2. L’inspection et l’export MCP utilisent les mêmes gestionnaires et schémas ; les détails à la demande évitent de déverser tout le document dans le contexte.
  3. Une véritable interaction du panneau et une requête MCP observent la même trace synthétique ; l’inspection est en lecture seule et les limites de buffer sont respectées.

  Limite : Préserver les défauts connus ; aucune réparation sans rapport ni ajout de fonctionnalité.

### D — premières extractions circonscrites par famille, puis toute la file du recensement

| Ilya — étapes sous sa responsabilité | Cyrill — étapes sous sa responsabilité |
|---|---|
| [P20 / #1022](https://github.com/mysteropodes/nemo/issues/1022) · **D1** · Extraire la sérialisation des métadonnées de dossiers | [P22 / #1024](https://github.com/mysteropodes/nemo/issues/1024) · **D1** · Extraire les fonctions de conversion d’horloge des expressions |
| [H02 / #1059](https://github.com/mysteropodes/nemo/issues/1059) · **D1** · Cartographier la responsabilité de capture et restauration de l’historique limité à une image | [A01 / #1061](https://github.com/mysteropodes/nemo/issues/1061) · **D1** · Extraire les tirages aléatoires d’expressions à graine déterminée |
| [P21 / #1023](https://github.com/mysteropodes/nemo/issues/1023) · **D1** · Extraire l’entrée d’historique caractérisée limitée à une image | [P28 / #1030](https://github.com/mysteropodes/nemo/issues/1030) · **D1** · Extraire l’analyseur textuel de sondage FFmpeg |
| [P23 / #1025](https://github.com/mysteropodes/nemo/issues/1025) · **D2** · Extraire le solveur pur d’affectation hongrois | [P29 / #1031](https://github.com/mysteropodes/nemo/issues/1031) · **D1** · Extraire l’adaptateur natif d’écriture de projet |
| [P24 / #1026](https://github.com/mysteropodes/nemo/issues/1026) · **D1** · Extraire le noyau de transition d’image de lecture | [P30 / #1032](https://github.com/mysteropodes/nemo/issues/1032) · **D2** · Extraire les données d’affectation des raccourcis et leur persistance |
| [H03 / #1060](https://github.com/mysteropodes/nemo/issues/1060) · **D2** · Cartographier un geste de rotation de sélection et sa frontière d’annulation | [P31 / #1033](https://github.com/mysteropodes/nemo/issues/1033) · **D2** · Enregistrer le cycle de vie existant du timelapse Labs |
| [P25 / #1027](https://github.com/mysteropodes/nemo/issues/1027) · **D2** · Extraire le geste de rotation de sélection caractérisé | — |
| [P26 / #1028](https://github.com/mysteropodes/nemo/issues/1028) · **D2** · Extraire le suivi LRU des images et la politique d’éviction | — |
| [P27 / #1029](https://github.com/mysteropodes/nemo/issues/1029) · **D2** · Extraire la passe Rust existante de luminosité/contraste | — |

- [ ] **[P20 / #1022](https://github.com/mysteropodes/nemo/issues/1022) — Extraire la sérialisation des métadonnées de dossiers**

  Responsable **Ilya/D1** · compétence `extraction` · `gpt-6-sol` / **medium**. Prédécesseurs : [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Périmètre : `src/js/timeline.js: SM.exportJSON / SM.importJSON champs de dossier`; `src/js/tweens.js: capture/restauration de dossier`; `nouveau src/js/domain/document/folder-codec.js`.

  1. Un jeu indépendant avec deux dossiers, noms, indicateurs collapsed et appartenances effectue un aller-retour via le véritable export/import avec ID et métadonnées exacts.
  2. Les champs de dossier absents des anciens formats conservent les valeurs par défaut actuelles ; les métadonnées imbriquées inconnues suivent une politique de préservation explicitement consignée, sans redéfinir le format du projet.
  3. Les opérations existantes d’annulation de renommage/regroupement de dossiers et de sauvegarde/réouverture utilisent la même sémantique de copie du codec ; aucun changement des fonctionnalités de dossiers/groupes liés ni nouveau propriétaire des écritures persistantes.

  Limite : Périmètre : `SM.exportJSON` et `SM.importJSON` dans `src/js/timeline.js`, uniquement pour `layerFolders` et `folderId` par calque (`2117`, `2170`, `2458`, `2562`) ; capture/restauration des métadonnées de dossiers dans `tweens.js` ; nouveau `src/js/domain/document/folder-codec.js`. Ilya D1 suggéré. Réserver les deux monolithes ; aucune modification concurrente P21/P23/P24/P30. L’analyseur existant `project-document.js` est déjà extrait ; ne pas le recréer.

- [ ] **[H02 / #1059](https://github.com/mysteropodes/nemo/issues/1059) — Cartographier la responsabilité de capture et restauration de l’historique limité à une image**

  Responsable **Ilya/D1** · compétence `architecture` · `gpt-6-sol` / **high**. Prédécesseurs : [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036).

  Périmètre : `src/js/tweens.js: _cloneStrokesForUndo / pushUndoActiveFrame / undo / redo (en lecture seule)`.

  1. Suivre les appelants de Fill et prouver l’ensemble images/calques touché ; identifier tous les hooks de cache/interface/sélection et le schéma d’entrée `{frame,layers:[{strokes,isKeyframe,isInterpolated}]}`.
  2. Caractériser capture→undo→redo sur deux calques à une même image et documenter le comportement actuel sur image/contexte différents. L’historique des calques entiers possède des gardes de contexte ; la branche limitée à une image ne les partage pas. Consigner tout échec existant sans le réparer silencieusement.
  3. Spécifier les interfaces exactes de capture/application et la politique de clonage des champs lourds, ainsi qu’un test indépendant d’une exception stringify restaurant les champs lourds en direct. Exclure la refonte des instantanés de calques entiers.

  Limite : Cartographie circonscrite uniquement. Produire dans l’issue le contrat exact et le dossier d’implémentation ; ne pas réparer le comportement connu de contexte/annulation ni lancer une refonte générale.

- [ ] **[P21 / #1023](https://github.com/mysteropodes/nemo/issues/1023) — Extraire l’entrée d’historique caractérisée limitée à une image**

  Responsable **Ilya/D1** · compétence `extraction` · `gpt-6-sol` / **medium**. Prédécesseurs : [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036), [P20 / #1022](https://github.com/mysteropodes/nemo/issues/1022), [P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008), [H02 / #1059](https://github.com/mysteropodes/nemo/issues/1059).

  Périmètre : `src/js/tweens.js section d’annulation`; `nouveau module application/history`; `tests de contrat d’historique`.

  1. Un type identifié d’entrée d’historique possède un cycle de vie explicite apply/revert et des tests indépendants multidocuments/contextes.
  2. Interface/API/MCP partagent la responsabilité de l’annulation et exactement une entrée par transaction déclarée.
  3. Retirer l’ancien propriétaire de capture/application des entrées d’image ; préserver explicitement les limites caractérisées dans H02 et garder l’historique de calques entiers hors de cette tâche.

  Limite : Ne pas remplacer tout le système d’annulation en une tâche. Extraire uniquement l’entrée limitée à une image caractérisée dans H02 ; P03 suit séparément les responsabilités d’historique restantes.

- [ ] **[P22 / #1024](https://github.com/mysteropodes/nemo/issues/1024) — Extraire les fonctions de conversion d’horloge des expressions**

  Responsable **Cyrill/D1** · compétence `extraction` · `sonnet` / **medium**. Prédécesseurs : [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Périmètre : `src/js/motion.js: exprStepTime / exprToFrames / exprToSeconds`; `nouveau src/js/domain/animation/expression-time.js`.

  1. Des exemples fixes couvrent la conversion images/secondes, les arguments omis, un fps nul et des pas invalides/non positifs, avec le comportement actuel.
  2. `stepTime` modifie l’évaluation ultérieure dépendante du contexte dans la même expression ; les valeurs d’argument simples `time`/`frame` conservent leur comportement documenté sans quantification.
  3. L’évaluation réelle des expressions appelle les fonctions extraites ; aucun accès à `_ectx`/window/state dans le module domaine et les limites passent.

  Limite : Périmètre : `motion.js:1712–1732` : `exprStepTime`, `exprToFrames`, `exprToSeconds` ; nouveau `src/js/domain/animation/expression-time.js`, évaluateur d’expressions existant comme appelant. Entrées explicites de contexte/fps/coercition numérique. Cyrill D1 suggéré ; créneau exclusif de motion.js.

- [ ] **[A01 / #1061](https://github.com/mysteropodes/nemo/issues/1061) — Extraire les tirages aléatoires d’expressions à graine déterminée**

  Responsable **Cyrill/D1** · compétence `extraction` · `sonnet` / **medium**. Prédécesseurs : [P22 / #1024](https://github.com/mysteropodes/nemo/issues/1024).

  Périmètre : `src/js/motion.js: exprSeed / _rand01 / _gauss01 / _randomWith / exprRandom*`; `nouveau module domaine d’aléatoire d’animation`.

  1. Des vecteurs indépendants fixes de graine/compteur/image préservent tirages variables ou fixes et répétabilité ; les graines de propriétés différentes restent distinctes.
  2. Les min/max scalaires et tableaux, la consommation des paires gaussiennes et les incréments de compteur correspondent à la référence capturée ; aucune amélioration de l’algorithme aléatoire.
  3. Le moteur d’expressions existant appelle le nouveau module ; les tests déterministes ne requièrent ni DOM ni état global, et les contrôles de taille/dépendances passent.

  Limite : Exclure noise/wiggle et les améliorations d’algorithme. P22 doit d’abord libérer le créneau de l’ensemble du fichier motion.js.

- [ ] **[P23 / #1025](https://github.com/mysteropodes/nemo/issues/1025) — Extraire le solveur pur d’affectation hongrois**

  Responsable **Ilya/D2** · compétence `extraction` · `gpt-6-luna` / **medium**. Prédécesseurs : [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Périmètre : `src/js/tweens.js: hungarian(cost)`; `nouveau src/js/domain/tween/assignment.js`.

  1. Les matrices vides, 1×1 et petites matrices carrées vérifiées indépendamment par force brute renvoient des affectations de coût minimal.
  2. Des matrices fixes avec égalités préservent l’ordre actuel de départage ; les matrices d’entrée restent inchangées.
  3. La mise en correspondance de production invoque le module et les jeux fixes de correspondance de tween restent conformes à la référence ; aucune nouvelle heuristique ni affirmation de parité Rust.

  Limite : Périmètre : `tweens.js:415–445`, `hungarian(cost)`, appelé par `autoMatchJS` existant et la mise en correspondance relationnelle. Nouveau `src/js/domain/tween/assignment.js`. Cyrill D1 ou Ilya D2 suggéré ; créneau exclusif de tweens.js. Tout le moteur `autoMatchJS` ne constitue pas une petite tâche pure : l’extraction des caractéristiques utilise Paper et des dépendances de raffinement. Ne pas nommer ce dossier « extraction complète du moteur de correspondance de tween ».

- [ ] **[P24 / #1026](https://github.com/mysteropodes/nemo/issues/1026) — Extraire le noyau de transition d’image de lecture**

  Responsable **Ilya/D1** · compétence `extraction` · `gpt-6-sol` / **medium**. Prédécesseurs : [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Périmètre : `src/js/timeline.js: advancePlayFrame`; `nouveau noyau domaine de pas de lecture et enveloppe existante`.

  1. Des cas indépendants couvrent la limite en lecture avant, l’arrêt sans boucle, le bouclage normal, l’inversion ping-pong et une zone de travail d’une seule image.
  2. L’enveloppe préserve la modification de direction et exactement les mêmes appels de boucle audio ; le véritable `startPlay` appelle toujours le noyau extrait.
  3. Une lecture navigateur à horloge fixe atteint les images attendues ; aucun changement de saut d’images, de pré-calcul automatique ou de stockage du fps, et les limites du domaine ainsi que l’interdiction des globales passent.

  Limite : Périmètre : `timeline.js:29`, `advancePlayFrame(cur)` ; une nouvelle fonction domaine de pas de lecture renvoie image suivante/direction/événement de boucle, l’enveloppe applique l’état et appelle `SMAudio.onLoop`. L’accumulateur rAF et le pré-calcul automatique de `startPlay` restent hors de cette tâche. Ilya D1 suggéré.

- [ ] **[H03 / #1060](https://github.com/mysteropodes/nemo/issues/1060) — Cartographier un geste de rotation de sélection et sa frontière d’annulation**

  Responsable **Ilya/D2** · compétence `architecture` · `gpt-6-luna` / **high**. Prédécesseurs : [C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039).

  Périmètre : `src/js/tools.js: rotate onMouseDown / onMouseDrag / onMouseUp / rotateCenterSegments (en lecture seule)`.

  1. Cartographier les objets sélectionnés stables et chaque champ associé modifié ordinaire/pinceau vectoriel/dégradé, y compris pivot initial et espace de coordonnées.
  2. Capturer begin→plusieurs drags→mouse-up→undo sur un jeu ordinaire et un jeu de pinceau vectoriel ; caractériser Escape/cancel et le remplacement du document comme comportement actuel ou échec connu.
  3. Définir les interfaces begin/update/commit/cancel avec un seul propriétaire de l’historique et les chemins exacts du nouveau module et des appelants. Le code actuel empile l’historique au mouse-down et modifie Paper en direct pendant le drag ; une enveloppe générique seule ne prouve pas l’annulation.

  Limite : Cartographie circonscrite uniquement. Produire dans l’issue le contrat exact et le dossier d’implémentation ; ne pas réparer le comportement connu de contexte/annulation ni lancer une refonte générale.

- [ ] **[P25 / #1027](https://github.com/mysteropodes/nemo/issues/1027) — Extraire le geste de rotation de sélection caractérisé**

  Responsable **Ilya/D2** · compétence `extraction` · `gpt-6-luna` / **medium**. Prédécesseurs : [C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006), [P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008), [H03 / #1060](https://github.com/mysteropodes/nemo/issues/1060).

  Périmètre : `src/js/tools.js section de transformation`; `src/js/select-bridge.js liaison concernée`; `nouveau module applicatif de geste de sélection`.

  1. begin/update/commit/cancel agit sur des ID sélectionnés stables, une convention de coordonnées et un instantané initial.
  2. Commit possède une seule entrée d’annulation. Lorsqu’elle est prise en charge, l’annulation restaure l’état initial ; sinon, préserver l’échec caractérisé dans H03 et exposer sa limite précise. Les tests distinguent les défauts existants d’annulation/contexte des régressions d’extraction.
  3. Faire passer uniquement le geste de rotation cartographié par H03 par le nouveau cycle de vie et retirer son propriétaire d’écritures en double ; la mise à l’échelle et la rotation en édition de nœuds restent inchangées.

  Limite : Ne pas réparer les outils de sélection sans rapport ; ne pas assigner tools.js simultanément à un autre rédacteur.

- [ ] **[P26 / #1028](https://github.com/mysteropodes/nemo/issues/1028) — Extraire le suivi LRU des images et la politique d’éviction**

  Responsable **Ilya/D2** · compétence `extraction` · `gpt-6-luna` / **medium**. Prédécesseurs : [C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Périmètre : `src/js/engine-bridge.js: _noteImageRegistered / _touchImage / _imgTotalBytes / enforceImageBudget`; `nouveau module applicatif de rendu image-budget`.

  1. Des séquences indépendantes prouvent que les images du build courant sont protégées, que les images inactives les moins récemment utilisées sont retirées en premier et que totaux d’octets/compteurs correspondent aux dimensions.
  2. Un échec de `engine.retire_images` laisse intacts le suivi et `registeredImageIds` ; une réussite efface les deux afin que l’utilisation suivante recharge les données.
  3. La véritable construction de scène utilise le cycle begin/use/end et l’API existante de budget/statistiques ; aucun nouveau budget par défaut ni optimisation des copies CPU/GPU, et le rendu d’image fixe survit à l’éviction/rechargement.

  Limite : Périmètre : `engine-bridge.js:627–677` : `_imgBytes`, `_imgLastUsed`, `_imgUsedThisBuild`, état de tick/budget/éviction et `_noteImageRegistered`, `_touchImage`, `_imgTotalBytes`, `enforceImageBudget` ; appels de cycle de vie au début/à la fin de scène (~807/~2653), statistiques/budget publics à 4583–4598. Nouveau module applicatif/render image-budget ; adaptateurs de chargement GPU et stockage des chemins conservés inchangés. Ilya D2 suggéré.

- [ ] **[P27 / #1029](https://github.com/mysteropodes/nemo/issues/1029) — Extraire la passe Rust existante de luminosité/contraste**

  Responsable **Ilya/D2** · compétence `extraction` · `gpt-6-luna` / **medium**. Prédécesseurs : [C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038), [P12 / #1014](https://github.com/mysteropodes/nemo/issues/1014).

  Périmètre : `geometry-wasm/src/engine.rs: create_color_adjust_pipeline / color_adjust_pass`; `nouveau geometry-wasm/src/engine/color_adjust.rs`.

  1. Le compositeur de production crée et invoque la passe extraite avec des entrées explicites device/queue/layout/source/target/uniform et des octets de shader inchangés.
  2. Les jeux fixes de rendu luminosité/contraste et paramètres neutres concordent avant/après dans la tolérance existante, y compris pour le comportement alpha.
  3. Compilation Rust, visibilité des modules et contrôles de taille/dépendances passent ; aucune migration d’espace couleur/précision ni autre passe d’effet dans cette tâche.

  Limite : Périmètre : `geometry-wasm/src/engine.rs:1665–1802`, `create_color_adjust_pipeline` et `color_adjust_pass` ; nouveau `geometry-wasm/src/engine/color_adjust.rs` ; `color_adjust.wgsl` existant inchangé. Les sites de création/utilisation dans `create_engine`/`run_one_effect` ne concernent que le raccordement. Ilya D2 suggéré après accord sur la règle des modules Rust.

- [ ] **[P28 / #1030](https://github.com/mysteropodes/nemo/issues/1030) — Extraire l’analyseur textuel de sondage FFmpeg**

  Responsable **Cyrill/D1** · compétence `extraction` · `sonnet` / **medium**. Prédécesseurs : [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013).

  Périmètre : `src-tauri/src/video_decode.rs: parse_probe / parse_hms`; `nouveau src-tauri/src/media_probe.rs`.

  1. Les textes capturés existants H264/HEVC/VP9/ProRes/dimensions impaires/25fps donnent les mêmes valeurs width,height,fps,duration,codec.
  2. Les cas audio seul et Duration/vidéo/dimensions/fps malformés préservent les chaînes d’erreur existantes, sans lancer de sous-processus.
  3. `open_session_core` invoque le nouvel analyseur ; les tests Cargo ciblés et de frontières du code passent. Aucune installation FFmpeg ni réparation de codec ou de synchronisation de recherche.

  Limite : Périmètre : `video_decode.rs:710–761`, `parse_probe` et `parse_hms` ; sept tests `parse_probe_*` existants autour de 1459–1512 ; nouveau `src-tauri/src/media_probe.rs`, `open_session_core` reste l’appelant. Cyrill D1 suggéré.

- [ ] **[P29 / #1031](https://github.com/mysteropodes/nemo/issues/1031) — Extraire l’adaptateur natif d’écriture de projet**

  Responsable **Cyrill/D1** · compétence `extraction` · `sonnet` / **medium**. Prédécesseurs : [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036), [P20 / #1022](https://github.com/mysteropodes/nemo/issues/1022).

  Périmètre : `src/js/project.js: writeProjectTo bloc de système de fichiers`; `nouveau src/js/adapters/project-native-save.js`.

  1. La réussite écrit `<path>.saving`, le renomme et ne retourne qu’après achèvement ; l’appelant marque ensuite le projet sauvegardé avec des octets JSON inchangés.
  2. Un échec de renommage/écriture suit l’ordre de repli **actuel** suppression du temporaire/écriture directe et propage l’échec final. Ce repli constitue une limite préexistante de résistance aux interruptions ; son extraction ne certifie pas une sauvegarde atomique.
  3. Le chemin de sauvegarde navigateur reste inchangé ; sauvegarde et enregistrer sous de l’interface native invoquent l’adaptateur, les tests injectent des échecs fs et le contrôle normal de modules passe. Aucune extraction d’ouverture/lecture/synchronisation cloud.

  Limite : Périmètre : bloc de système de fichiers dans `project.js:120–139` `writeProjectTo(path)` ; nouveau `src/js/adapters/project-native-save.js` recevant `path`, du JSON déjà sérialisé et une interface fs. Les mises à jour existantes d’état projet/récents/modifications non sauvegardées/sauvegarde automatique restent dans l’appelant. Cyrill D1 suggéré ; ne peut pas chevaucher le raccordement de projet de P20.

- [ ] **[P30 / #1032](https://github.com/mysteropodes/nemo/issues/1032) — Extraire les données d’affectation des raccourcis et leur persistance**

  Responsable **Cyrill/D2** · compétence `extraction` · `sonnet` / **medium**. Prédécesseurs : [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041), [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007).

  Périmètre : `src/js/timeline.js: tables de raccourcis / substitutions / setShortcutKey`; `nouveau registre de raccourcis de préférences`.

  1. Les valeurs par défaut, substitutions stockées et réinitialisation survivent à un nouveau contexte de préférences ; la sortie action-vers-touche correspond à la table de référence fixe.
  2. Conflits, touches réservées et JSON stocké malformé préservent le comportement actuel ; un échec de stockage ne modifie pas silencieusement la sémantique documentée du résultat.
  3. Le véritable routage clavier et la réaffectation dans la modale utilisent le registre, avec une requête/commande de préférence enregistrée sur le même état ; aucune refonte de tout le routage des événements clavier.

  Limite : Périmètre : `timeline.js:7457–7587` : `TOOL_SHORTCUTS`, `COMMAND_SHORTCUTS`, `READONLY_SHORTCUTS`, `shortcutOverrides`, `shortcutDefFor`, `shortcutKeyFor`, `shortcutClashFor`, `setShortcutKey` ; nouveau registre de raccourcis de préférences avec interfaces localStorage et actualisation des badges. Garder `runCommandShortcut`, `runToolShortcut` et le rendu DOM de la modale comme appelants. Cyrill D2 suggéré ; créneau exclusif de timeline.js.

- [ ] **[P31 / #1033](https://github.com/mysteropodes/nemo/issues/1033) — Enregistrer le cycle de vie existant du timelapse Labs**

  Responsable **Cyrill/D2** · compétence `capabilities` · `sonnet` / **medium**. Prédécesseurs : [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041), [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007).

  Périmètre : `src/js/labs/timelapse.js: timelapseStart / timelapseStop`; `nouveau descripteur de fonctionnalité timelapse`.

  1. Le descripteur nomme disponibilité de start/stop/query, entrée fps et forme du résultat, indicateur `nemo-labs-timelapse`, prise en charge requise de canvas/MediaRecorder ; les états absent/non chargé/désactivé ont des motifs explicites.
  2. Lorsque le module existant est chargé/activé dans un jeu isolé, les gestionnaires enregistrés appellent le même cycle start/stop ; les start/stop répétés et cas sans enregistreur correspondent au comportement actuel.
  3. La désactivation termine le minuteur/enregistrement détenu selon le contrat actuellement observé ; l’absence de MediaRecorder reste une indisponibilité. Tout défaut d’enregistrement/nettoyage est consigné plutôt que réparé, et aucune fonctionnalité produit n’est nouvellement activée.

  Limite : Périmètre : `src/js/labs/timelapse.js`, `SMLabs.timelapseStart`, `SMLabs.timelapseStop`, `SMLabs.register('timelapse',...)` ; nouveau descripteur de fonctionnalité et tests de contrat. Ne pas l’ajouter aux scripts de démarrage uniquement pour revendiquer une couverture. Cyrill D2 suggéré après le contrat du registre.

### E — intégration et clôture

| Ilya — étapes sous sa responsabilité | Cyrill — étapes sous sa responsabilité |
|---|---|
| [P33 / #1035](https://github.com/mysteropodes/nemo/issues/1035) · **O** · Accepter le recensement figé terminé au regard des preuves intégrées | [P32 / #1034](https://github.com/mysteropodes/nemo/issues/1034) · **O** · Vérifier une exécution de validation et de prise en main agent depuis un clone propre |

- [ ] **[P32 / #1034](https://github.com/mysteropodes/nemo/issues/1034) — Vérifier une exécution de validation et de prise en main agent depuis un clone propre**

  Responsable **Cyrill/O** · compétence `validation` · `opus` / **high**. Prédécesseurs : [P09 / #1011](https://github.com/mysteropodes/nemo/issues/1011), [P10 / #1012](https://github.com/mysteropodes/nemo/issues/1012), [P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013), [P16 / #1018](https://github.com/mysteropodes/nemo/issues/1018), [F03 / #1049](https://github.com/mysteropodes/nemo/issues/1049), [T01 / #1050](https://github.com/mysteropodes/nemo/issues/1050), [T03 / #1052](https://github.com/mysteropodes/nemo/issues/1052), [T04 / #1053](https://github.com/mysteropodes/nemo/issues/1053), [N21 / #1353](https://github.com/mysteropodes/nemo/issues/1353).

  Périmètre : `documentation/commandes d’entrée existantes`; `comptes rendus dédiés d’acceptation depuis un clone propre`.

  1. De nouvelles sessions Codex/Claude trouvent le document d’exécution unique et le contrat de module, exécutent la validation locale nommée et identifient l’installation Nemo visée.
  2. Les jobs requis échoués/manquants ne peuvent pas devenir une réussite globale ; toutes les limites de plateforme et tous les échecs de référence connus restent visibles.
  3. Un nouvel agent trouve une tâche assignée Ready et les artefacts exacts de rapport sans compétence personnelle non suivie ni second document de transmission.

  Limite : Aucune activation d’Actions hébergées ; l’acceptation du code et celle de l’installation restent des conclusions distinctes.

- [ ] **[P33 / #1035](https://github.com/mysteropodes/nemo/issues/1035) — Accepter le recensement figé terminé au regard des preuves intégrées**

  Responsable **Ilya/O** · compétence `orchestration` · `gpt-6-astra` / **high**. Prédécesseurs : [P03 / #1005](https://github.com/mysteropodes/nemo/issues/1005), [P08 / #1010](https://github.com/mysteropodes/nemo/issues/1010), [P32 / #1034](https://github.com/mysteropodes/nemo/issues/1034). Toutes les tâches N06–N21 ci-dessus doivent également être acceptées ; aucun pourcentage de parent ne remplace leurs reçus.

  Périmètre : `recensement figé de remédiation et tous les comptes rendus liés des tâches terminées (en lecture seule)`; `contrôles finaux d’exhaustivité du code et des capacités`.

  1. Chaque responsabilité recensée possède une implémentation fusionnée ou une décision hors code relue ; toutes les tâches d’extraction créées dynamiquement sont terminées.
  2. Aucun ancien propriétaire des écritures, façade obsolète ou exception de code hérité expirée ne subsiste ; le retrait relève de la tâche de migration propriétaire, jamais d’un travail caché dans cet audit.
  3. Les contrôles normaux de couverture, de frontières, d’enregistrement manquant et de schémas passent leurs contrôles négatifs ; les défauts produit connus restent séparés et acceptés par les humains.

  Limite : Il s’agit d’une tâche d’acceptation finale, pas d’un substitut aux migrations inachevées. Terminer uniquement P04-P32 ne prouve pas la disparition de tous les monolithes.

## 9. Acceptation finale — la ligne d’arrivée

- [x] N00–N05 possèdent des preuves d’acceptation exactes : installation du tableau et du plan, réconciliation de la file sur le main actuel, contrats figés d’autorité et de transition, faisabilité de l’évaluation headless, faisabilité du viewport natif et file bornée d’implémentation de production. Le pourcentage d’un parent de suivi ou un prototype de faisabilité ne vaut pas achèvement de production.
- [ ] Le moteur Rust natif est l’unique autorité d’écriture pour les chemins convenus de révision du document, commande/historique, évaluation/ordonnancement, médias/ressources/GPU, production du viewport et export. Tauri/JavaScript reste l’interface ; Paper.js et navigateur/WASM sont des adaptateurs de frontière aux capacités déclarées, pas des autorités concurrentes.
- [ ] Chaque famille de référence requise possède un oracle indépendant de parité à révision fixe et passe ses contrôles applicables de sauvegarde/chargement, annulation/rétablissement, sélection, animation, rendu, export, pont natif, navigateur réel et bureau installé sur le candidat final identifié. Un ancien writer peut avoir été retiré plus tôt sur la branche d’intégration, mais la famille reste en attente jusqu’à ces réussites. L’acceptation du paquet, l’acceptation navigateur et la parité du code source sont consignées séparément aux SHA exacts.
- [ ] Le dénominateur fixe des workflows de référence atteint 100 % de couverture acceptée sur les surfaces convenues, hormis les décisions précises relatives aux défauts déjà présents. Les anciens formats de projet s’importent sans perte silencieuse de contenu ; expressions et comportement public des scripts/plugins sont pris en compte. Une fonctionnalité requise temporairement indisponible ne peut pas être comptée Done.
- [ ] Le recensement figé du code et des consommateurs de P03 est complet, y compris les petites tâches inscrites ensuite. Aucun monolithe écrit manuellement ne reste caché derrière une exception héritée ; les fichiers légitimes générés/tiers/de données ont des décisions explicites.
- [ ] Chaque fonctionnalité possède une API publique cohérente, une seule autorité d’état, les contrats applicables de cycle de vie/ressources et un enregistrement de capacité. Les consommateurs existants interface/API/MCP utilisent la même implémentation ; les anciens propriétaires d’écritures et chemins de contournement sont retirés.
- [ ] Les contrôles pertinents unitaires, de non-régression, navigateur et natifs protègent le comportement migré. Les rapports de couverture et d’échec sont inspectables au SHA final du code. Les défauts connus sont une dette produit explicite, pas des prérequis de réparation ni des tests présentés à tort comme réussis.
- [ ] La validation locale normale impose les frontières adoptées, les profils de taille, la fraîcheur des schémas et l’exhaustivité des enregistrements. Chaque vérificateur possède un contrôle négatif pertinent qui échoue.
- [ ] Un nouvel agent peut ajouter une déclaration de fonctionnalité selon la convention documentée et l’exercer via le MCP Rust livré sans modifier un répartiteur central. Les deux véritables clients disposent de preuves identifiées d’acceptation de l’installation pour la tranche macOS prise en charge.
- [ ] Le débogage fournit une inspection corrélée bornée et un chemin reproductible de jeu d’essai isolé. Stdout du protocole, propriété des documents et données utilisateur restent intacts.
- [ ] Ilya et Cyrill acceptent le résultat fonctionnel et structurel complet et ses limites préexistantes explicites de produit/plateforme. Préserver un code/artefact opérationnel récupérable, réconcilier le diff final de l’intégration avec le `main` actuel et ne promouvoir que par PR normalement protégée et revue. Fermer les parents de suivi restants, réconcilier le tableau principal et le compte rendu central final, et libérer les responsabilités terminées de branche/worktree/exécution.

Aucune affirmation ci-dessus n’exige de réparer tous les bugs produit préexistants. Aucune extraction ouverte, aucun propriétaire d’écritures d’état manquant ni aucune preuve architecturale absente ne peut être renommé dette produit uniquement pour déclarer la remédiation terminée.

## 10. Buzz, rapports horaires et dossiers de démarrage des deux équipes

### Un seul espace visible de compte rendu

Utiliser le [journal partagé de progression des orchestrateurs, issue #1062](https://github.com/mysteropodes/nemo/issues/1062), accessible depuis [Hourly progress reports sur le tableau principal](https://github.com/users/mysteropodes/projects/2/views/10). Les deux humains et les futurs contributeurs lisent le même journal chronologique. C’est une issue de coordination, pas une tâche exécutable ; elle ne gonfle pas le nombre de tâches achevées. Les issues et PR élémentaires conservent les preuves détaillées ; le rapport central y renvoie. Aucun dossier de rapports, aucune branche de statut ni PR de point de contrôle n’est nécessaire.

- [ ] Au début de la session, chaque orchestrateur indique le responsable humain et l’équipe, les ID des tâches actives, le modèle/l’effort, les créneaux de rédaction/exécution alloués et le prochain point de contrôle. Lire le dernier rapport de l’autre équipe avant d’allouer des travaux susceptibles de se chevaucher.
- [ ] Pendant l’activité, publier un rapport par équipe chaque heure. Signaler aussi un blocage significatif, une transmission de responsabilité, un résultat prêt pour revue ou une fin de session sans attendre le minuteur. Utiliser les comptes rendus existants des délégués ; ne pas arrêter les agents occupés pour remplir un rapport.
- [ ] Inclure un état de session explicite `active`, `paused` ou `finished`. Un minuteur ne permet pas de déduire une autorisation de reprendre une session inactive. Les prises en charge conservées restent attribuées à leurs responsables pendant la pause ; le silence ne les libère pas.
- [ ] Avant de publier, lire les commentaires récents et réconcilier le marqueur de cette équipe/heure. Modifier un rapport déjà livré ou y renvoyer au lieu de le dupliquer. Après une écriture au résultat incertain, examiner l’issue réelle avant de réessayer.
- [ ] Ne mettre à jour les champs modifiés des tâches sur Project #2 qu’à partir de preuves récentes, puis les relire. Publier ensuite le lien du rapport dans la conversation Nemo Buzz désignée. Si la livraison Buzz est indisponible, conserver le rapport GitHub et consigner cette limite. Ne pas inventer d’autre destination ni envoyer de messages directs.

Copier ce format court de rapport ; omettre les détails inchangés et utiliser des liens :

```text
<!-- nemo-hourly:team=<Ilya-or-Cyrill>;hour=<YYYY-MM-DDTHH> -->
Heure UTC / équipe / session active-paused-finished :
O, D1, D2 : modèle-effort réels et ID des tâches actives :
SHA du code intégré ; SHA/PR des candidats indiqués séparément :
Accepté depuis le dernier rapport : résultat, SHA fusionné, issue/PR et preuves :
Progression en attente d’acceptation : résultat, issue/PR et contrôle restant :
Preuves : liens des contrôles/couverture/traces ; échecs, blocages ou contrôles non exécutés :
Blocages actuels et responsable ; prochaine action concrète par flux occupé :
Prises en charge de fichiers entiers/exécution conservées ou libérées ; push/revue en attente :
Recensement délimité : nombres acceptés / actifs / en revue / bloqués et changements :
Modifications du tableau relues ; quota/capacité si réellement connus :
Prochain point de contrôle ; livraison du lien de rapport dans Buzz :
```

Un rapport horaire consigne la progression ; il ne constitue pas une acceptation. Ne jamais marquer une fonctionnalité Done parce que son délégué a répondu, que ses tests ont réussi isolément ou que sa PR vient simplement d’être ouverte.

### Instructions d’espace de travail, compléments d’équipe et rappels

Conserver trois niveaux dont les objectifs sont explicites :

| Niveau | Contenu | Responsable de sa maintenance |
|---|---|---|
| Instructions partagées de l’espace Nemo | Définition de l’état de référence, invariants architecturaux, tableau/journal principaux, responsabilités, tests et règles de Done. | Propriétaire signataire actuel du Project ; tous les collaborateurs inscrits reçoivent la même valeur partagée. |
| Team Instructions | Nom du responsable humain, langue, réglages de modèle/effort autorisés, politique de quota, limites des délégués locaux et responsabilités de revue. | Chaque humain pour sa propre équipe. Un complément d’équipe n’affaiblit pas les règles partagées du projet. |
| Tâche et prompt planifié | Tâche/prise en charge actuelle, prochaine action immédiate, fréquence/destination du compte rendu et intention de pause/reprise. | L’orchestrateur conformément à l’instruction humaine actuelle. |

La vue Settings originale de `0.5.23-nemo.13` est en lecture seule : `NemoWorkspaceSettingsCard.tsx` affiche la politique et `project_preload.rs` incorpore `docs/NEMO_WORKSPACE_INSTRUCTIONS.md` lors de la compilation. Modifier ce texte intégré exige de reconstruire l’application/ACP ; modifier un rappel rédigé manuellement ne l’exige pas. Le parcours existant **Edit team → Team Instructions → Save changes** enregistre durablement un complément d’équipe. Dans cette version, les processus déjà lancés ont besoin d’un redémarrage sûr pour recevoir les instructions d’équipe modifiées ; la sauvegarde seule ne prouve pas leur prise en compte.

La mise à jour Buzz `0.5.23-nemo.14` autorisée séparément ajoute un éditeur persistant des instructions d’espace de travail utilisant les paramètres existants du Project signé, avec repli/réinitialisation sur les valeurs intégrées et révision sauvegardée visible. Seul le propriétaire signataire du Project modifie la valeur partagée ; les autres membres utilisent leurs propres Team Instructions pour les adaptations propres à leur équipe. Ne pas présenter comme partagée une valeur propre à un appareil ni élargir les accès pour faire fonctionner l’éditeur. Ouvrir **Settings → Agents → Nemo workspace → Review workspace instructions**. Le propriétaire modifie **Shared workspace instructions**, puis utilise **Save shared instructions**, **Reset to built-in** ou **Reload saved value**. En cas de conflit à la sauvegarde, le brouillon est conservé ; copier tout travail non sauvegardé, recharger la valeur actuellement enregistrée, réconcilier les changements puis réessayer. Dans cette version, les modifications partagées prennent effet entre les tours de l’agent ; une révision sauvegardée ne prouve pas qu’un agent inactif ou hors ligne l’a reçue. **Les modifications de Team Instructions exigent toujours le redémarrage de l’agent local concerné à un point de reprise sûr.** Il faut consigner les preuves de version publiée, d’installateur et de prise en compte à l’exécution avant d’affirmer que cette mise à jour est installée sur l’un ou l’autre Mac. Les changements plus larges du transport Buzz restent hors de la remédiation Nemo.

Utiliser ce complément partagé du programme dans le nouvel éditeur d’espace de travail, ou immédiatement comme instruction de tâche rédigée manuellement pendant l’installation de la mise à jour :

```text
La remédiation Nemo utilise engineering/remediation/EXECUTION_PLAN.en.md et sa copie française comme un seul plan opérationnel. L’état de référence désigne l’état exactement observé, y compris les défauts connus. Extraire des modules cohérents, préserver les consommateurs d’état/données/historique, ajouter une protection contre les régressions et des contrôles de taille/dépendances/schémas, et enregistrer les capacités détenues par les fonctionnalités via l’API applicative partagée et le MCP Rust livré. Ne pas élargir la tâche à des réparations produit sans rapport.

Tableau principal : https://github.com/users/mysteropodes/projects/2 . Project 8 est un instantané historique. Les prises en charge actives et les preuves appartiennent à chaque issue/PR élémentaire. Les deux orchestrateurs publient chaque heure pendant l’activité, ainsi qu’aux blocages/transmissions/fins de session, dans https://github.com/mysteropodes/nemo/issues/1062 ; relier le rapport depuis la conversation Nemo Buzz désignée. Un responsable humain et un rédacteur par tâche. Utiliser au maximum un orchestrateur et deux délégués locaux par humain, préserver les prises en charge et quotas existants, et réutiliser une branche/PR d’un sprint à l’autre. Aucune branche ni PR de compte rendu.

Lire avant d’écrire et relire les modifications du tableau/des rapports. Une session inactive reste inactive. Les minuteurs ne lancent pas de workers, n’interrompent pas les agents occupés, n’annulent ni ne réassignent les prises en charge, ne réparent pas le produit et ne certifient pas Done. Chaque équipe de collaborateurs relit, valide localement, fusionne et clôture ses propres tâches selon la section 7 ; les PR d’auteurs externes exigent l’approbation d’un collaborateur. Un accusé du bot de politique n’est pas une acceptation technique. Définir Validation owner sur l’orchestrateur de l’équipe responsable ; coordonner les décisions partagées réelles sans approbation interéquipes systématique. Si un A2A distant est explicitement demandé, préserver des comptes rendus distincts stored/processed/accepted/progress/terminal et réconcilier les effets incertains avant de réessayer. La disponibilité d’un outil ne confère pas de nouvelle autorité.
```

### Configuration par humain et contrôles d’activation

Préparer les déclencheurs en état **paused**. Ne les activer qu’au démarrage effectif du travail d’exécution, et les suspendre dès que l’équipe s’arrête. La planification seule n’active pas les rappels de coordination. Utiliser **un seul déclencheur horaire par équipe**, ciblant son orchestrateur. Ne pas installer à la fois un heartbeat Codex et un workflow horaire Buzz pour la même équipe. Le déclencheur retenu est consigné dans #1062 pour qu’un autre orchestrateur ne puisse pas le dupliquer par accident.

| Ilya | Cyrill |
|---|---|
| Utiliser la liste anglaise. O=`gpt-6-astra` high ; D1=`gpt-6-sol` medium ; D2=`gpt-6-luna` medium. | Utiliser la liste française. O=`opus` high aux jalons ; commencer avec un délégué `sonnet` medium et n’activer le second qu’avec du travail indépendant et un quota suffisant. |
| Le heartbeat Codex **Nemo — Ilya hourly coordination** (`nemo-ilya-hourly-coordination`) est préparé et **PAUSED**. Au démarrage effectif de l’exécution, cibler la tâche réelle de l’orchestrateur et l’activer ; le garder en pause pendant la planification et l’inactivité. | Préparer le rappel horaire rédigé manuellement en état **paused** sur votre propre compte, ciblant votre orchestrateur réel ; ne l’activer qu’au démarrage de l’exécution. Ce dossier ne prétend pas qu’un minuteur a été créé ou livré sur votre Mac. |
| Consigner la destination du heartbeat et son premier compte rendu réellement livré dans #1062. En cas de passage à un workflow Buzz, suspendre d’abord ce heartbeat. | Buzz expose l’option de workflow **Schedule → Every hour**. Choisir la conversation Nemo désignée comme destination du message et s’adresser explicitement à l’orchestrateur réel. Vérifier une livraison réelle avant de déclarer la planification opérationnelle. |
| Suspendre le déclencheur en fin de session ; publier les prises en charge conservées/libérées. Reprendre uniquement avec un nouvel avis de début de session. | Suspendre le déclencheur en fin de session ou lors d’une transmission liée au quota. Un compte rendu manquant est un problème de configuration à signaler, pas une autorisation de lancer un agent de remplacement. |

- [ ] Installer uniquement la version Mac identifiée depuis le lien de téléchargement partagé par Ilya, vérifier sa version/empreinte par rapport au compte rendu de publication et conserver une application antérieure récupérable. Le lien doit identifier le véritable installateur testé ; un texte provisoire n’est pas un téléchargement.
- [ ] Ouvrir les paramètres de l’espace de travail et vérifier la source/révision partagée. Si vous n’êtes pas le propriétaire du Project signé, conserver la politique partagée et modifier uniquement votre complément d’équipe.
- [ ] Dans les instructions d’équipe, indiquer votre nom, la langue des rapports, les valeurs par défaut du modèle/de l’effort et la politique de quota d’après le tableau. Sauvegarder le complément d’équipe, préserver le point de reprise de la tâche en cours et redémarrer l’agent local concerné lorsque cela peut se faire sans risque. Vérifier le nouveau complément et la révision partagée dans son prochain accusé de démarrage. Les modifications de l’espace de travail partagé seules utilisent l’actualisation au tour suivant ; elles n’exigent pas de redémarrer une tâche occupée.
- [ ] Dans un bref accusé de prise en main, l’orchestrateur identifie son responsable humain, le tableau principal, le journal, les tâches actuelles, les modèles réels et la révision effective des instructions. Cela confirme leur prise en compte, pas seulement leur stockage.
- [ ] Enregistrer le rappel en état **paused** avec le prompt ci-dessous, votre propre nom et la destination réelle. Confirmer qu’aucun minuteur en double n’existe déjà ; consigner le déclencheur/la destination dans le journal partagé.
- [ ] Au démarrage effectif de l’exécution, activer le déclencheur ; observer une livraison planifiée réelle et le lien du rapport. Consigner les heures d’envoi/réception et l’URL du rapport obtenu. La configuration du planificateur, le stockage dans le relais et l’exécution réelle par l’orchestrateur sont des observations distinctes.
- [ ] En fin de session, publier une transmission et suspendre le déclencheur. À la session suivante, lire les derniers rapports des deux équipes, réconcilier les prises en charge puis le réactiver.

Rappel horaire prêt à copier ; remplacer `<human>` par Ilya ou Cyrill. Cette copie française fournit la formulation destinée à Cyrill :

```text
Coordonner la session active de remédiation Nemo de <human>. Ne lire que la dernière transmission et les rapports récents dans https://github.com/mysteropodes/nemo/issues/1062 ainsi que les tâches actives de cet humain sur le tableau principal https://github.com/users/mysteropodes/projects/2 . Project 8 est un instantané historique. Si l’humain n’a pas de session active, ou si elle est explicitement en pause/terminée, rester silencieux et ne pas commencer de travail.

Pendant l’activité, publier un rapport horaire concis dans l’issue 1062 : heure UTC et état de session ; humain/flux et modèle-effort réels si connus ; ID des tâches actives ; SHA/PR vérifiés des candidats ; résultats depuis le dernier rapport ; liens des preuves de tests et de couverture, et limites ; blocages ; prochaines actions concrètes ; prises en charge conservées/libérées ; quota/capacité si connus. Lire d’abord les commentaires récents et utiliser le marqueur nemo-hourly pour cet humain et cette heure UTC afin d’éviter les doublons. Réconcilier un résultat de publication incertain avant de réessayer. Utiliser les comptes rendus disponibles des délégués ; ne pas interrompre les agents occupés.

Ne mettre à jour que les champs des tâches actuelles de cet humain sur Project 2 lorsque des preuves récentes le justifient, puis les relire. Relier le rapport une seule fois dans la conversation Nemo Buzz désignée ; si la livraison est indisponible, consigner la limite sans inventer de destination ni envoyer de message direct. Préserver l’état de référence exact et les responsabilités actuelles. Ne pas lancer de workers, annuler/réassigner les prises en charge, modifier le produit, fusionner, reconstruire, publier des versions ni créer de branches de compte rendu. Un rapport ne constitue pas une clôture. Informer le responsable humain de la publication du rapport horaire demandé pendant l’activité et des blocages significatifs. Arrêter les rapports réguliers après un avis de fin de session jusqu’au début explicite d’une nouvelle session.
```

### Installateur Buzz pour Ilya et Cyrill

Télécharger **Buzz `0.5.23-nemo.14` pour les Mac Apple Silicon** depuis le [dossier v.14 partagé](https://drive.google.com/drive/folders/15g9EHoc5FlSSZPH7ek2x8dgBLfzeETOm). Le dossier utilise le réglage **Anyone with the link → Viewer**. Il contient `Buzz_0.5.23-nemo.14_aarch64.dmg`, `SHA256.txt` et `RELEASE-NOTES.txt`.

L’installateur fait **191361056 octets** et provient du code `6dc631213f41010a3aa5ae8b92463f748f96d076`. Son SHA-256 est `3751a65fe876b3af7674a25f9150d0162e7e6c08984a9ec404113686a0eda9a2`. Exécuter `shasum -a 256 Buzz_0.5.23-nemo.14_aarch64.dmg` dans le dossier de téléchargement et comparer le résultat complet avant de l’ouvrir.

- [ ] Terminer le travail actif des agents ou enregistrer un point de reprise, conserver une application antérieure récupérable, installer cette version et vérifier la version affichée. Redémarrer les anciens processus d’agents à ce point de reprise sûr pour qu’ils utilisent le harnais mis à jour ; l’installation de l’application seule ne met pas à jour un processus déjà en cours d’exécution.
- [ ] Suivre la configuration de l’espace de travail et des équipes ci-dessus. Confirmer **Source**, **Saved revision** et **Running sessions**, puis obtenir de chaque orchestrateur un accusé de prise en compte de sa source/révision effective d’instructions. Les futures modifications de la politique partagée sur le harnais mis à jour s’appliquent entre les tours sans nouvelle compilation ; les modifications de Team Instructions exigent toujours de redémarrer le processus d’agent concerné.
- [ ] Consigner l’installation et la prise en compte réelle pour chaque humain dans #1062. Garder les rappels en pause jusqu’au démarrage effectif de l’exécution Nemo.

L’application et le DMG ont passé les contrôles Apple de notarisation, d’agrafage du ticket et de Gatekeeper. L’installateur monté contient la version attendue et l’exécutable correspondant. Les tests ciblés d’interface, d’éditeur natif, d’instructions, de fournisseur/session et du cœur ont réussi ; les écrans propriétaire/membre ont été vérifiés visuellement à l’aide de jeux d’essai. L’application de bureau et l’ACP ont été reconstruits, avec des binaires auxiliaires signés inchangés et le bundle Codex réutilisé depuis `.13`. Il s’agit de résultats de compilation et de tests : l’installation sur le Mac de chaque utilisateur, la propagation réelle de la politique entre deux comptes et la livraison des rappels planifiés restent les contrôles explicites de configuration ci-dessus. Aucune instruction partagée en service n’a été modifiée et aucun minuteur n’a été activé pendant la livraison.
