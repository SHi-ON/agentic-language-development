#!/usr/bin/env bash
# Per-dyad watchdog supervisor (C2): launches one runner invocation,
# enforces the v2 per-dyad guard (CPU-hours, wall minutes, retained
# GiB from the frozen amendment), and journals `aborted` with measured
# cost on breach. Findings go to stderr (captured in the dyad log).
#
# Linux-only: requires /proc, procps `ps --ppid`, and getconf CLK_TCK.
# Not macOS/BSD-portable (fails fast below when /proc is absent).
#
# Usage:
#   ala-watchdog.sh --cohort A --kind primary --index 0 \
#     --evidence-root <dir> --amendment <amendment.v2.json> \
#     --manifest-sha256 <hex> [--ala <ala-binary>] [--poll-seconds 60] \
#     -- <runner command...>
#
# --amendment points at operator-supplied JSON shaped like:
#   {"newAllowance": {"perDyadCpuHoursMax": <num>, "perDyadWallMinutesMax": <num>,
#                     "perDyadRetainedGiBMax": <num>, "campaignCpuHoursMax": <num|null>}}
#   (no in-repo example exists; the campaign supplies this file).
# --manifest-sha256 is the dyad manifest's sha256 (from the run inputs or
#   receipt); the watchdog cannot re-derive it. Journaled bodies carry
#   kind=terminal with state aborted|note through `ala journal`, whose
#   hash-chained entries verify like run_dyad's own journal lines.
#
# Exit codes: runner's code when it exits within guard; 3 on guard
# breach (process terminated, `aborted` journalled); 4 when the run
# completed past a guard (guard finding journalled as `note`);
# 11 on usage/guard-source errors.
#
# No `set -e`: nonzero runner exits and reaped-PID races are handled
# explicitly so breach journaling always runs.
set -uo pipefail

if [ ! -d "/proc/$$" ]; then echo "watchdog: Linux /proc required" >&2; exit 11; fi
if ! ps --ppid $$ >/dev/null 2>&1; then echo "watchdog: procps ps --ppid required" >&2; exit 11; fi

COHORT=""; KIND=""; INDEX=""; EVIDENCE_ROOT=""; AMENDMENT=""; MANIFEST_SHA256=""; ALA="ala"; POLL=60
while [ $# -gt 0 ]; do
  case "$1" in
    --cohort) COHORT="$2"; shift 2 ;;
    --kind) KIND="$2"; shift 2 ;;
    --index) INDEX="$2"; shift 2 ;;
    --evidence-root) EVIDENCE_ROOT="$2"; shift 2 ;;
    --amendment) AMENDMENT="$2"; shift 2 ;;
    --manifest-sha256) MANIFEST_SHA256="$2"; shift 2 ;;
    --ala) ALA="$2"; shift 2 ;;
    --poll-seconds) POLL="$2"; shift 2 ;;
    --) shift; break ;;
    *) echo "watchdog: unknown argument: $1" >&2; exit 11 ;;
  esac
done
for var in COHORT KIND INDEX EVIDENCE_ROOT AMENDMENT MANIFEST_SHA256; do
  if [ -z "${!var}" ]; then echo "watchdog: missing --$(echo "$var" | tr 'A-Z_' 'a-z-')" >&2; exit 11; fi
done
if [ $# -eq 0 ]; then echo "watchdog: missing runner command after --" >&2; exit 11; fi

GUARDS="$(python3 - "$AMENDMENT" <<'EOF'
import json, sys
a = json.load(open(sys.argv[1]))
allow = a.get('newAllowance', {})
print(allow.get('perDyadCpuHoursMax'), allow.get('perDyadWallMinutesMax'), allow.get('perDyadRetainedGiBMax'), allow.get('campaignCpuHoursMax'))
EOF
)"
read -r CPU_MAX WALL_MAX GIB_MAX CAMPAIGN_MAX <<<"$GUARDS"
for value in "$CPU_MAX" "$WALL_MAX" "$GIB_MAX"; do
  if ! python3 -c "float('$value')" 2>/dev/null; then echo "watchdog: guard source has non-numeric per-dyad caps" >&2; exit 11; fi
done
if { [ "$COHORT" = "A" ] || [ "$COHORT" = "B" ]; } && [ "$CAMPAIGN_MAX" = "None" ]; then
  echo "watchdog: cohort $COHORT slots may not run while campaignCpuHoursMax is null (pre-G2)" >&2
  exit 11
fi

LABEL="$COHORT/$KIND/$INDEX"
DYAD_DIR="$EVIDENCE_ROOT/$COHORT/$KIND-$(printf '%04d' "$INDEX")"
HZ="$(getconf CLK_TCK)"
START_EPOCH="$(date +%s)"

log() { echo "$(date -u +%FT%TZ) watchdog[$LABEL] $*" >&2; }

# Whole-tree CPU seconds (live PIDs; wrappers churn ~0 CPU, the worker
# lives whole-run, so the approximation misses only reaped children).
tree_pids() {
  local pid="$1" child
  echo "$pid"
  for child in $(ps --ppid "$pid" -o pid= 2>/dev/null); do
    tree_pids "$child"
  done
}

tree_cpu_seconds() {
  local total=0 pid ticks
  for pid in $(tree_pids "$1"); do
    ticks="$(awk '{print $14 + $15}' "/proc/$pid/stat" 2>/dev/null || true)"
    if [ -n "$ticks" ]; then total="$(python3 -c "print($total + $ticks)")"; fi
  done
  python3 -c "print($total / $HZ)"
}

tree_rss_kb() {
  local total=0 pid rss
  for pid in $(tree_pids "$1"); do
    rss="$(awk '/VmRSS/ {print $2}' "/proc/$pid/status" 2>/dev/null || true)"
    if [ -n "$rss" ]; then total=$((total + rss)); fi
  done
  echo "$total"
}

dyad_gib() {
  if [ -d "$DYAD_DIR" ]; then du -sb "$DYAD_DIR" 2>/dev/null | awk '{print $1 / 1073741824}'; else echo 0; fi
}

journal() {
  # args: state reason wall_ms cpu_hours gib (cpu/gib may be empty)
  local state="$1" reason="$2" wall_ms="$3" cpu="$4" gib="$5" body
  body="$(STATE="$state" REASON="$reason" WALL_MS="$wall_ms" CPU="$cpu" GIB="$gib" \
    COHORT="$COHORT" KIND="$KIND" INDEX="$INDEX" MSHA="$MANIFEST_SHA256" python3 - <<'EOF'
import json, os
body = {
    "cohort": os.environ["COHORT"],
    "slotKind": os.environ["KIND"],
    "slotIndex": "%04d" % int(os.environ["INDEX"]),
    "manifestSha256": os.environ["MSHA"],
    "kind": "terminal",
    "state": os.environ["STATE"],
    "reason": os.environ["REASON"],
    "wallMilliseconds": int(os.environ["WALL_MS"]),
}
if os.environ["CPU"]:
    body["cpuHours"] = float(os.environ["CPU"])
if os.environ["GIB"]:
    body["retainedGiB"] = float(os.environ["GIB"])
print(json.dumps(body))
EOF
  )" || { log "JOURNAL BODY FAILED (state=$state)"; return 0; }
  "$ALA" journal --journal "$EVIDENCE_ROOT/journal.jsonl" --line "$body" >&2 || \
    log "JOURNAL APPEND FAILED (state=$state)"
}

log "start caps cpu=${CPU_MAX}h wall=${WALL_MAX}m gib=${GIB_MAX} poll=${POLL}s"
"$@" &
CHILD="$!"
BREACH=""
while kill -0 "$CHILD" 2>/dev/null; do
  sleep "$POLL"
  if ! kill -0 "$CHILD" 2>/dev/null; then break; fi
  CPU_H="$(python3 -c "print($(tree_cpu_seconds "$CHILD") / 3600)")"
  WALL_M="$(python3 -c "print(($(date +%s) - $START_EPOCH) / 60)")"
  GIB="$(dyad_gib)"
  RSS="$(tree_rss_kb "$CHILD")"
  log "sample cpu_h=$CPU_H wall_m=$WALL_M gib=$GIB rss_kb=$RSS"
  if python3 -c "exit(0 if float('$CPU_H') > float('$CPU_MAX') else 1)"; then BREACH="cpu-hours $CPU_H > $CPU_MAX"; break; fi
  if python3 -c "exit(0 if float('$WALL_M') > float('$WALL_MAX') else 1)"; then BREACH="wall minutes $WALL_M > $WALL_MAX"; break; fi
  if python3 -c "exit(0 if float('$GIB') > float('$GIB_MAX') else 1)"; then BREACH="retained GiB $GIB > $GIB_MAX"; break; fi
done

WALL_MS="$((($(date +%s) - START_EPOCH) * 1000))"
if [ -n "$BREACH" ]; then
  log "BREACH $BREACH; terminating tree $CHILD"
  for pid in $(tree_pids "$CHILD"); do kill -TERM "$pid" 2>/dev/null || true; done
  for _ in $(seq 1 30); do kill -0 "$CHILD" 2>/dev/null || break; sleep 1; done
  if kill -0 "$CHILD" 2>/dev/null; then
    log "SIGTERM ignored; SIGKILL tree $CHILD"
    for pid in $(tree_pids "$CHILD"); do kill -KILL "$pid" 2>/dev/null || true; done
  fi
  wait "$CHILD" 2>/dev/null || true
  journal aborted "watchdog: per-dyad guard breached ($BREACH)" "$WALL_MS" "$CPU_H" "$GIB"
  log "dyad aborted and journalled"
  exit 3
fi

wait "$CHILD"
CODE="$?"
# Final verification: no dyad may complete past a guard.
# (CPU at completion comes from the dyad receipt; the tree is reaped.)
GIB="$(dyad_gib)"
WALL_M="$(python3 -c "print($WALL_MS / 60000)")"
if python3 -c "exit(0 if float('$WALL_M') > float('$WALL_MAX') else 1)" || \
   python3 -c "exit(0 if float('$GIB') > float('$GIB_MAX') else 1)"; then
  # CPU at completion comes from the dyad receipt (the tree is reaped);
  # wall and disk are measured here. A breach past completion is a
  # campaign anomaly: the terminal receipt already exists, so the
  # watchdog journals a `note` finding instead of rewriting history.
  journal note "watchdog: dyad completed past its guard (wall_m=$WALL_M gib=$GIB)" "$WALL_MS" "" "$GIB"
  log "GUARD FINDING at completion (exit $CODE kept)"
  exit 4
fi
log "runner exited $CODE within guard"
exit "$CODE"
