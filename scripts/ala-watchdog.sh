#!/usr/bin/env bash
# Per-dyad watchdog supervisor (C2): launches one runner invocation,
# enforces the v2 per-dyad guard (CPU-hours, wall minutes, retained
# GiB from the frozen amendment), and journals `aborted` with measured
# cost on breach. Findings go to stderr (captured in the dyad log).
#
# Usage:
#   ala-watchdog.sh --cohort A --kind primary --index 0 \
#     --evidence-root <dir> --amendment <amendment.v2.json> \
#     --journal-append <ala-journal-append.ts> [--poll-seconds 60] \
#     -- <runner command...>
#
# Exit codes: runner's code when it exits within guard; 3 on guard
# breach (process terminated, `aborted` journalled); 4 when the run
# completed past a guard (guard finding journalled as `note`);
# 11 on usage/guard-source errors.
#
# No `set -e`: nonzero runner exits and reaped-PID races are handled
# explicitly so breach journaling always runs.
set -uo pipefail

COHORT=""; KIND=""; INDEX=""; EVIDENCE_ROOT=""; AMENDMENT=""; JOURNAL_APPEND=""; POLL=60
while [ $# -gt 0 ]; do
  case "$1" in
    --cohort) COHORT="$2"; shift 2 ;;
    --kind) KIND="$2"; shift 2 ;;
    --index) INDEX="$2"; shift 2 ;;
    --evidence-root) EVIDENCE_ROOT="$2"; shift 2 ;;
    --amendment) AMENDMENT="$2"; shift 2 ;;
    --journal-append) JOURNAL_APPEND="$2"; shift 2 ;;
    --poll-seconds) POLL="$2"; shift 2 ;;
    --) shift; break ;;
    *) echo "watchdog: unknown argument: $1" >&2; exit 11 ;;
  esac
done
for var in COHORT KIND INDEX EVIDENCE_ROOT AMENDMENT JOURNAL_APPEND; do
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
  npx tsx "$JOURNAL_APPEND" --evidence-root "$EVIDENCE_ROOT" --cohort "$COHORT" \
    --kind "$KIND" --index "$INDEX" --state "$1" --reason "$2" --wall-milliseconds "$3" >&2 || \
    log "JOURNAL APPEND FAILED (state=$1)"
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
  journal aborted "watchdog: per-dyad guard breached ($BREACH)" "$WALL_MS"
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
  journal note "watchdog: dyad completed past its guard (wall_m=$WALL_M gib=$GIB)" "$WALL_MS"
  log "GUARD FINDING at completion (exit $CODE kept)"
  exit 4
fi
log "runner exited $CODE within guard"
exit "$CODE"
