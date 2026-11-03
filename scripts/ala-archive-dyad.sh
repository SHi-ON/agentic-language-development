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
set -euo pipefail

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
tar --sort=name --mtime='2026-01-01' --owner=0 --group=0 --numeric-owner \
  -c -C "$(dirname "$DYAD")" "$NAME" | gzip -n > "$TARBALL"
sha256sum "$TARBALL" | sed "s|$OUT/||" > "$OUT/$NAME.sha256"
(cd "$OUT" && sha256sum -c "$NAME.sha256")
echo "ARCHIVE OK: $TARBALL ($(du -h "$TARBALL" | cut -f1))"
