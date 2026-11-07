# Index documentaire de Framecap

Ce fichier oriente un agent qui découvre le projet. Il inventorie les fichiers Markdown du dépôt, résume leur rôle et propose un ordre de lecture. Les spécifications décrivent les objectifs attendus; pour connaître l'état réellement implémenté, comparer avec `PROGRESS.md` et le code.

## Parcours conseillé

1. `README.md` — comprendre ce que fait le projet et comment le construire, le lancer et le configurer.
2. `INSTALL.md` — installer depuis une archive GitHub, compiler et activer Framecap pour un jeu Steam.
3. `IDEA.md` et `REQUIREMENT.md` — saisir l'intention d'origine, les objectifs et les exigences globales.
4. `PROGRESS.md` — distinguer ce qui est réalisé de ce qui reste à faire.
5. `PHASE1.md` à `PHASE3.md`, puis `specs/AUTO-MODE.md` et `specs/OVERLAY.md` — consulter les spécifications techniques des fonctionnalités déjà implémentées. Ces deux documents sont les références modifiables du mode automatique et de l’overlay.
6. `PHASE4.md`, puis `PHASE5.md` — lire les plans de validation et d’industrialisation avant de démarrer ces phases.

## Index des fichiers à lire

| [`INDEX.md`](INDEX.md) | Présent index documentaire : inventaire des Markdown et guide de découverte du projet. |
| [`README.md`](README.md) | Guide pratique du dépôt : prérequis, build, lancement avec `vkcube`, activation pour un jeu Steam, tests et variables de configuration. C'est le point de départ pour exécuter le logiciel. |
| [`INSTALL.md`](INSTALL.md) | Procédure d'installation pour les utilisateurs : téléchargement depuis GitHub, compilation, configuration, activation Steam, vérification et désinstallation. |
| [`IDEA.md`](IDEA.md) | Énoncé très concis de l'idée du projet : plafonnement adaptatif du framerate Vulkan à partir de la gigue du frametime. |
| [`REQUIREMENT.md`](REQUIREMENT.md) | Cahier des charges global : objectif, approche de Vulkan Layer, logique de régulation envisagée, contraintes de performance, feuille de route et attentes de documentation pour les agents. |
| [`PROGRESS.md`](PROGRESS.md) | Suivi de l'avancement par phase : fonctionnalités réalisées, validations, limites connues et tâches restantes. À consulter avant de planifier ou modifier une fonctionnalité. |
| [`PHASE1.md`](PHASE1.md) | Spécification de la phase d'observabilité : collecte du frametime et du jitter, télémétrie GPU, overlay, architecture, critères d'acceptation et tests. |
| [`PHASE2.md`](PHASE2.md) | Spécification du limiteur manuel : plafond initial, échéancier par queue, synchronisation, configuration, commandes F10/F11, bascule F12, critères et tests. |
| [`PHASE3.md`](PHASE3.md) | Contexte d'implémentation, validation et limites de la phase de régulation dynamique; renvoie vers la spécification détaillée AUTO-MODE. |
| [`PHASE4.md`](PHASE4.md) | Plan de validation multi-environnements : compatibilité jeux/drivers/WSI, overhead, stabilité automatique et comparaison des taux de polling GPU. |
| [`PHASE5.md`](PHASE5.md) | Plan d'industrialisation : configuration TOML structurée, overlay, documentation bilingue, documentation du code et préparation de la publication GitHub. |
| [`specs/AUTO-MODE.md`](specs/AUTO-MODE.md) | Spécification de référence et modifiable du contrôle automatique : règles, seuils, paramètres TOML utilisés et exigences de tests/documentation. |
| [`specs/OVERLAY.md`](specs/OVERLAY.md) | Spécification de référence de la présentation de l’overlay : contenu, alignement, retours visuels et opacité du fond configurable. |

## Repères pour contribuer

- Ne pas considérer une fonctionnalité décrite dans une spécification comme déjà implémentée : vérifier `PROGRESS.md` et les sources.
- Après un changement fonctionnel, mettre à jour le suivi et les instructions de lancement/configuration dans `README.md` lorsque c'est pertinent.
- Les fichiers `PHASE*.md` sont des spécifications de référence; maintenir les critères et les limites cohérents avec l'implémentation.
- Pour toute modification de `specs/AUTO-MODE.md`, ajuster la logique et ses tests, puis synchroniser les autres documents concernés; ce fichier est la référence pour le comportement du mode automatique.
- Pour toute modification de `specs/OVERLAY.md`, ajuster l’implémentation et les tests concernés, puis synchroniser les autres documents concernés; ce fichier est la référence pour la présentation de l’overlay.

## Index des fichiers source

| Fichier | Contenu et rôle |
| --- | --- |
| [`framecap.toml`](framecap.toml) | Fichier de configuration principal ; définit les paramètres du limiteur, de l'overlay et du mode automatique. |
| [`src/lib.rs`](src/lib.rs) | Point d'entrée du crate ; définit les modules principaux et les structures de données de base (MetricsSnapshot, FramecapLayer). |
| [`src/layer/mod.rs`](src/layer/mod.rs) | Module racine de la couche Framecap ; expose les sous-modules (auto, config, diagnostics, etc.). |
| [`src/layer/lib.rs`](src/layer/lib.rs) | Logique principale de la couche Framecap ; contient la structure `FramecapLayer` et gère le cycle de vie des métriques et de l'overlay. |
| [`src/layer/auto.rs`](src/layer/auto.rs) | Implémentation du mode automatique ; contient les règles de régulation, les seuils et la logique d'ajustement dynamique du framerate. |
| [`src/layer/config.rs`](src/layer/config.rs) | Gestion de la configuration ; charge et parse le fichier `framecap.toml` en structures Rust. |
| [`src/layer/diagnostics.rs`](src/layer/diagnostics.rs) | Système de journalisation diagnostique ; écrit les logs d'initialisation et les erreurs dans un fichier local. |
| [`src/layer/input_x11.rs`](src/layer/input_x11.rs) | Gestion des entrées utilisateur sous X11 ; gère les raccourcis clavier (F9-F12). |
| [`src/layer/limiter.rs`](src/layer/limiter.rs) | Logique du limiteur de framerate ; gère le plafond manuel et la synchronisation des queues. |
| [`src/layer/metrics.rs`](src/layer/metrics.rs) | Agrégation des métriques ; calcule les moyennes mobiles, le jitter et les statistiques GPU. |
| [`src/layer/overlay.rs`](src/layer/overlay.rs) | Gestion de l'affichage de l'overlay ; définit le contenu, le rendu et la visibilité de l'interface utilisateur. |
| [`src/layer/renderer.rs`](src/layer/renderer.rs) | Moteur de rendu Vulkan ; gère la création des surfaces, des buffers et le dessin des éléments graphiques. |
| [`src/layer/telemetry.rs`](src/layer/telemetry.rs) | Collecte de la télémétrie GPU ; fournit les données de charge et d'utilisation du matériel. |
| [`src/layer/vulkan_interface.rs`](src/layer/vulkan_interface.rs) | Abstraction de l'interface Vulkan ; gère les appels bas niveau et la communication avec le driver. |
| [`src/layer/shaders/overlay_background.frag`](src/layer/shaders/overlay_background.frag) | Shader de fragment pour l'arrière-plan de l'overlay (source humaine). |
| [`src/layer/shaders/overlay_background.vert`](src/layer/shaders/overlay_background.vert) | Shader de sommet pour l'arrière-plan de l'overlay (source humaine). |
| [`src/layer/shaders/overlay_background.frag.spv`](src/layer/shaders/overlay_background.frag.spv) | Shader de fragment compilé (SPIR-V) pour l'arrière-plan de l'overlay. |
| [`src/layer/shaders/overlay_background.vert.spv`](src/layer/shaders/overlay_background.vert.spv) | Shader de sommet compilé (SPIR-V) pour l'arrière-plan de l'overlay. |
