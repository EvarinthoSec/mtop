#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
BINARY=${BINARY:-"$PROJECT_ROOT/target/release/mtop"}
PREFIX=${PREFIX:-/usr/local}
DESTDIR=${DESTDIR:-}

if [ "$#" -gt 1 ]; then
    printf '%s\n' "usage: PREFIX=/path DESTDIR=/stage $0 [prefix]" >&2
    exit 2
fi
if [ "$#" -eq 1 ]; then
    PREFIX=$1
fi

if [ ! -f "$BINARY" ]; then
    printf '%s\n' "error: release binary not found at $BINARY; run 'cargo build --release' first" >&2
    exit 1
fi
if [ ! -x "$BINARY" ]; then
    printf '%s\n' "error: release binary is not executable: $BINARY" >&2
    exit 1
fi

install_file() {
    mode=$1
    source=$2
    destination=$3
    directory=$(dirname -- "$destination")
    mkdir -p "$directory"
    install -m "$mode" "$source" "$destination"
}

install_file 0755 "$BINARY" "$DESTDIR$PREFIX/bin/mtop"
install_file 0644 "$SCRIPT_DIR/mtop.1" "$DESTDIR$PREFIX/share/man/man1/mtop.1"
install_file 0644 "$SCRIPT_DIR/mtop.desktop" "$DESTDIR$PREFIX/share/applications/mtop.desktop"

printf 'installed mtop under %s%s\n' "$DESTDIR" "$PREFIX"
