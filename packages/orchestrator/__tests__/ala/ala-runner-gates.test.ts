import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterEach, describe, expect, it } from 'vitest';

import {
  alaAttemptJournalEntry,
  alaDevAttemptsUsed,
  alaFailureJournalEntry,
  alaSupervisorJournalEntry,
  planAlaDyadSlots,
  readLv01ExecutedCaseIds,
  runAlaDyad,
} from '../../src/ala/ala-runner.js';
import { alaDyadSeeds } from '../../src/ala/ala-seeds.js';

const root = fileURLToPath(new URL('../../../../', import.meta.url));
const temporary: string[] = [];

afterEach(() => {
  for (const dir of temporary.splice(0)) rmSync(dir, { recursive: true, force: true });
});

function evidenceRoot(): string {
  const dir = `${tmpdir()}/ald-ala-gates-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
  temporary.push(dir);
  mkdirSync(dir, { recursive: true });
  return dir;
}

describe('ALA runner gates (no training runs)', () => {
  it('refuses study cohorts without a P5 admission record', async () => {
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'A',
        kind: 'primary',
        index: 0,
        evidenceRoot: evidenceRoot(),
        retainedCorpus: [],
        treatmentBatchVersion: 1,
        historicalReplayAcknowledgement: 'historical-sham-replay',
      }),
    ).rejects.toThrowError(/no P5 admission record/);
  });

  it('refuses development beyond the 3-attempt budget', async () => {
    const dir = evidenceRoot();
    const journal = [0, 1, 2]
      .map((index) => JSON.stringify({ at: '2026-10-09T00:00:00Z', cohort: 'development', kind: 'primary', index, state: 'valid' }))
      .join('\n');
    writeFileSync(join(dir, 'journal.jsonl'), `${journal}\n`);
    expect(alaDevAttemptsUsed([
      { at: '', cohort: 'development', kind: 'primary', index: 0, state: 'valid' },
      { at: '', cohort: 'calibration', kind: 'primary', index: 0, state: 'invalid' },
      { at: '', cohort: 'A', kind: 'primary', index: 0, state: 'valid' },
      { at: '', cohort: 'development', kind: 'primary', index: 1, state: 'planned' },
    ])).toBe(2);
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 3,
        evidenceRoot: dir,
        retainedCorpus: [],
        treatmentBatchVersion: 1,
        historicalReplayAcknowledgement: 'historical-sham-replay',
      }),
    ).rejects.toThrowError(/attempt budget \(3\) is exhausted/);
  });

  it('refuses to re-run an attempted slot', async () => {
    const dir = evidenceRoot();
    writeFileSync(
      join(dir, 'journal.jsonl'),
      `${JSON.stringify({ at: '2026-10-09T00:00:00Z', cohort: 'development', kind: 'primary', index: 0, state: 'attempted' })}\n`,
    );
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: [],
        treatmentBatchVersion: 1,
        historicalReplayAcknowledgement: 'historical-sham-replay',
      }),
    ).rejects.toThrowError(/already attempted/);
  });

  it('resume refuses an unattempted slot', async () => {
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 9,
        evidenceRoot: evidenceRoot(),
        retainedCorpus: [],
        treatmentBatchVersion: 1,
        historicalReplayAcknowledgement: 'historical-sham-replay',
        resume: true,
      }),
    ).rejects.toThrowError(/nothing to resume/);
  });

  it('resume refuses a terminal slot', async () => {
    const dir = evidenceRoot();
    writeFileSync(
      join(dir, 'journal.jsonl'),
      `${JSON.stringify({ at: '2026-10-09T00:00:00Z', cohort: 'development', kind: 'primary', index: 0, state: 'valid' })}\n`,
    );
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: [],
        treatmentBatchVersion: 1,
        historicalReplayAcknowledgement: 'historical-sham-replay',
        resume: true,
      }),
    ).rejects.toThrowError(/terminal receipt/);
  });

  it('refuses new collection (version 2) before anything is written', async () => {
    // The per-slot flow builds both branch records from one scheduled
    // case, which the collection-entry gate rejects: version 2 must
    // refuse outright until cross-slot scheduling lands, writing
    // neither journal entries nor treatment-batch.json.
    const dir = evidenceRoot();
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: [],
        treatmentBatchVersion: 2,
      }),
    ).rejects.toThrowError(/cross-slot scheduling/);
    expect(readdirSync(dir)).toEqual([]);
  });

  it('refuses fresh version 1 without the historical-replay acknowledgement', async () => {
    // Version selection alone is not consent to replay the sham
    // construction: v1 without the explicit acknowledgement refuses
    // before anything is written.
    const dir = evidenceRoot();
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: [],
        treatmentBatchVersion: 1,
      }),
    ).rejects.toThrowError(/historicalReplayAcknowledgement/);
    expect(readdirSync(dir)).toEqual([]);
  });

  it('journals the historical-replay acknowledgement in the attempt record', async () => {
    // Acknowledge v1, then stop at the retained-identity audit (before
    // training) so the test stays cheap: the attempted entry must carry
    // the acknowledgement for the audit trail.
    const dir = evidenceRoot();
    const colliding = Object.values(alaDyadSeeds('development', 'primary', '0000'));
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: colliding,
        treatmentBatchVersion: 1,
        historicalReplayAcknowledgement: 'historical-sham-replay',
      }),
    ).rejects.toThrowError(/collides with a retained identity/);
    const lines = readFileSync(join(dir, 'journal.jsonl'), 'utf8').trim().split('\n');
    expect(lines).toHaveLength(1);
    expect(JSON.parse(lines[0] as string)).toMatchObject({
      cohort: 'development',
      kind: 'primary',
      index: 0,
      state: 'attempted',
      historicalReplayAck: 'historical-sham-replay',
    });
  });

  it('refuses study protocol 2 with batch version 1 before anything is written', async () => {
    const dir = evidenceRoot();
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: [],
        treatmentBatchVersion: 1,
        historicalReplayAcknowledgement: 'historical-sham-replay',
        protocolVersion: 2,
      }),
    ).rejects.toThrowError(/requires batch version 2/);
    expect(readdirSync(dir)).toEqual([]);
  });

  it('admits protocol 2 with batch version 2 past the gates without a historical ack', async () => {
    // Corrected-study collection passes coherence and admission gates, then
    // stops at the retained-identity audit (before training) so the test
    // stays cheap: the attempted entry must carry no acknowledgement.
    const dir = evidenceRoot();
    const colliding = Object.values(alaDyadSeeds('development', 'primary', '0000', 'ala-ledger-value-v2'));
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: colliding,
        treatmentBatchVersion: 2,
        protocolVersion: 2,
      }),
    ).rejects.toThrowError(/collides with a retained identity/);
    const lines = readFileSync(join(dir, 'journal.jsonl'), 'utf8').trim().split('\n');
    expect(lines).toHaveLength(1);
    expect(JSON.parse(lines[0] as string)).toMatchObject({
      cohort: 'development',
      kind: 'primary',
      index: 0,
      state: 'attempted',
    });
    expect(JSON.parse(lines[0] as string)).not.toHaveProperty('historicalReplayAck');
  });

  it('plans 240 test slots for version 1 with the legacy parity mapping', () => {
    const counts = { validationFitCases: 120, validationSelectionCases: 120, withinSupportTestCases: 120 };
    const slots = planAlaDyadSlots(counts, 1);
    expect(slots).toHaveLength(240);
    slots.forEach((entry, i) => {
      const slot = i + 1;
      expect(entry.slot).toBe(slot);
      expect(entry.partition).toBe('within-support-test');
      expect(entry.receiver).toBe(slot % 2 === 1 ? 'baby-b' : 'baby-a');
      expect(entry.caseIndex).toBe((slot - 1) % 120);
    });
  });

  it('plans 720 slots across all partitions for version 2', () => {
    const counts = { validationFitCases: 120, validationSelectionCases: 120, withinSupportTestCases: 120 };
    const slots = planAlaDyadSlots(counts, 2);
    expect(slots).toHaveLength(720);
    expect(new Set(slots.map((entry) => entry.slot)).size).toBe(720);
    const partitions = ['validation-fit', 'validation-selection', 'within-support-test'] as const;
    partitions.forEach((partition, block) => {
      const group = slots.filter((entry) => entry.partition === partition);
      expect(group).toHaveLength(240);
      expect(group[0]?.slot).toBe(block * 240 + 1);
      expect(group[0]?.receiver).toBe('baby-b');
      // Receivers still alternate, but every role visits each of its
      // cases exactly once per partition (complete walk, no parity
      // duplication).
      for (const role of ['baby-a', 'baby-b'] as const) {
        const roleCases = group.filter((entry) => entry.receiver === role).map((entry) => entry.caseIndex).sort((a, b) => a - b);
        expect(roleCases).toEqual(Array.from({ length: 120 }, (_, i) => i));
      }
    });
  });

  it('reads executed case ids back from slot schedule records', async () => {
    const dir = evidenceRoot();
    for (const slot of [1, 2]) {
      const slotDir = join(dir, `slot-${String(slot).padStart(4, '0')}`);
      mkdirSync(slotDir, { recursive: true });
      writeFileSync(join(slotDir, 'schedule.json'), JSON.stringify({ commitment: 'c', executedCaseId: `case-${slot}` }));
    }
    await expect(readLv01ExecutedCaseIds(dir, [1, 2])).resolves.toEqual(['case-1', 'case-2']);
    await expect(readLv01ExecutedCaseIds(dir, [1, 3])).rejects.toThrow();
  });

  it('journals the acknowledgement only for version 1 attempts', () => {
    expect(
      alaAttemptJournalEntry({ cohort: 'A', kind: 'primary', index: 0, resuming: false, protocolVersion: 1 }),
    ).toMatchObject({ state: 'attempted', historicalReplayAck: 'historical-sham-replay' });
    const v2 = alaAttemptJournalEntry({ cohort: 'A', kind: 'primary', index: 0, resuming: true, protocolVersion: 2 });
    expect(v2.state).toBe('resumed');
    expect(v2).not.toHaveProperty('historicalReplayAck');
  });

  it('records failure entries with elapsed wall time and truncated reasons', () => {
    const failed = alaFailureJournalEntry({
      cohort: 'development',
      kind: 'primary',
      index: 2,
      state: 'failed',
      reason: 'boom',
      wallStartEpochMs: 1000,
      atEpochMs: 2500,
    });
    expect(failed).toMatchObject({ state: 'failed', reason: 'boom', wallMilliseconds: 1500 });
    const long = alaFailureJournalEntry({
      cohort: 'A',
      kind: 'reserve',
      index: 0,
      state: 'invalid',
      reason: 'x'.repeat(600),
      wallStartEpochMs: 1000,
      atEpochMs: 1000,
    });
    expect(long.reason?.length).toBe(500);
    expect(long.wallMilliseconds).toBe(0);
  });

  it('writes supervisor entries only for non-verdict states with a reason', () => {
    const aborted = alaSupervisorJournalEntry({
      cohort: 'A',
      kind: 'primary',
      index: 3,
      state: 'aborted',
      reason: 'watchdog: wall guard exceeded',
      wallMilliseconds: 999,
    });
    expect(aborted).toMatchObject({ state: 'aborted', wallMilliseconds: 999 });
    expect(() =>
      alaSupervisorJournalEntry({ cohort: 'A', kind: 'primary', index: 3, state: 'note', reason: '' }),
    ).toThrowError(/require a reason/);
  });

  it('counts failed and invalid attempts against the dev budget', () => {
    const at = '2026-11-02T00:00:00.000Z';
    expect(
      alaDevAttemptsUsed([
        { at, cohort: 'development', kind: 'primary', index: 0, state: 'attempted' },
        { at, cohort: 'development', kind: 'primary', index: 0, state: 'failed', reason: 'crash' },
        { at, cohort: 'development', kind: 'primary', index: 1, state: 'invalid', reason: 'verdict' },
        { at, cohort: 'development', kind: 'primary', index: 2, state: 'note', reason: 'note' },
      ]),
    ).toBe(2);
  });

  it('treats invalid and aborted priors as terminal and failed as resumable-only', async () => {
    const attempt = (dir: string, resume: boolean) =>
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: [],
        resume,
        treatmentBatchVersion: 1,
        historicalReplayAcknowledgement: 'historical-sham-replay',
      });
    const seedJournal = (dir: string, state: string) => {
      writeFileSync(
        join(dir, 'journal.jsonl'),
        `${JSON.stringify({ at: '2026-11-02T00:00:00.000Z', cohort: 'development', kind: 'primary', index: 0, state })}\n`,
      );
    };
    const invalidDir = evidenceRoot();
    seedJournal(invalidDir, 'invalid');
    await expect(attempt(invalidDir, true)).rejects.toThrowError(/terminal receipt/);
    const abortedDir = evidenceRoot();
    seedJournal(abortedDir, 'aborted');
    await expect(attempt(abortedDir, false)).rejects.toThrowError(/terminal receipt/);
    const failedDir = evidenceRoot();
    seedJournal(failedDir, 'failed');
    await expect(attempt(failedDir, false)).rejects.toThrowError(/already attempted/);
  });

  it('forbids the historical acknowledgement on the corrected-study path', async () => {
    const dir = evidenceRoot();
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: [],
        treatmentBatchVersion: 2,
        historicalReplayAcknowledgement: 'historical-sham-replay',
        protocolVersion: 2,
      }),
    ).rejects.toThrowError(/forbidden on the corrected-study path/);
    expect(readdirSync(dir)).toEqual([]);
  });

  it('treats explicit protocol 1 exactly like the default', async () => {
    const dir = evidenceRoot();
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'development',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: [],
        treatmentBatchVersion: 2,
        protocolVersion: 1,
      }),
    ).rejects.toThrowError(/cross-slot scheduling/);
    expect(readdirSync(dir)).toEqual([]);
  });

  it('resume bypasses the attempted refusal but keeps downstream gates', async () => {
    // Cohort A with a crashed (attempted, never terminal) slot: resume must
    // pass the attempted gate and then fail at P5 admission — proving the
    // bypass without training.
    const dir = evidenceRoot();
    writeFileSync(
      join(dir, 'journal.jsonl'),
      `${JSON.stringify({ at: '2026-10-09T00:00:00Z', cohort: 'A', kind: 'primary', index: 0, state: 'attempted' })}\n`,
    );
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v1.json',
        cohort: 'A',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: [],
        treatmentBatchVersion: 1,
        historicalReplayAcknowledgement: 'historical-sham-replay',
        resume: true,
      }),
    ).rejects.toThrowError(/no P5 admission record/);
  });
});

describe('v2 execution manifest binding (B2 false-start regression)', () => {
  it('verifies the v2 manifest before reaching the admission gate', async () => {
    // The v2 manifest binds the corrected-study triplet. Manifest
    // verification is step 1 of the runner, so reaching the P5
    // admission refusal proves the v2 manifest loads and verifies;
    // a missing or drifted binding would fail with a manifest error
    // instead (the B2 false start ran the v2 path against the v1
    // manifest and crashed past the gates on an unbound path).
    const dir = evidenceRoot();
    await expect(
      runAlaDyad({
        root,
        manifestPath: 'protocols/ala-lv-execution-manifest.v2.json',
        cohort: 'A',
        kind: 'primary',
        index: 0,
        evidenceRoot: dir,
        retainedCorpus: [],
        treatmentBatchVersion: 2,
        protocolVersion: 2,
      }),
    ).rejects.toThrowError(/no P5 admission record/);
  });
});
