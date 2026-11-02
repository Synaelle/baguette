#!/bin/bash
# Builds Baguette for Linux: Baguette-x86_64.AppImage (the window) and
# baguette-cli (command line, for scripts and automation), and copies them next
# to this file. The Linux counterpart of build.bat.
# "build.sh fast" is for trying changes: no LTO, parallel and incremental, and
# the version always ends in -dev. Releases use plain build.sh.
# Needs Rust (stable), tauri-cli 2 (cargo install tauri-cli --version "^2" --locked)
# and the WebKitGTK headers, e.g. on Debian/Ubuntu:
#   apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev patchelf file
set -euo pipefail
cd "$(dirname "$0")"

# version: the VERSION file (major.minor.micro, bumped by hand for each
# release), plus -<short commit>-dev for a build with uncommitted changes
BASEVER=$(tr -d '\r\n' < VERSION)
COMMIT=$(git rev-parse --short HEAD 2>/dev/null || true)
# line endings and file modes differ between a Windows checkout and WSL: not changes
DIRTY=$(git -c core.autocrlf=true -c core.filemode=false status --porcelain 2>/dev/null | head -1)
if [ -z "$COMMIT" ]; then
    BAGUETTE_VERSION="$BASEVER-dev"
elif [ -z "$DIRTY" ] && [ "${1:-}" != "fast" ]; then
    BAGUETTE_VERSION="$BASEVER"
else
    BAGUETTE_VERSION="$BASEVER-$COMMIT-dev"
fi
export BAGUETTE_VERSION
echo "version $BAGUETTE_VERSION"

# rust-toolchain.toml names Windows' MSVC toolchain: Linux builds use stable
export RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN:-stable}
# a checkout on a Windows drive (WSL's /mnt/c) builds much faster with its
# target folder on the Linux side, and keeps it apart from the Windows build's
if [ -z "${CARGO_TARGET_DIR:-}" ]; then
    case "$PWD" in
        /mnt/*) export CARGO_TARGET_DIR="$HOME/.cache/baguette-target" ;;
        *) export CARGO_TARGET_DIR="$PWD/target/linux" ;;
    esac
fi

PROFILE=release
[ "${1:-}" = "fast" ] && PROFILE=fast
cargo build --profile "$PROFILE" -p baguette-cli
cp -f "$CARGO_TARGET_DIR/$PROFILE/baguette-cli" baguette-cli

# the AppImage: Tauri's bundler (it fetches linuxdeploy the first time).
# linuxdeploy's own strip is too old for current distros' libraries (NO_STRIP),
# and WSL often has no FUSE to run the AppImage tools (extract-and-run instead).
export NO_STRIP=1 APPIMAGE_EXTRACT_AND_RUN=1
SEMVER=${BAGUETTE_VERSION#v}
CONFIG="{\"version\":\"$SEMVER\",\"bundle\":{\"active\":true,\"targets\":[\"appimage\"]}}"
TAURI_ARGS=(--bundles appimage --config "$CONFIG" --ci)
[ "$PROFILE" = "fast" ] && TAURI_ARGS+=(-- --profile fast)
(cd app && cargo tauri build "${TAURI_ARGS[@]}")
IMAGE=$(ls -t "$CARGO_TARGET_DIR/$PROFILE/bundle/appimage/"*.AppImage | head -1)
cp -f "$IMAGE" Baguette-x86_64.AppImage
chmod +x Baguette-x86_64.AppImage baguette-cli
echo "built Baguette-x86_64.AppImage baguette-cli v$BAGUETTE_VERSION"
