# Phase 4 — Validation multi-environnements et mesures

## Objectif

Valider le comportement déjà implémenté des phases 1 à 3 dans des conditions
réelles, caractériser son coût et documenter les limites observées. Cette phase
ne doit pas ajouter de fonctionnalité utilisateur : toute correction découverte
pendant les essais doit être isolée, testée et reportée dans `PROGRESS.md`.

## État de départ

- Les tests unitaires et d’intégration Vulkan `vkcube` passent selon
  `PROGRESS.md`.
- La validation visuelle de l’overlay de phase 1 est positive.
- La validation prolongée de l’affichage et du cycle F9 reste à faire.
- Les essais multi-jeux, multi-drivers, X11/XWayland et Wayland natif ainsi que
  la mesure de l’overhead restent à faire.
- Le plantage au démarrage de Wreckfest 2 et les essais Steam/Proton restent à
  investiguer.
- Le taux d’échantillonnage GPU (`gpu_poll_rate_ms`) doit être comparé à 50,
  100 et 200 ms. La télémétrie GPU AMD est la seule source implémentée à ce jour.

## Périmètre et principes

- Tester la compatibilité de la Layer et de l’overlay indépendamment de la
  disponibilité de la télémétrie GPU : sur un GPU sans backend, l’état
  « non pris en charge » est un résultat valide, pas un échec.
- Comparer les résultats à une exécution sans Framecap et, lorsque pertinent,
  avec Framecap chargé mais l’overlay masqué ou le limiteur désactivé.
- Garder fixes le jeu, la scène, le pilote, le backend d’affichage, la
  résolution et les réglages du jeu pendant une comparaison.
- Ne pas présenter une mesure du temps CPU de présentation comme un temps
  d’exécution GPU ou comme une mesure de balayage écran.
- Ne pas conclure à une compatibilité Wayland native à partir d’un essai
  XWayland.

## Travaux

### 1. Établir la matrice de compatibilité

Tester, selon les environnements réellement disponibles :

| Axe | Couverture attendue |
| --- | --- |
| Application de référence | `vkcube`, avec le test d’intégration existant |
| Charge GPU | FurMark ou un stress test Vulkan équivalent |
| Jeux | Plusieurs jeux Vulkan lancés directement et depuis Steam/Proton |
| Affichage | X11/XCB, XWayland et Wayland natif, identifiés séparément |
| Matériel | Plusieurs GPU et drivers si disponibles; noter le modèle et la version |
| Modes Framecap | Sans layer, overlay visible/masqué, plafond OFF/manuel/auto |

Pour chaque jeu, vérifier le chargement effectif de la layer, l’absence de
blocage ou de plantage, la présentation des images, les métriques et le cycle
F9. Vérifier également F10/F11 dans les modes manuel et auto, F12, puis le
comportement dégradé lorsque les raccourcis globaux ne sont pas disponibles.
Ne pas considérer l’indisponibilité des raccourcis sous Wayland natif comme une
défaillance de présentation.

Consulter le journal de diagnostic pour le plantage de Wreckfest 2 déjà signalé.
Reproduire d’abord sans Framecap, puis avec la layer, et relever uniquement les
informations nécessaires pour déterminer si la layer est en cause. Corriger un
défaut attribué à Framecap avant de poursuivre; sinon documenter le résultat et
les limites de l’investigation.

### 2. Comparer les fréquences de polling GPU

Réaliser des essais contrôlés avec `gpu_poll_rate_ms = 50`, `100` et `200`.
Garder identiques le GPU, le driver, l’application, la scène, la fenêtre de
lissage (`gpu_smoothing_window_ms`), l’expiration (`gpu_expiry_ms`) et la
configuration automatique.

Pour chaque fréquence :

1. Confirmer dans le journal la configuration effectivement chargée.
2. Observer une période au repos puis une charge reproductible, par exemple un
   stress test Vulkan.
3. Vérifier la fraîcheur et la continuité des mesures, la réactivité aux
   changements de charge, les états d’erreur et les décisions du mode auto.
4. Mesurer l’utilisation CPU de Framecap et relever toute variation de
   frametime ou de comportement.
5. Répéter les essais dans les mêmes conditions; noter la durée et les
   variations de charge entre répétitions.

Conserver 100 ms comme référence actuelle. La recommandation finale (50, 100
ou 200 ms) doit être motivée par les résultats de réactivité, de fraîcheur et
de coût mesurés; ne pas changer le défaut avant d’avoir documenté ce compromis.

### 3. Mesurer l’overhead

Comparer les mêmes scènes et durées avec Framecap absent, puis chargé avec
l’overlay visible et masqué. Désactiver le plafond pour isoler l’observation et
le rendu; mesurer séparément le coût du limiteur lors d’un essai plafonné.

Relever au minimum :

- frametime de présentation p50, p95 et p99 et FPS observées;
- utilisation CPU du processus et, si possible, coût attribuable au polling;
- éventuelles erreurs Vulkan, pertes d’images visibles, saccades ou instabilités;
- configuration et versions exactes des logiciels et du matériel.

Les résultats sont descriptifs : le cahier des charges ne fixe pas encore de
seuil numérique d’overhead. Signaler les écarts reproductibles, fournir les
données brutes ou la méthode de mesure, puis proposer un seuil si les mesures
permettent de le justifier.

### 4. Vérifier la stabilité de la régulation

Sur des scènes à charge basse, élevée et variable, vérifier que le mode auto :

- attend des métriques complètes et une mesure GPU disponible et fraîche;
- respecte les règles, les pas et l’intervalle minimum configurés;
- ne modifie pas la cible en mode OFF et conserve le comportement de F9;
- ne produit pas d’oscillation ou de changements répétés inattendus.

Consigner les valeurs de configuration, les conditions de charge, les
transitions de mode et les changements de cible. Les seuils de l’algorithme
restent expérimentaux; toute proposition de réglage doit être justifiée par
des essais répétés plutôt que par une seule session.

## Résultats à consigner

Mettre à jour `PROGRESS.md` avec la date, la matrice effectivement testée, les
résultats positifs et négatifs, les défauts ouverts et la recommandation de
polling. Pour chaque essai manuel, consigner au minimum :

| Champ | Exemple de contenu |
| --- | --- |
| Application et scène | Nom, version, scène/preset et durée |
| Environnement | X11, XWayland ou Wayland natif; GPU, driver et versions |
| Configuration | `gpu_poll_rate_ms`, fenêtre/expiration, mode et cible |
| Comparaison | Sans layer, overlay visible/masqué, limiteur OFF/actif |
| Mesures | FPS et frametime p50/p95/p99, CPU, télémétrie et erreurs |
| Verdict | Réussi, échec, non testé ou non applicable; commentaire et journal utile |

Ne pas inclure de journaux complets contenant des chemins locaux dans la
documentation publiée. Mettre à jour `README.md` si les essais révèlent une
instruction de lancement, une compatibilité ou une limite qui doit être connue
des utilisateurs.

## Critères de sortie

- `cargo fmt --check`, `cargo build` et `cargo test` passent; le résultat du
  test Vulkan conditionnel est explicitement rapporté.
- Le scénario de référence `vkcube` fonctionne avec et sans Framecap.
- La matrice indique clairement les environnements exécutés, non exécutés et
  impossibles à tester; aucune compatibilité non vérifiée n’est revendiquée.
- Les comparaisons 50/100/200 ms et les mesures d’overhead sont consignées avec
  leur méthode et leurs limites.
- Le cycle F9 et les contrôles clavier sont vérifiés dans les environnements
  qui les prennent en charge; les limitations Wayland sont explicites.
- Le plantage Wreckfest 2 a un résultat d’investigation documenté.
- Les éventuels défauts bloquants attribuables à Framecap sont corrigés et
  couverts par des tests, ou restent explicitement ouverts avec leurs preuves.
