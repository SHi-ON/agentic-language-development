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

/** Synthetic test-grade native index: each eligible type maps to one
 * distinct token (weight 10 on its type, 0 elsewhere), so distinct
 * scheduled targets select distinct tokens without training. Fully
 * disclosed fixture weights, not a trained ledger; collisions in the
 * batch therefore reflect chance overlap, not engineered coincidences.
 */
const ELIGIBLE_TYPES = [1, 2, 3, 4, 6, 7, 8, 9, 11, 12, 13, 14];
// Members of the real fixedTokenInventory(32) (S01..S32): the
// association token must be an inventory member to ever be selected.
const TYPE_TOKENS = ['S03', 'S05', 'S07', 'S09', 'S11', 'S13', 'S15', 'S17', 'S19', 'S21', 'S23', 'S25'];

function syntheticIndex(role: 'baby-a' | 'baby-b'): Lv01NativeLedgerIndex {
  return {
    predictionFunctionVersion: LV01_PREDICTION_FUNCTION_VERSION,
    receiverRole: role,
    cutoff: { sequence: 2, turn: 1 },
    selectedAssociations: ELIGIBLE_TYPES.map((type, i) => {
      const weights = new Array(16).fill(0);
      weights[type] = 10;
      return association(TYPE_TOKENS[i] as string, weights);
    }),
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
  counts: { training: 0, 'validation-fit': 0, 'validation-selection': 0, 'within-support-test': 6 },
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
  [0, 1, 2, 3, 4, 5].map((caseIndex) => {
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
    nativeIndex: 'synthetic test-grade fixture (no training): each of the 12 eligible types maps to one distinct token at weight 10, so distinct scheduled targets select distinct tokens deterministically',
    roles: ['baby-a', 'baby-b'],
    casesPerRole: 6,
  },
  batch,
  verdict,
}, null, 2)}\n`);
