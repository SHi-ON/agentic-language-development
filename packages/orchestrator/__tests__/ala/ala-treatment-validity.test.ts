/** Offline semantic fixtures: branch IDs, scheduled IDs, ordered content,
 * and token identity are distinct fields. The diagnostic trusts selections
 * as recorded; the historical collector never calls the collection-entry
 * gate, while repaired construction must. See the source modules.
 */
import { describe, expect, it } from 'vitest';
import { assertValidLedgerBatch, diagnoseLedgerBatch, type AlaTreatmentDiagnosticCase } from './helpers/treatment-diagnostic.js';
import { SeededPrng } from '@ald/hashing';
import {
  LV01_PREDICTION_FUNCTION_VERSION,
  type Lv01NativeLedgerIndex,
} from '@ald/learners';
import {
  buildLedgerTreatmentBatch,
  buildValidLedgerTreatmentBatch,
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

// Synthetic fixtures explicitly assign an identity to each example. The
// retained-shape fixture supplies the SAME identity for both branch records.
function checkLedgerBatchValidity(cases: readonly Lv01TreatedCase[], identities?: readonly string[]) {
  return diagnoseLedgerBatch(cases.map((c, i) => ({
    caseId: c.caseId,
    scheduledCaseId: identities?.[i] ?? `synthetic:${c.caseId}`,
    receiverRole: c.receiverRole,
    target: c.targetTypeCode,
    candidates: c.candidateTypeCodes,
    selection: c.selectedToken,
    shuffledSource: c.shuffledSourceCaseId,
    shuffledDelivered: c.shuffledDeliveredToken,
  })));
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
    const verdict = checkLedgerBatchValidity(batch.cases, ['scheduled-1', 'scheduled-1']);
    expect(verdict.ok).toBe(false);
    expect(verdict.reasons.join('\n')).toMatch(/one distinct scheduled case/);
    expect(verdict.reasons.join('\n')).toMatch(/sham shuffle/);
  });

  it('rejects the previously accepted out-of-domain positive (type 5) under the hardened gate', () => {
    // Hardened contract: the earlier positive used type 5, outside the
    // eligibleTypeCodes [1,2,3,4,6,7,8,9,11,12,13,14]. Construction still
    // runs, but the gate now refuses the batch with a domain reason.
    // The in-domain accept case lives in the next test.
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
    expect(verdict.ok).toBe(false);
    expect(verdict.genuineCollisions).toBe(0);
    expect(verdict.repeatedContent).toBe(0);
    expect(verdict.reasons.join('\n')).toContain('ex2: target 5 is not an eligible type code');
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

  it('rejects domain-invalid cases with exact reasons while the valid baseline accepts', () => {
    const baseline: AlaTreatmentDiagnosticCase[] = [
      { caseId: 'd-a', scheduledCaseId: 'sched-a', receiverRole: 'baby-b', target: 1, candidates: [1, 2, 3, 4], selection: 'T05', shuffledSource: 'd-b', shuffledDelivered: 'T21' },
      { caseId: 'd-b', scheduledCaseId: 'sched-b', receiverRole: 'baby-b', target: 6, candidates: [6, 7, 8, 9], selection: 'T21', shuffledSource: 'd-a', shuffledDelivered: 'T05' },
    ];
    const accepted = diagnoseLedgerBatch(baseline);
    expect(accepted.ok).toBe(true);
    expect(accepted.genuineCollisions).toBe(0);
    expect(accepted.repeatedContent).toBe(0);
    const subcases: Array<{ target: number; candidates: number[]; fragment: string }> = [
      { target: 1, candidates: [2, 3, 4, 6], fragment: 'd-a: target 1 is not among candidates' },
      { target: 5, candidates: [5, 6, 7, 8], fragment: 'd-a: target 5 is not an eligible type code' },
      { target: 1, candidates: [1, 2, 3, 5], fragment: 'd-a: candidate 5 is not an eligible type code' },
      { target: 1, candidates: [1, 1, 2, 3], fragment: 'd-a: candidates must be distinct' },
      { target: 1, candidates: [1, 2, 3], fragment: 'd-a: candidates must contain exactly four type codes' },
      { target: 1, candidates: [1, 2, 3, 4, 6], fragment: 'd-a: candidates must contain exactly four type codes' },
    ];
    for (const sub of subcases) {
      const batch = [{ ...baseline[0], target: sub.target, candidates: sub.candidates }, baseline[1]];
      const verdict = diagnoseLedgerBatch(batch);
      expect(verdict.ok).toBe(false);
      expect(verdict.genuineCollisions).toBe(0);
      expect(verdict.repeatedContent).toBe(0);
      expect(verdict.reasons.join('\n')).toContain(sub.fragment);
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
    // Conditioning repetition is observable even when delivery fails.
    const wrongDelivery = checkLedgerBatchValidity(batch.cases.map((c) => ({ ...c, shuffledDeliveredToken: 'T31' })));
    expect(wrongDelivery.ok).toBe(false);
    expect(wrongDelivery.repeatedContent).toBe(2);
    expect(wrongDelivery.genuineCollisions).toBe(0);
  });
});

describe('collection-entry gate on the repaired path', () => {
  const mkEntry = (caseId: string, scheduledCaseId: string, target: number, cands: number[]) => ({
    scheduledCaseId,
    treatmentCase: {
      caseId,
      receiverRole: 'baby-b' as const,
      targetTypeCode: target,
      candidateTypeCodes: cands,
    },
  });

  it('accepts a cross-case batch built through the repaired constructor', () => {
    const batch = buildValidLedgerTreatmentBatch({
      entries: [
        mkEntry('valid-a', 'scheduled-a', 1, [1, 2, 3, 4]),
        mkEntry('valid-b', 'scheduled-b', 6, [6, 7, 8, 9]),
      ],
      nativeIndexes: NATIVE_INDEXES,
      symbolInventory: INVENTORY,
      derangementSeed: DERANGEMENT_SEED,
    });
    expect(batch.version).toBe(2);
    expect(batch.batchCommitment).toMatch(/^sha256:[0-9a-f]{64}$/);
    const verdict = assertValidLedgerBatch(batch.cases.map((c) => ({
      caseId: c.caseId,
      scheduledCaseId: c.scheduledCaseId,
      receiverRole: c.receiverRole,
      target: c.targetTypeCode,
      candidates: c.candidateTypeCodes,
      selection: c.selectedToken,
      shuffledSource: c.shuffledSourceCaseId,
      shuffledDelivered: c.shuffledDeliveredToken,
    })));
    expect(verdict.ok).toBe(true);
  });

  it('refuses the historical sham shape at collection entry', () => {
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
    const batch = buildLedgerTreatmentBatch({
      cases: [consistent, shuffled],
      nativeIndexes: NATIVE_INDEXES,
      symbolInventory: INVENTORY,
      derangementSeed: DERANGEMENT_SEED,
    });
    const diagnostic = batch.cases.map((c) => ({
      caseId: c.caseId,
      scheduledCaseId: 'scheduled-1',
      receiverRole: c.receiverRole,
      target: c.targetTypeCode,
      candidates: c.candidateTypeCodes,
      selection: c.selectedToken,
      shuffledSource: c.shuffledSourceCaseId,
      shuffledDelivered: c.shuffledDeliveredToken,
    }));
    expect(diagnoseLedgerBatch(diagnostic).ok).toBe(false);
    expect(() => assertValidLedgerBatch(diagnostic)).toThrowError(/LV01 treatment gate:.*sham shuffle/);
  });

  it('refuses duplicate scheduled identities at construction', () => {
    expect(() => buildValidLedgerTreatmentBatch({
      entries: [
        mkEntry('dup-a', 'scheduled-1', 1, [1, 2, 3, 4]),
        mkEntry('dup-b', 'scheduled-1', 6, [6, 7, 8, 9]),
      ],
      nativeIndexes: NATIVE_INDEXES,
      symbolInventory: INVENTORY,
      derangementSeed: DERANGEMENT_SEED,
    })).toThrowError(/distinct scheduled cases for role baby-b/);
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

const explicitCases: readonly AlaTreatmentDiagnosticCase[] = [
  { caseId: 'branch-a', scheduledCaseId: 'schedule-a', receiverRole: 'baby-b', target: 1,
    candidates: [1, 2, 3, 4], selection: 'T05', shuffledSource: 'branch-b', shuffledDelivered: 'T21' },
  { caseId: 'branch-b', scheduledCaseId: 'schedule-b', receiverRole: 'baby-b', target: 6,
    candidates: [6, 7, 8, 9], selection: 'T21', shuffledSource: 'branch-a', shuffledDelivered: 'T05' },
];

describe('explicit diagnostic identities and malformed batches', () => {
  it('rejects an empty batch', () => {
    expect(diagnoseLedgerBatch([]).reasons).toContain('batch is empty or not a list');
  });
  it('rejects non-batch and null-case inputs without throwing', () => {
    const malformed = [null, {}, [null], [{}]] as unknown as readonly AlaTreatmentDiagnosticCase[][];
    for (const batch of malformed) {
      const verdict = diagnoseLedgerBatch(batch);
      expect(verdict.ok).toBe(false);
      expect(verdict.genuineCollisions).toBe(0);
      expect(verdict.repeatedContent).toBe(0);
    }
    expect(diagnoseLedgerBatch(null as unknown as readonly AlaTreatmentDiagnosticCase[]).reasons)
      .toContain('batch is empty or not a list');
    expect(diagnoseLedgerBatch([null] as unknown as readonly AlaTreatmentDiagnosticCase[])
      .reasons.join(' ')).toMatch(/malformed/);
  });
  it('rejects a missing schedule identity without falling back to branch names', () => {
    expect(diagnoseLedgerBatch(explicitCases.map((c) => ({ ...c, scheduledCaseId: '' }))).ok).toBe(false);
  });
  it('rejects inconsistent content under a single schedule identity', () => {
    const verdict = diagnoseLedgerBatch(explicitCases.map((c) => ({ ...c, scheduledCaseId: 'shared' })));
    expect(verdict.ok).toBe(false);
    expect(verdict.reasons.join(' ')).toMatch(/inconsistent conditioning/);
  });
  it('rejects duplicate branch records before resolving donors', () => {
    expect(diagnoseLedgerBatch([...explicitCases, explicitCases[0]]).reasons.join(' ')).toMatch(/duplicate branch/);
  });
  it('uses explicit identities regardless of branch suffix conventions', () => {
    const renamed = explicitCases.map((c, i) => ({ ...c,
      caseId: `shared-ledger-${i === 0 ? 'consistent' : 'shuffled'}:turn:0`,
      shuffledSource: `shared-ledger-${i === 1 ? 'consistent' : 'shuffled'}:turn:0`,
    }));
    expect(diagnoseLedgerBatch(renamed).ok).toBe(true);
  });
  it('rejects missing donors and cross-role deliveries without counting collisions', () => {
    expect(diagnoseLedgerBatch([{ ...explicitCases[0], shuffledSource: 'absent' }, explicitCases[1]]).ok).toBe(false);
    const crossRole = explicitCases.map((c, i) => ({ ...c,
      receiverRole: i === 0 ? 'baby-a' as const : 'baby-b' as const,
      selection: 'T05', shuffledDelivered: 'T05',
    }));
    const verdict = diagnoseLedgerBatch(crossRole);
    expect(verdict.ok).toBe(false);
    expect(verdict.genuineCollisions).toBe(0);
  });
});
