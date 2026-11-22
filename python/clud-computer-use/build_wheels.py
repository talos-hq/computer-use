"""Build platform wheels from checksum-verified Computer Use release archives."""

import argparse
import base64
import csv
import hashlib
import io
import json
import re
import zipfile
from pathlib import Path

PROJECT = "clud_computer_use"
HERE = Path(__file__).resolve().parent
SUPPORTED = {
    "linux-x86_64.tar.gz": "manylinux_2_17_x86_64",
    "linux-aarch64.tar.gz": "manylinux_2_17_aarch64",
    "darwin-universal.tar.gz": "macosx_11_0_universal2",
    "windows-x86_64.zip": "win_amd64",
    "windows-aarch64.zip": "win_arm64",
}


def digest(data: bytes) -> str:
    return base64.urlsafe_b64encode(hashlib.sha256(data).digest()).decode().rstrip("=")


def wheel(version: str, assets: Path, output: Path, suffix: str, platform: str, hashes: dict) -> Path:
    filename = f"computer-use-{version}-{suffix}"
    payload = (assets / filename).read_bytes()
    sha256 = hashlib.sha256(payload).hexdigest()
    if hashes.get(filename) != sha256:
        raise ValueError(f"SHA-256 mismatch for {filename}")
    if suffix.startswith("linux-"):
        binary = f"{filename.removesuffix('.tar.gz')}/computer-use"
    elif suffix.startswith("darwin-"):
        binary = "ComputerUse.app/Contents/MacOS/computer-use"
    else:
        binary = "computer-use.exe"

    tag = f"py3-none-{platform}"
    info = f"{PROJECT}-{version}.dist-info"
    entries = {
        f"{PROJECT}/__init__.py": (HERE / "launcher.py").read_bytes(),
        f"{PROJECT}/payload.json": json.dumps(
            {"archive": filename, "binary": binary, "sha256": sha256}
        ).encode(),
        f"{PROJECT}/{filename}": payload,
        f"{info}/METADATA": (
            f"Metadata-Version: 2.4\nName: clud-computer-use\nVersion: {version}\n"
            "Summary: Signed native Talos HQ Computer Use for clud\n"
            "Requires-Python: >=3.12\nLicense-Expression: MIT\n"
            "License-File: LICENSE.md\n"
            "Project-URL: Source, https://github.com/talos-hq/computer-use\n\n"
        ).encode(),
        f"{info}/licenses/LICENSE.md": (HERE.parents[1] / "LICENSE.md").read_bytes(),
        f"{info}/WHEEL": (
            f"Wheel-Version: 1.0\nGenerator: computer-use release\nRoot-Is-Purelib: false\nTag: {tag}\n"
        ).encode(),
        f"{info}/entry_points.txt": (
            "[console_scripts]\nclud-computer-use = clud_computer_use:main\n"
            "computer-use = clud_computer_use:main\n"
            "clud-computer-use-archive = clud_computer_use:archive_path\n"
        ).encode(),
    }
    manifest = io.StringIO(newline="")
    record = csv.writer(manifest)
    for name, data in entries.items():
        record.writerow((name, f"sha256={digest(data)}", len(data)))
    record.writerow((f"{info}/RECORD", "", ""))

    path = output / f"{PROJECT}-{version}-{tag}.whl"
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_STORED) as bundle:
        for name, data in entries.items():
            bundle.writestr(name, data)
        bundle.writestr(f"{info}/RECORD", manifest.getvalue())
    return path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    parser.add_argument("assets", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", args.version):
        parser.error("expected a three-component release version")
    hashes = {name: digest for digest, name in (
        line.split() for line in (args.assets / "SHA256SUMS").read_text().splitlines()
    )}
    args.output.mkdir(parents=True, exist_ok=True)
    for suffix, platform in SUPPORTED.items():
        print(wheel(args.version, args.assets, args.output, suffix, platform, hashes))


if __name__ == "__main__":
    main()
