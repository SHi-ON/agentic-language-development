import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterEach, describe, expect, it } from 'vitest';

import { alaDevAttemptsUsed, runAlaDyad } from '../../src/ala/ala-runner.js';
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
