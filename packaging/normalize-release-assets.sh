#!/usr/bin/env bash
set -euo pipefail

version=${1:?usage: normalize-release-assets.sh VERSION [DIST_DIR]}
dist=${2:-dist}
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
  printf 'invalid release version: %s\n' "$version" >&2
  exit 2
fi

shopt -s nullglob
packages_deb=("$dist"/*.deb)
packages_rpm=("$dist"/*.rpm)
packages_arch=("$dist"/*.pkg.tar.zst)
packages_apk=("$dist"/*.apk)
keys=("$dist"/*.rsa.pub)
mac_archives=("$dist"/mtop-macos-*.tar.gz)
windows_archives=("$dist"/mtop-windows-x86_64-*.zip)

if (( ${#packages_deb[@]} != 1 || ${#packages_rpm[@]} != 1 || ${#packages_arch[@]} != 2 || ${#packages_apk[@]} != 2 || ${#keys[@]} != 1 || ${#mac_archives[@]} != 2 || ${#windows_archives[@]} != 1 )); then
  printf 'unexpected release input counts: deb=%d rpm=%d arch=%d apk=%d keys=%d mac=%d windows=%d\n' \
    "${#packages_deb[@]}" "${#packages_rpm[@]}" "${#packages_arch[@]}" "${#packages_apk[@]}" "${#keys[@]}" "${#mac_archives[@]}" "${#windows_archives[@]}" >&2
  exit 1
fi

stage=$(mktemp -d "$dist/.normalized.XXXXXX")
trap 'rm -rf "$stage"' EXIT
mv -- "${packages_deb[0]}" "$stage/mtop-linux-amd64-${version}.deb"
mv -- "${packages_rpm[0]}" "$stage/mtop-linux-x86_64-${version}.rpm"
for package in "${packages_arch[@]}"; do
  name=$(basename "$package")
  if [[ "$name" == *debug* ]]; then
    destination="mtop-linux-x86_64-${version}-debug.pkg.tar.zst"
  else
    destination="mtop-linux-x86_64-${version}.pkg.tar.zst"
  fi
  mv -- "$package" "$stage/$destination"
done
for package in "${packages_apk[@]}"; do
  if [[ "$(basename "$package")" == mtop-doc-* ]]; then
    destination="mtop-linux-x86_64-${version}-docs.apk"
  else
    destination="mtop-linux-x86_64-${version}.apk"
  fi
  mv -- "$package" "$stage/$destination"
done
mv -- "${keys[0]}" "$stage/mtop-alpine-signing-key-${version}.rsa.pub"
for archive in "${mac_archives[@]}" "${windows_archives[@]}"; do
  mv -- "$archive" "$stage/"
done
mv -- "$stage"/* "$dist"/
rmdir "$stage"
trap - EXIT
