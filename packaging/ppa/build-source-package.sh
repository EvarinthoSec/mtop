#!/usr/bin/env bash
set -euo pipefail

TAG=${1:?usage: build-source-package.sh vVERSION [SERIES] [PPA_REVISION] [OUTPUT_DIR]}
SERIES=${2:-resolute}
PPA_REVISION=${3:-1}
OUTPUT_DIR=${4:-"$PWD/dist/ppa-source"}
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)

if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  printf 'error: PPA uploads require a stable vMAJOR.MINOR.PATCH tag: %s\n' "$TAG" >&2
  exit 2
fi
if [[ "$SERIES" != resolute ]]; then
  printf 'error: this recipe currently supports only Ubuntu resolute (26.04); got %s\n' "$SERIES" >&2
  exit 2
fi
if [[ ! "$PPA_REVISION" =~ ^[1-9][0-9]*$ ]]; then
  printf 'error: PPA revision must be a positive integer: %s\n' "$PPA_REVISION" >&2
  exit 2
fi
if ! git -C "$REPO_ROOT" cat-file -e "refs/tags/$TAG^{commit}" 2>/dev/null; then
  git -C "$REPO_ROOT" fetch origin "refs/tags/$TAG:refs/tags/$TAG"
fi

VERSION=${TAG#v}
WORKDIR=$(mktemp -d "${TMPDIR:-/tmp}/mtop-ppa.XXXXXX")
trap 'rm -rf "$WORKDIR"' EXIT HUP INT TERM
SOURCE_DIR="$WORKDIR/mtop-$VERSION"
mkdir -p "$WORKDIR"
git -C "$REPO_ROOT" archive --format=tar --prefix="mtop-$VERSION/" "$TAG" | tar -xf - -C "$WORKDIR"

PACKAGE_VERSION=$(cargo metadata --manifest-path "$SOURCE_DIR/Cargo.toml" --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "mtop"))')
if [[ "$PACKAGE_VERSION" != "$VERSION" ]]; then
  printf 'error: tag %s has Cargo version %s\n' "$TAG" "$PACKAGE_VERSION" >&2
  exit 1
fi

cp -a "$SCRIPT_DIR/debian" "$SOURCE_DIR/debian"
mkdir -p "$SOURCE_DIR/.cargo" "$SOURCE_DIR/packaging/ppa"
cp "$SCRIPT_DIR/install-vendor-licenses.py" "$SOURCE_DIR/packaging/ppa/install-vendor-licenses.py"
(cd "$SOURCE_DIR" && cargo vendor --locked vendor > .cargo/config.toml)

TAG_EPOCH=$(git -C "$REPO_ROOT" log -1 --format=%ct "$TAG")
CHANGELOG_DATE=$(date -R -u --date="@$TAG_EPOCH")
DEBIAN_VERSION="${VERSION}-1~ppa1~ubuntu26.04.${PPA_REVISION}"
cat > "$SOURCE_DIR/debian/changelog" <<EOF
mtop (${DEBIAN_VERSION}) resolute; urgency=medium

  * Publish upstream mtop ${VERSION} to the Launchpad PPA.

 -- EvarinthoSec <evarin@anantix.network>  ${CHANGELOG_DATE}
EOF

mkdir -p "$OUTPUT_DIR"
(
  cd "$SOURCE_DIR"
  dpkg-buildpackage -S -sa -us -uc -d
)
shopt -s nullglob
FILES=("$WORKDIR"/mtop_"$DEBIAN_VERSION".*)
if [[ ${#FILES[@]} -lt 4 ]]; then
  printf 'error: expected .dsc, .changes, orig, and Debian source archives for %s\n' "$DEBIAN_VERSION" >&2
  printf 'found: %s\n' "${FILES[*]:-none}" >&2
  exit 1
fi
cp -- "${FILES[@]}" "$OUTPUT_DIR/"
if [[ ! -f "$OUTPUT_DIR/mtop_${DEBIAN_VERSION}_source.changes" || ! -f "$OUTPUT_DIR/mtop_${DEBIAN_VERSION}.dsc" ]]; then
  printf 'error: source package output is incomplete\n' >&2
  exit 1
fi
printf 'built unsigned Launchpad source package %s for %s\n' "$DEBIAN_VERSION" "$SERIES"
printf 'output: %s\n' "$OUTPUT_DIR"
