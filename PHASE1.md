# Spécification technique — Phase 1 : Observabilité

## 1. Objectif

Mesurer et afficher en temps réel le frametime observé, le jitter et la charge GPU pour une application Vulkan Linux. Cette phase fournit les données nécessaires aux phases de limitation et de régulation, sans modifier le comportement de cadence de l’application.

## 2. Périmètre

Inclus :
- Une Vulkan Layer passive, activable par application, qui observe les présentations.
- Un overlay propre au projet, dessiné dans les images présentées par l’application.
- Une fenêtre glissante de mesures et un fournisseur de télémétrie GPU extensible.
- Le traitement explicite des métriques indisponibles ou périmées.

Exclus :
- Tout délai ou plafonnement de FPS.
- La décision automatique de régulation.
- La promesse de mesurer le temps d’exécution GPU exact d’une frame. La Layer ne peut pas déduire ce temps à partir du seul appel de présentation.

## 3. Base technique

La base de référence est Rust avec `ash`, conformément à la recommandation du cahier des charges. La Layer expose les points d’entrée Vulkan requis par le loader et transmet les appels aux fonctions aval. Les composants de calcul des métriques et de télémétrie doivent rester indépendants des points d’entrée Vulkan autant que possible.

L’overlay est rendu dans la swapchain de l’application par la Layer. Cette approche évite de dépendre d’un protocole de fenêtre toujours-au-dessus, dont la disponibilité diffère entre X11 et les compositeurs Wayland. La Layer ne doit pas intercepter les applications qui ne l’ont pas activée.

## 4. Architecture logique

1. **Entrée Layer :** initialisation et destruction des instances, appareils, queues et swapchains; interception de `vkQueuePresentKHR`.
2. **Collecteur frame :** horodatage monotone des présentations et émission d’échantillons par swapchain.
3. **Agrégateur :** calcul du frametime et du jitter sur les derniers échantillons valides.
4. **Télémétrie GPU :** interrogation asynchrone d’un fournisseur compatible et association de la mesure au GPU utilisé par l’appareil Vulkan.
5. **Overlay :** lecture d’un instantané cohérent des métriques et rendu d’une ligne compacte dans l’image présentée.

Aucun accès aux fichiers sysfs, appel fournisseur ou calcul non borné ne doit être effectué dans le chemin critique de `vkQueuePresentKHR`. Les instantanés sont copiés sous une synchronisation courte; aucun verrou n’est conservé pendant un appel Vulkan aval.

## 5. Définitions des métriques

### Frametime

Le frametime affiché en phase 1 est l’intervalle entre deux appels successifs à `vkQueuePresentKHR` observés pour la même swapchain, horodatés avec une horloge monotone haute résolution. Il est exprimé en millisecondes; le FPS instantané associé est `1000 / frametime_ms` lorsque l’intervalle est non nul.

Cette valeur décrit l’intervalle de présentation côté CPU. Elle ne représente pas nécessairement la durée d’exécution des commandes GPU ni l’instant où l’écran a effectivement balayé l’image. L’overlay doit l’intituler « Frametime » ou « Intervalle », sans le présenter comme une mesure GPU exacte.

### Jitter

Le jitter d’affichage est l’écart-type des frametimes correspondant aux mêmes échantillons temporels que le FPS moyen et le 10% low. Ces échantillons utilisent la fenêtre glissante `gpu_smoothing_window_ms`. Après un changement de CAP, les trois métriques repassent ensemble en collecte jusqu’à ce qu’au moins un intervalle valide soit disponible et que la durée minimale `gpu_poll_rate_ms` soit écoulée. Le graphique de jitter conserve un historique séparé.

### 10% low (1er décile du framerate)

Le 10% low est le premier décile de la distribution des FPS instantanés observés dans la même fenêtre temporelle `gpu_smoothing_window_ms` que le FPS moyen et le jitter. Autrement dit, il s’agit du niveau que les 10 % des frames les plus lentes atteignent ou dépassent : un seuil de "pire 10 %" pour capturer les rares baisses de performance.

Cette métrique est importante pour la phase 3 car elle permet de caractériser la qualité de la distribution du framerate sans imposer un plafonnement immédiat. Un algorithme intelligent alternatif peut viser à maintenir cette mesure au-dessus d’un seuil cible, tout en gardant le CPU et le GPU dans une zone de stabilité. On ne doit pas la confondre avec un ceiling de FPS : il s’agit d’une mesure de distribution, pas d’un cap explicite.

La valeur est exprimée en FPS. Les trois métriques de présentation restent en état « collecte… » tant que la génération courante n'a pas d'intervalle valide et que `gpu_poll_rate_ms` n'est pas écoulé.

### Charge GPU

La charge est un pourcentage fourni par un backend de télémétrie du GPU. La priorité est donnée au backend **AMD** via les interfaces sysfs (`/sys/class/drm/*/device/in_use_percent` ou `amdgpu`). Une interface interne commune doit exposer au minimum : valeur, horodatage, état (`disponible`, `périmée`, `non prise en charge`, `erreur`) et identifiant du GPU associé.

Les backends sont isolés derrière une interface; aucun pourcentage synthétique ne doit être produit en cas d'absence de source AMD compatible. L'association entre `VkPhysicalDevice` et le GPU mesuré doit utiliser un identifiant matériel stable lorsque disponible. Si plusieurs GPU rendent l'association ambiguë, la charge est déclarée indisponible.

L'échantillonnage GPU est asynchrone, avec une fréquence configurable (valeur initiale proposée : 10 Hz). La mesure est considérée périmée après une durée configurable (valeur initiale proposée : 1 seconde). Une mesure périmée n'est jamais affichée comme une valeur actuelle.

#### Fournisseur GPU AMD

Implémentation via lecture asynchrone du sysfs d'AMD :
- **Fichiers cibles :** `/sys/class/drm/card*/device/in_use_percent` (si exposé) ou alternative `amdgpu`
- **Fréquence :** Polling non bloquant tous les ~100ms via `tokio::time::interval` ou multithreading
- **Gestion d'erreur :** Si le fichier n'existe pas, retourner `TelemetryStatus::Unsupported`


## 6. Overlay

Affichage compact, sans interaction requise :

`FT 16.7 ms | Jitter 0.6 ms | 10% low 58 FPS | GPU 92%`

Les états de collecte, de télémétrie indisponible ou périmée doivent être visibles textuellement. Le rendu ne doit pas modifier la résolution, le format ou le nombre d’images de la swapchain de l’application.

La Layer possède ses propres ressources graphiques. Elle doit respecter les dépendances de synchronisation de la queue et de la swapchain, ne pas réutiliser une ressource avant sa fin d’utilisation et libérer les ressources à la destruction/recréation de la swapchain. Tout échec de rendu de l’overlay doit laisser la présentation de l’application fonctionner sans overlay lorsque cela reste possible.

## 7. Cycle de collecte

1. À chaque `vkQueuePresentKHR`, enregistrer l’heure monotone et mettre à jour l’intervalle de la swapchain concernée.
2. Mettre à jour la fenêtre glissante et produire un instantané immuable des métriques.
3. Lire le dernier échantillon GPU publié par le backend asynchrone.
4. Rendre l’instantané dans l’image présentée en respectant les synchronisations Vulkan.
5. Transmettre l’appel de présentation à la Layer aval ou à l’implémentation Vulkan.

La gestion des présentations contenant plusieurs swapchains doit conserver des statistiques séparées par swapchain et ne pas compter plusieurs fois le même événement de présentation.

## 8. Configuration et erreurs

- L’activation de la Layer est explicite et limitée à l’application ciblée.
- La configuration initiale peut provenir de variables d’environnement préfixées `FRAMECAP_`; elle est lue lors de l’initialisation et validée avant usage.
- Une configuration invalide désactive la fonctionnalité concernée et journalise une erreur concise; elle ne doit pas empêcher le démarrage de l’application.
- Une API Vulkan non prise en charge, un backend GPU absent ou une swapchain recréée réinitialise proprement les données dépendantes, sans valeurs périmées affichées comme actuelles.

## 9. Critères d’acceptation

- L’overlay affiche frametime, jitter et 10% low pour une application Vulkan de test, puis indique la collecte au démarrage.
- La charge GPU est associée au bon périphérique lorsqu’un backend compatible est disponible; sinon son indisponibilité est visible et aucune valeur n’est inventée.
- La fenêtre de jitter respecte les 60 derniers intervalles valides et les tests unitaires couvrent la fenêtre incomplète, les valeurs invalides et la remise à zéro.
- Le 10% low est calculé sur les 60 derniers intervalles valides et reste en état de collecte tant qu’un nombre suffisant d’échantillons n’a pas été accumulé.
- L’activation de la Layer n’introduit aucun plafonnement intentionnel.
- La désactivation ou l’échec du rendu de l’overlay n’empêche pas l’application de présenter ses images.
- Les chemins X11 et Wayland sont vérifiés avec une application Vulkan de test; la Layer n’assume pas un protocole de fenêtre propre à l’un des deux.
- Des mesures de référence documentent le coût CPU de la collecte et du rendu de l’overlay.

## 10. Tests requis

- Tests unitaires de calcul des intervalles, de la fenêtre glissante et de l'écart-type.
- Tests de contrat des états du backend GPU, y compris données manquantes, périmées et association ambiguë.
- Test d'intégration de chargement via le Vulkan loader et de présentation avec/sans overlay.
- Vérification manuelle sur X11 et Wayland lorsque les deux environnements sont disponibles.

## 11. Application de test

FurMark sera utilisé comme application de test pour la validation :
- **Justification :** FurMark est un stress test Vulkan/OpenGL bien connu qui sollicite intensément le GPU, permettant de valider la détection de charge élevée et la réaction dynamique future.
- **Configuration de test :** Lancer FurMark avec les presets "Vulkan GPU Stress" ou équivalent.
- **Activation de la Layer :** Via un manifeste Vulkan et les variables du Vulkan Loader `VK_LAYER_PATH` / `VK_INSTANCE_LAYERS`; les réglages Framecap sont dans `framecap.toml`.
- **Validation :** Observer l'overlay et vérifier que les métriques varient durant le stress test (charge GPU montante, jitter potentiellement accru).

## 10. Tests requis

- Tests unitaires de calcul des intervalles, de la fenêtre glissante et de l’écart-type.
- Tests de contrat des états du backend GPU, y compris données manquantes, périmées et association ambiguë.
- Test d’intégration de chargement via le Vulkan loader et de présentation avec/sans overlay.
- Vérification manuelle sur X11 et Wayland lorsque les deux environnements sont disponibles.
