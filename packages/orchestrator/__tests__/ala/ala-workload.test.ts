import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import { alaPrototypeWorkload, alaStudyCounts } from '../../src/ala/ala-workload.js';

const root = fileURLToPath(new URL('../../../../', import.meta.url));

describe('ALA workload resolution', () => {
  it('binds executable counts to the frozen study protocol', () => {
    const counts = alaStudyCounts(join(root, 'protocols/ala-lv-study.v1.json'));
    expect(counts).toEqual({
      trainingCases: 3000,
      validationFitCases: 240,
      validationSelectionCases: 240,
      withinSupportTestCases: 240,
    });
    const workload = alaPrototypeWorkload(counts);
    expect(workload.profile).toBe('prototype');
    expect(workload.trainingTurns).toBe(3000);
  });

  it('rejects missing or non-positive partition counts', () => {
    const dir = mkdtempSync(join(tmpdir(), 'ald-ala-workload-'));
    const bad = join(dir, 'study.json');
    writeFileSync(bad, JSON.stringify({ task: { partitions: { training: { cases: 0 } } } }));
    expect(() => alaStudyCounts(bad)).toThrowError(/no positive training cases/);
    writeFileSync(bad, JSON.stringify({ task: {} }));
    expect(() => alaStudyCounts(bad)).toThrowError(/no positive training cases/);
  });
});
