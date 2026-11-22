#!/usr/bin/env bash
# Per-dyad evidence archive (C3): deterministic tarball + sha256 sidecar.
#
# Usage:
#   ala-archive-dyad.sh --dyad <dir> --out <archive-dir>
#
# Writes <archive-dir>/<dyad-name>.tar.gz and <dyad-name>.sha256, then
# re-verifies the hash. Deterministic bytes (sorted names, fixed mtime
# and ownership, gzip -n) so re-archives hash identically. Archives
# whatever the dyad dir holds: partial (failed) dyads are evidence too,
# so there is no completeness gate here (C4 checks the archive).
# The root journal.jsonl and admission.json ship alongside (whole files,
# copied by the operator, not packed per dyad).
#
# Portable across GNU and BSD/macOS toolchains: uses gtar when present
# (else tar with a --sort=name probe) and sha256sum when present (else
# `shasum -a 256`). Same fixture input yields identical sha256 on macOS
# and Linux.
set -euo pipefail

resolve_tar() {
  local candidate probe
  for candidate in gtar tar; do
    if command -v "$candidate" >/dev/null 2>&1; then
      probe="$(mktemp -d)"
      if (cd "$probe" && "$candidate" --sort=name --mtime='2026-01-01' --owner=0 --group=0 --numeric-owner -cf /dev/null .) >/dev/null 2>&1; then
        rm -rf "$probe"
        echo "$candidate"
        return 0
      fi
      rm -rf "$probe"
    fi
  done
  echo "archive: no tar with --sort=name support (need GNU tar; on macOS: brew install gnu-tar)" >&2
  exit 11
}
TAR="$(resolve_tar)"

SHA256SUM=()
if command -v sha256sum >/dev/null 2>&1; then
  SHA256SUM=(sha256sum)
elif command -v shasum >/dev/null 2>&1; then
  SHA256SUM=(shasum -a 256)
else
  echo "archive: need sha256sum or shasum -a 256" >&2
  exit 11
fi

DYAD=""; OUT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --dyad) DYAD="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    *) echo "archive: unknown argument: $1" >&2; exit 11 ;;
  esac
done
if [ -z "$DYAD" ] || [ -z "$OUT" ]; then echo "archive: --dyad and --out are required" >&2; exit 11; fi
if [ ! -d "$DYAD" ]; then echo "archive: dyad dir missing: $DYAD" >&2; exit 1; fi

NAME="$(basename "$DYAD")"
mkdir -p "$OUT"
TARBALL="$OUT/$NAME.tar.gz"
rm -f "$TARBALL" "$OUT/$NAME.sha256"
"$TAR" --sort=name --mtime='2026-01-01' --owner=0 --group=0 --numeric-owner \
  -c -C "$(dirname "$DYAD")" "$NAME" | gzip -n > "$TARBALL"
"${SHA256SUM[@]}" "$TARBALL" | sed "s|$OUT/||" > "$OUT/$NAME.sha256"
(cd "$OUT" && "${SHA256SUM[@]}" -c "$NAME.sha256")
echo "ARCHIVE OK: $TARBALL ($(du -h "$TARBALL" | cut -f1))"
