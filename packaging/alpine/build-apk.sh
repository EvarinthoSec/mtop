#!/bin/sh
set -eu

VERSION=${1:?usage: build-apk.sh VERSION SOURCE_ROOT OUTPUT_DIR}
SOURCE_ROOT=${2:?usage: build-apk.sh VERSION SOURCE_ROOT OUTPUT_DIR}
OUTPUT_DIR=${3:?usage: build-apk.sh VERSION SOURCE_ROOT OUTPUT_DIR}
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

apk add --no-cache alpine-sdk cargo rust git python3
cargo build --locked --release --manifest-path "$SOURCE_ROOT/Cargo.toml" --package mtop

mkdir -p "$OUTPUT_DIR/alpine"
install -m 0755 "$SOURCE_ROOT/target/release/mtop" "$OUTPUT_DIR/alpine/mtop"
install -m 0644 "$SOURCE_ROOT/packaging/mtop.1" "$OUTPUT_DIR/alpine/mtop.1"
install -m 0644 "$SOURCE_ROOT/packaging/mtop.desktop" "$OUTPUT_DIR/alpine/mtop.desktop"
install -m 0644 "$SOURCE_ROOT/LICENSE" "$OUTPUT_DIR/alpine/LICENSE"
install -m 0644 "$SOURCE_ROOT/THIRD_PARTY_NOTICES.md" "$OUTPUT_DIR/alpine/THIRD_PARTY_NOTICES.md"
install -m 0644 "$SOURCE_ROOT/THIRD_PARTY_LICENSES/btop-Apache-2.0.txt" \
    "$OUTPUT_DIR/alpine/btop-Apache-2.0.txt"
python3 "$SCRIPT_DIR/render_apkbuild.py" --version "$VERSION" \
    --output "$OUTPUT_DIR/alpine/APKBUILD"

cd "$OUTPUT_DIR/alpine"
abuild-keygen -a -n -q
set -- /root/.abuild/*.rsa.pub
if [ ! -f "$1" ]; then
    printf '%s\n' "error: abuild did not create a package verification key" >&2
    exit 1
fi
cp "$1" "$OUTPUT_DIR/"
install -m 0644 "$1" "/etc/apk/keys/$(basename -- "$1")"
abuild -F -P "$OUTPUT_DIR/alpine/repo" checksum
abuild -F -P "$OUTPUT_DIR/alpine/repo" -r
python3 - "$OUTPUT_DIR/alpine/repo" "$OUTPUT_DIR" <<'PY'
from pathlib import Path
import shutil
import sys

repo, output = map(Path, sys.argv[1:])
packages = sorted(path for path in repo.rglob("*.apk") if path.name.startswith("mtop"))
if not packages:
    raise SystemExit("error: abuild did not produce an mtop APK package")
for package in packages:
    shutil.copy2(package, output / package.name)
PY
