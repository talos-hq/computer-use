#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
binary=${1:?missing binary}
app=${2:?missing app path}
version=${3:?missing version}
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 0644 "$root/rust/scripts/ComputerUseBundle/Contents/Info.plist" "$app/Contents/Info.plist"
install -m 0644 "$root/rust/scripts/ComputerUseBundle/Contents/Resources/AppIcon.icns" "$app/Contents/Resources/"
install -m 0755 "$binary" "$app/Contents/MacOS/computer-use"
for name in cua-driver cua-cursor-theme cua-perception; do
    ln -s computer-use "$app/Contents/MacOS/$name"
done
for key in CFBundleShortVersionString CFBundleVersion; do
    plutil -replace "$key" -string "$version" "$app/Contents/Info.plist"
done
