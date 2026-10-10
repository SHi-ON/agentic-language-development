/**
 * N1.3 semantic treatment-validity fixture (deterministic, no training).
 *
 * A ledger-shuffle control is valid only when donors and recipients are
 * distinct SCHEDULED EXAMPLES (target + candidates + role), not merely
 * distinct branch IDs. The retained calibration construction copies one
 * scheduled example into both ledger cases, so its derangement swaps
 * identical selections; these tests fail that sham shape and pass genuinely
 * distinct batches. Genuine token collisions (distinct examples, same
 * selected token) are allowed and counted. Random-control checks cover the
 * intended protocol: per-case seed separation, reproducibility, valid
 * inventory, and no stream restart — all deterministic, no flakiness.
 */
import { describe, expect, it } from 'vitest';
import { SeededPrng } from '@ald/hashing';
import {
  LV01_PREDICTION_FUNCTION_VERSION,
  type Lv01NativeLedgerIndex,
} from '@ald/learners';
import {
  buildLedgerTreatmentBatch,
  type Lv01TreatedCase,
} from '../../src/experiments/lv01-ledger-treatments.js';
import { buildLv01TreatmentCases } from '../../src/experiments/lv01-collector.js';
import { deriveAlaSeed } from '../../src/ala/ala-seeds.js';
import { fixedTokenModule } from '@ald/gateway';

const INVENTORY = Array.from({ length: 32 }, (_, i) => `T${String(i).padStart(2, '0')}`);

function association(token: string, weights: number[]) {
  return {
    token,
    eventHash: `sha256:${token}`,
    sequence: 1,
    turn: 0,
    policyContextHash: 'ctx',
    associationAgeTurns: 0,
    associationOverTypeCodes: weights,
  };
}

function babyBIndex(): Lv01NativeLedgerIndex {
  const w05 = new Array(16).fill(0);
  w05[1] = 10;
  const w17 = new Array(16).fill(0);
  w17[5] = 10;
  return {
    predictionFunctionVersion: LV01_PREDICTION_FUNCTION_VERSION,
    receiverRole: 'baby-b',
    cutoff: { sequence: 2, turn: 1 },
    selectedAssociations: [association('T05', w05), association('T17', w17)],
  };
}

const NATIVE_INDEXES = {
  babyA: { ...babyBIndex(), receiverRole: 'baby-a' as const },
  babyB: babyBIndex(),
};

const DERANGEMENT_SEED = '0'.repeat(64);

interface ValidityVerdict {
  readonly ok: boolean;
  readonly reasons: string[];
  readonly genuineCollisions: number;
}

/** Semantic validity: distinct scheduled examples per role, real donors, role match. */
function checkLedgerBatchValidity(cases: readonly Lv01TreatedCase[]): ValidityVerdict {
  const reasons: string[] = [];
  let genuineCollisions = 0;
  const keyOf = (c: Lv01TreatedCase): string =>
    `${c.receiverRole}|${c.targetTypeCode}|${[...c.candidateTypeCodes].sort((a, b) => a - b).join(',')}`;
  const byId = new Map(cases.map((c) => [c.caseId, c]));
  for (const role of ['baby-a', 'baby-b'] as const) {
    const roleCases = cases.filter((c) => c.receiverRole === role);
    if (roleCases.length === 0) continue;
    const distinct = new Set(roleCases.map(keyOf));
    if (distinct.size < 2) {
      reasons.push(
        `role ${role}: one distinct scheduled example across ${roleCases.length} cases (branch IDs differ but examples do not)`,
      );
    }
  }
  for (const c of cases) {
    const donor = byId.get(c.shuffledSourceCaseId);
    if (donor === undefined) {
      reasons.push(`${c.caseId}: donor ${c.shuffledSourceCaseId} is not in the batch`);
      continue;
    }
    if (donor.caseId === c.caseId) {
      reasons.push(`${c.caseId}: donor is self (derangement has a fixed point)`);
      continue;
    }
    if (donor.receiverRole !== c.receiverRole) {
      reasons.push(`${c.caseId}: donor role ${donor.receiverRole} mismatches ${c.receiverRole}`);
    }
    if (keyOf(donor) === keyOf(c)) {
      reasons.push(`${c.caseId}: donor is the same scheduled example (sham shuffle)`);
    } else if (c.shuffledDeliveredToken === c.selectedToken) {
      genuineCollisions += 1;
    }
  }
  return { ok: reasons.length === 0, reasons, genuineCollisions };
}

describe('N1.3 ledger treatment validity', () => {
  it('fails the retained sham shape: same example, distinct branch IDs', () => {
    const [consistent, shuffled] = buildLv01TreatmentCases(
      {
        targetTypeCode: 1,
        candidateTypeCodes: [3, 2, 1, 11],
        candidateRefs: ['a', 'b', 'c', 'd'],
        receiver: 'baby-b',
        stateHash: 'state',
        scheduledCaseId: 'scheduled-1',
      },
      'prefix',
    );
    expect(consistent.caseId).not.toBe(shuffled.caseId);
    const batch = buildLedgerTreatmentBatch({
      cases: [consistent, shuffled],
      nativeIndexes: NATIVE_INDEXES,
      symbolInventory: INVENTORY,
      derangementSeed: DERANGEMENT_SEED,
    });
    const verdict = checkLedgerBatchValidity(batch.cases);
    expect(verdict.ok).toBe(false);
    expect(verdict.reasons.join('\n')).toMatch(/one distinct scheduled example/);
    expect(verdict.reasons.join('\n')).toMatch(/sham shuffle/);
  });

  it('passes two distinct scheduled examples per role with distinct selections', () => {
    const mk = (id: string, target: number, cands: number[]) => ({
      caseId: id,
      receiverRole: 'baby-b' as const,
      targetTypeCode: target,
      candidateTypeCodes: cands,
    });
    const batch = buildLedgerTreatmentBatch({
      cases: [mk('ex1', 1, [1, 2, 3, 4]), mk('ex2', 5, [5, 6, 7, 8])],
      nativeIndexes: NATIVE_INDEXES,
      symbolInventory: INVENTORY,
      derangementSeed: DERANGEMENT_SEED,
    });
    const [first, second] = batch.cases;
    expect(first.selectedToken).toBe('T05');
    expect(second.selectedToken).toBe('T17');
    const verdict = checkLedgerBatchValidity(batch.cases);
    expect(verdict.reasons).toEqual([]);
    expect(verdict.ok).toBe(true);
    // Donor permutation, role match, exclusion hold on distinct examples.
    for (const c of batch.cases) {
      expect(c.shuffledSourceCaseId).not.toBe(c.caseId);
      expect(c.shuffledDeliveredToken).not.toBe(c.selectedToken);
    }
  });

  it('allows and counts genuine token collisions', () => {
    const mk = (id: string, target: number, cands: number[]) => ({
      caseId: id,
      receiverRole: 'baby-b' as const,
      targetTypeCode: target,
      candidateTypeCodes: cands,
    });
    const batch = buildLedgerTreatmentBatch({
      cases: [mk('ex1', 1, [1, 2, 3, 4]), mk('ex3', 1, [1, 9, 10, 11])],
      nativeIndexes: NATIVE_INDEXES,
      symbolInventory: INVENTORY,
      derangementSeed: DERANGEMENT_SEED,
    });
    // Distinct examples (different candidate sets), same argmax token.
    expect(batch.cases[0].selectedToken).toBe(batch.cases[1].selectedToken);
    const verdict = checkLedgerBatchValidity(batch.cases);
    expect(verdict.ok).toBe(true);
    expect(verdict.genuineCollisions).toBe(2);
  });
});

describe('N1.3 random-control separation (intended protocol)', () => {
  const base = {
    cohort: 'calibration' as const,
    slotKind: 'primary' as const,
    slotIndex: '0002',
    purpose: 'channel' as const,
    role: 'shared' as const,
    partition: 'within-support-test' as const,
  };

  it('separates per-case streams and reproduces them', () => {
    const a = deriveAlaSeed({ ...base, caseId: 'case-a' });
    const b = deriveAlaSeed({ ...base, caseId: 'case-b' });
    const aAgain = deriveAlaSeed({ ...base, caseId: 'case-a' });
    expect(a).not.toBe(b);
    expect(aAgain).toBe(a);
  });

  it('draws valid inventory artifacts deterministically without stream restart', () => {
    const context = {
      runContext: {
        symbolInventory: INVENTORY,
        config: { maxSymbolsPerMessage: 1 },
      },
      maxSymbolRepeats: 3,
    } as never;
    const draw = (seed: string, n: number): string[][] => {
      const prng = new SeededPrng(seed);
      return Array.from({ length: n }, () => fixedTokenModule.randomArtifact(prng, context).symbols);
    };
    const first = draw('case-seed-a', 64);
    const second = draw('case-seed-a', 64);
    expect(second).toEqual(first);
    for (const symbols of first) {
      expect(symbols.length).toBeGreaterThan(0);
      for (const s of symbols) expect(INVENTORY).toContain(s);
    }
    // A restarted stream would repeat its first draw; an advancing stream varies.
    expect(new Set(first.map((s) => s.join(','))).size).toBeGreaterThan(1);
    expect(draw('case-seed-b', 8)).not.toEqual(first.slice(0, 8));
  });
});
