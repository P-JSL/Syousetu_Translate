#!/usr/bin/env python3
"""Install licensed, user-owned ezTrans XP binaries locally, never in Git."""

from __future__ import annotations

import argparse
import shutil
import sys
import tempfile
from pathlib import Path, PurePosixPath
from zipfile import BadZipFile, ZipFile

CORE_DLLS = {"J2KEngine.dll", "J2KEngineH.dll"}
SUPPORT_DLLS = {"JKPreProcess.dll", "K2JEngine.dll"}
ASSET_FOLDERS = {"Dat", "Ehnd"}
MAX_UNPACKED_BYTES = 512 * 1024 * 1024


def members_to_install(archive: ZipFile):
    selected = []
    found = set()
    total = 0
    for member in archive.infolist():
        if member.is_dir():
            continue
        raw_name = member.filename
        path = PurePosixPath(raw_name)
        if (path.is_absolute() or ".." in path.parts or "\\" in raw_name
                or not path.parts or ":" in path.parts[0]):
            raise ValueError(f"Unsafe archive path: {raw_name!r}")
        top_level = len(path.parts) == 1
        allowed = ((top_level and path.name in CORE_DLLS | SUPPORT_DLLS)
                   or (len(path.parts) > 1 and path.parts[0] in ASSET_FOLDERS))
        if not allowed:
            continue
        if (member.external_attr >> 16) & 0o170000 == 0o120000:
            raise ValueError(f"Symlinks are not allowed: {raw_name}")
        key = str(path).casefold()
        if key in found:
            raise ValueError(f"Duplicate case-insensitive asset name: {raw_name}")
        found.add(key)
        total += member.file_size
        if total > MAX_UNPACKED_BYTES:
            raise ValueError("Engine assets exceed the 512 MiB safety limit")
        selected.append((member, path))
    if not any(str(path) in CORE_DLLS for _, path in selected):
        raise ValueError("J2KEngine.dll or J2KEngineH.dll is missing")
    if not any(path.parts[0] == "Dat" for _, path in selected):
        raise ValueError("Dat dictionary folder is missing")
    return selected


def install(archive_path: Path, destination: Path) -> tuple[int, int]:
    destination = destination.resolve()
    if destination.exists():
        raise FileExistsError(f"Destination already exists (not overwriting): {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with ZipFile(archive_path) as archive:
        selected = members_to_install(archive)
        with tempfile.TemporaryDirectory(prefix=".syousetu-engine-", dir=destination.parent) as temp:
            stage = Path(temp) / "engine"
            stage.mkdir()
            for member, path in selected:
                out = stage.joinpath(*path.parts)
                out.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(member) as source, out.open("wb") as sink:
                    shutil.copyfileobj(source, sink, length=1024 * 1024)
                if out.stat().st_size != member.file_size:
                    raise OSError(f"Extracted file size mismatch: {path}")
            stage.rename(destination)
    total = sum(info.file_size for info, _ in selected)
    return len(selected), total


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--dest", type=Path, default=Path(__file__).resolve().parent.parent / "engine")
    args = parser.parse_args()
    try:
        count, size = install(args.archive, args.dest)
    except (OSError, ValueError, BadZipFile) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    print(f"Installed {count} engine files ({size:,} bytes) into {args.dest.resolve()}")
    print("The engine directory is ignored by Git. Keep vendor binaries and dictionaries private.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
