# Cahier des Charges : Outil de Frame-Pacing Intelligent (Linux/Vulkan)

## 1. Objectif du Projet
L'objectif est de développer un outil capable d'ajuster dynamiquement le framerate d'une application Vulkan sous Linux pour garantir une fluidité visuelle optimale. Au lieu d'un simple limiteur de FPS statique, l'outil doit réagir en temps réel aux variations de performance pour minimiser la gigue (jitter) et les micro-saccades.

## 2. Approche Technique
- **Méthode de contrôle :** Plafonnement dynamique basé sur l'analyse de la variance (Dynamic Frame Capping based on Jitter Analysis).
- **Architecture :** Implémentation via une **Vulkan Layer**. Cette approche permet d'intercepter les appels aux fonctions de présentation (`vkQueuePresent`) sans modifier le code source des applications.
- **Langage cible :**
    - **Rust** (Recommandé) : Pour la sécurité mémoire et la performance, utilisant la crate `ash`.
    - **C++** : Alternative standard pour une compatibilité maximale avec les drivers et le SDK Vulkan.

## 3. Logique Algorithmique (Boucle de Rétroaction)
L'outil doit fonctionner selon un cycle d'analyse et d'ajustement :
1. **Mesure :** Capturer le temps de rendu réel de chaque frame (delta entre l'acquisition de l'image et sa présentation).
2. **Analyse :** Calculer l'écart-type (variance) des temps de frame sur une fenêtre glissante (ex: 60 dernières frames).
3. **Décision dynamique :**
    - **Saturation détectée :** Uniquement si la variance dépasse un seuil critique **et** si la charge GPU atteint un seuil proche de 100 %, abaisser progressivement le plafond de FPS pour stabiliser le flux. La variance élevée seule ne suffit pas à conclure à une saturation GPU.
    - **Stabilité détectée :** Si la variance est faible et constante **et** que la charge GPU reste sous le seuil proche de 100 %, augmenter progressivement le plafond de FPS pour maximiser la fluidité.
4. **Action :** Injecter un délai de sommeil (sleep) ou un blocage de thread précis juste avant l'appel à `vkQueuePresent` pour atteindre le plafond calculé.

## 4. Spécifications de Performance
- **Overhead :** L'impact sur le temps de rendu doit être négligeable (latence minimale).
- **Précision :** Utilisation des timers haute résolution du noyau Linux pour garantir une précision au microseconde.
- **Compatibilité :** Fonctionnement sur les serveurs d'affichage X11 et Wayland.

## 5. Roadmap de Développement
1. **Phase 1 (Observabilité) :** Mesure et affichage dans un overlay propre du frametime, du jitter (sur une fenêtre glissante) et de la charge GPU. La mesure de la charge GPU est nécessaire pour appliquer le critère de saturation de la phase 3; si aucune source compatible n'est exposée par le matériel ou le pilote, elle doit être indiquée comme indisponible. Voir [PHASE1.md](PHASE1.md).
2. **Phase 2 (Base) :** Création de la Vulkan Layer capable de limiter manuellement le FPS, en s'appuyant sur les mesures de la phase 1. Voir [PHASE2.md](PHASE2.md).
3. **Phase 3 (Intelligence) :** Développement de l'algorithme de régulation automatique à partir de l'analyse du jitter. Voir [PHASE3.md](PHASE3.md).
4. **Phase 4 (Validation) :** vérification de la compatibilité X11/Wayland, de la précision des mesures et de l'overhead de la layer et de l'overlay, améliorer la logique de ramp-up et ramp-down
5. **Phase 5 (Industrialisation)**: Amélioration de l'affichage de l'overlay, documentation des paramètres du fichier de configuration, traduire les documents en anglais, commenter le code, publier sur GitHub


## 6. Exigences de Développement Agentique
1. Toutes les avancées d'implémentation feront l'objet d'une mise-à-jour du fichier PROGRESS.md, qui contiendra, par phase, une synthèse de l'avancée réalisée ainsi que la date/heure  correspondante.
2. Les agents pourront consulter ce fichier pour déterminer ce qu'il reste à réaliser comparatvement aux spécifications détaillées présentées dans les différents fichiers PHASE?.md
3. A chaque avancée, l'agent enregistrera dans README.md les différentes instructions pour que je puisse lancer par moi même, dans une console, le programme avec ses paramètres adéquats. L'ensemble des paramètres y seront documentés.
4. Le code doit être documenté, avec à minima une description succinte de chaque fonction
