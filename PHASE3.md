# Phase 3 — Régulation dynamique (preuve de concept)

## Objectif

Permettre à l'utilisateur d'activer ou de désactiver l'ajustement automatique
du plafond FPS avec les raccourcis globaux, en conservant le limiteur existant
et sa cible initiale. La régulation est expérimentale; l'algorithme
`gpu_threshold` s'appuie sur le FPS, le « 10% low » et la charge GPU, tandis
que `vrm_temp` surveille la température GPU VRM. Elle ne prétend pas
identifier la cause de toutes les baisses de framerate.

## Spécification de référence

Les règles de décision, les mesures et leurs conditions de validité, les
bornes, les paramètres `framecap.toml` employés ainsi que les tests attendus
sont isolés dans la [spécification AUTO-MODE](specs/AUTO-MODE.md). Ce document
est la référence à modifier pour faire évoluer le mode automatique. Tout
changement de cette spécification doit être répercuté dans la logique, ses
tests et les documents concernés.

## Contexte d'implémentation

- Le cycle F9 suit manuel → chaque algorithme AUTO dans `algorithm_order` →
  OFF, puis restaure la dernière cible en manuel. F10/F11 ajustent le CAP en
  manuel et l'hystérésis active en points de pourcentage en mode AUTO.
- Le FPS affiché, le « 10% low » et le jitter partagent les mêmes échantillons
  dans la fenêtre glissante `gpu_smoothing_window_ms`; ils restent ensemble en
  `COLLECT` au démarrage et après changement de CAP jusqu'à ce qu'un intervalle
  soit disponible et que `gpu_poll_rate_ms` soit écoulé. La charge GPU utilise
  son propre lissage temporel de même durée. Le « 10% low » est le 10e
  percentile des FPS de présentation de cette fenêtre. La décision utilise le
  « 10% low » et les métriques du régulateur, et non le FPS moyen affiché.
- En mode auto, les métriques sont évaluées après actualisation, au plus une
  fois par `gpu_poll_rate_ms` et par périphérique Vulkan; plusieurs swapchains
  d'une même présentation ne multiplient pas les évaluations.
- Le GPU AMD est la seule source de télémétrie actuellement implémentée. Les
  raccourcis nécessitent X11/XWayland; le mode automatique ne peut pas être
  activé au clavier sous Wayland natif.
- Les mesures GPU ou VRM absentes/périmées, les métriques de présentation
  incomplètes et une cible désactivée ne déclenchent pas d'adaptation.

## Validation et limites

Les tests unitaires de `src/layer/auto.rs` couvrent les règles GPU-threshold et
VRM, la zone d'hystérésis thermique, les mesures périmées, les seuils GPU et
jitter, les bornes ainsi que le réglage interactif de l'hystérésis. La
validation prolongée sur plusieurs classes de GPU et de jeux reste à
effectuer; les règles demeurent expérimentales et ne sont pas universelles.

Les tests et le formatage du projet doivent passer après toute modification
de la logique. Pour les règles détaillées et la liste à jour des paramètres,
consulter la [spécification AUTO-MODE](specs/AUTO-MODE.md), plutôt que de
dupliquer ici les seuils et valeurs de configuration.
