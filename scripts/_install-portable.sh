#!/usr/bin/env bash
# Install a release archive, unpacked bundle, or local release build.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
prefix="$HOME/.local"
source_path=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix) prefix=${2:?--prefix needs a directory}; shift 2 ;;
        --help|-h)
            echo "Usage: $0 [--prefix ~/.local] [release.tar.gz | bundle-directory | binary]"
            exit 0 ;;
        --*) echo "Unknown option: $1" >&2; exit 2 ;;
        *) [[ -z "$source_path" ]] || { echo 'Only one source is allowed' >&2; exit 2; }
           source_path=$1; shift ;;
    esac
done
if [[ "$prefix" == '~' ]]; then
    prefix="$HOME"
elif [[ "$prefix" == '~/'* ]]; then
    prefix="$HOME/${prefix:2}"
fi
if [[ -z "$source_path" ]]; then
    source_path="$root/rust/target/release/computer-use"
fi
python3 - "$prefix" "$source_path" "$root" <<'PY'
import os
from pathlib import Path, PurePosixPath
import shutil
import sys
import tarfile

prefix, source, repo = (Path(arg).expanduser().absolute() for arg in sys.argv[1:])
dest = prefix / "lib" / "computer-use"

def install_file(src, path, mode):
    # Atomic replacement also permits updating a running executable on Linux.
    path.parent.mkdir(parents=True, exist_ok=True)
    pending = path.with_name(f".{path.name}.install-{os.getpid()}")
    with pending.open("xb") as out:
        shutil.copyfileobj(src, out)
    pending.chmod(mode)
    os.replace(pending, path)

# Inspect archives before writing, and extract only regular release payload files.
if source.name.endswith(".tar.gz"):
    with tarfile.open(source, "r:gz") as archive:
        members = archive.getmembers()
        roots = {PurePosixPath(m.name).parts[0] for m in members if m.name}
        if len(roots) != 1:
            raise SystemExit("Release archive must have one top-level directory")
        files = []
        for member in members:
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts:
                raise SystemExit("Unsafe archive path")
            rel = PurePosixPath(*path.parts[1:])
            if rel.as_posix() in {"computer-use", "LICENSE", "README.md"} or (rel.parts and rel.parts[0] == "wayland-helper"):
                if not (member.isdir() or member.isfile()):
                    raise SystemExit(f"Unexpected non-regular payload: {member.name}")
                if member.isfile():
                    files.append((member, dest.joinpath(*rel.parts)))
        if not any(path == dest / "computer-use" for _, path in files):
            raise SystemExit("Release archive has no computer-use binary")
        for member, path in files:
            with archive.extractfile(member) as src:
                install_file(src, path, 0o755 if member.mode & 0o111 else 0o644)
else:
    binary = source / "computer-use" if source.is_dir() else source
    if not binary.is_file():
        raise SystemExit(f"No release binary at {binary}; run scripts/release-linux.sh first")
    dest.mkdir(parents=True, exist_ok=True)
    with binary.open("rb") as src:
        install_file(src, dest / "computer-use", 0o755)
    helpers = binary.parent / "wayland-helper"
    if not helpers.is_dir():
        helpers = repo / "wayland-helper"
    if helpers.is_dir():
        shutil.copytree(helpers, dest / "wayland-helper", dirs_exist_ok=True)
    license_path = binary.parent / "LICENSE"
    if not license_path.is_file():
        license_path = repo / "LICENSE.md"
    if license_path.is_file():
        shutil.copy2(license_path, dest / "LICENSE")
(dest / "computer-use").chmod(0o755)
print(f"Installed {dest / 'computer-use'}")
PY
mkdir -p -- "$prefix/bin"
ln -sfn ../lib/computer-use/computer-use "$prefix/bin/computer-use"
for applet in cua-driver cua-cursor-theme cua-perception; do
    ln -sfn computer-use "$prefix/lib/computer-use/$applet"
    ln -sfn computer-use "$prefix/bin/$applet"
done
printf 'Add %s/bin to PATH.\n' "$prefix"
