#!/usr/bin/env tsx
/**
 * Build one small genuine valid ledger-treatment batch through the
 * repaired cross-case construction path and print canonical JSON.
 *
 * Construction only: a precomputed engine schedule supplies two distinct
 * scheduled cases per role; selections use a synthetic test-grade native
 * index (fully disclosed in the output provenance). No training, no
 * cohorts, no P5 admission. The collection-entry gate runs inside the
 * constructor and fails closed; the printed verdict is the gate verdict.
 * The paper seals this output as a new input with digests.
 */
import { ReferentialScenarioEngine } from '@ald/scenario';
import { fixedTokenInventory } from '@ald/types';
import {
  LV01_PREDICTION_FUNCTION_VERSION,
  type Lv01NativeLedgerIndex,
} from '@ald/learners';

import {
  buildLv01Schedule,
  scheduledCaseFor,
} from '../packages/orchestrator/src/experiments/lv01-schedule.js';
import {
  buildValidLedgerTreatmentBatch,
  deriveLv01DerangementSeed,
} from '../packages/orchestrator/src/experiments/lv01-ledger-treatments.js';
import { diagnoseLedgerBatch } from '../packages/orchestrator/src/experiments/lv01-treatment-gate.js';

const SCHEDULE_SEED = 'ala-valid-ledger-batch/v1';
const SLOT = { slotKind: 'primary', slotIndex: '0001', stage: 'ala-calibration', partition: 'within-support' } as const;

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

/** Synthetic test-grade native index (same shape as the validity-test
 * fixture): deterministic selections without training. Disclosed, not
 * a trained ledger.
 */
function syntheticIndex(role: 'baby-a' | 'baby-b'): Lv01NativeLedgerIndex {
  const w05 = new Array(16).fill(0);
  w05[1] = 10;
  w05[2] = 10;
  const w17 = new Array(16).fill(0);
  w17[5] = 10;
  const w21 = new Array(16).fill(0);
  w21[6] = 10;
  return {
    predictionFunctionVersion: LV01_PREDICTION_FUNCTION_VERSION,
    receiverRole: role,
    cutoff: { sequence: 2, turn: 1 },
    selectedAssociations: [association('T05', w05), association('T17', w17), association('T21', w21)],
  };
}

const engine = new ReferentialScenarioEngine(
  {
    version: 1,
    symbolInventory: fixedTokenInventory(32),
    interactionMode: 'cooperative-signaling',
    heldOutTypeCodes: [0, 5, 10, 15],
  },
  SCHEDULE_SEED,
);
const schedule = buildLv01Schedule({
  engine,
  counts: { training: 0, 'validation-fit': 0, 'validation-selection': 0, 'within-support-test': 2 },
});
const inventory = fixedTokenInventory(32);
const derangementSeed = deriveLv01DerangementSeed({
  root: 'ala-ledger-value-v1',
  studyId: 'ALA-LV',
  stage: SLOT.stage,
  policyVersion: 'v1',
  slotKind: SLOT.slotKind,
  slotIndex: SLOT.slotIndex,
  partition: SLOT.partition,
});
const entries = (['baby-a', 'baby-b'] as const).flatMap((role) =>
  [0, 1].map((caseIndex) => {
    const scheduled = scheduledCaseFor(schedule, 'within-support-test', role, caseIndex);
    return {
      scheduledCaseId: scheduled.caseId,
      treatmentCase: {
        caseId: `valid-${role}-${caseIndex}:turn:0`,
        receiverRole: role,
        targetTypeCode: scheduled.targetTypeCode,
        candidateTypeCodes: [...scheduled.candidateTypeCodes],
      },
    };
  }),
);
const batch = buildValidLedgerTreatmentBatch({
  entries,
  nativeIndexes: { babyA: syntheticIndex('baby-a'), babyB: syntheticIndex('baby-b') },
  symbolInventory: inventory,
  derangementSeed,
});
const verdict = diagnoseLedgerBatch(batch.cases.map((entry) => ({
  caseId: entry.caseId,
  scheduledCaseId: entry.scheduledCaseId,
  receiverRole: entry.receiverRole,
  target: entry.targetTypeCode,
  candidates: entry.candidateTypeCodes,
  selection: entry.selectedToken,
  shuffledSource: entry.shuffledSourceCaseId,
  shuffledDelivered: entry.shuffledDeliveredToken,
})));
process.stdout.write(`${JSON.stringify({
  artifact: 'ala-valid-ledger-batch/v1',
  provenance: {
    generator: 'scripts/build-valid-ledger-batch.ts',
    scheduleSeed: SCHEDULE_SEED,
    scheduleCommitment: schedule.commitment,
    derangementSeedParts: {
      root: 'ala-ledger-value-v1',
      studyId: 'ALA-LV',
      ...SLOT,
      policyVersion: 'v1',
    },
    nativeIndex: 'synthetic test-grade fixture (no training; selections deterministic from fixture weights)',
    roles: ['baby-a', 'baby-b'],
    casesPerRole: 2,
  },
  batch,
  verdict,
}, null, 2)}\n`);
