#!/usr/bin/env python3
"""Generate a portable WinGet manifest set for a versioned mtop ZIP."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

VERSION_RE = re.compile(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?")
SHA256_RE = re.compile(r"[0-9a-fA-F]{64}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--installer-url", required=True)
    parser.add_argument("--installer-sha256", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    if not VERSION_RE.fullmatch(args.version):
        parser.error("version must be numeric SemVer with an optional prerelease")
    if not SHA256_RE.fullmatch(args.installer_sha256):
        parser.error("installer SHA256 must be 64 hexadecimal characters")
    if not args.installer_url.startswith("https://github.com/EvarinthoSec/mtop/releases/download/"):
        parser.error("installer URL must be an HTTPS mtop GitHub Release URL")

    version = args.version
    directory = args.output / "e" / "EvarinthoSec" / "mtop" / version
    directory.mkdir(parents=True, exist_ok=True)
    manifest_version = "1.12.0"

    files = {
        f"EvarinthoSec.mtop.yaml": f'''# yaml-language-server: $schema=https://aka.ms/winget-manifest.version.1.12.0.schema.json
PackageIdentifier: EvarinthoSec.mtop
PackageVersion: "{version}"
DefaultLocale: en-US
ManifestType: version
ManifestVersion: {manifest_version}
''',
        f"EvarinthoSec.mtop.installer.yaml": f'''# yaml-language-server: $schema=https://aka.ms/winget-manifest.installer.1.12.0.schema.json
PackageIdentifier: EvarinthoSec.mtop
PackageVersion: "{version}"
Platform:
- Windows.Desktop
Installers:
- Architecture: x64
  InstallerType: zip
  InstallerUrl: {args.installer_url}
  InstallerSha256: {args.installer_sha256.lower()}
  NestedInstallerType: portable
  NestedInstallerFiles:
  - RelativeFilePath: mtop.exe
    PortableCommandAlias: mtop
ManifestType: installer
ManifestVersion: {manifest_version}
''',
        f"EvarinthoSec.mtop.locale.en-US.yaml": f'''# yaml-language-server: $schema=https://aka.ms/winget-manifest.defaultLocale.1.12.0.schema.json
PackageIdentifier: EvarinthoSec.mtop
PackageVersion: "{version}"
PackageLocale: en-US
Publisher: EvarinthoSec
PublisherUrl: https://github.com/EvarinthoSec
PublisherSupportUrl: https://github.com/EvarinthoSec/mtop/issues
PackageName: mtop
PackageUrl: https://github.com/EvarinthoSec/mtop
License: MIT
ShortDescription: Cross-platform terminal system monitor
Description: A terminal dashboard for CPU, memory, disks, networks, processes, uptime, and optional GPU metrics.
Tags:
- monitoring
- terminal
- system-monitor
- processes
ManifestType: defaultLocale
ManifestVersion: {manifest_version}
''',
    }
    for filename, content in files.items():
        (directory / filename).write_bytes(content.encode("utf-8"))


if __name__ == "__main__":
    main()
