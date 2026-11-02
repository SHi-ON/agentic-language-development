#!/usr/bin/env tsx
/**
 * Supervisor journal writer (watchdog aborts, crash notes, operator notes).
 *
 * Usage:
 *   tsx scripts/ala-journal-append.ts --evidence-root <dir> --cohort <c>
 *     --kind <k> --index <n> --state aborted|failed|note --reason <text>
 *     [--wall-milliseconds <n>]
 *
 * Runner verdicts (valid/invalid) and attempt records are never written
 * through this path; the builder refuses them.
 */
import { alaSupervisorJournalEntry, appendAlaJournal } from '../packages/orchestrator/src/ala/ala-runner.js';
import type { AlaCohort, AlaSlotKind } from '../packages/orchestrator/src/ala/ala-seeds.js';

function arg(name: string): string {
  const index = process.argv.indexOf(name);
  const value = index === -1 ? undefined : process.argv[index + 1];
  if (value === undefined || value.startsWith('--')) {
    console.error(`FAIL: missing required ${name}`);
    process.exit(11);
  }
  return value;
}

const evidenceRoot = arg('--evidence-root');
const cohort = arg('--cohort') as AlaCohort;
const kind = arg('--kind') as AlaSlotKind;
const index = Number.parseInt(arg('--index'), 10);
const state = arg('--state');
if (state !== 'aborted' && state !== 'failed' && state !== 'note') {
  console.error('FAIL: --state must be aborted|failed|note (runner verdicts are never supervisor-written)');
  process.exit(11);
}
if (!Number.isInteger(index) || index < 0) {
  console.error('FAIL: --index must be a non-negative integer');
  process.exit(11);
}
const wallFlag = process.argv.indexOf('--wall-milliseconds');
const wallMilliseconds = wallFlag === -1 ? undefined : Number.parseInt(process.argv[wallFlag + 1] as string, 10);
if (wallMilliseconds !== undefined && (!Number.isInteger(wallMilliseconds) || wallMilliseconds < 0)) {
  console.error('FAIL: --wall-milliseconds must be a non-negative integer');
  process.exit(11);
}

await appendAlaJournal(
  evidenceRoot,
  alaSupervisorJournalEntry({ cohort, kind, index, state, reason: arg('--reason'), wallMilliseconds }),
);
console.log(`appended ${state} for ${cohort}/${kind}/${index}`);
