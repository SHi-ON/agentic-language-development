/**
 * ALA-LV thin Prototype runner (P3).
 *
 * One dyad end to end: disposable worker, parent training, freeze
 * verification, uniform collection-time ordinary, 240 scheduled paired cases
 * with committed treatment batches, branch execution from restored
 * pre-states, and independent per-case audit. Reuses the LV01 engine
 * family (schedule, training, ordinary wiring, treatments, executor,
 * auditor) with ALA-native seeds, run IDs, manifests, and receipts;
 * it never synthesizes an old confirmatory packet and never weakens a
 * historical validator.
 *
 * Gates (fail closed):
 * - execution-manifest drift (stale source/config/protocols/deps),
 * - study cohorts without a P5 admission record bound to the manifest,
 * - development/calibration beyond 3 attempts (amendment budget),
 * - re-running an attempted slot (terminal receipts are append-only).
 */
import { mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join } from 'node:path';

import { type EvidenceDatabase } from '@ald/evidence';
import { InMemorySignerRegistry, hashCanonical } from '@ald/hashing';
import { indexLv01TrainingLedger } from '@ald/learners';
import { LV01_UNTOUCHED_TYPE_CODES, ReferentialScenarioEngine } from '@ald/scenario';
import { fixedTokenInventory, type RunConfig, type SignerRegistry } from '@ald/types';

import { createProductionRuntime } from '../production.js';
import {
  buildLv01Schedule,
  scheduleCaseId,
  verifyLv01ExecutedCoverage,
  verifyLv01ScheduleCoverage,
} from '../experiments/lv01-schedule.js';
import {
  buildValidLedgerTreatmentBlock,
  deriveLv01DerangementSeed,
  type Lv01AnyLedgerTreatmentBatch,
  type Lv01LedgerTreatmentBatchV2,
} from '../experiments/lv01-ledger-treatments.js';
import {
  authenticateFromManifest,
  loadLv01AuditShared,
  loadLv01AuditedBranch,
  typedLedger,
} from '../experiments/lv01-audit-loader.js';
import { auditLv01PairedCase } from '../experiments/lv01-auditor.js';
import {
  buildLv01TreatmentCases,
  createLv01CollectorHarness,
  executeLv01PairedCaseBranches,
  lv01ParentCheckpointHash,
  prepareLv01PairedCase,
  prepareLv01PairedCaseFromBlock,
  readLv01BundleDocs,
  scheduledLv01BranchTarget,
} from '../experiments/lv01-collector.js';
import {
  buildLv01HeadsSidecar,
  deriveLv01LedgerCutoff,
  mapLv01LedgerAssociations,
} from '../experiments/lv01-paired-predictions.js';
import { trainLv01SlotParent } from '../experiments/lv01-parent-training.js';
import { LV01_BRANCHES, type Lv01PairedCasePlan } from '../experiments/ledger-value.js';

import {
  ALA_POLICY_VERSION,
  ALA_POLICY_VERSION_V2,
  ALA_SEED_ROOT,
  ALA_SEED_ROOT_V2,
  ALA_STUDY_ID,
  alaDyadSeeds,
  alaEngineSlotNumber,
  auditAlaSeedsAgainstRetained,
  type AlaCohort,
  type AlaSlotKind,
} from './ala-seeds.js';
import { alaProtocolPaths, verifyAlaExecutionManifest, type AlaExecutionManifest } from './ala-manifest.js';
import { alaPrototypeWorkload, alaStudyCounts } from './ala-workload.js';

export type AlaDyadState =
  | 'planned'
  | 'attempted'
  | 'resumed'
  | 'valid'
  | 'invalid'
  | 'aborted'
  | 'failed'
  | 'note'
  | 'unattempted';

export interface AlaDyadReceipt {
  readonly cohort: AlaCohort;
  readonly kind: AlaSlotKind;
  readonly index: number;
  readonly protocolVersion: 1 | 2;
  readonly studyVersion: string;
  readonly state: 'valid' | 'invalid';
  readonly reason: string;
  readonly runIdPrefix: string;
  readonly manifestSha256: string;
  readonly sourceCommit: string;
  readonly freezeDigests: { readonly policies: string; readonly checkpoints: string; readonly ledgerHeads: string };
  readonly scheduleCommitment: string;
  readonly caseCommitments: readonly string[];
  readonly casesAttempted: number;
  readonly casesValid: number;
  readonly cost: {
    readonly cpuMicroseconds: number;
    readonly wallMilliseconds: number;
    readonly peakBytes: number;
    readonly evidenceBytes: number;
    readonly verificationMilliseconds: number;
    readonly unresolved: readonly string[];
  };
}

export interface AlaAdmissionRecord {
  readonly status: 'admitted';
  readonly manifestSha256: string;
  readonly cohort: 'A' | 'B';
  readonly primaries: number;
  readonly reserves: number;
  readonly admittedAtUtc: string;
}

function fail(message: string): never {
  throw new Error(`ALA runner: ${message}`);
}

function pad4(n: number): string {
  return String(n).padStart(4, '0');
}

async function directoryBytes(root: string): Promise<number> {
  let total = 0;
  const names = await readdir(root, { withFileTypes: true });
  for (const entry of names) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) total += await directoryBytes(path);
    else if (entry.isFile()) total += (await readFile(path)).length;
  }
  return total;
}

export interface AlaJournalEntry {
  readonly at: string;
  readonly cohort: AlaCohort;
  readonly kind: AlaSlotKind;
  readonly index: number;
  readonly state: AlaDyadState;
  readonly reason?: string;
  /**
   * Historical-replay acknowledgement for version 1 attempt records:
   * the v1 path replays the retained sham construction byte-identically,
   * so the caller acknowledges that explicitly and the attempt keeps it.
   */
  readonly historicalReplayAck?: 'historical-sham-replay';
}

/**
 * Attempt/resumed journal entry. Version 1 carries the historical-replay
 * acknowledgement; version 2 attempts never do.
 */
export function alaAttemptJournalEntry(input: {
  readonly cohort: AlaCohort;
  readonly kind: AlaSlotKind;
  readonly index: number;
  readonly resuming: boolean;
  readonly protocolVersion: 1 | 2;
}): AlaJournalEntry {
  return {
    at: new Date().toISOString(),
    cohort: input.cohort,
    kind: input.kind,
    index: input.index,
    state: input.resuming ? 'resumed' : 'attempted',
    ...(input.protocolVersion === 1 ? { historicalReplayAck: 'historical-sham-replay' as const } : {}),
  };
}

export type AlaDyadSchedulePartition = 'validation-fit' | 'validation-selection' | 'within-support-test';

export interface AlaDyadSlotPlan {
  readonly slot: number;
  readonly partition: AlaDyadSchedulePartition;
  readonly receiver: 'baby-a' | 'baby-b';
  readonly caseIndex: number;
}

/**
 * Slot-to-case plan for one dyad. Version 1 executes the
 * within-support-test partition only (240 slots, parity receivers,
 * parity-duplicated cases). Version 2 executes all three partitions
 * in validation-fit, validation-selection, within-support-test order
 * (720 slots): receivers still alternate, but each role visits each
 * of its cases exactly once per partition.
 */
export function planAlaDyadSlots(
  counts: { readonly validationFitCases: number; readonly validationSelectionCases: number; readonly withinSupportTestCases: number },
  protocolVersion: 1 | 2,
): readonly AlaDyadSlotPlan[] {
  const partitions: { partition: AlaDyadSchedulePartition; count: number }[] =
    protocolVersion === 1
      ? [{ partition: 'within-support-test', count: counts.withinSupportTestCases }]
      : [
          { partition: 'validation-fit', count: counts.validationFitCases },
          { partition: 'validation-selection', count: counts.validationSelectionCases },
          { partition: 'within-support-test', count: counts.withinSupportTestCases },
        ];
  const slots: AlaDyadSlotPlan[] = [];
  let slot = 0;
  for (const { partition, count } of partitions) {
    for (let position = 0; position < 2 * count; position += 1) {
      slot += 1;
      slots.push({
        slot,
        partition,
        receiver: position % 2 === 0 ? 'baby-b' : 'baby-a',
        caseIndex: protocolVersion === 1 ? position % count : Math.floor(position / 2),
      });
    }
  }
  return slots;
}

/** Executed scheduled-case ids read back from per-slot schedule records. */
export async function readLv01ExecutedCaseIds(dyadDir: string, slots: readonly number[]): Promise<string[]> {
  const ids: string[] = [];
  for (const slot of slots) {
    const raw = (await readFile(
      join(dyadDir, `slot-${String(slot).padStart(4, '0')}`, 'schedule.json'),
      'utf8',
    )) as string;
    const record = JSON.parse(raw) as { executedCaseId?: unknown };
    if (typeof record.executedCaseId !== 'string' || record.executedCaseId.length === 0) {
      fail(`slot ${slot} schedule record carries no executed case id`);
    }
    ids.push(record.executedCaseId);
  }
  return ids;
}

export async function readAlaJournal(evidenceRoot: string): Promise<AlaJournalEntry[]> {
  const path = join(evidenceRoot, 'journal.jsonl');
  if (!existsSync(path)) return [];
  const lines = (await readFile(path, 'utf8')).split('\n').filter((line) => line.trim().length > 0);
  return lines.map((line) => JSON.parse(line) as AlaJournalEntry);
}

async function appendAlaJournal(evidenceRoot: string, entry: AlaJournalEntry): Promise<void> {
  await mkdir(evidenceRoot, { recursive: true });
  const path = join(evidenceRoot, 'journal.jsonl');
  const prior = existsSync(path) ? await readFile(path, 'utf8') : '';
  await writeFile(path, `${prior}${JSON.stringify(entry)}\n`, 'utf8');
}

/** Count attempted development/calibration dyads against the 3-attempt budget. */
export function alaDevAttemptsUsed(journal: readonly AlaJournalEntry[]): number {
  const attempted = new Set<string>();
  for (const entry of journal) {
    if (
      (entry.cohort === 'development' || entry.cohort === 'calibration') &&
      entry.state !== 'planned' && entry.state !== 'unattempted' && entry.state !== 'note'
    ) {
      attempted.add(`${entry.cohort}/${entry.kind}/${entry.index}`);
    }
  }
  return attempted.size;
}

export async function runAlaDyad(input: {
  readonly root: string;
  readonly manifestPath: string;
  readonly cohort: AlaCohort;
  readonly kind: AlaSlotKind;
  readonly index: number;
  readonly evidenceRoot: string;
  readonly retainedCorpus: readonly string[];
  readonly resume?: boolean;
  /**
   * Explicit batch-version selection: no silent default on this path.
   * Version 1 is historical replay of the retained sham audit only.
   * Version 2 is corrected-study collection from gate-validated
   * cross-case blocks; it requires study protocol version 2.
   */
  readonly treatmentBatchVersion: 1 | 2;
  /**
   * Explicit historical-replay acknowledgement, required for version 1.
   * Version 1 is not ordinary construction: it replays the historical
   * sham construction byte-identically, so selecting 1 alone is refused
   * and the caller must also pass 'historical-sham-replay', which the
   * attempt record journals. Version 2 refuses before this is read.
   */
  readonly historicalReplayAcknowledgement?: 'historical-sham-replay';
  /**
   * Study protocol selection. Version 1 is the retained audit (default,
   * reproducing today's behavior byte-identically). Version 2 is the
   * corrected study and requires batch version 2; batch version 2
   * still refuses outright until cross-slot scheduling lands (A1).
   */
  readonly protocolVersion?: 1 | 2;
}): Promise<AlaDyadReceipt> {
  const wallStart = Date.now();
  const cpuStart = process.cpuUsage();
  const rssStart = process.memoryUsage().rss;

  // 1. Manifest: stale source/config/protocols/deps never run.
  const manifest: AlaExecutionManifest = verifyAlaExecutionManifest(input.root, input.manifestPath);
  const manifestBytes = await readFile(join(input.root, input.manifestPath), 'utf8');
  const manifestSha256 = hashCanonical('ala-manifest-identity/v1', manifestBytes);

  // 2. Gates: terminal receipts are final; study needs admission; dev is budgeted.
  // Resume re-enters a crashed (attempted/failed, never terminal) slot with the
  // same seeds: a deterministic rerun, not a new attempt, so it consumes no
  // additional budget.
  const journal = await readAlaJournal(input.evidenceRoot);
  const prior = journal.filter(
    (e) => e.cohort === input.cohort && e.kind === input.kind && e.index === input.index,
  );
  const resuming = input.resume === true;
  if (prior.some((e) => e.state === 'valid' || e.state === 'invalid' || e.state === 'aborted')) {
    fail(`slot ${input.cohort}/${input.kind}/${input.index} has a terminal receipt; receipts are append-only`);
  }
  if (prior.some((e) => e.state === 'attempted' || e.state === 'resumed' || e.state === 'failed')) {
    if (!resuming) {
      fail(`slot ${input.cohort}/${input.kind}/${input.index} was already attempted; terminal receipts are append-only`);
    }
  } else if (resuming) {
    fail(`slot ${input.cohort}/${input.kind}/${input.index} was never attempted; nothing to resume`);
  }
  if (input.cohort === 'A' || input.cohort === 'B') {
    const admissionPath = join(input.evidenceRoot, 'admission.json');
    if (!existsSync(admissionPath)) fail(`cohort ${input.cohort} has no P5 admission record; collection stays disabled`);
    const admission = JSON.parse(await readFile(admissionPath, 'utf8')) as AlaAdmissionRecord;
    if (admission.status !== 'admitted' || admission.cohort !== input.cohort) {
      fail(`cohort ${input.cohort} admission record is invalid`);
    }
    if (admission.manifestSha256 !== manifestSha256) fail('admission record binds a different execution manifest');
  } else if (!resuming && alaDevAttemptsUsed(journal) >= 3) {
    fail('development/calibration attempt budget (3) is exhausted');
  }
  const protocolVersion = input.protocolVersion ?? 1;
  const protocolPaths = alaProtocolPaths(protocolVersion);
  const seedRoot = protocolVersion === 2 ? ALA_SEED_ROOT_V2 : ALA_SEED_ROOT;
  const policyVersion = protocolVersion === 2 ? ALA_POLICY_VERSION_V2 : ALA_POLICY_VERSION;
  if (input.treatmentBatchVersion === 1 && protocolVersion === 2) {
    fail('study protocol version 2 requires batch version 2; refusing before anything is written');
  }
  if (input.treatmentBatchVersion === 2 && protocolVersion === 1) {
    fail('batch version 2 (cross-slot scheduling) requires study protocol version 2; refusing before anything is written');
  }
  if (protocolVersion === 1 && input.historicalReplayAcknowledgement !== 'historical-sham-replay') {
    fail('version 1 replays the historical sham construction byte-identically: '
      + 'pass historicalReplayAcknowledgement "historical-sham-replay" explicitly; refusing');
  }
  if (protocolVersion === 2 && input.historicalReplayAcknowledgement !== undefined) {
    fail('the historical-replay acknowledgement is forbidden on the corrected-study path; refusing');
  }
  await appendAlaJournal(
    input.evidenceRoot,
    alaAttemptJournalEntry({
      cohort: input.cohort,
      kind: input.kind,
      index: input.index,
      resuming,
      protocolVersion,
    }),
  );

  const slotLabel = pad4(input.index);
  const engineSlot = alaEngineSlotNumber(input.cohort, input.kind, input.index);
  const engineSlotLabel = pad4(engineSlot);
  const dyadDir = join(input.evidenceRoot, input.cohort, `${input.kind}-${slotLabel}`);
  if (resuming) {
    // Deterministic rerun from identical seeds: remove partial output so no
    // stale case can mix with the rerun. The journal retains the crash record.
    await rm(dyadDir, { recursive: true, force: true });
  }
  await mkdir(dyadDir, { recursive: true });

  // 3. Seeds in the ALA namespace; audit against retained LV01 identities.
  const seeds = alaDyadSeeds(input.cohort, input.kind, slotLabel, seedRoot);
  auditAlaSeedsAgainstRetained(Object.values(seeds), input.retainedCorpus);

  // 4. Disposable worker: fresh store, fresh signers, Prototype topology.
  const dbPath = join(dyadDir, 'evidence.sqlite');
  const production = createProductionRuntime({
    databasePath: dbPath,
    bundleRoot: dyadDir,
    softwareCommit: manifest.protocolCommit,
    anchorPolicy: 'skip',
  });
  const database: EvidenceDatabase = production.database;
  const registries = new Map<string, SignerRegistry>();
  const signerProvider = (runId: string): SignerRegistry => {
    const existing = registries.get(runId);
    if (existing !== undefined) return existing;
    const created = InMemorySignerRegistry.generate(runId);
    registries.set(runId, created);
    return created;
  };

  const studyCounts = alaStudyCounts(join(input.root, protocolPaths.study));
  const workload = alaPrototypeWorkload(studyCounts);

  try {
    // 5. Train the parent once (internal LV01 engine family; ALA run identity).
    const parentRunId = `ala-${input.cohort}-${input.kind}-${slotLabel}-parent`;
    const parentBundleDir = join(dyadDir, 'parent-bundle');
    const trained = await trainLv01SlotParent({
      runtime: production.runtime,
      database,
      signerProvider,
      runId: parentRunId,
      seeds: { scenario: seeds.scenario, babyA: seeds.babyA, babyB: seeds.babyB, gateway: seeds.channel, analysis: seeds.analysis },
      training: {
        track: 'scratch-rl',
        modelRef: 'gru-actor-critic-v1',
        learningSignal: 'extrinsic-task',
        maxTurnsPerRun: workload.trainingTurns,
        evaluationTurns: 0,
      },
      ledgerValuePlan: {
        version: 1,
        designCommitmentHash: manifest.protocols[protocolPaths.study]!.sha256,
        analysisCommitmentHash: manifest.protocols[protocolPaths.analysis]!.sha256,
        seedResourceCommitmentHash: manifest.protocols[protocolPaths.amendment]!.sha256,
        predictionFunctionVersion: 'lv01-ledger-value-prediction/v1',
        partitionContractVersion: 'lv01-within-support/v1',
      },
      runsRoot: dyadDir,
      bundleDir: parentBundleDir,
      softwareCommit: manifest.protocolCommit,
      deploymentMode: 'prototype',
      protocolGitCommit: manifest.protocolCommit,
    });

    // 6. Freeze verification from actual bytes (never a stored boolean).
    const parentDocs = await readLv01BundleDocs(trained.bundleDir);
    const parent = JSON.parse(JSON.stringify(parentDocs.runConfig)) as RunConfig;
    if (parent.experimentId !== 'LV01') fail('parent is not the internal LV01 engine family');
    const freezeDigests = {
      policies: hashCanonical('ala-freeze-policies/v1', parentDocs.policies),
      checkpoints: hashCanonical('ala-freeze-checkpoints/v1', parentDocs.checkpoints),
      ledgerHeads: hashCanonical('ala-freeze-ledger-heads/v1', {
        babyA: (parentDocs.ledgerA as Array<{ entryHash: string }>).map((e) => e.entryHash),
        babyB: (parentDocs.ledgerB as Array<{ entryHash: string }>).map((e) => e.entryHash),
      }),
    };
    const parentCheckpointHash = lv01ParentCheckpointHash(parentDocs);
    const authenticate = authenticateFromManifest(parentDocs.runManifest);
    // Corrected path: heads object stored once per dyad; branch payloads
    // carry its digest instead of the embedded arrays.
    if (protocolVersion === 2) {
      const sidecarRecordsA = mapLv01LedgerAssociations(parentDocs.ledgerA as never, 'baby-a', authenticate);
      const sidecarRecordsB = mapLv01LedgerAssociations(parentDocs.ledgerB as never, 'baby-b', authenticate);
      const sidecar = buildLv01HeadsSidecar({
        babyA: typedLedger(parentDocs.ledgerA, 'parent baby-a-ledger.jsonl').map((event) => event.entryHash),
        babyB: typedLedger(parentDocs.ledgerB, 'parent baby-b-ledger.jsonl').map((event) => event.entryHash),
        cutoffBabyA: deriveLv01LedgerCutoff(sidecarRecordsA),
        cutoffBabyB: deriveLv01LedgerCutoff(sidecarRecordsB),
      });
      await writeFile(join(dyadDir, 'training-ledger-heads.json'), `${JSON.stringify(sidecar, null, 2)}\n`, 'utf8');
    }
    // Branch harnesses open the same store file; close the training handle
    // first (same discipline as the collector's own live tests).
    production.close();

    // 7. Schedule over the ALA scenario seed; ordinary providers from parent.
    const engine = new ReferentialScenarioEngine(
      {
        version: 1,
        symbolInventory: fixedTokenInventory(32),
        interactionMode: parent.interactionMode,
        heldOutTypeCodes: [...LV01_UNTOUCHED_TYPE_CODES],
      },
      seeds.scenario,
    );
    const schedule = buildLv01Schedule({
      engine,
      counts: {
        training: 0,
        'validation-fit': workload.validationFitCases,
        'validation-selection': workload.validationSelectionCases,
        'within-support-test': workload.withinSupportTestCases,
      },
      receiverRoles: ['baby-a', 'baby-b'],
    });
    const inventory = fixedTokenInventory(32);
    // Collection-time ordinary is the uniform null (same as the production
    // collect-lv01-stage path): ordinary fitting and BIC selection are
    // analysis-time operations on the sealed packets, never fitted from
    // the parent at collection time.
    const uniformOrdinary = { ordinaryId: 'uniform', ordinaryFit: { kind: 'uniform' } } as const;

    // 8. Paired cases: v1 walks the test partition (240 slots); v2
    // walks validation-fit, validation-selection, then test (720 slots).
    const caseCommitments: string[] = [];
    let verificationMilliseconds = 0;
    let peakBytes = Math.max(rssStart, process.memoryUsage().rss);
    const slotPlan = planAlaDyadSlots(workload, protocolVersion);
    const scheduledFor = (entry: AlaDyadSlotPlan) => ({
      ...entry,
      scheduled: scheduledLv01BranchTarget({
        schedule,
        partition: entry.partition,
        caseIndex: entry.caseIndex,
        receiver: entry.receiver,
      }),
      childRunIdPrefix: `ala-${input.cohort}-${input.kind}-${slotLabel}-s${pad4(entry.slot)}`,
    });
    // Corrected path: gate-validated treatment blocks per role per
    // partition, built and persisted BEFORE the slot loop (and before
    // any branch runs). A sham block refuses here with the attempt
    // already journalled as attempted, leaving the audit trail intact;
    // terminal refusal states arrive in A6.
    const blockBatches = new Map<string, Lv01LedgerTreatmentBatchV2>();
    if (protocolVersion === 2) {
      const blockRecordsA = mapLv01LedgerAssociations(parentDocs.ledgerA as never, 'baby-a', authenticate);
      const blockRecordsB = mapLv01LedgerAssociations(parentDocs.ledgerB as never, 'baby-b', authenticate);
      const blockIndexes = {
        babyA: indexLv01TrainingLedger(blockRecordsA, 'baby-a', deriveLv01LedgerCutoff(blockRecordsA)),
        babyB: indexLv01TrainingLedger(blockRecordsB, 'baby-b', deriveLv01LedgerCutoff(blockRecordsB)),
      };
      const blockKeys = new Map<string, AlaDyadSlotPlan[]>();
      for (const entry of slotPlan) {
        const key = `${entry.partition}:${entry.receiver}`;
        const group = blockKeys.get(key) ?? [];
        group.push(entry);
        blockKeys.set(key, group);
      }
      for (const [key, group] of [...blockKeys.entries()].sort(([a], [b]) => (a < b ? -1 : 1))) {
        const [partition, role] = key.split(':') as [AlaDyadSchedulePartition, 'baby-a' | 'baby-b'];
        const entries: { scheduledCaseId: string; treatmentCase: ReturnType<typeof buildLv01TreatmentCases>[number] }[] = [];
        for (const entry of group) {
          const planned = scheduledFor(entry);
          for (const treatmentCase of buildLv01TreatmentCases(planned.scheduled, planned.childRunIdPrefix)) {
            entries.push({ scheduledCaseId: planned.scheduled.scheduledCaseId, treatmentCase });
          }
        }
        const block = buildValidLedgerTreatmentBlock({
          entries,
          nativeIndexes: blockIndexes,
          symbolInventory: inventory,
          derangementSeed: deriveLv01DerangementSeed({
            root: seedRoot,
            studyId: ALA_STUDY_ID,
            stage: `ala-${input.cohort}`,
            policyVersion,
            slotKind: input.kind,
            slotIndex: engineSlotLabel,
            partition,
          }),
        });
        await writeFile(
          join(dyadDir, `treatment-batch-${partition}-${role}.json`),
          `${JSON.stringify(block, null, 2)}\n`,
          'utf8',
        );
        blockBatches.set(key, block);
      }
    }
    for (const entry of slotPlan) {
      const { slot, partition, receiver, caseIndex, scheduled, childRunIdPrefix } = scheduledFor(entry);
      let plan: Lv01PairedCasePlan;
      let batch: Lv01AnyLedgerTreatmentBatch;
      if (protocolVersion === 1) {
        const treatmentCases = buildLv01TreatmentCases(scheduled, childRunIdPrefix);
        const derangementSeed = deriveLv01DerangementSeed({
          root: seedRoot,
          studyId: ALA_STUDY_ID,
          stage: `ala-${input.cohort}`,
          policyVersion,
          slotKind: input.kind,
          slotIndex: engineSlotLabel,
          partition: 'within-support',
        });
        const recordsA = mapLv01LedgerAssociations(parentDocs.ledgerA as never, 'baby-a', authenticate);
        const recordsB = mapLv01LedgerAssociations(parentDocs.ledgerB as never, 'baby-b', authenticate);
        const prepared = prepareLv01PairedCase({
          parent,
          parentCheckpointHash,
          babyAInitialPolicyRef: 'policies/baby-a-latest.json',
          babyBInitialPolicyRef: 'policies/baby-b-latest.json',
          // Explicit historical replay, acknowledged at the runner gate
          // above: the retained per-slot audit shape.
          treatmentBatchVersion: 1,
          childRunIdPrefix,
          // Engine-internal draw scope: stage is the LV01-family constant;
          // uniqueness across ALA dyads comes from the ALA slot scenario
          // seed root. External identity stays cohort/kind/index.
          actionDrawScope: { stage: 'pilot', slotKind: input.kind, slotIndex: engineSlotLabel, partition: 'within-support' },
          treatmentCases: [...treatmentCases],
          nativeIndexes: {
            babyA: indexLv01TrainingLedger(recordsA, 'baby-a', deriveLv01LedgerCutoff(recordsA)),
            babyB: indexLv01TrainingLedger(recordsB, 'baby-b', deriveLv01LedgerCutoff(recordsB)),
          },
          symbolInventory: inventory,
          derangementSeed,
          slotSeeds: { scenario: seeds.scenario, babyA: seeds.babyA, babyB: seeds.babyB, gateway: seeds.channel, analysis: seeds.analysis },
          scheduledCase: { partition: 'within-support-test', caseIndex, receiverRole: receiver },
        });
        plan = prepared.plan;
        batch = prepared.batch;
      } else {
        const block = blockBatches.get(`${partition}:${receiver}`);
        if (block === undefined) fail(`treatment block for ${partition}:${receiver} was not built`);
        const prepared = prepareLv01PairedCaseFromBlock({
          parent,
          parentCheckpointHash,
          babyAInitialPolicyRef: 'policies/baby-a-latest.json',
          babyBInitialPolicyRef: 'policies/baby-b-latest.json',
          childRunIdPrefix,
          actionDrawScope: { stage: 'pilot', slotKind: input.kind, slotIndex: engineSlotLabel, partition: 'within-support' },
          slotSeeds: { scenario: seeds.scenario, babyA: seeds.babyA, babyB: seeds.babyB, gateway: seeds.channel, analysis: seeds.analysis },
          scheduledCase: { partition, caseIndex, receiverRole: receiver },
          blockBatch: block,
        });
        plan = prepared.plan;
        batch = prepared.batch;
      }
      const caseDir = join(dyadDir, `slot-${pad4(slot)}`);
      await mkdir(caseDir, { recursive: true });
      // Treatment batch commits BEFORE any branch runs.
      if (protocolVersion === 1) {
        await writeFile(join(caseDir, 'treatment-batch.json'), `${JSON.stringify(batch, null, 2)}\n`, 'utf8');
      }
      await writeFile(
        join(caseDir, 'schedule.json'),
        protocolVersion === 1
          ? `${JSON.stringify({ commitment: schedule.commitment, executedCaseId: scheduled.scheduledCaseId }, null, 2)}\n`
          : `${JSON.stringify({ commitment: schedule.commitment, partition, executedCaseId: scheduled.scheduledCaseId }, null, 2)}\n`,
        'utf8',
      );
      const harness = await createLv01CollectorHarness({
        root: dyadDir,
        databasePath: dbPath,
        softwareCommit: manifest.protocolCommit,
        ordinary: uniformOrdinary,
      });
      try {
        await executeLv01PairedCaseBranches({ plan, harness, caseDir, softwareCommit: manifest.protocolCommit });
      } finally {
        harness.close();
      }
      peakBytes = Math.max(peakBytes, process.memoryUsage().rss);
      const verifyStart = Date.now();
      const executions = (await readdir(join(caseDir, 'branches'))).sort();
      if (executions.length !== 7) fail(`case ${slot} executed ${executions.length} branches, expected 7`);
      const branchDocs = [];
      for (const runId of executions) {
        branchDocs.push(await readLv01BundleDocs(join(caseDir, 'branches', runId)));
      }
      const paired = branchDocs.map((docs) => ({ docs, branch: loadLv01AuditedBranch(docs) }));
      for (const { branch } of paired) {
        if (branch.payload.state.scenarioStateHash !== scheduled.stateHash) {
          fail(`case ${slot} branch ${branch.branch} served outside the schedule`);
        }
      }
      // The auditor requires canonical LV01 branch order; run directories
      // sort alphabetically, so reorder explicitly and keep docs attached.
      const order = new Map<string, number>(LV01_BRANCHES.map((name, position) => [name, position]));
      paired.sort((a, b) => (order.get(a.branch.branch) ?? 99) - (order.get(b.branch.branch) ?? 99));
      const audited = paired.map((entry) => entry.branch);
      const first = audited[0];
      const firstDocs = paired[0]?.docs;
      if (first === undefined || firstDocs === undefined) fail(`case ${slot} has no audited branches`);
      const shared = loadLv01AuditShared({ parentDocs, branchDocs: firstDocs, firstBranch: first, treatmentBatch: batch });
      const { caseCommitment } = auditLv01PairedCase({ plan, shared, branches: audited });
      verificationMilliseconds += Date.now() - verifyStart;
      peakBytes = Math.max(peakBytes, process.memoryUsage().rss);
      caseCommitments.push(caseCommitment);
    }

    const plannedCaseIds = slotPlan.map((entry) => scheduleCaseId(entry.partition, entry.receiver, entry.caseIndex));
    const scheduleCommitment =
      protocolVersion === 1
        ? verifyLv01ScheduleCoverage(schedule, plannedCaseIds)
        : verifyLv01ExecutedCoverage(
            schedule,
            plannedCaseIds,
            await readLv01ExecutedCaseIds(
              dyadDir,
              slotPlan.map((entry) => entry.slot),
            ),
          );
    const cpu = process.cpuUsage(cpuStart);
    const receipt: AlaDyadReceipt = {
      cohort: input.cohort,
      kind: input.kind,
      index: input.index,
      protocolVersion,
      studyVersion: `ala-lv-study/v${protocolVersion}`,
      state: 'valid',
      reason: 'all paired cases executed and audited',
      runIdPrefix: `ala-${input.cohort}-${input.kind}-${slotLabel}`,
      manifestSha256,
      sourceCommit: manifest.protocolCommit,
      freezeDigests,
      scheduleCommitment,
      caseCommitments,
      casesAttempted: slotPlan.length,
      casesValid: caseCommitments.length,
      cost: {
        cpuMicroseconds: Math.round(cpu.user + cpu.system),
        wallMilliseconds: Date.now() - wallStart,
        peakBytes,
        evidenceBytes: await directoryBytes(dyadDir),
        verificationMilliseconds,
        unresolved: [],
      },
    };
    await writeFile(join(dyadDir, 'dyad-receipt.json'), `${JSON.stringify(receipt, null, 2)}\n`, 'utf8');
    await appendAlaJournal(input.evidenceRoot, {
      at: new Date().toISOString(),
      cohort: input.cohort,
      kind: input.kind,
      index: input.index,
      state: 'valid',
      reason: receipt.reason,
    });
    return receipt;
  } finally {
    try {
      database.close();
    } catch {
      // Already closed after training; branch harnesses manage their own handles.
    }
  }
}
