/**
 * V2 per-dyad retained-file manifest, completeness check, and diagnostic
 * extraction (A7). A v2 dyad is audit-eligible exactly when it retains:
 *
 * - parent-bundle/ (training evidence directory)
 * - training-ledger-heads.json (heads sidecar; branch payloads carry its
 *   digest, never the embedded arrays)
 * - treatment-batch-<partition>-<role>.json for the full 3x2 grid of
 *   validation-fit, validation-selection, within-support-test by
 *   baby-a, baby-b (gate-validated version-2 blocks, committed BEFORE
 *   any branch runs)
 * - slot-NNNN/schedule.json per executed slot ({commitment,
 *   partition, executedCaseId})
 * - slot-NNNN/branches/ with exactly 7 branch bundles per slot
 * - dyad-receipt.json with state valid and per-case commitments
 *
 * plus a journal export: the evidence-root journal lines for this
 * dyad's cohort/kind/index, copied out for archiving (the root
 * journal stays the source of truth). C4 completeness is checked
 * against this module; nothing here redefines construction.
 *
 * Extraction projects each block file to evidence-free diagnostic
 * cases through the same mapper construction uses
 * ({@link toAlaTreatmentDiagnosticCases}): recorded selection and
 * delivery tokens are verdict inputs, but no receiver actions,
 * scores, or message-stream tokens ever appear. The driver feeds
 * each role's whole-dyad batch to the paper diagnostic's own
 * verdict() (imported, never reimplemented).
 */
import { existsSync } from 'node:fs';
import { readdir, readFile, stat } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { hashCanonical } from '@ald/hashing';
import {
  LV01_LEDGER_TREATMENT_BATCH_DOMAIN_V2,
  toAlaTreatmentDiagnosticCases,
  type Lv01ValidTreatedCase,
} from '../experiments/lv01-ledger-treatments.js';
import type { AlaTreatmentDiagnosticCase } from '../experiments/lv01-treatment-gate.js';
import type { AlaDyadSchedulePartition } from './ala-runner.js';

export const V2_DIAGNOSTIC_ROWS_SCHEMA = 'ala-v2-diagnostic-rows/v1' as const;

export const V2_BLOCK_PARTITIONS: readonly AlaDyadSchedulePartition[] = [
  'validation-fit',
  'validation-selection',
  'within-support-test',
];

export const V2_BLOCK_ROLES: readonly ('baby-a' | 'baby-b')[] = ['baby-a', 'baby-b'];

const V2_BRANCHES_PER_SLOT = 7;

function fail(message: string): never {
  throw new Error(`v2 retained: ${message}`);
}

export function v2BlockFileName(partition: AlaDyadSchedulePartition, role: 'baby-a' | 'baby-b'): string {
  return `treatment-batch-${partition}-${role}.json`;
}

interface V2SlotRecord {
  readonly slot: number;
  readonly partition: AlaDyadSchedulePartition;
  readonly executedCaseId: string;
  readonly role: 'baby-a' | 'baby-b';
}

function parseExecutedCaseId(executedCaseId: string, where: string): {
  partition: AlaDyadSchedulePartition;
  role: 'baby-a' | 'baby-b';
} {
  const match = /^(validation-fit|validation-selection|within-support-test):(baby-a|baby-b):(\d{4})$/.exec(executedCaseId);
  if (match === null) fail(`${where} carries a malformed executed case id ${executedCaseId}`);
  return {
    partition: (match as RegExpExecArray)[1] as AlaDyadSchedulePartition,
    role: (match as RegExpExecArray)[2] as 'baby-a' | 'baby-b',
  };
}

async function readJson(path: string, where: string): Promise<unknown> {
  let raw: string;
  try {
    raw = (await readFile(path, 'utf8')) as string;
  } catch {
    fail(`${where} is unreadable at ${path}`);
  }
  try {
    return JSON.parse(raw as string) as unknown;
  } catch {
    fail(`${where} is not valid JSON at ${path}`);
  }
}

async function readV2SlotRecords(dyadDir: string): Promise<{ records: V2SlotRecord[]; problems: string[] }> {
  const problems: string[] = [];
  let entries: string[];
  try {
    entries = (await readdir(dyadDir)).sort();
  } catch {
    return { records: [], problems: [`dyad directory ${dyadDir} is unreadable`] };
  }
  const slots = entries
    .map((entry) => /^slot-(\d{4})$/.exec(entry))
    .filter((match): match is RegExpExecArray => match !== null)
    .map((match) => Number(match[1]))
    .sort((a, b) => a - b);
  if (slots.length === 0) {
    return { records: [], problems: ['dyad retains no slot directories'] };
  }
  const records: V2SlotRecord[] = [];
  for (const slot of slots) {
    const stem = `slot-${String(slot).padStart(4, '0')}`;
    const record = (await readJson(join(dyadDir, stem, 'schedule.json'), `${stem} schedule record`)) as {
      partition?: unknown;
      executedCaseId?: unknown;
    };
    if (!(V2_BLOCK_PARTITIONS as readonly string[]).includes(record.partition as string)) {
      problems.push(`${stem} schedule record names partition ${JSON.stringify(record.partition)}`);
      continue;
    }
    if (typeof record.executedCaseId !== 'string' || record.executedCaseId.length === 0) {
      problems.push(`${stem} schedule record carries no executed case id`);
      continue;
    }
    const parsed = parseExecutedCaseId(record.executedCaseId, `${stem} schedule record`);
    if (parsed.partition !== record.partition) {
      problems.push(`${stem} schedule record partition ${record.partition} disagrees with ${record.executedCaseId}`);
      continue;
    }
    let branches: string[];
    try {
      branches = await readdir(join(dyadDir, stem, 'branches'));
    } catch {
      problems.push(`${stem} has no readable branches directory`);
      continue;
    }
    if (branches.length !== V2_BRANCHES_PER_SLOT) {
      problems.push(`${stem} retains ${branches.length} branch bundles, want ${V2_BRANCHES_PER_SLOT}`);
      continue;
    }
    records.push({ slot, partition: parsed.partition, role: parsed.role, executedCaseId: record.executedCaseId });
  }
  return { records, problems };
}

interface V2ValidatedBlock {
  readonly problems: string[];
  readonly cases: Lv01ValidTreatedCase[] | undefined;
}

function validateV2BlockFile(
  partition: AlaDyadSchedulePartition,
  role: 'baby-a' | 'baby-b',
  executed: ReadonlySet<string>,
  parsed: unknown,
): V2ValidatedBlock {
  const file = v2BlockFileName(partition, role);
  const problems: string[] = [];
  const doc = parsed as {
    version?: unknown;
    derangementSeed?: unknown;
    cases?: unknown;
    batchCommitment?: unknown;
  };
  if (doc.version !== 2) {
    return { problems: [`${file} is version ${JSON.stringify(doc.version)}, want 2`], cases: undefined };
  }
  if (typeof doc.derangementSeed !== 'string' || doc.derangementSeed.length === 0) {
    problems.push(`${file} carries no derangement seed`);
  }
  if (!Array.isArray(doc.cases) || doc.cases.length === 0) {
    problems.push(`${file} carries no treatment cases`);
    return { problems, cases: undefined };
  }
  const cases = doc.cases as Lv01ValidTreatedCase[];
  const seen = new Set<string>();
  for (const entry of cases) {
    if (typeof entry.caseId !== 'string' || entry.caseId.length === 0) {
      problems.push(`${file} has a case with no record id`);
      continue;
    }
    if (seen.has(entry.caseId)) problems.push(`${file} repeats record id ${entry.caseId}`);
    seen.add(entry.caseId);
    if (entry.receiverRole !== role) {
      problems.push(`${file} record ${entry.caseId} names role ${entry.receiverRole}`);
    }
    if (typeof entry.scheduledCaseId !== 'string' || entry.scheduledCaseId.length === 0) {
      problems.push(`${file} record ${entry.caseId} carries no scheduled identity`);
    }
  }
  const scheduledCounts = new Map<string, number>();
  for (const entry of cases) {
    if (typeof entry.scheduledCaseId !== 'string' || entry.scheduledCaseId.length === 0) continue;
    scheduledCounts.set(entry.scheduledCaseId, (scheduledCounts.get(entry.scheduledCaseId) ?? 0) + 1);
  }
  const scheduled = new Set(scheduledCounts.keys());
  for (const id of [...executed].sort()) {
    if (!scheduled.has(id)) problems.push(`${file} is missing executed case ${id}`);
  }
  for (const id of [...scheduled].sort()) {
    if (!executed.has(id)) problems.push(`${file} covers unexecuted case ${id}`);
  }
  if (new Set(scheduledCounts.values()).size > 1) {
    problems.push(`${file} has uneven record counts across scheduled cases`);
  }
  if (typeof doc.batchCommitment !== 'string' || doc.batchCommitment.length === 0) {
    problems.push(`${file} carries no batch commitment`);
  } else if (problems.length === 0) {
    const recomputed = hashCanonical(LV01_LEDGER_TREATMENT_BATCH_DOMAIN_V2, {
      derangementSeed: doc.derangementSeed,
      cases,
    });
    if (recomputed !== doc.batchCommitment) {
      problems.push(`${file} batch commitment does not match its cases (edited after commit)`);
    }
  }
  return { problems, cases: problems.length === 0 ? cases : undefined };
}

export interface V2CompletenessReport {
  readonly dyad: string;
  readonly ok: boolean;
  readonly slots: number;
  readonly blocks: number;
  readonly problems: readonly string[];
}

/**
 * Audit-eligibility check for one retained v2 dyad. Collects every
 * defect (never first-fail) so the census can report the full gap.
 */
export async function checkV2DyadComplete(dyadDir: string): Promise<V2CompletenessReport> {
  const dyad = basename(dyadDir);
  const problems: string[] = [];
  const { records, problems: slotProblems } = await readV2SlotRecords(dyadDir);
  problems.push(...slotProblems);

  let receipt: {
    state?: unknown;
    casesAttempted?: unknown;
    caseCommitments?: unknown;
    cohort?: unknown;
    kind?: unknown;
    index?: unknown;
  } | undefined;
  try {
    receipt = (await readJson(join(dyadDir, 'dyad-receipt.json'), 'dyad receipt')) as typeof receipt;
  } catch (error) {
    problems.push((error as Error).message.replace(/^v2 retained: /, ''));
  }
  if (receipt !== undefined) {
    if (receipt.state !== 'valid') {
      problems.push(`dyad receipt state is ${JSON.stringify(receipt.state)}, want "valid"`);
    }
    if (receipt.casesAttempted !== records.length) {
      problems.push(`dyad receipt attempted ${JSON.stringify(receipt.casesAttempted)} cases over ${records.length} slots`);
    }
    if (!Array.isArray(receipt.caseCommitments) || receipt.caseCommitments.length !== records.length) {
      problems.push('dyad receipt commitments do not cover every slot');
    }
    for (const field of ['cohort', 'kind', 'index'] as const) {
      if (receipt[field] === undefined) problems.push(`dyad receipt carries no ${field}`);
    }
  }

  try {
    const bundle = await stat(join(dyadDir, 'parent-bundle'));
    if (!bundle.isDirectory()) problems.push('parent-bundle is not a directory');
  } catch {
    problems.push('parent-bundle is missing');
  }
  try {
    await readJson(join(dyadDir, 'training-ledger-heads.json'), 'heads sidecar');
  } catch (error) {
    problems.push((error as Error).message.replace(/^v2 retained: /, ''));
  }

  const executedByBlock = new Map<string, Set<string>>();
  for (const record of records) {
    const key = `${record.partition}:${record.role}`;
    const group = executedByBlock.get(key) ?? new Set<string>();
    group.add(record.executedCaseId);
    executedByBlock.set(key, group);
  }
  const expectedBlocks: { partition: AlaDyadSchedulePartition; role: 'baby-a' | 'baby-b' }[] = [];
  for (const partition of V2_BLOCK_PARTITIONS) {
    for (const role of V2_BLOCK_ROLES) {
      expectedBlocks.push({ partition, role });
      if (!executedByBlock.has(`${partition}:${role}`)) {
        problems.push(`no executed slots cover block ${partition}:${role}`);
      }
    }
  }
  let entries: string[] = [];
  try {
    entries = await readdir(dyadDir);
  } catch {
    problems.push(`dyad directory ${dyadDir} is unreadable`);
  }
  const stray = entries
    .filter((entry) => entry.startsWith('treatment-batch-') && entry.endsWith('.json'))
    .filter((entry) => !expectedBlocks.some(({ partition, role }) => v2BlockFileName(partition, role) === entry))
    .sort();
  for (const entry of stray) problems.push(`stray treatment file ${entry}`);

  let blocks = 0;
  for (const { partition, role } of expectedBlocks) {
    const file = v2BlockFileName(partition, role);
    if (!existsSync(join(dyadDir, file))) {
      problems.push(`${file} is missing`);
      continue;
    }
    const executed = executedByBlock.get(`${partition}:${role}`) ?? new Set<string>();
    const parsed = await readJson(join(dyadDir, file), `block ${partition}:${role}`);
    const validated = validateV2BlockFile(partition, role, executed, parsed);
    problems.push(...validated.problems);
    if (validated.problems.length === 0) blocks += 1;
  }

  return { dyad, ok: problems.length === 0, slots: records.length, blocks, problems };
}

export interface V2ExtractedBlock {
  readonly partition: AlaDyadSchedulePartition;
  readonly role: 'baby-a' | 'baby-b';
  readonly scheduledCases: number;
  readonly records: number;
  readonly cases: readonly AlaTreatmentDiagnosticCase[];
}

export interface V2DiagnosticRows {
  readonly schema: typeof V2_DIAGNOSTIC_ROWS_SCHEMA;
  readonly dyad: string;
  readonly extractedAt: string;
  readonly blocks: readonly V2ExtractedBlock[];
}

/**
 * Project a complete v2 dyad to per-block diagnostic rows. Fails
 * closed with every defect when the dyad is incomplete; callers
 * checking eligibility should use {@link checkV2DyadComplete} for
 * the non-throwing report.
 */
export async function extractV2DiagnosticBatches(dyadDir: string): Promise<V2DiagnosticRows> {
  const dyad = basename(dyadDir);
  const { records, problems: slotProblems } = await readV2SlotRecords(dyadDir);
  const problems = [...slotProblems];
  const executedByBlock = new Map<string, Set<string>>();
  for (const record of records) {
    const key = `${record.partition}:${record.role}`;
    const group = executedByBlock.get(key) ?? new Set<string>();
    group.add(record.executedCaseId);
    executedByBlock.set(key, group);
  }
  const blocks: V2ExtractedBlock[] = [];
  for (const partition of V2_BLOCK_PARTITIONS) {
    for (const role of V2_BLOCK_ROLES) {
      const file = v2BlockFileName(partition, role);
      if (!existsSync(join(dyadDir, file))) {
        problems.push(`${file} is missing`);
        continue;
      }
      const executed = executedByBlock.get(`${partition}:${role}`) ?? new Set<string>();
      if (executed.size === 0) {
        problems.push(`no executed slots cover block ${partition}:${role}`);
        continue;
      }
      let parsed: unknown;
      try {
        parsed = await readJson(join(dyadDir, file), `block ${partition}:${role}`);
      } catch (error) {
        problems.push((error as Error).message.replace(/^v2 retained: /, ''));
        continue;
      }
      const validated = validateV2BlockFile(partition, role, executed, parsed);
      problems.push(...validated.problems);
      if (validated.cases !== undefined) {
        blocks.push({
          partition,
          role,
          scheduledCases: new Set(validated.cases.map((entry) => entry.scheduledCaseId)).size,
          records: validated.cases.length,
          cases: toAlaTreatmentDiagnosticCases(validated.cases),
        });
      }
    }
  }
  if (problems.length > 0) {
    fail(`dyad ${dyad} is not extractable:\n  - ${problems.join('\n  - ')}`);
  }
  return { schema: V2_DIAGNOSTIC_ROWS_SCHEMA, dyad, extractedAt: new Date().toISOString(), blocks };
}

/**
 * Copy this dyad's journal lines out of the evidence-root journal
 * for archiving. The root journal stays the source of truth; the
 * export is a convenience copy the census never trusts over it.
 */
export async function exportV2DyadJournalLines(dyadDir: string, journalPath: string): Promise<string[]> {
  const receipt = (await readJson(join(dyadDir, 'dyad-receipt.json'), 'dyad receipt')) as {
    cohort?: unknown;
    kind?: unknown;
    index?: unknown;
  };
  if (typeof receipt.cohort !== 'string' || typeof receipt.kind !== 'string' || typeof receipt.index !== 'number') {
    fail('dyad receipt carries no cohort/kind/index identity for the journal export');
  }
  let raw: string;
  try {
    raw = (await readFile(journalPath, 'utf8')) as string;
  } catch {
    fail(`journal is unreadable at ${journalPath}`);
  }
  const lines = raw
    .split('\n')
    .filter((line) => line.trim().length > 0)
    .map((line) => {
      try {
        return { line, entry: JSON.parse(line) as { cohort?: unknown; kind?: unknown; index?: unknown } };
      } catch {
        fail(`journal at ${journalPath} has a malformed line`);
      }
    });
  return lines
    .filter(
      ({ entry }) =>
        entry.cohort === receipt.cohort && entry.kind === receipt.kind && entry.index === receipt.index,
    )
    .map(({ line }) => line);
}
