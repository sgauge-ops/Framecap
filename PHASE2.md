# Spécification technique — Phase 2 : Couche Vulkan et limiteur manuel

## 1. Objectif

Étendre la Layer passive de la phase 1 avec un limiteur manuel de FPS configurable. Le limiteur utilise les événements de présentation observés et conserve les métriques et l’overlay déjà livrés. La régulation reste manuelle : aucun ajustement automatique du plafond n’est effectué dans cette phase.

## 2. Préconditions et périmètre

La phase 1 est validée : chargement de la Layer, mesure par swapchain, overlay et gestion de la télémétrie GPU. La phase 2 ajoute :
- Un plafond initial de 60 FPS par défaut, surchargeable par configuration.
- Un ordonnanceur temporel par file de présentation.
- Une attente à échéance absolue avant la transmission de la présentation.
- Les commandes clavier F10 (diminuer) et F11 (augmenter) pour modifier le plafond en cours d’exécution.
- La commande F12 pour masquer ou afficher l’overlay sans interrompre la présentation ni la collecte des métriques.

Sont exclus : adaptation du plafond selon le jitter ou la charge GPU, interface de configuration graphique et prise en charge d’API graphiques autres que Vulkan.

## 3. Configuration

Clé principale de `framecap.toml` : `[framecap].target_fps`.
- Valeur par défaut : le limiteur démarre à 60 FPS.
- Valeur égale à `0` : limiteur désactivé.
- Valeur positive finie : active le plafond demandé à la place de la valeur par défaut.
- Valeur mal formée, non finie ou hors des bornes configurées : le limiteur est désactivé et une erreur est journalisée; l’application continue de fonctionner.

Les bornes des commandes clavier sont explicites, documentées et testées : minimum de 1 FPS et maximum de 240 FPS par défaut. Elles ne doivent pas être déduites silencieusement des capacités du moniteur. La synchronisation verticale et les limites de l’application peuvent produire un FPS inférieur au plafond; le limiteur ne promet pas d’atteindre la valeur cible.

La configuration est lue à l’initialisation de la Layer depuis `framecap.toml`. F10 et F11 modifient ensuite la cible à chaud, par pas de 5 FPS, en respectant les bornes. Si la cible est désactivée par la valeur `0`, F10 et F11 sont sans effet et ne réactivent pas le limiteur.

## 4. Sémantique du limiteur

Le limiteur s’applique une fois par appel `vkQueuePresentKHR` et par queue, même si l’appel contient plusieurs swapchains. Les mesures de frametime restent séparées par swapchain. Chaque queue possède son propre échéancier et son propre état; les appels concurrents ne doivent pas partager une échéance globale incorrecte.

Pour un plafond `F` en FPS, la période cible est `T = 1 / F`. L’échéance est calculée à partir de l’horodatage monotone de la présentation précédente. Si l’appel arrive avant l’échéance, le thread appelant attend jusqu’à celle-ci avant de transférer l’appel à la Layer aval. Si l’échéance est déjà dépassée, aucune attente n’est ajoutée.

L’échéancier suit une cadence absolue pour éviter la dérive cumulative. En cas de retard supérieur à une période, il est recalé sur l’heure courante plutôt que de provoquer une rafale de présentations de rattrapage. Un changement de cible ou la destruction de la queue réinitialise l’échéancier.

## 5. Attente et synchronisation

- Utiliser une horloge monotone et une attente absolue Linux (`clock_nanosleep` avec `TIMER_ABSTIME`, ou une abstraction équivalente testable).
- Ne pas effectuer de boucle d’attente active par défaut. Toute stratégie d’affinage de précision doit être optionnelle, bornée et mesurée.
- Ne pas conserver un verrou interne pendant l’attente ni pendant l’appel Vulkan aval.
- Ne pas consommer, remplacer ou réordonner les sémaphores de l’application. La temporisation se produit avant le transfert de `vkQueuePresentKHR` et ne change ni les paramètres ni le résultat transmis à l’implémentation aval.
- En cas d’erreur de l’horloge ou de l’ordonnanceur, poursuivre immédiatement la présentation et journaliser l’erreur avec limitation de fréquence.

La temporisation intervient après que l’application a appelé la présentation. Elle peut donc prolonger la durée pendant laquelle les ressources et sémaphores de l’application restent en attente; ce comportement doit être mesuré et documenté.

## 6. Architecture logicielle

- **Configuration :** parse une fois `framecap.toml` et expose une configuration immuable.
- **Ordonnanceur :** maintient, par queue, l’intervalle cible et la prochaine échéance.
- **Horloge :** abstraction injectée dans les tests pour simuler l’heure et les attentes sans temporisation réelle.
- **Point d’interception :** attend juste avant l’appel aval à `vkQueuePresentKHR`; le collecteur de la phase 1 continue d’enregistrer le même flux.
- **Journalisation :** erreurs et configuration invalide uniquement; aucune écriture de journal par frame en fonctionnement normal.

Le calcul de l’échéance et les mises à jour d’état doivent être protégés contre les appels concurrents. Le verrou est relâché avant toute attente. Une modification de cible par F10/F11 réinitialise les échéanciers concernés.

## 7. Cas particuliers

- `vkQueuePresentKHR` avec plusieurs swapchains : une seule attente par appel; les métriques restent individualisées.
- Destruction/recréation d’une swapchain : les statistiques de cette swapchain sont réinitialisées; l’échéancier par queue est conservé sauf destruction de la queue.
- Plusieurs queues : un échéancier indépendant par queue; ne pas prétendre imposer un plafond global cohérent entre plusieurs queues dans cette phase.
- VSync ou plafond propre au jeu : conserver la limite la plus contraignante observée; ne pas tenter de désactiver la synchronisation de l’application.
- Plafond désactivé ou erreur de configuration : aucun délai ajouté.
- Présentation déjà en retard : ne pas ajouter de délai négatif ni tenter de rattraper les images perdues.
- F10/F11 : appliquer un pas de 5 FPS sans dépasser les bornes; les pressions répétées aux bornes ne doivent ni déborder ni produire de cible invalide.
- F12 : alterner la visibilité de l’overlay tout en conservant la présentation et la collecte des métriques.

## 8. Critères d’acceptation

- Sans `target_fps` dans `framecap.toml`, la cible initiale est de 60 FPS.
- Avec `[framecap].target_fps = 0`, aucun délai n’est ajouté; une valeur positive valide remplace la cible initiale.
- Avec une cible valide et une application non limitée par ailleurs, l’intervalle moyen entre présentations converge vers la période demandée dans les limites de l’ordonnanceur Linux et de la synchronisation de l’application.
- F10 diminue la cible de 5 FPS et F11 l’augmente de 5 FPS, sans franchir les bornes de 1 et 240 FPS.
- Les retards ne s’accumulent pas en dérive et ne provoquent pas de rafales de rattrapage.
- Plusieurs queues sont indépendantes; les appels concurrents ne corrompent pas les échéances.
- Les paramètres Vulkan et résultats aval sont préservés; les tests couvrent les appels multi-swapchains et les erreurs.
- L’overlay reste fonctionnel et continue d’afficher les métriques mesurées.
- Le coût de l’attente et l’écart au plafond sont mesurés sur une durée suffisante et consignés; aucune précision absolue n’est promise indépendamment du noyau et de la charge système.

## 9. Tests requis

- Tests unitaires de calcul des périodes, échéances, retards, reconfiguration et remise à zéro avec horloge simulée.
- Tests de concurrence pour appels sur une même queue et sur des queues différentes.
- Tests de configuration : absente, zéro, valide, mal formée, non finie et hors bornes.
- Test d’intégration avec une application Vulkan connue, limite activée puis désactivée, et comparaison des intervalles mesurés.
- Vérification que les erreurs du limiteur ne bloquent jamais la transmission de la présentation.
- Tests des commandes F10/F11, des pas, des bornes, de la concurrence avec la présentation et de leur absence d’effet quand la cible est désactivée.
## 10. Commandes manuelles du plafond

Les commandes clavier du plafond sont intégrées après le limiteur de base et ses tests. F10 et F11 modifient la cible; F12 alterne la visibilité de l’overlay.

### 10.1 Augmenter la limite (+5 FPS) ([F11])

- **Action :** Augmenter le plafond de FPS actuel de +5
- **Comportement :** `target_fps` → `min(target_fps + 5, MAX_CAP)`, avec `MAX_CAP=240` par défaut.
- **Journalisation :** "[CONTROL] FPS Limit raised: X → Y fps"

### 10.2 Diminuer la limite (-5 FPS) ([F10])

- **Action :** Diminuer le plafond de FPS actuel de -5
- **Comportement :** `target_fps` → `max(target_fps - 5, MIN_FLOOR)`, avec `MIN_FLOOR=1` par défaut.
- **Journalisation :** "[CONTROL] FPS Limit lowered: X → Y fps"

### 10.3 Configuration des actions manuelles

Clés de `framecap.toml` :
- `input_enabled = true` : Activer les contrôles manuels (défaut si la layer est active)
- `hotkey_up = "F11"` : Touche pour augmenter le FPS
- `hotkey_down = "F10"` : Touche pour diminuer le FPS

Champs de configuration dans la layer :
```rust
pub struct ManualControlConfig {
    pub enabled: bool,              // Par défaut: true
    pub up_key: KeyCode,            // Par défaut: F11
    pub down_key: KeyCode,          // Par défaut: F10
    pub fps_step: f32,              // Par défaut: 5.0
    pub max_fps_cap: f32,           // Par défaut: 240.0
    pub min_fps_floor: f32,         // Par défaut: 1.0
}
```

### 10.4 Contraintes techniques

- Les commandes F10/F11 font partie du périmètre requis; une option de configuration peut toutefois les désactiver.
- Sous X11/XWayland, les événements sont reçus par un listener global séparé du chemin critique de présentation. Cette interception peut empêcher l’application de recevoir ces touches.
- Wayland natif ne fournit pas de raccourcis globaux via ce backend; le plafond reste fonctionnel et l’indisponibilité des commandes doit être journalisée. Un backend Wayland est hors de ce premier raccordement.
- L'état doit être synchronisé atomiquement pour éviter les data races entre thread d'input et thread de rendu.

### 10.5 Critères de validation pour les contrôles manuels

- Les touches [F10] et [F11] ajustent correctement le plafond avec le pas configuré.
- La journalisation indique chaque changement effectif de valeur, sans journalisation répétitive lorsque la borne est atteinte.
- Les bornes minimale et maximale sont respectées lors des changements.
- L'application reste fonctionnelle même si les événements clavier ne peuvent être interceptés (mode dégradé).

### 10.6 Étape finale : bascule d’overlay ([F12])

F12 alterne la visibilité de l’overlay au moyen du listener X11/XWayland et d’un état atomique partagé. Lorsque l’overlay est masqué, la présentation, le plafonnement et la collecte des métriques continuent; seules les commandes Vulkan de rendu de l’overlay sont omises. Les transitions sont journalisées. La validation au clavier physique et avec un jeu Steam/Proton reste à effectuer.
