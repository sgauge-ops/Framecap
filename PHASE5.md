# Phase 5 — Industrialisation et publication

## Objectif

Rendre Framecap configurable, lisible, documenté et distribuable sans modifier
les comportements validés en phase 4. La phase inclut la configuration TOML
structurée, l’amélioration visuelle de l’overlay, la documentation en anglais,
la documentation du code et la préparation de la publication GitHub.

## État actuel

L’implémentation des points 1 (configuration TOML) et 2 (overlay) est réalisée
dans la branche de travail. La validation visuelle manuelle sur plusieurs jeux,
résolutions et fonds reste à confirmer dans le cadre des essais de phase 4.
Le suivi des changements et de leur validation se trouve dans `PROGRESS.md`.
Les points 3 à 5 restent à réaliser.

## État de départ et éléments déjà implémentés

Cette section décrit la situation avant la réalisation des points 1 et 2 :

- Tous les paramètres utilisateur résidaient dans `[framecap]`,
  sauf `log_path` dans `[diagnostics]`.
- Les quatre raccourcis F9/F10/F11/F12 étaient déjà configurables via
  `hotkey_auto`, `hotkey_down`, `hotkey_up` et `hotkey_toggle`.
- Les pas de hausse et de baisse automatiques étaient déjà distincts et
  configurables via `auto_increase_step_fps` et `auto_decrease_step_fps`.
- Le graphique de jitter était normalisé par le maximum observé.
- Le dépôt n’a pas actuellement de configuration Git visible dans
  l’environnement de travail; la destination GitHub devra être confirmée avant
  toute publication.

Les réglages déjà présents doivent être déplacés/documentés et testés, pas
réimplémentés sous un autre nom sans raison. Les valeurs du `framecap.toml`
actuel (qui diffèrent parfois des valeurs par défaut intégrées) doivent être
traitées comme des choix locaux d’exemple, et non comme les valeurs par défaut
du produit.

## Travaux

### 1. Regrouper et documenter la configuration

Faire évoluer `framecap.toml` vers les sections suivantes :

| Section | Paramètres à regrouper |
| --- | --- |
| `[framecap]` | `target_fps`, `window_size`, `gpu_poll_rate_ms` |
| `[key-bindings]` | `input_enabled`, `hotkey_auto`, `hotkey_down`, `hotkey_up`, `hotkey_toggle` |
| `[overlay]` | `gpu_smoothing_window_ms`, `jitter_graph_startup` (0 désactivé par défaut, 1 activé), `jitter_graph_width_px`, `jitter_graph_height_px`, `jitter_graph_window_ms` et les réglages visuels retenus |
| `[auto-mode]` | `algorithm_order`, seuils GPU communs et seuil de jitter; chaque `[auto-mode.algorithms.<nom>]` configure la métrique, la cible, l'hystérésis, les délais directionnels, les pas FPS, les bornes FPS et l'expiration de mesure |
| `[manual-mode]` | Pas de réglage manuel et bornes minimale/maximale, en conservant les valeurs actuelles par défaut (5, 1 et 240 FPS) |
| `[diagnostics]` | `log_path` |

Chaque paramètre exposé doit avoir un commentaire en anglais dans le fichier
d’exemple : unité, rôle, valeur par défaut intégrée, bornes et effet d’une
valeur invalide. Maintenir les règles existantes : raccourcis distincts,
valeurs finies, fenêtres et dimensions valides, et erreur journalisée sans
arrêter le jeu.

Ajouter des limites FPS minimale et maximale configurables pour le régulateur
automatique, avec les bornes actuelles (1 et 240 FPS) comme valeurs par défaut.
Les changements automatiques doivent être bornés par ces réglages; valider que
la borne minimale ne dépasse pas la borne maximale et que la cible initiale
reste gérée comme aujourd’hui. Garder les limites manuelles distinctes si leur
configuration est exposée dans `[manual-mode]`.

Avant le déplacement, auditer les clés `jitter_high`, `jitter_low` et
`gpu_saturation` : elles sont présentes dans la configuration, mais leur effet
sur le comportement doit être vérifié dans le code. Ne pas laisser de
paramètre documenté comme actif s’il n’influence pas réellement Framecap.
Décider et tester s’il faut le raccorder au comportement prévu ou le retirer
du schéma public, de l’exemple et des journaux.

Préserver une migration explicite depuis le format historique où ces options
se trouvent dans `[framecap]`. Si ancienne et nouvelle clé coexistent, refuser
le conflit avec une erreur qui nomme les clés; ne pas choisir silencieusement
une valeur. Documenter la période ou la méthode de migration et conserver les
valeurs par défaut intégrées actuelles.

Mettre à jour en cohérence le parseur, la validation, les diagnostics, le
fichier d’exemple, `README.md` et `PHASE3.md`. Tester la configuration par
défaut, la nouvelle structure, la migration historique, les conflits, les
bornes, les clés inconnues, les raccourcis et les erreurs TOML.

### 2. Améliorer la lisibilité et les retours de l’overlay

- Aligner les libellés et les valeurs dans des colonnes stables; conserver une
  position et une présentation cohérentes entre les images.
- Vérifier le clipping, les dimensions de swapchain réduites et les valeurs
  d’état « collecte », « périmée », « non prise en charge » et « erreur ».
- Utiliser des libellés homogènes et des unités explicites pour FPS,
  frametime, jitter, 10% low et charge GPU.
- Mettre en évidence pendant une seconde le raccourci venant d’être pressé et
  les champs CAP/TOL effectivement modifiés, sans ajouter de rendu ni
  d’attente bloquante au chemin de présentation. Les retours indépendants ne
  doivent pas se masquer mutuellement ni cacher les mesures.
- Fixer l’échelle verticale du graphique de jitter à 0–50 ms, indépendante des
  valeurs de la fenêtre; saturer/cliper les valeurs supérieures et indiquer
  clairement l’échelle.

Le jitter reste une durée : 50 ms correspondent à une période de 20 Hz, mais
le graphique ne doit pas présenter une mesure de jitter comme un FPS. Vérifier
les positions, les couleurs, la lisibilité et les transitions sur plusieurs
résolutions et fonds de jeu.

Ajouter ou adapter des tests de rendu pour l’alignement, les limites de
l’échelle, le clipping, les états de métriques et la mise en évidence des
actions. Compléter par une validation visuelle dans les environnements
disponibles.

### 3. Documenter le code en anglais

Rédiger en anglais les commentaires/doc-comments nécessaires pour expliquer
les API internes importantes, les invariants de synchronisation Vulkan, la
durée de vie des ressources, les états partagés entre threads et les choix
non évidents de régulation. Préférer des commentaires ciblés et exacts aux
commentaires évidents ou répétés; ne pas annoter chaque ligne.

Vérifier que les commentaires ne contredisent pas le comportement réel et que
la documentation générée ou les vérifications de documentation passent.

### 4. Fournir les documents anglais correspondants

Créer des versions anglaises séparées, suffixées `_EN.md`, pour chaque
document Markdown de référence : `README`, `INDEX`, `IDEA`, `REQUIREMENT`,
`PROGRESS` et `PHASE1` à `PHASE5`. Garder la version française comme source
de référence durant cette phase et synchroniser les modifications qui touchent
les deux langues.

Mettre à jour l’index bilingue, vérifier les liens relatifs et s’assurer que
les statuts, commandes, noms de paramètres et critères techniques sont
équivalents dans les deux langues. Les journaux, fichiers générés et notes
temporaires ne sont pas des documents à traduire.

### 5. Préparer et publier sur GitHub

Avant publication, vérifier les fichiers inclus, retirer du périmètre suivi les
binaires `target/`, journaux d’exécution et données locales, ajouter ou
confirmer les règles d’exclusion adaptées, vérifier les dépendances et la
licence du projet, et fournir des instructions de build/test reproductibles.
Ajouter une validation automatisée CI adaptée au projet si elle n’existe pas.

Confirmer avec le propriétaire le dépôt distant et sa visibilité. Ne pas
inventer une URL, créer un dépôt public ni publier de code tant que la
destination et l’autorisation ne sont pas établies. Une fois confirmées,
publier le code et les documents, vérifier le contenu distant et consigner
l’URL, la révision et les instructions de contribution dans `README.md`.

## Critères de sortie

- Le fichier TOML suit les six sections demandées et chaque option publique est
  commentée en anglais avec son unité, défaut et contraintes.
- Les anciens fichiers de configuration ont une migration testée; un conflit
  d’ancienne et nouvelle clé est signalé sans ambiguïté.
- Les réglages exposés ont un effet réel et testé, ou sont retirés et expliqués.
- Les pas automatiques montée/descente demeurent indépendamment configurables;
  les bornes minimale/maximale du mode automatique sont configurables et
  validées;
  les quatre touches F9/F10/F11/F12 restent configurables et sans collision.
- L’overlay a des colonnes stables, des unités cohérentes, une échelle de
  jitter fixe 0–50 ms et des retours visuels testés.
- Les tests, le formatage, le build et les vérifications de documentation
  passent; la validation visuelle est rapportée dans `PROGRESS.md`.
- Chaque document Markdown de référence a son équivalent anglais `_EN.md`,
  avec des liens vérifiés et un contenu cohérent.
- La préparation GitHub est terminée; la publication n’est marquée comme
  achevée qu’après confirmation de la destination et vérification du dépôt
  distant.
