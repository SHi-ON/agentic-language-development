#!/usr/bin/env bash
# Fleet host setup for the corrected ALA-LV campaign (B1, reused in C0).
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
# mounts or shares evidence). The paper checkout is read-only input for
# the A7 driver; this script never modifies it beyond checkout.
set -euo pipefail

NODE_VERSION="24.22.0"
PNPM_VERSION="12.9.1"

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
curl -sfI --max-time 20 https://nodejs.org/dist/ -o /dev/null
curl -sf --max-time 20 https://registry.npmjs.org/pnpm -o /dev/null
echo "egress to nodejs.org and the npm registry: OK"

export PATH="$HOME/.local/bin:$PATH"
mkdir -p "$HOME/.local/bin" "$HOME/.cache/ald-node"

case "$(uname -m)" in
  x86_64) NODE_ARCH="x64" ;;
  aarch64) NODE_ARCH="arm64" ;;
  *) echo "unsupported architecture: $(uname -m)" >&2; exit 11 ;;
esac
echo "=== [${ALIAS}] node ${NODE_VERSION} (${NODE_ARCH}) ==="
if [ "$(node --version 2>/dev/null || true)" = "v${NODE_VERSION}" ]; then
  echo "node v${NODE_VERSION} already present: $(command -v node)"
else
  TARBALL="$HOME/.cache/ald-node/node-v${NODE_VERSION}-linux-${NODE_ARCH}.tar.xz"
  if [ ! -f "$TARBALL" ]; then
    curl -sfL --max-time 300 -o "$TARBALL" "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${NODE_ARCH}.tar.xz"
  fi
  rm -rf "$HOME"/.local/node-v"${NODE_VERSION}"-linux-*
  tar -xJf "$TARBALL" -C "$HOME/.local"
  ln -sf "$HOME/.local/node-v${NODE_VERSION}-linux-${NODE_ARCH}/bin/node" "$HOME/.local/bin/node"
  ln -sf "$HOME/.local/node-v${NODE_VERSION}-linux-${NODE_ARCH}/bin/npm" "$HOME/.local/bin/npm"
  ln -sf "$HOME/.local/node-v${NODE_VERSION}-linux-${NODE_ARCH}/bin/npx" "$HOME/.local/bin/npx"
  ln -sf "$HOME/.local/node-v${NODE_VERSION}-linux-${NODE_ARCH}/bin/corepack" "$HOME/.local/bin/corepack"
  echo "installed: $(node --version) at $(command -v node)"
fi
[ "$(node --version)" = "v${NODE_VERSION}" ]

echo "=== [${ALIAS}] pnpm ${PNPM_VERSION} ==="
if [ "$(pnpm --version 2>/dev/null || true)" = "${PNPM_VERSION}" ]; then
  echo "pnpm ${PNPM_VERSION} already present: $(command -v pnpm)"
else
  npm install -g --prefix "$HOME/.local" "pnpm@${PNPM_VERSION}"
  echo "installed: pnpm $(pnpm --version) at $(command -v pnpm)"
fi
[ "$(pnpm --version)" = "${PNPM_VERSION}" ]

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

echo "=== [${ALIAS}] install ==="
pnpm --dir "${WORKDIR}/fork" install --frozen-lockfile

if [ "$SKIP_CHECK" = "yes" ]; then
  echo "=== [${ALIAS}] check SKIPPED ==="
  CHECK_RESULT="SKIPPED"
else
  echo "=== [${ALIAS}] check ==="
  pnpm --dir "${WORKDIR}/fork" run check
  CHECK_RESULT="PASSED"
fi

echo "=== [${ALIAS}] RECORD ==="
echo "alias: ${ALIAS} ($(uname -m))"
echo "node: $(node --version) ($(command -v node))"
echo "pnpm: $(pnpm --version) ($(command -v pnpm))"
echo "python: $(python3 --version)"
echo "fork: ${FORK} @ $(git -C "${WORKDIR}/fork" rev-parse HEAD)"
echo "paper: ${PAPER} @ $(git -C "${WORKDIR}/paper" rev-parse HEAD)"
echo "check: ${CHECK_RESULT}"
