"""Run the native Computer Use payload carried by this wheel."""

import hashlib
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path

PACKAGE = Path(__file__).parent
PIN = json.loads((PACKAGE / "payload.json").read_text(encoding="utf-8"))
ARCHIVE = PACKAGE / PIN["archive"]


def native_binary() -> Path:
    with ARCHIVE.open("rb") as archive:
        if hashlib.file_digest(archive, "sha256").hexdigest() != PIN["sha256"]:
            raise RuntimeError("Computer Use archive failed SHA-256 verification")
        archive.seek(0)
        cache = Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache")
        parent = cache / "clud-computer-use"
        target = parent / PIN["sha256"]
        binary = target / PIN["binary"]
        if binary.is_file():
            return binary
        parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as temporary:
            stage = Path(temporary) / "payload"
            stage.mkdir()
            if ARCHIVE.suffix == ".zip":
                with zipfile.ZipFile(archive) as bundle:
                    bundle.extractall(stage)
            else:
                with tarfile.open(fileobj=archive, mode="r:gz") as bundle:
                    bundle.extractall(stage, filter="data")
            if not (stage / PIN["binary"]).is_file():
                raise RuntimeError("Computer Use archive is missing its executable")
            try:
                stage.rename(target)
            except OSError:
                if not binary.is_file():
                    raise
        return binary


def archive_path() -> None:
    """Provide the intact archive to clud's checksum-pinned installer."""
    print(ARCHIVE.absolute())


def main() -> None:
    binary = native_binary()
    argv = [str(binary), *sys.argv[1:]]
    if os.name == "nt":
        raise SystemExit(subprocess.call(argv))
    os.execv(binary, argv)
