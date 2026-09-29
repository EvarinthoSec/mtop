#!/usr/bin/env python3
"""Render an Alpine APKBUILD for the current release binary."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

VERSION_RE = re.compile(
    r"(?P<base>[0-9]+(?:\.[0-9]+)*)(?:-(?P<suffix>alpha|beta|pre|rc)(?:\.(?P<number>[0-9]+))?)?"
)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    match = VERSION_RE.fullmatch(args.version)
    if match is None:
        parser.error("version must be numeric semver with optional alpha, beta, pre, or rc suffix")

    pkgver = match.group("base")
    suffix = match.group("suffix")
    if suffix:
        pkgver += f"_{suffix}{match.group('number') or ''}"

    template = Path(__file__).with_name("APKBUILD.in").read_text(encoding="utf-8")
    rendered = template.replace("@PKGVER@", pkgver)
    if "@" in rendered:
        raise SystemExit("unexpanded APKBUILD placeholder")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(rendered, encoding="utf-8")


if __name__ == "__main__":
    main()
