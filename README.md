# Framecap

Framecap est une Vulkan Layer Linux en cours d'évolution. Elle observe les présentations et dessine un overlay avec les FPS lissées, le jitter et l'état de la télémétrie GPU. Le FPS moyen, le 10% low et le jitter utilisent les mêmes échantillons de présentation et la fenêtre glissante temporelle `gpu_smoothing_window_ms` (1000 ms par défaut). Au démarrage et après un changement de CAP, ces trois métriques reviennent ensemble en `COLLECT` jusqu'à ce qu'un intervalle valide soit disponible et que `gpu_poll_rate_ms` (100 ms par défaut) se soit écoulé. En mode auto, les métriques sont évaluées après leur actualisation, au plus une fois par `gpu_poll_rate_ms`; la première évaluation est permise à la première présentation, et une présentation multi-swapchain ne provoque pas plusieurs évaluations. La charge GPU est lissée séparément sur la même durée et suit sa propre expiration. L'overlay affiche les raccourcis, le mode avec le libellé JIT/VRM/FAN de l'algorithme AUTO, le CAP et l'hystérésis active, puis les métriques et capteurs. Le graphique de jitter est désactivé par défaut; activez-le avec `[overlay].jitter_graph_startup = 1`. Lorsqu'il est activé, il est échelonné de 0 à 50 ms, apparaît sur la quatrième ligne (`jitter_graph_width_px` × `jitter_graph_height_px`, 100 × 10 px par défaut) et affiche une fenêtre glissante de 3 secondes (`jitter_graph_window_ms`). Le limiteur démarre en mode manuel à 60 FPS par défaut. F9 parcourt le mode manuel, les algorithmes AUTO configurés et OFF; F10/F11 ajustent l'hystérésis active en points de pourcentage en mode auto et le CAP en mode manuel. F9/F10/F11/F12 sont prises en charge sous X11/XWayland.

## Prérequis

- Rust et Cargo
- Le Vulkan Loader et un pilote Vulkan fonctionnels
- `vkcube` pour le test manuel (`vulkan-tools` sur la plupart des distributions)
- Un environnement graphique Wayland ou X11/XCB

## Build

Pour une procédure d'installation complète depuis GitHub, consultez
[`INSTALL.md`](./INSTALL.md).

Depuis la racine du dépôt :

```sh
cargo build
```

La bibliothèque de layer est produite dans `target/debug/libframecap.so`. Le manifeste de développement se trouve dans `layer/VkLayer_FRAMECAP_observability.json` et référence cette bibliothèque relativement à son emplacement.

## Lancer avec vkcube

La layer est activée uniquement pour le processus auquel les variables du Vulkan Loader sont transmises. Depuis la racine du dépôt :

```sh
env VK_LAYER_PATH="$PWD/layer" \
  VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability \
  vkcube --c 600 --wsi wayland
```

Pour le chemin X11/XCB :

```sh
env VK_LAYER_PATH="$PWD/layer" \
  VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability \
  vkcube --c 600 --wsi xcb
```

Pour comparer sans Framecap, retirer `VK_INSTANCE_LAYERS` :

```sh
env VK_LAYER_PATH="$PWD/layer" vkcube --c 600 --wsi wayland
```

`VK_LAYER_PATH` désigne le dossier contenant le manifeste; `VK_INSTANCE_LAYERS` demande au Vulkan Loader d'activer cette layer pour le processus. Ces deux variables sont des paramètres du Vulkan Loader, pas des réglages Framecap. `--c` est le nombre d'images de `vkcube`; `--wsi` accepte notamment `wayland`, `xcb` et `xlib`. Remplacer le WSI par un backend disponible sur la machine.

## Activer Framecap pour un jeu Steam

Construisez d'abord la layer depuis la racine du dépôt :

```sh
cargo build
```

Dans Steam, ouvrez **Propriétés** du jeu, puis **Général → Options de lancement**. Ajoutez la ligne suivante en remplaçant `/home/sebastien/Framecap` par le chemin absolu de votre copie du dépôt :

```sh
VK_LAYER_PATH=/home/sebastien/Framecap/layer VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability %command%
```

`VK_LAYER_PATH` doit désigner le dossier contenant `VkLayer_FRAMECAP_observability.json`. Le manifeste de développement fourni pointe vers `target/debug/libframecap.so` : gardez le dépôt et son répertoire `target` à cet emplacement relatif, et relancez `cargo build` après toute modification du code. Steam remplace `%command%` par la commande de lancement du jeu; ces variables du Vulkan Loader n'affectent donc que ce jeu. Les réglages Framecap se font dans `framecap.toml` (voir ci-dessous). Pour retirer Framecap, effacez ces options de lancement.

La cible est de 60 FPS par défaut. La régulation automatique est désactivée au démarrage; appuyez sur F9 pour la basculer. Les raccourcis F9/F10/F11/F12 nécessitent X11/XWayland; ils ne sont pas disponibles sous Wayland natif.

## Journaux de diagnostic

Framecap écrit dans le journal une entrée à chaque chargement de la layer, avec le PID de chaque processus. Par défaut, le journal est vidé à l'ouverture (`[diagnostics].append_log = 0`); avec `append_log = 1`, les nouvelles entrées sont ajoutées au contenu existant. Plusieurs processus Steam/Proton peuvent charger la layer pendant un même lancement; avec la valeur par défaut, chacun peut donc vider le journal à son initialisation. Activez `append_log = 1` pour conserver les traces de tous ces processus et des lancements précédents. `append_log` est l'unique option de conservation reconnue. Par défaut, le fichier est `~/.local/state/framecap/framecap.log`, ou `$XDG_STATE_HOME/framecap/framecap.log` si `XDG_STATE_HOME` est défini. Son chemin est aussi annoncé sur la sortie d'erreur du processus. Le journal contient le PID, l'exécutable et le dossier courant, quelques variables Vulkan/Steam/Framecap, le GPU sélectionné, la configuration effective, les résultats et durées des étapes Vulkan d'initialisation, ainsi que les erreurs de rendu et de raccourcis. Il n'enregistre pas les arguments complets de lancement ni les événements de présentation par frame.

Pour choisir un autre chemin, définissez `log_path` dans la section `[diagnostics]` de `framecap.toml`. Vérifiez ou copiez le journal après avoir fermé le jeu. Si aucun journal n'est créé, la layer n'a probablement pas été chargée; pour diagnostiquer le chargement du Vulkan Loader, on peut ajouter temporairement `VK_LOADER_DEBUG=error,warn,layer` aux options de lancement. Si le processus ne peut pas créer le fichier, Framecap écrit ses diagnostics sur stderr sans interrompre le jeu. Le journal contient des chemins d'accès locaux; relisez-le avant de le partager.

## Tests

Le test A/B avec `vkcube` nécessite une session graphique, `vkcube`, `timeout` et la bibliothèque partagée construite. Lancez :

```sh
cargo build && cargo test
```

Dans un environnement headless ou si une dépendance manque, le test d'intégration `vkcube` est ignoré; les tests unitaires restent exécutés.
Avec une session graphique et un mode de présentation immédiat disponible, le test compare aussi 30 présentations plafonnées à 30 FPS avec le limiteur désactivé.

Les snapshots de métriques sont immuables après leur création et conservent les métadonnées et l'horodatage de la mesure GPU source.

## Configuration

Tous les réglages Framecap se trouvent dans le fichier dédié [`framecap.toml`](./framecap.toml). Framecap le recherche dans le dossier courant du processus, puis dans `$XDG_CONFIG_HOME/framecap.toml` si cette variable est définie, puis dans `~/.config/framecap.toml`. Pour Steam, installez votre configuration ainsi :

```sh
mkdir -p ~/.config
cp /chemin/vers/Framecap/framecap.toml ~/.config/framecap.toml
```

Le fichier livré à la racine du dépôt sert d'exemple et de configuration par défaut pour les lancements depuis ce dossier. Si aucun candidat n'est trouvé, le journal indique les chemins vérifiés et les valeurs par défaut intégrées sont utilisées. Une erreur de lecture ou de syntaxe est également journalisée et entraîne le recours aux valeurs par défaut.

La configuration est organisée en sections : `[framecap]` pour la cible et la collecte (`target_fps = 0` démarre en mode OFF), `[key-bindings]` pour activer et affecter F9/F10/F11/F12, `[overlay]` pour le lissage des métriques, le graphique de jitter et l'opacité du fond, `[auto-mode]` pour les seuils GPU/jitter communs et l'ordre du cycle, puis `[auto-mode.algorithms.<nom>]` pour chaque algorithme prédéfini. Chaque algorithme expose la même configuration : métrique, cible, hystérésis en pourcentage, temporisations directionnelles, pas FPS, bornes FPS et expiration de mesure. Le fichier livré définit `gpu_threshold` (métrique `fps`, cible de référence `low_10_fps`), `vrm_temp` (température GPU VRM, cible 100 °C) et `fan_rpm` (vitesse de ventilateur, cible 2000 RPM). Les seuils GPU (98 % à la hausse, 90 % à la baisse) et le seuil de jitter (2 ms) ne s'appliquent qu'à `gpu_threshold`; les deux algorithmes matériels suspendent leurs ajustements si leur mesure est absente ou périmée. FPS, « 10% low » et jitter partagent leurs échantillons et restent ensemble en `COLLECT` au démarrage et après un changement de CAP jusqu'à ce qu'un intervalle valide soit disponible et que `gpu_poll_rate_ms` (100 ms par défaut) se soit écoulé; ils sont ensuite calculés sur la fenêtre glissante `gpu_smoothing_window_ms`. La charge GPU et les capteurs matériels sont lissés séparément. La télémétrie GPU est périmée au-delà de l'expiration maximale définie par les algorithmes. Les valeurs visibles sont mises à jour selon `refresh_rate`; les raccourcis doivent être distincts et les bornes FPS comprises entre 1 et 240.

F9 fait cycler **manuel → AUTO JIT → AUTO VRM → AUTO FAN → OFF → manuel** (l'ordre des algorithmes est défini par `algorithm_order`); au démarrage, `target_fps = 0` commence en mode OFF. OFF désactive le plafond mais conserve la dernière cible. En AUTO, l'overlay affiche le libellé de l'algorithme et F10/F11 diminuent/augmentent son hystérésis de 1 point de pourcentage; en manuel, ils diminuent/augmentent le CAP selon `step_fps`. Chaque algorithme possède ses temporisations et paramètres propres. Les couples pris en charge sont `fps`/`low_10_fps`, `gpu_vrm_temp_c`/cible numérique et `fan_rpm`/cible RPM numérique. Les règles de décision, les mesures et leurs conditions de validité sont détaillées dans la [spécification AUTO-MODE](./specs/AUTO-MODE.md).

Les anciens paramètres d'auto-mode à règle unique ne sont pas compatibles avec le schéma multi-algorithmes : en cas de clé inconnue ou d'erreur TOML, Framecap journalise le problème et utilise ses valeurs intégrées. Les anciennes clés `jitter_high`, `jitter_low` et `gpu_saturation` sont obsolètes et doivent être supprimées. Framecap ne réécrit pas le fichier; les changements sont pris en compte au prochain chargement de la layer (redémarrer le jeu).

Les FPS, le jitter, le 10% low et la télémétrie GPU utilisent des champs à positions fixes. Les colonnes F10/CAP/10% LOW, F11/HYS/GPU et F12/JIT sont alignées sur les lignes de l'overlay. Le graphique de jitter a une échelle verticale fixe de 0 à 50 ms; les valeurs supérieures sont plafonnées à 50 ms. Les touches pressées ainsi que les champs CAP et HYS modifiés sont mis en évidence pendant une seconde.

La section `[diagnostics]` accepte `log_path` pour remplacer le chemin par défaut `~/.local/state/framecap/framecap.log` (ou `$XDG_STATE_HOME/framecap/framecap.log`) et `append_log` (0 vide le fichier à l'ouverture, 1 conserve son contenu). Tout autre nom, notamment `keep_log`, est rejeté comme option inconnue. Les variables du Vulkan Loader comme `VK_LAYER_PATH` et `VK_INSTANCE_LAYERS` restent des variables d'environnement propres au Loader et ne sont pas des réglages Framecap. Les raccourcis globaux X11 interceptent F9/F10/F11/F12 sans modificateur et peuvent empêcher l'application de recevoir ces touches; ils ne fonctionnent pas sous Wayland natif.
