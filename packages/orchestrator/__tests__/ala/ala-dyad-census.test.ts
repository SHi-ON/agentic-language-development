/** Unit tests for the pure dyad-census reductions (B2/C4 script core). */
import { describe, expect, it } from 'vitest';
import {
  blockAccuracy,
  coverageFromExecuted,
  deliveryJoin,
  distinctShare,
  histogram,
} from '../../src/ala/ala-dyad-census.js';

describe('dyad census reductions', () => {
  it('blocks accuracies with a ragged tail', () => {
    expect(blockAccuracy([true, true, false, true], 2)).toEqual([1, 0.5]);
    expect(blockAccuracy([true, false, false], 2)).toEqual([0.5, 0]);
    expect(() => blockAccuracy([], 2)).toThrow(/no outcomes/);
    expect(() => blockAccuracy([true], 0)).toThrow(/positive integer/);
  });

  it('joins planned to actual deliveries, naming mismatches', () => {
    const planned = new Map([['a', 'T01'], ['b', 'T02'], ['c', 'T03']]);
    const actual = new Map([['a', 'T01'], ['b', 'T99']]);
    expect(deliveryJoin(planned, actual)).toEqual({ matched: 1, mismatched: ['b', 'c'] });
  });

  it('counts distinct shares', () => {
    expect(distinctShare(['x', 'y', 'x'])).toEqual({ distinct: 2, total: 3 });
    expect(distinctShare([])).toEqual({ distinct: 0, total: 0 });
  });

  it('covers executed slots per block, sorted', () => {
    const slots = [
      { partition: 'within-support-test', role: 'baby-b', caseId: 'c1' },
      { partition: 'validation-fit', role: 'baby-a', caseId: 'c2' },
      { partition: 'validation-fit', role: 'baby-a', caseId: 'c2' },
    ];
    expect(coverageFromExecuted(slots)).toEqual([
      { partition: 'validation-fit', role: 'baby-a', executed: 2, distinctCases: 1 },
      { partition: 'within-support-test', role: 'baby-b', executed: 1, distinctCases: 1 },
    ]);
  });

  it('histograms target codes', () => {
    expect(histogram([1, 2, 1])).toEqual({ '1': 2, '2': 1 });
  });
});
