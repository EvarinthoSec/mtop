#!/usr/bin/env python3
"""Render a checksummed Arch PKGBUILD for a tagged source release."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

VERSION_RE = re.compile(r"[0-9A-Za-z.+~-]+")
SHA256_RE = re.compile(r"[0-9a-f]{64}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--source-sha256", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    if not VERSION_RE.fullmatch(args.version) or not args.version[0].isdigit():
        parser.error("version must be a release version without a leading v")
    if not SHA256_RE.fullmatch(args.source_sha256):
        parser.error("source SHA256 must be 64 lowercase hexadecimal characters")

    template = Path(__file__).with_name("PKGBUILD.in").read_text(encoding="utf-8")
    rendered = template.replace("@PKGVER@", args.version.replace("-", "_"))
    rendered = rendered.replace("@TAG_VERSION@", args.version)
    rendered = rendered.replace("@SOURCE_SHA256@", args.source_sha256)
    if re.search(r"@[A-Z][A-Z0-9_]*@", rendered):
        raise SystemExit("unexpanded PKGBUILD placeholder")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(rendered, encoding="utf-8")


if __name__ == "__main__":
    main()
