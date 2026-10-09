import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterEach, describe, expect, it } from 'vitest';

import { alaDevAttemptsUsed, runAlaDyad } from '../../src/ala/ala-runner.js';

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
      }),
    ).rejects.toThrowError(/already attempted/);
  });
});
