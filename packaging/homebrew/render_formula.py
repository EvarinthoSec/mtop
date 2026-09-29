#!/usr/bin/env python3
"""Render the Homebrew formula uploaded with an mtop GitHub release."""

from __future__ import annotations

import argparse
import re
from pathlib import Path


SHA256_RE = re.compile(r"[0-9a-f]{64}")
VERSION_RE = re.compile(r"[0-9A-Za-z.+~-]+")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--arm64-sha256", required=True)
    parser.add_argument("--x86-64-sha256", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    if not VERSION_RE.fullmatch(args.version) or not args.version[0].isdigit():
        parser.error("version must be a plain release version without a leading v")
    for label, digest in (
        ("arm64", args.arm64_sha256),
        ("x86_64", args.x86_64_sha256),
    ):
        if not SHA256_RE.fullmatch(digest):
            parser.error(f"{label} SHA256 must be 64 lowercase hexadecimal characters")

    template_path = Path(__file__).with_name("mtop.rb.template")
    formula = template_path.read_text(encoding="utf-8")
    replacements = {
        "@VERSION@": args.version,
        "@ARM64_SHA256@": args.arm64_sha256,
        "@X86_64_SHA256@": args.x86_64_sha256,
    }
    for placeholder, value in replacements.items():
        if formula.count(placeholder) != 1:
            raise SystemExit(f"expected exactly one {placeholder} in {template_path}")
        formula = formula.replace(placeholder, value)
    if "@" in formula:
        raise SystemExit("unexpanded formula placeholder")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(formula, encoding="utf-8")


if __name__ == "__main__":
    main()
