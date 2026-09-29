#!/usr/bin/env bash
set -euo pipefail

if [[ -n "${LAUNCHPAD_GPG_PASSPHRASE_FILE:-}" ]]; then
  exec gpg --batch --yes --pinentry-mode loopback --passphrase-file "$LAUNCHPAD_GPG_PASSPHRASE_FILE" "$@"
fi
exec gpg --batch --yes "$@"
