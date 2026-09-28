#!/usr/bin/env python3
"""Give cargo-packager outputs stable release names and validate signatures."""

from __future__ import annotations

import argparse
from pathlib import Path


EXPECTED = {
    "windows-x86_64": [
        (".exe", "Wyck_{version}_windows-x86_64-setup.exe"),
    ],
    "macos-x86_64": [
        (".dmg", "Wyck_{version}_macos-x86_64.dmg"),
        (".app.tar.gz", "Wyck_{version}_macos-x86_64.app.tar.gz"),
    ],
    "macos-aarch64": [
        (".dmg", "Wyck_{version}_macos-aarch64.dmg"),
        (".app.tar.gz", "Wyck_{version}_macos-aarch64.app.tar.gz"),
    ],
    "linux-x86_64": [
        (".AppImage", "Wyck_{version}_linux-x86_64.AppImage"),
        (".deb", "Wyck_{version}_linux-x86_64.deb"),
    ],
}


def matching_file(root: Path, suffix: str) -> Path:
    matches = sorted(
        path
        for path in root.rglob(f"*{suffix}")
        if path.is_file() and not path.name.endswith(".sig")
    )
    if len(matches) != 1:
        found = ", ".join(str(path) for path in matches) or "none"
        raise SystemExit(f"Expected one *{suffix} package, found: {found}")
    return matches[0]


def rename_pair(source: Path, destination: Path) -> None:
    signature = Path(f"{source}.sig")
    if not signature.is_file() or not signature.read_text(encoding="utf-8").strip():
        raise SystemExit(f"Missing signature for {source.name}")
    if destination.exists() or Path(f"{destination}.sig").exists():
        raise SystemExit(f"Refusing to overwrite {destination.name}")
    source.replace(destination)
    signature.replace(Path(f"{destination}.sig"))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--platform", choices=EXPECTED, required=True)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()

    args.input.mkdir(parents=True, exist_ok=True)
    for suffix, name in EXPECTED[args.platform]:
        source = matching_file(args.input, suffix)
        destination = args.input / name.format(version=args.version)
        rename_pair(source, destination)


if __name__ == "__main__":
    main()
