# Nemo : les fondations de sa prochaine étape

**Mémorandum à l’attention d’Ilya et de Cyrill · 13 septembre 2026**  
Fondé sur l’examen du code source et de l’avancement de la remédiation effectué à cette date.

Nemo devrait conserver son interface Tauri/JavaScript et orienter les travaux de remédiation restants vers un moteur natif indépendant et une zone de visualisation native. Cette approche préserve le travail utile déjà réalisé tout en donnant au projet une voie plus crédible vers des usages créatifs exigeants. La décision à prendre maintenant consiste à fixer cette orientation et à en vérifier la faisabilité. La réalisation du moteur complet constitue un engagement distinct, avec son propre périmètre et sa propre estimation.

## Où en est le projet

Nemo dispose déjà de nombreuses fonctionnalités, et sa remédiation a réellement progressé. Sur les 59 tâches définies à l’origine, 41 sont closes et 18 restent ouvertes. Les travaux achevés comprennent une répartition plus claire des responsabilités, des opérations mieux sécurisées, des tests et la séparation de parties du code auparavant entremêlées.

Cela ne signifie pas que l’ensemble de la remédiation est terminé à 70 %. Il reste à rapprocher l’inventaire global du code des travaux achevés, puis à le transformer en une liste validée des tâches restantes. Certaines entrées de cet inventaire correspondent à des travaux déjà livrés ; d’autres nécessitent encore une séparation du code ou une implémentation. Il n’est donc pas encore possible de donner un chiffre unique et fiable pour le travail restant.

La remédiation actuelle vise principalement à rendre Nemo plus facile à comprendre, à modifier, à tester et à enrichir. Ses critères de clôture permettent de conserver des défauts existants et des limitations propres aux plateformes, à condition de les documenter explicitement. La terminer serait utile, mais ne démontrerait pas que Nemo peut supporter des charges de travail comparables à celles d’After Effects ou de DaVinci Resolve.

Le problème architectural tient au fait qu’une trop grande partie du calcul de l’état du projet et de la préparation du rendu dépend encore de l’interface et des objets qu’elle manipule en direct. L’accélération GPU existe déjà, mais elle ne supprime ni le coût de préparation de chaque image, ni les transferts de médias entre composants, ni la coordination entre édition et export.

## Ce que changerait l’architecture proposée

L’interface visible pourrait rester largement familière : panneaux, commandes, timeline et habillage de l’application en Tauri/JavaScript. Un moteur natif serait responsable du projet, calculerait l’animation, traiterait les médias et produirait l’image affichée dans la zone de visualisation. L’interface lui transmettrait les commandes d’édition au lieu de participer à toutes les étapes de production de chaque image.

Cela va au-delà du déplacement du rendu vers une tâche en arrière-plan. Le moteur doit disposer d’une représentation cohérente du projet, maîtriser la mémoire et les ressources GPU, et pouvoir abandonner les calculs devenus inutiles lorsque l’utilisateur change de direction. La zone de visualisation doit également afficher le résultat natif sans faire transiter à répétition des images complètes par JavaScript.

L’édition pendant l’export en est un exemple concret. L’export actuel détecte les modifications du projet et s’arrête pour éviter de mélanger plusieurs versions. L’architecture cible permettrait à l’export de continuer à partir d’une version figée du projet, tandis que l’édition se poursuit indépendamment. Ce bénéfice vient de la répartition des responsabilités et de l’organisation des calculs, pas simplement de l’utilisation de Rust.

## Les deux choix

| | Terminer la remédiation telle qu’elle est définie | Adapter la remédiation à l’architecture native |
|---|---|---|
| Bénéfice immédiat | Une version du système actuel plus facile à maintenir et à tester. | Les mêmes progrès structurels, avec des séparations entre composants conçues pour le futur moteur. |
| Principal avantage | Moins de perturbations et moins de nouvelles décisions architecturales avant la fin des travaux structurels. | Moins de risques de rendre le nouveau code dépendant d’hypothèses qu’il faudra ensuite supprimer lors de la migration. |
| Principal coût | Certains travaux futurs pourraient devoir être repris lors du transfert de la gestion du projet et du rendu. | Davantage de conception, de vérifications de faisabilité et de tests de transition maintenant ; un effort d’implémentation distinct ensuite. |
| Résultat attendu à la fin de la remédiation | De meilleures bases pour développer l’architecture actuelle. | De meilleures bases et une orientation de migration testée. Le moteur natif de production reste à réaliser. |
| Perspective pour les fortes charges | Des gains de performance restent possibles, mais les dépendances à l’interface et les transferts de données demeurent des contraintes. | Une voie plus solide vers des charges soutenues, une fois le moteur natif réalisé et validé. |

Conserver le plan actuel est raisonnable si l’objectif à court terme est un outil créatif au périmètre ciblé et si la priorité est de terminer les travaux structurels. Cela n’empêche pas une migration ultérieure. L’inconvénient est que de nouvelles fonctionnalités pourraient renforcer les dépendances au modèle actuel de calcul et de rendu, augmentant ainsi le travail à reprendre par la suite.

Le plan adapté convient mieux à un produit destiné à traiter des animations de plus en plus complexes, des volumes importants de séquences vidéo et des traitements plus longs. Après la migration complète, les résultats recherchés seraient une interaction plus réactive sous forte charge, une consommation mémoire plus prévisible, un rendu autonome en arrière-plan et une meilleure concordance entre prévisualisation et export.

Ce sont des objectifs de conception, pas des résultats démontrés. Des compositions volumineuses peuvent toujours dépasser la puissance de calcul disponible. Les images mises en cache, les proxys et une résolution de prévisualisation réduite resteraient utiles. Atteindre le niveau des applications professionnelles établies exige aussi des effets aboutis, une prise en charge solide des médias, une gestion des couleurs et des tests approfondis ; l’architecture ne fournit pas à elle seule ces capacités.

La migration apporterait ses propres difficultés : intégrer la zone de visualisation native aux éléments d’interface superposés et aux interactions utilisateur, préserver les projets existants et le comportement des animations, et valider les plateformes de bureau prises en charge. Une version navigateur aurait besoin d’un périmètre fonctionnel clairement défini. Pendant la transition, maintenir les anciens et les nouveaux mécanismes ajouterait du travail et pourrait retarder certaines fonctionnalités visibles.

## Ce que cela implique pour le développement avec l’IA

Pour nous, l’intérêt de cette décision est de permettre à Nemo de gagner en sophistication sans perdre en fiabilité. L’IA facilite l’ajout rapide de quantités importantes de code. La question plus difficile est de savoir si une modification de l’animation, de l’historique, de l’export ou du traitement des médias reste cohérente partout où elle produit un effet.

Des responsabilités claires et des interfaces entre composants limitées et explicites facilitent la répartition et la revue du travail assisté par l’IA. Elles réduisent la quantité de contexte nécessaire à chaque tâche. Ajouter des agents ne résout pas le désaccord de deux composants sur l’état courant du projet, et des tests de code réussis ne suffisent pas à établir que le déplacement dans la timeline est agréable ou qu’une image exportée est correcte.

Notre jugement de créateurs reste indispensable à la validation : projets représentatifs, justesse du timing, fidélité visuelle, réactivité de l’édition et reprise après interruption d’une opération. Ces critères doivent guider la migration, davantage que le volume de code produit ou une démonstration réussie de la zone de visualisation.

## Coût et prochaine décision

L’estimation calibrée situe la **préparation supplémentaire à environ 12 heures, avec une hypothèse haute de 18 heures**, qu’elle soit confiée à une équipe locale conforme à l’organisation actuelle ou aux deux équipes existantes sur des travaux distincts. En solo, l’estimation est d’environ 13 heures, avec une hypothèse haute de 19,5 heures. Les configurations mobilisant davantage d’agents ont été exclues, car elles dépassent les limites de fonctionnement actuelles.

Ce chiffrage couvre les ajustements du plan, des interfaces compatibles avec la cible, des modalités de transition explicites et deux vérifications de faisabilité limitées. **Il ne couvre ni la réalisation du moteur natif complet ni la démonstration de performances professionnelles.** Cette implémentation reste à définir et à estimer séparément.

Dans les deux scénarios, la principale incertitude reste la liste des tâches à réaliser, qui n’est pas encore arrêtée. L’évaluation détaillée fournit donc des totaux conditionnels plutôt qu’une échéance promise. Elle suppose que les personnes chargées de la revue et les machines de validation sont disponibles ; les attentes externes de durée indéterminée sont exclues.

La prochaine étape devrait consister à consolider cette liste tout en testant l’intégration de la zone de visualisation native et le calcul indépendant d’un petit projet de test existant. Ces résultats donneraient une base concrète à la décision d’implémentation suivante.

## Conclusion

Adopter dès maintenant le moteur natif et la zone de visualisation native comme cible de Nemo, tout en préservant les travaux de remédiation validés et l’interface actuelle. Ajuster les prochaines séparations entre composants avant que davantage de code dépende du modèle de rendu actuel. Effectuer les vérifications de faisabilité limitées, puis chiffrer et approuver la migration effective en prenant pour référence des charges de travail créatives représentatives.

Pour les outils de plus en plus sophistiqués que nous souhaitons construire, c’est l’orientation la plus solide. Elle préserve l’investissement actuel et donne des fondations plus claires aux développements futurs. Sa réussite devra finalement se mesurer à ce que nous pouvons créer, prévisualiser et livrer confortablement dans Nemo.

---

Références : [Plan d’exécution](../EXECUTION_PLAN.fr.md) et [architecture actuelle et cible — en anglais](../reference/01_CURRENT_AND_TARGET.md). L’avancement et les estimations reflètent l’évaluation du 13 septembre 2026 sur main `9f523462c201740b7f6805ab7bc6c886571fdf30` ; il s’agit d’un mémorandum de décision daté, et non d’un suivi d’avancement en temps réel. Ce mémorandum propose une orientation ; il ne modifie pas le plan d’exécution et n’autorise pas l’implémentation.
