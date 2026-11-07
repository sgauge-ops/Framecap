# Suivi d'avancement

Dernière mise à jour : 2026-10-30

## Phase 1 — Observabilité : validée visuellement, fallback sûr conservé

Réalisé :
- Couche Vulkan explicite avec interception du loader, des swapchains, des queues et de `vkQueuePresentKHR`.
- Collecte des intervalles de présentation, calcul du jitter d'affichage à partir des mêmes échantillons temporels que le FPS et le 10% low, et stabilisation des snapshots de métriques.
- Fournisseur AMD sysfs compatible (`in_use_percent` / `gpu_busy_percent`) avec association sur identifiants PCI et état `Available`, `Expired`, `Unsupported`, `Error`.
- Reprise de la logique de queue et correction d'un bug de dispatch par poignée Vulkan au lieu d'une lecture mémoire invalide.
- Vérification de validité par tests unitaires et intégration Vulkan : `cargo test -- --nocapture` passe, y compris le test `vkcube` avec et sans layer.
- Validation runtime visuelle confirmée : l'overlay est bien affiché et la présentation est jugée satisfaisante par le test manuel.
- `README.md` documente le build, le lancement par processus sous Wayland/XCB, les tests et le statut des variables de configuration.

Diagnostic technique retenu :
- Le premier essai d'injection d'overlay dans `vkQueuePresentKHR` pouvait casser la présentation sur certains pilotes/drivers Vulkan, notamment RADV.
- Le correctif prudent actuellement retenu évite d'écraser le flux de présentation, tout en maintenant un diagnostic fonctionnel et une validation visuelle positive de la phase 1.

À compléter :
- Si l'on veut un overlay persistant en production, reprendre la piste d'injection avec une synchronisation Vulkan stricte et compatible driver, sans réintroduire le blocage présentation.
- Exécuter avec la couche Khronos de validation; elle n'est pas installée dans l'environnement courant.
- Ajouter une étape de build explicite de la bibliothèque partagée avant le test d'intégration dans les environnements propres; ce test nécessite aussi `vkcube`, `timeout` et une session graphique.
- Mesurer et documenter le coût CPU de la collecte et du rendu.
- Valider les variations de charge avec FurMark ou un test de stress Vulkan équivalent.

## Phase 2 — Limiteur manuel : implémentée, validation Steam/Proton en cours

Réalisé :
- Ajout du plafond initial à 60 FPS et de la configuration `[framecap].target_fps` dans `framecap.toml` (0 pour désactiver, valeurs finies de 1 à 240 FPS).
- Ajout d'un échéancier absolu monotone et indépendant par queue, avec recalage après retard et attente Linux sans verrou détenu.
- Ajout du contrôleur atomique de cible et des raccourcis globaux F10/F11 sous X11/XWayland, réglables par configuration et désactivables; le listener est optionnel, hors chemin de présentation, et journalise son indisponibilité sans désactiver le limiteur.
- Tests unitaires couvrant configuration, bornes, cadence, retards et ajustements du contrôleur.
- Test d'intégration Vulkan `vkcube` comparant limite désactivée et plafond à 30 FPS; 30 présentations plafonnées respectent le seuil de durée attendu.
- Validation des événements F10/F11 via XTest sur X11 : la cible passe de 60 à 65 puis revient à 60 FPS.
- À chaque changement effectif de cible, l'historique de présentation de chaque swapchain est réinitialisé; FPS, « 10% low » et jitter repassent ensemble en collecte jusqu'à ce qu'un intervalle valide soit recueilli et que `gpu_poll_rate_ms` soit écoulé.
- L'overlay affiche sur la première ligne la cible FPS préfixée par « CAP », en cyan, suivie des indications de touches; les métriques sont sur la seconde ligne. À la fin de la phase 2, F9 (Auto) était une indication uniquement; son basculement est implémenté en phase 3.
- F12 masque/réaffiche l'overlay via le listener X11/XWayland; la présentation, le plafond et la collecte des métriques restent actifs lorsque l'overlay est masqué.
- Fonctionnement de F12 confirmé lors d'un test utilisateur.
- Jeux testés depuis Steam/Proton; un plantage au démarrage a été constaté avec Wreckfest 2. Le journal de diagnostic est disponible pour analyser la cause.
- L'utilisation GPU est maintenant lissée par une moyenne glissante temporelle de 1000 ms par défaut (environ dix échantillons à la fréquence de collecte de 100 ms); la fenêtre est configurable et une mesure indisponible invalide immédiatement l'historique.
- Ajout d'un journal de diagnostic avec contexte processus, configuration effective, étapes et durées d'initialisation Vulkan et erreurs non répétitives; chemin par défaut sous `XDG_STATE_HOME` ou `~/.local/state`, surcharge `[diagnostics].log_path` dans `framecap.toml`. `[diagnostics].append_log` choisit de vider le journal à l'ouverture (défaut 0) ou de conserver son contenu (1).

À réaliser :
- Investiguer le plantage au démarrage de Wreckfest 2 à partir du journal de diagnostic généré.
- Continuer la validation avec d'autres jeux Steam/Proton et environnements X11/XWayland.
- Ajouter un backend de raccourcis pour Wayland natif si nécessaire.

## Phase 3 — Régulation dynamique : POC implémentée

Réalisé :
- Tous les réglages Framecap précédemment lus via les variables d'environnement sont centralisés dans `framecap.toml`; le fichier est recherché dans le dossier courant, puis dans `$XDG_CONFIG_HOME/framecap.toml` s'il est défini, puis dans `~/.config/framecap.toml`. En l'absence de fichier, le journal liste les chemins vérifiés. Le chemin du journal se configure sous `[diagnostics].log_path`.
- F9 fait cycler sous X11/XWayland le mode manuel, les algorithmes AUTO dans `algorithm_order`, puis OFF; le plafond est mémorisé pendant OFF et restauré au retour en manuel. L'overlay affiche l'indice de l'algorithme AUTO actif. La spécification de référence est [`specs/AUTO-MODE.md`](specs/AUTO-MODE.md).
- Au démarrage, le mode est manuel avec la cible configurée (60 FPS par défaut), ou OFF si `target_fps = 0`. F10/F11 ajustent le CAP en FPS en manuel et l'hystérésis active en points de pourcentage en mode AUTO; ils n'agissent pas en mode OFF.
- Les règles, leurs seuils, toutes les conditions de décision, les paramètres TOML utilisés et les tests requis sont définis dans [`specs/AUTO-MODE.md`](specs/AUTO-MODE.md), qui est la référence modifiable du mode automatique.
- Deux algorithmes prédéfinis sont configurés dans `framecap.toml` avec un schéma commun et des temporisations indépendantes : `gpu_threshold` régule autour du « 10% low » sous les seuils GPU/jitter configurés; `vrm_temp` vise 100 °C avec une zone neutre ±5 % et suspend ses ajustements si la télémétrie VRM est absente ou périmée.
- Les seuils GPU sont validés indépendamment comme valeurs finies comprises entre 0 et 100 %; sinon, ils reprennent leurs valeurs par défaut (98 et 90). `frame_jitter_limit` est validé dans la plage 0–500 ms. Le régulateur utilise ces valeurs validées; le journal de configuration les indique.
- En mode auto, F10/F11 modifient immédiatement et atomiquement l'hystérésis active de -/+1 point de pourcentage (bornes 0–100); l'overlay affiche sa valeur courante et les indications de touches.
- La deuxième ligne de l'overlay affiche les FPS à la place du frametime. Le FPS moyen, le 10e percentile (« 10% low ») et le jitter partagent les mêmes échantillons dans la fenêtre glissante `gpu_smoothing_window_ms` (1000 ms par défaut). Au démarrage et après un changement de CAP, ils restent tous en collecte jusqu'à ce qu'un intervalle soit disponible et que `gpu_poll_rate_ms` (100 ms par défaut) soit écoulé. La charge GPU conserve son lissage temporel séparé.
- Le graphique du jitter est optionnel et désactivé au démarrage par défaut (`[overlay].jitter_graph_startup = 0`); `1` l'active. Quand il est activé, il affiche une fenêtre glissante de 3 secondes configurable par `jitter_graph_window_ms` et conserve son historique lors des changements de cible.
- Tests unitaires déterministes couvrant le cycle F9 avec restauration de la cible, ainsi que les règles du régulateur, leurs seuils GPU/jitter, la disponibilité/fraîcheur GPU, les métriques incomplètes, l'intervalle de changement et les bornes (voir la spécification de référence).
- Sauvegarde de l'état du projet avant l'ajout des seuils configurables : `../Framecap-backup-20261101-1126.tar.gz` (sources et configuration, sans le répertoire généré `target/`).
- Tests couvrant le tracé/clipping, la persistance de l'historique aux changements de génération, l'expiration des échantillons par durée et la validation TOML des paramètres du graphique.
- Validation après le graphique de jitter : `cargo fmt`, `cargo build` et `cargo test` passent (45 tests unitaires et 8 tests d'intégration).
- Validation de l'implémentation multi-algorithmes : `cargo fmt`, `cargo check` et la suite `cargo test` passent; tests dédiés couvrent le cycle F9, la zone thermique VRM, les expirations et l'hystérésis.

Limites du POC :
- La fréquence observée est estimée depuis le dernier intervalle de présentation; le GPU AMD est la seule source de télémétrie actuellement implémentée.
- Les raccourcis ne fonctionnent que sous X11/XWayland; le mode automatique reste désactivé si l'entrée globale est indisponible.
- La stratégie ne fait pas encore l'objet d'une validation prolongée sur plusieurs GPU/jeux. Voir [`specs/AUTO-MODE.md`](specs/AUTO-MODE.md) pour les règles et paramètres.

## Phase 4 — Validation : en cours

Réalisé :
- Mesure de l'overhead effectuée sur `vkcube` (1200 frames) :
  - Base (Sans Layer) : ~8,85s
  - Overlay visible : ~8,85s (Coût négligeable)
  - Overlay masqué : ~8,85s (Coût de collecte/polling négligeable)
  - Limiteur actif (60 FPS) : ~20,00s (Fonctionnement validé)
- Validation du cycle F9 et des contrôles clavier dans l'environnement de test.

À réaliser :
- Comparaison des fréquences de polling GPU (10, 50, 100, 200 ms).
- Investigation Wreckfest 2 : le jeu a initialement atteint l'écran de présentation avec l'overlay, puis a planté au début de la 3D; le dump récent attribue l'accès invalide à `bc250_xefg_original.dll`, dans le chemin OptiScaler XeFG/MFG/XeSS. Le lancement sans OptiScaler a fonctionné. Le routage multi-instance Vulkan a été corrigé et des diagnostics d'énumération ajoutés. `[diagnostics].append_log` permet de choisir entre journal vierge (défaut) et historique conservé; avec la valeur par défaut, plusieurs processus chargent la layer et peuvent chacun vider le même fichier.
- Validation prolongée sur plusieurs jeux/drivers.
- Mesure de l'overhead sur des scènes plus complexes.

Le plan de validation est documenté dans [`PHASE4.md`](PHASE4.md). Il couvre notamment la comparaison des fréquences `gpu_poll_rate_ms` de 50, 100 et 200 ms, les mesures d'overhead et une matrice de compatibilité explicite.

## Phase 5 — Industrialisation : en cours

- Réalisé — configuration (point 1 de [`PHASE5.md`](PHASE5.md)) : les paramètres sont répartis entre `[framecap]`, `[key-bindings]`, `[overlay]`, `[auto-mode]`, `[manual-mode]` et `[diagnostics]`. Les commentaires de `framecap.toml` précisent unités, défauts et contraintes. Les anciennes clés déplacées restent lues à des fins de migration (sans réécriture du fichier); les doublons ancien/nouveau sont refusés. `jitter_high`, `jitter_low` et `gpu_saturation`, qui n'avaient aucun effet fonctionnel, sont acceptés comme obsolètes avec avertissement et retirés de la configuration active. Les pas et bornes manuels, ainsi que les bornes automatiques, sont configurables.
- Réalisé — implémentation de l'overlay (point 2 de [`PHASE5.md`](PHASE5.md)) : colonnes de texte à largeur fixe, fond assombri avec opacité paramétrable (0 transparent, 1 opaque), libellés/unités homogènes et retours visuels d'une seconde pour les touches, le plafond et l'hystérésis. L'échelle du graphique de jitter est fixe de 0 à 50 ms et les valeurs au-delà sont saturées. La validation visuelle manuelle sur plusieurs jeux/résolutions reste à effectuer avec les essais de phase 4.
- Le plan d'industrialisation complet est documenté dans [`PHASE5.md`](PHASE5.md). Restent les points 3 à 5 : documentation ciblée du code en anglais, versions anglaises des documents Markdown et préparation/publication GitHub après confirmation du dépôt distant.
