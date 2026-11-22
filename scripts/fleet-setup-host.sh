#!/usr/bin/env bash
# Fleet host setup for the Rust dyad campaign.
#
# Idempotent: re-running converges (matching toolchain versions are kept,
# checkouts are fetched and moved to the pinned refs, install/check rerun).
# Recorded: prints a record block the operator appends to the campaign log.
#
# Usage:
#   fleet-setup-host.sh --alias TVPS \
#     --fork git@github.com:SHi-ON/agentic-language-development.git --ref <sha> \
#     --paper git@github.com:SHi-ON/agentic-language-development-paper.git --paper-ref <sha> \
#     --workdir ~/ald [--skip-check]
#
# No host gets write access to another host's evidence root (nothing here
# mounts or shares evidence). The paper checkout is retained read-only as
# A7 driver input but is no longer installed or tested here; this script
# never modifies it beyond checkout.
set -euo pipefail

ALIAS=""; FORK=""; REF=""; PAPER=""; PAPER_REF=""; WORKDIR=""; SKIP_CHECK="no"
while [ $# -gt 0 ]; do
  case "$1" in
    --alias) ALIAS="$2"; shift 2 ;;
    --fork) FORK="$2"; shift 2 ;;
    --ref) REF="$2"; shift 2 ;;
    --paper) PAPER="$2"; shift 2 ;;
    --paper-ref) PAPER_REF="$2"; shift 2 ;;
    --workdir) WORKDIR="$2"; shift 2 ;;
    --skip-check) SKIP_CHECK="yes"; shift ;;
    *) echo "unknown argument: $1" >&2; exit 11 ;;
  esac
done
for var in ALIAS FORK REF PAPER PAPER_REF WORKDIR; do
  if [ -z "${!var}" ]; then echo "missing --$(echo "$var" | tr 'A-Z' 'a-z')" >&2; exit 11; fi
done

echo "=== [${ALIAS}] egress ==="
curl -sfI --max-time 20 https://sh.rustup.rs -o /dev/null
curl -sfI --max-time 20 https://static.crates.io -o /dev/null
echo "egress to sh.rustup.rs and static.crates.io: OK"

export PATH="$HOME/.cargo/bin:$PATH"

echo "=== [${ALIAS}] rustup ==="
if command -v rustup >/dev/null 2>&1; then
  echo "rustup already present: $(command -v rustup)"
else
  curl --proto '=https' --tlsv1.2 -sSf --max-time 300 https://sh.rustup.rs -o /tmp/rustup-init.sh
  sh /tmp/rustup-init.sh -y --default-toolchain none --profile minimal
  echo "installed: $(command -v rustup)"
fi

checkout() {
  local url="$1" ref="$2" dir="$3" name="$4"
  echo "=== [${ALIAS}] ${name} @ ${ref} ==="
  if [ -d "${dir}/.git" ]; then
    git -C "$dir" fetch origin
  else
    mkdir -p "$(dirname "$dir")"
    git clone "$url" "$dir"
  fi
  git -C "$dir" checkout --detach "$ref"
  local got want
  got="$(git -C "$dir" rev-parse HEAD)"
  want="$(git -C "$dir" rev-parse "$ref")"
  if [ "$got" != "$want" ]; then echo "${name} checkout mismatch: ${got} != ${want}" >&2; exit 1; fi
  if [ -n "$(git -C "$dir" status --short)" ]; then echo "${name} worktree is dirty" >&2; exit 1; fi
  echo "${name} clean at ${got}"
}

checkout "$FORK" "$REF" "${WORKDIR}/fork" "fork"
checkout "$PAPER" "$PAPER_REF" "${WORKDIR}/paper" "paper"

RUST_TOOLCHAIN="$(grep -E '^channel[[:space:]]*=' "${WORKDIR}/fork/rust-toolchain.toml" | sed -E 's/^channel[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/')"
if [ -z "$RUST_TOOLCHAIN" ]; then echo "no channel in fork rust-toolchain.toml" >&2; exit 11; fi
echo "=== [${ALIAS}] rust ${RUST_TOOLCHAIN} (pinned by fork rust-toolchain.toml) ==="
rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal
rustup default "$RUST_TOOLCHAIN"
[ "$(rustc --version | cut -d' ' -f2)" = "${RUST_TOOLCHAIN}" ]

echo "=== [${ALIAS}] install ==="
(cd "${WORKDIR}/fork" && cargo fetch --locked)

if [ "$SKIP_CHECK" = "yes" ]; then
  echo "=== [${ALIAS}] check SKIPPED ==="
  CHECK_RESULT="SKIPPED"
else
  echo "=== [${ALIAS}] check ==="
  (cd "${WORKDIR}/fork" && cargo fmt --all -- --check \
    && cargo clippy --workspace --all-targets --locked -- -D warnings \
    && cargo test --workspace --locked)
  CHECK_RESULT="PASSED"
fi

echo "=== [${ALIAS}] RECORD ==="
echo "alias: ${ALIAS} ($(uname -m))"
echo "rustc: $(rustc --version)"
echo "cargo: $(cargo --version)"
echo "rustup: $(rustup toolchain list | tr '\n' ' ')"
echo "python: $(python3 --version)"
echo "fork: ${FORK} @ $(git -C "${WORKDIR}/fork" rev-parse HEAD)"
echo "paper: ${PAPER} @ $(git -C "${WORKDIR}/paper" rev-parse HEAD)"
echo "check: ${CHECK_RESULT}"
