#!/bin/sh
# Builds a single-file bundle at flatpak-out/vagalume-rgb.flatpak. Needs cargo,
# flatpak-builder and the Flathub remote; the SDK is fetched on first run.
set -eu
cd "$(dirname "$0")/../.."
gnome="${GNOME_VERSION:-51}"
out=flatpak-out

cargo vendor --locked vendor >/dev/null
mkdir -p "$out"
sed "s/@GNOME@/$gnome/" packaging/flatpak/org.vagalumergb.VagalumeRgb.yml \
    > packaging/flatpak/.manifest.yml

flatpak-builder --user --force-clean --install-deps-from=flathub \
    --repo="$out/repo" "$out/build" packaging/flatpak/.manifest.yml
flatpak build-bundle --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo \
    "$out/repo" "$out/vagalume-rgb.flatpak" org.vagalumergb.VagalumeRgb
