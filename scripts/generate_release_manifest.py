#!/usr/bin/env python3
"""Validate release assets and write latest.json plus SHA256SUMS."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote


PLATFORMS = {
    "windows-x86_64": ("Wyck_{version}_windows-x86_64-setup.exe", "nsis"),
    "macos-x86_64": ("Wyck_{version}_macos-x86_64.app.tar.gz", "app"),
    "macos-aarch64": ("Wyck_{version}_macos-aarch64.app.tar.gz", "app"),
    "linux-x86_64": ("Wyck_{version}_linux-x86_64.AppImage", "appimage"),
}

INSTALLERS = [
    "Wyck_{version}_windows-x86_64-setup.exe",
    "Wyck_{version}_macos-x86_64.dmg",
    "Wyck_{version}_macos-aarch64.dmg",
    "Wyck_{version}_linux-x86_64.AppImage",
    "Wyck_{version}_linux-x86_64.deb",
]


def require_signed(root: Path, name: str) -> tuple[Path, str]:
    package = root / name
    signature = root / f"{name}.sig"
    if not package.is_file():
        raise SystemExit(f"Missing release package: {name}")
    if not signature.is_file():
        raise SystemExit(f"Missing release signature: {signature.name}")
    signature_text = signature.read_text(encoding="utf-8").strip()
    if not signature_text:
        raise SystemExit(f"Empty release signature: {signature.name}")
    return package, signature_text


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--notes", type=Path, required=True)
    args = parser.parse_args()

    match = re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", args.tag)
    if not match:
        raise SystemExit(f"The release tag is not stable SemVer: {args.tag}")
    version = args.tag[1:]
    args.assets.mkdir(parents=True, exist_ok=True)

    for template in INSTALLERS:
        require_signed(args.assets, template.format(version=version))

    release_url = f"https://github.com/{args.repository}/releases/download/{args.tag}"
    platforms = {}
    for platform, (template, package_format) in PLATFORMS.items():
        name = template.format(version=version)
        _, signature = require_signed(args.assets, name)
        platforms[platform] = {
            "signature": signature,
            "url": f"{release_url}/{quote(name)}",
            "format": package_format,
        }

    manifest = {
        "version": version,
        "notes": args.notes.read_text(encoding="utf-8").strip(),
        "pub_date": datetime.now(timezone.utc).isoformat(timespec="seconds").replace(
            "+00:00", "Z"
        ),
        "platforms": platforms,
    }
    latest = args.assets / "latest.json"
    latest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    checksum_file = args.assets / "SHA256SUMS"
    files = sorted(
        path for path in args.assets.iterdir() if path.is_file() and path != checksum_file
    )
    checksum_file.write_text(
        "".join(f"{sha256(path)}  {path.name}\n" for path in files),
        encoding="ascii",
    )


if __name__ == "__main__":
    main()
