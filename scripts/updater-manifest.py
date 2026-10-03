#!/usr/bin/env python3
"""Assemble a complete updater feed only after all release builds succeed.

Installer-specific aliases follow tauri-apps/tauri-action's
src/upload-version-json.ts (MIT/Apache-2.0). Unlike its per-platform merge,
this job requires every supported target in one atomic manifest.
"""

import argparse
import base64
import datetime
import json
from pathlib import Path
import re


def build(directory: Path, version: str, repository: str) -> dict:
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError("Invalid release version")
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", repository):
        raise ValueError("Invalid release repository")
    files = {
        "darwin-aarch64": "xNAUT-macos-aarch64.app.tar.gz",
        "darwin-x86_64": "xNAUT-macos-x64.app.tar.gz",
        "windows-x86_64": f"xNAUT-{version}-windows-x64.msi",
        "windows-x86_64-nsis": f"xNAUT-{version}-windows-x64-setup.exe",
    }
    platforms = {}
    for platform, name in files.items():
        artifact = directory / name
        if not artifact.is_file() or artifact.stat().st_size == 0:
            raise ValueError(f"Missing or empty updater artifact: {name}")
        signature = (directory / f"{name}.sig").read_text().strip()
        # Tauri signatures are base64-encoded minisign envelopes. Actual
        # cryptographic verification against the configured public key is a
        # separate release gate on the downloaded assets.
        envelope = base64.b64decode(signature, validate=True).decode().splitlines()
        if (
            len(envelope) != 4
            or not envelope[0].startswith("untrusted comment:")
            or not envelope[2].startswith("trusted comment:")
        ):
            raise ValueError(f"Invalid updater signature: {name}")
        platforms[platform] = {
            "signature": signature,
            "url": f"https://github.com/{repository}/releases/download/v{version}/{name}",
        }
    platforms["windows-x86_64-msi"] = dict(platforms["windows-x86_64"])
    return {
        "version": version,
        "notes": f"xNAUT {version} — see release notes for details",
        "pub_date": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "platforms": platforms,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("version")
    parser.add_argument("--repository", default="48Nauts-Operator/xNaut")
    args = parser.parse_args()
    manifest = build(args.directory, args.version, args.repository)
    (args.directory / "latest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print("Complete updater manifest:", ", ".join(manifest["platforms"]))
