#!/usr/bin/env bash
# Build the Linux x86_64 release through soldr in a glibc 2.34 Nix shell.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
export COMPUTER_USE_RELEASE_BINARY="${COMPUTER_USE_RELEASE_BINARY:-}"
if [[ ${1:-} == --from-binary ]]; then
    COMPUTER_USE_RELEASE_BINARY=$(realpath -e -- "${2:?--from-binary needs a soldr-built executable}")
    shift 2
fi
if [[ $(uname -sm) != 'Linux x86_64' ]]; then
    echo 'This release script currently supports native Linux x86_64 only.' >&2
    exit 1
fi
export COMPUTER_USE_ROOT="$root"
export COMPUTER_USE_RELEASE_OUT="$(realpath -m -- "${1:-$root/dist}")"
export CARGO_TARGET_DIR="$(realpath -m -- "${CARGO_TARGET_DIR:-$root/rust/target}")"
# Writable build outputs avoid stale read-only metadata on a later rebuild.
export SOLDR_ZCCACHE_MODE=copy
if [[ ${COMPUTER_USE_RELEASE_SHELL:-} != 1 ]]; then
    export COMPUTER_USE_RELEASE_SHELL=1
    exec nix-shell "$root/nix/release-shell.nix" --run '"$COMPUTER_USE_ROOT/scripts/release-linux.sh" "$COMPUTER_USE_RELEASE_OUT"'
fi
version=$(awk '/^\[workspace.package\]/{p=1;next} /^\[/{p=0} p && /^version = /{gsub(/"/, "", $3); print $3; exit}' "$root/rust/Cargo.toml")
[[ -n "$version" ]]
if [[ -n "$COMPUTER_USE_RELEASE_BINARY" ]]; then
    release_binary="$COMPUTER_USE_RELEASE_BINARY"
else
    (cd -- "$root/rust" && nice -n 10 soldr cargo build -j 4 --release -p cua-driver \
        --bin computer-use --locked)
    release_binary="$CARGO_TARGET_DIR/release/computer-use"
fi
version_output=$("$release_binary" --version)
if [[ "$version_output" != *" $version" ]]; then
    echo "Expected a computer-use $version executable" >&2
    exit 1
fi
mkdir -p -- "$COMPUTER_USE_RELEASE_OUT"
stage=$(safe-mktemp "${TMPDIR:-/tmp}/computer-use-release-$$")
trap 'safe-rm -r -- "$stage"' EXIT
name="computer-use-$version-linux-x86_64"
bundle="$stage/$name"
mkdir -p -- "$bundle"
install -m755 -- "$release_binary" "$bundle/computer-use"
strip --strip-unneeded "$bundle/computer-use"
# Standard loader on glibc distros; NixOS supports it with programs.nix-ld.enable.
# The Nix package patches this to its own interpreter, without needing nix-ld.
patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 --remove-rpath "$bundle/computer-use"
if readelf -d "$bundle/computer-use" | grep -E 'NEEDED.*(libX11|libXi[.]|libXtst|libXext|libxkbcommon|libpipewire)' ; then
    echo 'Unexpected hard-linked desktop library' >&2
    exit 1
fi
for applet in cua-driver cua-cursor-theme cua-perception; do
    ln -s computer-use "$bundle/$applet"
done
cp -R -- "$root/wayland-helper" "$bundle/wayland-helper"
install -m644 -- "$root/LICENSE.md" "$bundle/LICENSE"
install -m644 -- "$root/docs/release-linux.md" "$bundle/README.md"
archive="$COMPUTER_USE_RELEASE_OUT/$name.tar.gz"
# Preserve the relative symlinks; never use tar --dereference.
tar --sort=name --owner=0 --group=0 --numeric-owner -czf "$archive" -C "$stage" "$name"
(cd -- "$COMPUTER_USE_RELEASE_OUT" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
printf 'Archive: %s\n' "$archive"
cat "$archive.sha256"
