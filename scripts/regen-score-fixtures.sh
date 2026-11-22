#!/usr/bin/env bash
# Regenerate the locked-mean-anchored score fixtures (P0-FORK-12).
#
# Rewrites crates/ald-cli/tests/fixtures/cohort-{A,B}/primary-0000/ from the
# locked means pinned in regen_score_fixtures.py, then runs the
# score_fixture suite as a self-check. On a fresh clone the regen must
# leave `git diff` empty.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
python3 "$ROOT/scripts/regen_score_fixtures.py"
cargo test -p ald-cli --test score_fixture
