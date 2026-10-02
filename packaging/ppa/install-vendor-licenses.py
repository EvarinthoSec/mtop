#!/usr/bin/env python3
"""Preserve license and notice files from vendored Cargo dependencies."""
from __future__ import annotations

import shutil
import sys
from pathlib import Path


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: install-vendor-licenses.py VENDOR_DIR OUTPUT_DIR")
    vendor = Path(sys.argv[1])
    output = Path(sys.argv[2])
    if not vendor.is_dir():
        raise SystemExit(f"vendor directory is missing: {vendor}")

    copied = 0
    for crate in sorted(path for path in vendor.iterdir() if path.is_dir()):
        for candidate in sorted(crate.iterdir()):
            name = candidate.name.upper()
            if candidate.is_file() and (
                name.startswith("LICENSE") or name.startswith("COPYING") or name.startswith("NOTICE")
            ):
                target = output / crate.name / candidate.name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(candidate, target)
                copied += 1
    if not copied:
        raise SystemExit("no vendored license/notice files were found")


if __name__ == "__main__":
    main()
