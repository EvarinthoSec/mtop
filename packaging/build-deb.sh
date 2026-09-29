#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
VERSION=${1:?usage: build-deb.sh VERSION [ARCH]}
VERSION=${VERSION#v}
ARCH=${2:-amd64}
BINARY=${BINARY:-"$PROJECT_ROOT/target/release/mtop"}
OUTPUT_DIR=${OUTPUT_DIR:-"$PROJECT_ROOT/dist"}

if [ "$ARCH" != amd64 ]; then
    printf '%s\n' "error: this packaging script currently supports amd64, not $ARCH" >&2
    exit 2
fi
if [ ! -x "$BINARY" ]; then
    printf '%s\n' "error: release binary not found or not executable: $BINARY" >&2
    exit 1
fi
if ! command -v dpkg-deb >/dev/null 2>&1; then
    printf '%s\n' "error: dpkg-deb is required to build Debian packages" >&2
    exit 1
fi

case "$VERSION" in
    ""|[!0-9]*|*[!0-9A-Za-z.+~-]*)
        printf '%s\n' "error: invalid release version: $VERSION" >&2
        exit 2
        ;;
esac
DEB_VERSION=${VERSION%%-*}
if [ "$DEB_VERSION" != "$VERSION" ]; then
    DEB_VERSION="$DEB_VERSION~${VERSION#*-}"
fi
ASSET_VERSION=${VERSION%%-*}
if [ "$ASSET_VERSION" != "$VERSION" ]; then
    ASSET_VERSION="$ASSET_VERSION.${VERSION#*-}"
fi

STAGE=$(mktemp -d "${TMPDIR:-/tmp}/mtop-deb.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
mkdir -p \
    "$STAGE/DEBIAN" \
    "$STAGE/usr/bin" \
    "$STAGE/usr/share/man/man1" \
    "$STAGE/usr/share/applications"
install -m 0755 "$BINARY" "$STAGE/usr/bin/mtop"
install -m 0644 "$SCRIPT_DIR/mtop.1" "$STAGE/usr/share/man/man1/mtop.1"
install -m 0644 "$SCRIPT_DIR/mtop.desktop" "$STAGE/usr/share/applications/mtop.desktop"

cat > "$STAGE/DEBIAN/control" <<EOF
Package: mtop
Version: $DEB_VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: EvarinthoSec
Depends: libc6 (>= 2.35), libgcc-s1
Homepage: https://launchpad.net/mtop-monitor
Description: Cross-platform terminal system monitor
 A fast terminal dashboard for CPU, memory, disks, networks, processes, and accelerators.
EOF

mkdir -p "$OUTPUT_DIR"
PACKAGE="$OUTPUT_DIR/mtop_${ASSET_VERSION}_${ARCH}.deb"
dpkg-deb --build --root-owner-group "$STAGE" "$PACKAGE"
printf 'built %s\n' "$PACKAGE"
