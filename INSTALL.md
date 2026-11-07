# Installer Framecap

Framecap est une couche Vulkan pour Linux. Il n'y a pas d'application à lancer
séparément : le Vulkan Loader la charge pour chaque jeu auquel on l'active.
Cette procédure installe les sources dans votre dossier personnel, compile la
couche et l'active pour un jeu Steam seulement.

## Prérequis

- Linux 64 bits, une session graphique et un pilote Vulkan fonctionnel.
- Rust et Cargo. Si nécessaire, installez-les avec [rustup](https://rustup.rs/),
  puis ouvrez un nouveau terminal.
- `curl` et `tar` pour télécharger l'archive.
- `vkcube` est facultatif; il permet de vérifier rapidement la couche.

Exemple pour Debian ou Ubuntu :

```sh
sudo apt update
sudo apt install curl build-essential libvulkan1 mesa-vulkan-drivers vulkan-tools
```

Le nom du paquet Vulkan varie selon la distribution. Installez le Vulkan Loader
et le pilote correspondant à votre GPU. `vulkan-tools` fournit `vkcube` pour le
test facultatif.

## Télécharger et compiler

Sur la page GitHub du projet, remplacez `OWNER/REPOSITORY` par le propriétaire
et le nom réels du dépôt. Si sa branche par défaut n'est pas `main`, adaptez
aussi le nom de branche dans l'URL.

```sh
set -o pipefail
INSTALL_DIR="$HOME/.local/opt/framecap"
mkdir -p "$INSTALL_DIR"
curl -fL "https://github.com/OWNER/REPOSITORY/archive/refs/heads/main.tar.gz" \
  | tar -xz --strip-components=1 -C "$INSTALL_DIR"

cd "$INSTALL_DIR"
cargo build
```

La bibliothèque produite est `target/debug/libframecap.so`. Conservez ensemble
le dossier d'installation, `target/` et `layer/` : le manifeste Vulkan de
Framecap référence la bibliothèque par rapport à son emplacement. Après une
mise à jour des sources, relancez `cargo build`.

## Configurer

Copiez le fichier d'exemple dans le dossier utilisateur puis modifiez-le à votre
convenance :

```sh
mkdir -p "$HOME/.config"
cp "$INSTALL_DIR/framecap.toml" "$HOME/.config/framecap.toml"
```

Framecap cherche `framecap.toml` dans le dossier courant du processus, puis dans
`$XDG_CONFIG_HOME` s'il est défini, puis dans `~/.config`. Un fichier présent
dans le dossier du jeu ou un autre emplacement prioritaire peut donc remplacer
la configuration utilisateur.

Au premier lancement, le limiteur démarre à la cible configurée (60 FPS par
défaut); l'overlay est affiché par défaut. Les raccourcis globaux F9 à F12
nécessitent X11/XWayland; ils ne sont pas disponibles sous Wayland natif.

## Activer pour un jeu Steam

Dans Steam, ouvrez **Propriétés → Général → Options de lancement** du jeu et
ajoutez :

```sh
VK_LAYER_PATH="$HOME/.local/opt/framecap/layer" VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability %command%
```

Steam remplace `%command%` par la commande du jeu. Ces options n'activent
Framecap que pour ce jeu. Pour désactiver ou désinstaller la couche, retirez ces
options de lancement.

## Vérifier l'installation

Avec `vkcube` installé et une session graphique disponible, testez la couche
depuis son dossier d'installation :

```sh
cd "$HOME/.local/opt/framecap"
VK_LAYER_PATH="$PWD/layer" \
  VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability \
  vkcube --c 300
```

Le journal doit indiquer que la couche Vulkan Framecap a été chargée. Par
défaut, il est écrit dans
`~/.local/state/framecap/framecap.log` (ou sous `$XDG_STATE_HOME` si défini).
Pour diagnostiquer un jeu Steam, ajoutez temporairement
`PROTON_LOG=1 PROTON_LOG_DIR=/tmp` à ses options de lancement et consultez
également le journal Framecap.

## Désinstaller

1. Retirez les variables `VK_LAYER_PATH` et `VK_INSTANCE_LAYERS` des options de
   lancement Steam.
2. Supprimez le dossier installé si vous n'en avez plus besoin :

   ```sh
   rm -r "$HOME/.local/opt/framecap"
   ```

3. Supprimez éventuellement la configuration `~/.config/framecap.toml` et le
   journal `~/.local/state/framecap/framecap.log` si vous ne souhaitez pas les
   conserver.

Framecap n'installe pas de manifeste Vulkan global et ne modifie pas les
paramètres Vulkan des autres jeux.
