# Spécification du mode automatique

## Rôle et autorité

Ce document est la référence du régulateur automatique du CAP FPS. Toute
évolution doit être accompagnée des changements de code, de tests et des mises
à jour correspondantes dans `README.md`, `PHASE3.md`, `PROGRESS.md`,
`framecap.toml` et `specs/OVERLAY.md`.

## Algorithmes configurables

Les algorithmes prédéfinis sont définis sous
`[auto-mode.algorithms.<nom>]`. `algorithm_order` indique l'ordre du cycle F9
et l'indice affiché dans l'overlay. Tous les algorithmes utilisent le même
schéma et doivent fournir les paramètres suivants :

| Clé | Rôle |
| --- | --- |
| `metric` | Mesure supervisée. |
| `target` | Référence de mesure ou cible numérique dans la même unité. |
| `hysteresis_percent` | Marge en pourcentage autour de la cible. |
| `auto_increase_interval_ms` / `auto_decrease_interval_ms` | Délai minimal entre deux changements effectifs dans le sens indiqué. |
| `auto_increase_step_fps` / `auto_decrease_step_fps` | Variation du CAP lorsqu'une décision est prise. |
| `min_fps` / `max_fps` | Bornes du CAP de cet algorithme. |
| `measurement_expiry_ms` | Âge maximal de la mesure supervisée. |

Deux couples métrique/cible sont pris en charge :

- `gpu_threshold`: `metric = "fps"` et `target = "low_10_fps"`. Le « 10% low »
  est la référence FPS à maintenir par rapport au CAP, tandis que les seuils
  GPU/jitter gardent leur rôle de critères d'autorisation de hausse/baisse.
- `vrm_temp`: `metric = "gpu_vrm_temp_c"` et une cible numérique en °C.
  La température GPU VRM est lissée comme les autres capteurs matériels.

Les autres couples sont refusés et l'algorithme concerné reprend ses valeurs
intégrées. Les seuils communs `gpu_increase_limit`,
`gpu_decrease_limit` et `frame_jitter_limit` ne sont utilisés que par
`gpu_threshold`.

## Modes et raccourcis

Au démarrage, une valeur `[framecap].target_fps` positive active le mode
manuel; zéro démarre en OFF. F9 fait parcourir **manuel → AUTO 1 → AUTO 2 →
... → OFF → manuel**. Depuis un démarrage en OFF, F9 rétablit d'abord la
dernière cible en mode manuel. OFF désactive le CAP sans oublier sa dernière
valeur. Les règles automatiques ne changent pas le CAP en mode manuel ou OFF.

En mode AUTO, l'overlay affiche `AUTO (n)`, où `n` est l'indice à partir de 1
dans `algorithm_order`. F10 diminue et F11 augmente l'hystérésis active d'un
point de pourcentage par appui, dans la plage 0–100 %. Cette valeur est
immédiatement utilisée par le régulateur et affichée comme `HYS`. En mode
manuel, F10/F11 diminuent/augmentent le CAP selon
`[manual-mode].step_fps`. En OFF, ils ne changent pas de paramètre.

Les raccourcis globaux nécessitent X11/XWayland.

## Mesures et fréquence d'évaluation

Les métriques de présentation sont actualisées à chaque présentation. Une
évaluation automatique a lieu au plus une fois par
`[framecap].gpu_poll_rate_ms` par périphérique Vulkan. La première évaluation
est immédiate après l'entrée en AUTO. Plusieurs swapchains présentées dans le
même appel ne déclenchent pas plusieurs évaluations.

Le FPS moyen, le « 10% low » et le jitter utilisent les mêmes échantillons de
présentation, limités à `[overlay].gpu_smoothing_window_ms`. Le « 10% low »
est le 10e percentile des FPS de présentation. Après un changement de CAP,
ces historiques sont réinitialisés ensemble et demeurent indisponibles jusqu'à
la collecte d'un intervalle valide et l'écoulement de la période minimale
`gpu_poll_rate_ms`. La charge GPU et les capteurs matériels sont lissés
séparément.

Un algorithme ne prend aucune décision si sa cible CAP est désactivée, si ses
mesures requises sont absentes, non finies ou périmées, ou si les historiques
de présentation ne sont pas encore disponibles. Une mesure horodatée dans le
futur est également rejetée. L'expiration de chaque algorithme est
indépendante; l'expiration globale du fournisseur GPU garde les échantillons
disponibles jusqu'à la plus grande expiration configurée.

## Règles d'ajustement

Les décisions suivantes sont prises après validation des mesures. Le CAP
résultant est arrondi à l'entier inférieur et borné par `min_fps` et
`max_fps`. Une borne atteinte ne provoque pas de changement. Chaque algorithme
possède ses propres temporisations de hausse et de baisse; elles sont
comptées depuis le dernier changement effectif de même sens. Une tentative
sans changement ne démarre ni ne réinitialise de délai.

### `gpu_threshold`

La marge en FPS est `CAP courant × hysteresis_percent / 100`.

1. **Hausse** du CAP de `auto_increase_step_fps` si la charge GPU lissée est
   strictement inférieure à `gpu_increase_limit` et si
   `CAP courant - low_10_fps <= marge`. La différence n'est pas absolue.
2. Sinon, **baisse** du CAP de `auto_decrease_step_fps` si
   `low_10_fps < fps_observé - marge` et si la charge GPU est supérieure ou
   égale à `gpu_decrease_limit`, ou si le jitter dépasse `frame_jitter_limit`.
3. Dans les autres cas, le CAP reste inchangé.

`gpu_increase_limit` et `gpu_decrease_limit` sont des seuils indépendants,
finis et compris entre 0 et 100 %; leurs valeurs intégrées sont 98 et 90 %.
`frame_jitter_limit` est fini et compris entre 0 et 500 ms; sa valeur intégrée
est 2 ms. La télémétrie GPU doit avoir le statut `Available`, une valeur de
0–100 % et un âge inférieur à `measurement_expiry_ms`.

### `vrm_temp`

Soit `T` la cible en °C et `H` l'hystérésis en pourcentage. La zone neutre est
`[T - T×H/100, T + T×H/100]`, bornes incluses. Pour la configuration par
défaut (`T = 100 °C`, `H = 5 %`), elle s'étend de 95 à 105 °C :

- si la température est strictement sous la borne basse, augmenter le CAP;
- si elle est strictement au-dessus de la borne haute, diminuer le CAP;
- dans la zone neutre, ne pas modifier le CAP.

Si la lecture GPU VRM est absente ou plus ancienne que
`measurement_expiry_ms`, aucun ajustement n'est appliqué.

## Configuration et valeurs intégrées

Les deux algorithmes sont fournis par défaut, dans l'ordre `gpu_threshold`,
puis `vrm_temp`. Le fichier `framecap.toml` est l'exemple de configuration
utilisateur; les valeurs indiquées peuvent différer des valeurs intégrées.

| Paramètre | `gpu_threshold` intégré | `vrm_temp` intégré |
| --- | ---: | ---: |
| `metric` / `target` | `fps` / `low_10_fps` | `gpu_vrm_temp_c` / `100.0` |
| `hysteresis_percent` | 3 % | 5 % |
| `auto_increase_interval_ms` | 5000 ms | 10000 ms |
| `auto_decrease_interval_ms` | 5000 ms | 5000 ms |
| `auto_increase_step_fps` / `auto_decrease_step_fps` | 5 / 5 FPS | 5 / 5 FPS |
| `min_fps` / `max_fps` | 1 / 240 FPS | 1 / 240 FPS |
| `measurement_expiry_ms` | 1000 ms | 1000 ms |

Les intervalles doivent être positifs; les pas doivent être finis et positifs;
les bornes doivent être comprises entre 1 et 240 FPS et ordonnées;
l'hystérésis doit être finie et comprise entre 0 et 100 %. Une valeur invalide
reprend le défaut du paramètre concerné.

## Vérification attendue

Les tests couvrent les deux règles, la zone thermique inclusive et les
mesures thermiques absentes/périmées, les seuils GPU et jitter, le réglage
F10/F11 et ses bornes, le cycle F9 avec tous les algorithmes, les temporisations
directionnelles, les bornes FPS, la disponibilité des mesures, le schéma TOML
commun et l'affichage de l'indice/hystérésis dans l'overlay.
