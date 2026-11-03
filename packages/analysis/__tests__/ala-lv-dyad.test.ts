/** Dyad estimation tests on synthetic folds through the real selection path. */
import { describe, expect, it } from 'vitest';
import {
  alaLvCohortInterval,
  analyzeAlaLvDyad,
  analyzeAlaLvRole,
  type AlaLvRoleFolds,
} from '../src/ala-lv-dyad.js';
import type { Lv01OrdinaryRecord } from '../src/ledger-value.js';

const INVENTORY = Array.from({ length: 32 }, (_, i) => `T${String(i).padStart(2, '0')}`);

const row = (caseId: string, role: 'baby-a' | 'baby-b', action: number, token: string | null = 'T00'): Lv01OrdinaryRecord => ({
  caseId,
  receiverRole: role,
  candidateTypeCodes: [1, 2, 3, 4],
  deliveredToken: token,
  actualSelectedCandidateIndex: action,
});

function folds(role: 'baby-a' | 'baby-b'): AlaLvRoleFolds {
  const training = [0, 1, 2, 3].map((action, i) => row(`${role}-train-${i}`, role, action));
  const validationFit = [0, 0].map((action, i) => row(`${role}-fit-${i}`, role, action));
  const validationSelection = [1, 1].map((action, i) => row(`${role}-sel-${i}`, role, action));
  const test = [0, 2].map((action, i) => ({
    record: row(`${role}-test-${i}`, role, action),
    nativeDistribution: [0.4, 0.2, 0.2, 0.2],
    replayDistribution: [0.25, 0.25, 0.25, 0.25],
    targetIndex: 0,
    nativeSource: 'training-ledger',
  }));
  return { role, training, validationFit, validationSelection, test };
}

describe('dyad estimation', () => {
  it('selects, refits, and scores a role end to end', () => {
    const result = analyzeAlaLvRole(folds('baby-a'), INVENTORY);
    expect(Object.keys(result.validationBrierByPredictor)).toHaveLength(5);
    // Selected is the list-order argmin within the 1e-12 tie tolerance.
    const entries = Object.entries(result.validationBrierByPredictor);
    const best = entries.reduce((held, entry) => (entry[1] < held[1] - 1e-12 ? entry : held));
    expect(result.selectedPredictorId).toBe(best[0]);
    expect(result.testCases).toBe(2);
    expect(result.difference).toBeCloseTo(result.ordinaryBrier - result.nativeBrier, 12);
    expect(result.nativeMissingTokens).toBe(0);
    for (const value of [result.ordinaryBrier, result.nativeBrier, result.replayBrier]) {
      expect(value).toBeGreaterThanOrEqual(0);
      expect(value).toBeLessThanOrEqual(2);
    }
  });

  it('counts native missing-token fallbacks without failing', () => {
    const withMissing = folds('baby-b');
    const test = withMissing.test.map((entry) => ({ ...entry, nativeSource: 'uniform-missing-token' }));
    const result = analyzeAlaLvRole({ ...withMissing, test }, INVENTORY);
    expect(result.nativeMissingTokens).toBe(2);
  });

  it('averages roles equally into the dyad primary', () => {
    const dyad = analyzeAlaLvDyad('dyad-0', [folds('baby-a'), folds('baby-b')], INVENTORY);
    expect(dyad.primary).toBeCloseTo((dyad.roles[0]!.difference + dyad.roles[1]!.difference) / 2, 12);
    expect(() => analyzeAlaLvDyad('dyad-0', [folds('baby-a'), folds('baby-a')], INVENTORY)).toThrow(/both receiver roles/);
  });
});

describe('cohort intervals', () => {
  it('flags n=1 and zero variance as degenerate', () => {
    const one = alaLvCohortInterval('development', [0.05], 0.975);
    expect(one.degenerate).toMatch(/n=1/);
    expect(one.lower).toBeNull();
    const flat = alaLvCohortInterval('A', [0.05, 0.05, 0.05], 0.975);
    expect(flat.degenerate).toMatch(/zero variance/);
  });

  it('centers a symmetric t interval on the mean', () => {
    const interval = alaLvCohortInterval('A', [0.02, 0.04, 0.06], 0.975);
    expect(interval.degenerate).toBeNull();
    expect(interval.n).toBe(3);
    expect(interval.mean).toBeCloseTo(0.04, 12);
    expect(interval.lower).toBeLessThan(0.04);
    expect(interval.upper).toBeGreaterThan(0.04);
    expect(interval.lower! + interval.upper!).toBeCloseTo(0.08, 12);
  });
});
