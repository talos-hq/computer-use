#!/usr/bin/env bash
# Build the native Linux release through soldr against the glibc 2.17 sysroot.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
case "$(uname -sm)" in
    'Linux x86_64') arch=x86_64; interpreter=/lib64/ld-linux-x86-64.so.2 ;;
    'Linux aarch64') arch=aarch64; interpreter=/lib/ld-linux-aarch64.so.1 ;;
    *) echo 'Linux release builds require a native x86_64 or aarch64 host.' >&2; exit 1 ;;
esac
export COMPUTER_USE_ROOT="$root"
export COMPUTER_USE_RELEASE_OUT="$(realpath -m -- "${1:-$root/dist}")"
if [[ ${COMPUTER_USE_RELEASE_SHELL:-} != 1 ]]; then
    export COMPUTER_USE_RELEASE_SHELL=1
    exec nix-shell "$root/nix/release-shell.nix" --run '"$COMPUTER_USE_ROOT/scripts/release-linux.sh" "$COMPUTER_USE_RELEASE_OUT"'
fi
version=$(awk '/^\[workspace.package\]/{p=1;next} /^\[/{p=0} p && /^version = /{gsub(/"/, "", $3); print $3; exit}' "$root/rust/Cargo.toml")
[[ -n "$version" ]]
target="$arch-unknown-linux-gnu"
baseline=2.17
(cd -- "$root/rust" && nice -n 10 soldr build --target "$target.$baseline" \
    -j 4 --release -p cua-driver --bin computer-use --locked)
mkdir -p -- "$COMPUTER_USE_RELEASE_OUT"
if [[ ${GITHUB_ACTIONS:-} == true ]]; then
    # GitHub disposes of the hosted runner and its temporary files after this job.
    stage=$(mktemp -d "${RUNNER_TEMP:?}/computer-use-release.XXXXXXXX")
else
    stage=$(safe-mktemp "${TMPDIR:-/tmp}/computer-use-release-$$")
    trap 'safe-rm -r -- "$stage"' EXIT
fi
name="computer-use-$version-linux-$arch"
bundle="$stage/$name"
mkdir -p -- "$bundle"
install -m755 -- "$root/rust/target/$target/release/computer-use" "$bundle/computer-use"
strip --strip-unneeded "$bundle/computer-use"
# Standard loader on glibc distros; NixOS supports it with programs.nix-ld.enable.
# The Nix package patches this to its own interpreter, without needing nix-ld.
patchelf --set-interpreter "$interpreter" --remove-rpath "$bundle/computer-use"
dynamic=$(readelf -d "$bundle/computer-use")
if grep -E 'NEEDED.*(libX11|libXi[.]|libXtst|libXext|libxkbcommon|libpipewire)' <<< "$dynamic"; then
    echo 'Unexpected hard-linked desktop library' >&2
    exit 1
fi
# An accidental host link can run on CI while breaking the supported baseline.
glibc_versions=$(readelf --version-info "$bundle/computer-use")
if grep -Eq 'Name: GLIBC_(PRIVATE|ABI_)' <<< "$glibc_versions"; then
    echo 'Unexpected private or newer glibc ABI requirement' >&2
    exit 1
fi
latest_glibc=$(grep -oE 'GLIBC_[0-9]+(\.[0-9]+)+' <<< "$glibc_versions" | sort -Vu | tail -1)
if [[ $(printf '%s\n' "GLIBC_$baseline" "$latest_glibc" | sort -V | tail -1) != "GLIBC_$baseline" ]]; then
    printf 'Release requires %s, beyond the GLIBC_%s baseline\n' "$latest_glibc" "$baseline" >&2
    exit 1
fi
printf 'Highest required glibc symbol version: %s\n' "$latest_glibc"
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
