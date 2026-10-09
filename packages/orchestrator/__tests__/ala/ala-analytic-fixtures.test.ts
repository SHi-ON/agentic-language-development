/**
 * P4 deterministic analytic fixtures for the ALA-LV scoring path.
 *
 * The study's primary statistic is a difference of multiclass Brier scores,
 * so the scorer's fixed points are pinned here: uniform over four actions is
 * exactly 0.75, a perfect one-hot prediction is exactly zero, moving
 * probability mass away from the realized target strictly worsens the known
 * answer, and equal probability content scores equally regardless of the
 * surrounding wrapper (case identity carries no score).
 */
import { describe, expect, it } from 'vitest';

import { multiclassBrierScore } from '../../../analysis/src/causal-prediction.js';

const ACTIONS = ['pos-0', 'pos-1', 'pos-2', 'pos-3'];

describe('ALA analytic scoring fixtures', () => {
  it('scores uniform actions over four candidates at exactly 0.75', () => {
    for (const target of ACTIONS) {
      expect(
        multiclassBrierScore({ caseId: 'uniform', actionIds: [...ACTIONS], probabilities: [0.25, 0.25, 0.25, 0.25] }, target),
      ).toBe(0.75);
    }
  });

  it('scores a perfect one-hot prediction at zero', () => {
    expect(
      multiclassBrierScore({ caseId: 'perfect', actionIds: [...ACTIONS], probabilities: [0, 1, 0, 0] }, 'pos-1'),
    ).toBe(0);
  });

  it('worsens the known answer when probability mass moves off the target', () => {
    const good = multiclassBrierScore(
      { caseId: 'good', actionIds: [...ACTIONS], probabilities: [0.7, 0.1, 0.1, 0.1] },
      'pos-0',
    );
    const permuted = multiclassBrierScore(
      { caseId: 'permuted', actionIds: [...ACTIONS], probabilities: [0.1, 0.1, 0.1, 0.7] },
      'pos-0',
    );
    expect(good).toBeCloseTo(0.12, 10);
    expect(permuted).toBeCloseTo(1.32, 10);
    expect(permuted).toBeGreaterThan(good);
  });

  it('scores equal probability content equally under different case identities', () => {
    const probabilities = [0.5, 0.25, 0.125, 0.125];
    const first = multiclassBrierScore({ caseId: 'signed-content', actionIds: [...ACTIONS], probabilities }, 'pos-0');
    const second = multiclassBrierScore({ caseId: 'unsigned-equal-content', actionIds: [...ACTIONS], probabilities }, 'pos-0');
    expect(first).toBeCloseTo(0.34375, 10);
    expect(second).toBe(first);
  });

  it('fails closed on non-normalized and out-of-range probabilities', () => {
    expect(() =>
      multiclassBrierScore({ caseId: 'bad-sum', actionIds: [...ACTIONS], probabilities: [0.5, 0.5, 0.5, 0.5] }, 'pos-0'),
    ).toThrowError(/sum to one/);
    expect(() =>
      multiclassBrierScore({ caseId: 'bad-range', actionIds: [...ACTIONS], probabilities: [1.2, -0.1, -0.05, -0.05] }, 'pos-0'),
    ).toThrowError(/within \[0, 1\]/);
  });
});
