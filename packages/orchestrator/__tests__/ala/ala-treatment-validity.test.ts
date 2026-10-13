/**
 * N1.3 semantic treatment-validity fixture (deterministic, no training).
 *
 * A ledger-shuffle control is valid only when donors and recipients are
 * distinct SCHEDULED EXAMPLES (role + target + ORDERED candidates), not
 * merely distinct branch IDs. The retained calibration construction copies
 * one scheduled example into both ledger cases, so its derangement swaps
 * identical selections; these tests fail that sham shape and pass genuinely
 * distinct batches. Genuine token collisions (distinct examples, same
 * selected token) are allowed and counted. Random-control checks cover the
 * intended protocol: per-case seed separation, reproducibility, valid
 * inventory, and no stream restart — all deterministic, no flakiness.
 *
 * Rule interpretation (round-4 R2/R3): the frozen protocol's "no shared
 * target across shuffled cases" governs the shuffle mapping — donor
 * assignment must be a derangement (no fixed points, injective donors)
 * over same-role cases — not token inequality between cases. Example
 * identity uses ORDERED candidates: order is part of the conditioning
 * the selector consumes. This helper is an offline diagnostic over
 * retained/planned batches; it is not wired into any collection entry
 * point, so production can still construct a sham batch (diagnosis,
 * not enforced prevention).
 *
 * Scoped predicate (round-5 F1/F2), not a "valid iff": trusted inputs
 * are the recorded fields themselves — this helper does not re-derive
 * selections from ledger indexes or verify tokens against ledgers.
 * What it verifies: donor membership, injective donors, derangement,
 * donor/recipient role match, distinct scheduled examples (content
 * key), and REALIZED delivery (the delivered token is the declared
 * donor's selected token). A collision counts as genuine only when the
 * delivered token matches the donor's selection and also equals the
 * recipient's own selection; wrong-donor deliveries fail with a named
 * delivery-mismatch reason. Scheduled identity inside one batch is the
 * branch-record ID with the collector's branch suffix removed (in
 * synthetic fixtures the caseId already is the case identity); the
 * portable paper helper keys identity on the real scheduledCaseId from
 * retained schedule files.
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
  // Round-4 R2: T05 also associates with eligible type 2, so the
  // collision fixture can use distinct in-domain targets (1 vs 2)
  // while both examples select T05.
  w05[2] = 10;
  const w17 = new Array(16).fill(0);
  w17[5] = 10;
  const w21 = new Array(16).fill(0);
  w21[6] = 10;
  return {
    predictionFunctionVersion: LV01_PREDICTION_FUNCTION_VERSION,
    receiverRole: 'baby-b',
    cutoff: { sequence: 2, turn: 1 },
    selectedAssociations: [association('T05', w05), association('T17', w17), association('T21', w21)],
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
  readonly repeatedContent: number;
}

/** Scoped diagnostic: distinct scheduled cases per role, deranged injective donors, realized delivery. */
function checkLedgerBatchValidity(cases: readonly Lv01TreatedCase[]): ValidityVerdict {
  const reasons: string[] = [];
  let genuineCollisions = 0;
  let repeatedContent = 0;
  const keyOf = (c: Lv01TreatedCase): string =>
    `${c.receiverRole}|${c.targetTypeCode}|${[...c.candidateTypeCodes].join(',')}`;
  // Scheduled identity inside one batch: the branch-record ID with the
  // collector's branch suffix removed. Within a deployed batch that
  // identifies the scheduled case the records came from; in synthetic
  // fixtures the caseId already is the case identity. The portable paper
  // helper keys identity on the real scheduledCaseId instead (F2).
  const scheduledId = (c: Lv01TreatedCase): string =>
    c.caseId.replace(/-ledger-(consistent|shuffled):turn:\d+$/u, '');
  const byId = new Map(cases.map((c) => [c.caseId, c]));
  const donors = cases.map((c) => c.shuffledSourceCaseId);
  if (new Set(donors).size !== donors.length) {
    reasons.push('donor assignment is not injective (two cases share one shuffle target)');
  }
  for (const role of ['baby-a', 'baby-b'] as const) {
    const roleCases = cases.filter((c) => c.receiverRole === role);
    if (roleCases.length === 0) continue;
    const distinct = new Set(roleCases.map(scheduledId));
    if (distinct.size < 2) {
      reasons.push(
        `role ${role}: one distinct scheduled case across ${roleCases.length} cases (branch records share one scheduled identity)`,
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
    if (scheduledId(donor) === scheduledId(c)) {
      reasons.push(`${c.caseId}: donor is the same scheduled case (sham shuffle)`);
      continue;
    }
    if (keyOf(donor) === keyOf(c)) {
      // Distinct scheduled cases, identical conditioning: reported,
      // not invalid (F2.6).
      repeatedContent += 1;
    }
    if (c.shuffledDeliveredToken !== donor.selectedToken) {
      // Round-5 F1: the declared permutation must be REALIZED — the
      // delivered token has to be the donor's selected token.
      reasons.push(
        `${c.caseId}: delivery mismatch: donor ${donor.caseId} selected ${donor.selectedToken} but ${c.shuffledDeliveredToken} was delivered`,
      );
    } else if (c.shuffledDeliveredToken === c.selectedToken) {
      genuineCollisions += 1;
    }
  }
  return { ok: reasons.length === 0, reasons, genuineCollisions, repeatedContent };
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
    expect(verdict.reasons.join('\n')).toMatch(/one distinct scheduled case/);
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

  it('passes in-domain distinct examples (eligible off-diagonal types only)', () => {
    // Round-4 R2: the earlier positive uses type 5, outside the study's
    // eligibleTypeCodes [1,2,3,4,6,7,8,9,11,12,13,14]. This pair stays
    // inside the eligible domain and selects distinct tokens.
    const mk = (id: string, target: number, cands: number[]) => ({
      caseId: id,
      receiverRole: 'baby-b' as const,
      targetTypeCode: target,
      candidateTypeCodes: cands,
    });
    const batch = buildLedgerTreatmentBatch({
      cases: [mk('in1', 1, [1, 2, 3, 4]), mk('in2', 6, [6, 7, 8, 9])],
      nativeIndexes: NATIVE_INDEXES,
      symbolInventory: INVENTORY,
      derangementSeed: DERANGEMENT_SEED,
    });
    const [first, second] = batch.cases;
    expect(first.selectedToken).toBe('T05');
    expect(second.selectedToken).toBe('T21');
    const verdict = checkLedgerBatchValidity(batch.cases);
    expect(verdict.reasons).toEqual([]);
    expect(verdict.ok).toBe(true);
    expect(verdict.genuineCollisions).toBe(0);
  });

  it('allows and counts genuine token collisions', () => {
    const mk = (id: string, target: number, cands: number[]) => ({
      caseId: id,
      receiverRole: 'baby-b' as const,
      targetTypeCode: target,
      candidateTypeCodes: cands,
    });
    const batch = buildLedgerTreatmentBatch({
      cases: [mk('ex1', 1, [1, 2, 3, 4]), mk('ex3', 2, [2, 9, 12, 11])],
      nativeIndexes: NATIVE_INDEXES,
      symbolInventory: INVENTORY,
      derangementSeed: DERANGEMENT_SEED,
    });
    // Distinct examples (different targets and candidate sets, all codes
    // inside eligibleTypeCodes), same argmax token by inventory-order ties.
    expect(batch.cases[0].selectedToken).toBe(batch.cases[1].selectedToken);
    const verdict = checkLedgerBatchValidity(batch.cases);
    expect(verdict.ok).toBe(true);
    expect(verdict.genuineCollisions).toBe(2);
  });

  it('rejects deliveries that did not come from the declared donor (F1)', () => {
    const mk = (id: string, target: number, cands: number[]) => ({
      caseId: id,
      receiverRole: 'baby-b' as const,
      targetTypeCode: target,
      candidateTypeCodes: cands,
    });
    const batch = buildLedgerTreatmentBatch({
      cases: [mk('in1', 1, [1, 2, 3, 4]), mk('in2', 6, [6, 7, 8, 9])],
      nativeIndexes: NATIVE_INDEXES,
      symbolInventory: INVENTORY,
      derangementSeed: DERANGEMENT_SEED,
    });
    // Deliver each recipient its OWN selection instead of the donor's:
    // the declared permutation was not realized, and these are not
    // genuine collisions even though delivered == own selection.
    const ownDelivered = batch.cases.map((c) => ({
      ...c,
      shuffledDeliveredToken: c.selectedToken,
    }));
    const ownVerdict = checkLedgerBatchValidity(ownDelivered);
    expect(ownVerdict.ok).toBe(false);
    expect(ownVerdict.genuineCollisions).toBe(0);
    expect(ownVerdict.reasons.join('\n')).toMatch(/delivery mismatch/);
    // Deliver a token neither donor selected.
    const alienDelivered = batch.cases.map((c) => ({
      ...c,
      shuffledDeliveredToken: 'T31',
    }));
    const alienVerdict = checkLedgerBatchValidity(alienDelivered);
    expect(alienVerdict.ok).toBe(false);
    expect(alienVerdict.genuineCollisions).toBe(0);
    expect(alienVerdict.reasons.join('\n')).toMatch(/delivery mismatch/);
  });

  it('detects a fixed point in an otherwise injective identity mapping (F1)', () => {
    const base = {
      receiverRole: 'baby-b' as const,
      identicalTokenCoincidence: true,
    };
    const verdict = checkLedgerBatchValidity([
      {
        ...base,
        caseId: 'fp-a',
        targetTypeCode: 1,
        candidateTypeCodes: [1, 2, 3, 4],
        selectedToken: 'T05',
        shuffledSourceCaseId: 'fp-a',
        shuffledDeliveredToken: 'T05',
      },
      {
        ...base,
        caseId: 'fp-b',
        targetTypeCode: 6,
        candidateTypeCodes: [6, 7, 8, 9],
        selectedToken: 'T21',
        shuffledSourceCaseId: 'fp-b',
        shuffledDeliveredToken: 'T21',
      },
    ]);
    expect(verdict.ok).toBe(false);
    expect(verdict.reasons.join('\n')).toMatch(/fixed point/);
    expect(verdict.reasons.join('\n')).not.toMatch(/not injective/);
  });

  it('rejects a shared donor over distinct examples (F1)', () => {
    const base = { receiverRole: 'baby-b' as const, identicalTokenCoincidence: false };
    const verdict = checkLedgerBatchValidity([
      {
        ...base,
        caseId: 'sd-a',
        targetTypeCode: 1,
        candidateTypeCodes: [1, 2, 3, 4],
        selectedToken: 'T05',
        shuffledSourceCaseId: 'sd-c',
        shuffledDeliveredToken: 'T21',
      },
      {
        ...base,
        caseId: 'sd-b',
        targetTypeCode: 2,
        candidateTypeCodes: [2, 9, 12, 11],
        selectedToken: 'T05',
        shuffledSourceCaseId: 'sd-c',
        shuffledDeliveredToken: 'T21',
      },
      {
        ...base,
        caseId: 'sd-c',
        targetTypeCode: 6,
        candidateTypeCodes: [6, 7, 8, 9],
        selectedToken: 'T21',
        shuffledSourceCaseId: 'sd-a',
        shuffledDeliveredToken: 'T05',
      },
    ]);
    expect(verdict.ok).toBe(false);
    expect(verdict.reasons.join('\n')).toMatch(/not injective/);
  });

  it('allows distinct scheduled examples with identical conditioning (F2)', () => {
    // Equal content under different scheduled identities is reported
    // context, not invalidity: both examples select T05, the derangement
    // is realized, and both deliveries are genuine collisions.
    const mk = (id: string, target: number, cands: number[]) => ({
      caseId: id,
      receiverRole: 'baby-b' as const,
      targetTypeCode: target,
      candidateTypeCodes: cands,
    });
    const batch = buildLedgerTreatmentBatch({
      cases: [mk('rc1', 1, [1, 2, 3, 4]), mk('rc2', 1, [1, 2, 3, 4])],
      nativeIndexes: NATIVE_INDEXES,
      symbolInventory: INVENTORY,
      derangementSeed: DERANGEMENT_SEED,
    });
    const verdict = checkLedgerBatchValidity(batch.cases);
    expect(verdict.ok).toBe(true);
    expect(verdict.genuineCollisions).toBe(2);
    expect(verdict.repeatedContent).toBe(2);
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
