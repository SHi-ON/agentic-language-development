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
  verifyLv01ScheduleCoverage,
} from '../experiments/lv01-schedule.js';
import { deriveLv01DerangementSeed } from '../experiments/lv01-ledger-treatments.js';
import {
  authenticateFromManifest,
  loadLv01AuditShared,
  loadLv01AuditedBranch,
} from '../experiments/lv01-audit-loader.js';
import { auditLv01PairedCase } from '../experiments/lv01-auditor.js';
import {
  buildLv01TreatmentCases,
  createLv01CollectorHarness,
  executeLv01PairedCaseBranches,
  lv01ParentCheckpointHash,
  prepareLv01PairedCase,
  readLv01BundleDocs,
  scheduledLv01BranchTarget,
} from '../experiments/lv01-collector.js';
import {
  deriveLv01LedgerCutoff,
  mapLv01LedgerAssociations,
} from '../experiments/lv01-paired-predictions.js';
import { trainLv01SlotParent } from '../experiments/lv01-parent-training.js';
import { LV01_BRANCHES } from '../experiments/ledger-value.js';

import {
  ALA_POLICY_VERSION,
  ALA_SEED_ROOT,
  ALA_STUDY_ID,
  alaDyadSeeds,
  alaEngineSlotNumber,
  auditAlaSeedsAgainstRetained,
  type AlaCohort,
  type AlaSlotKind,
} from './ala-seeds.js';
import { verifyAlaExecutionManifest, type AlaExecutionManifest } from './ala-manifest.js';
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
   * Version 2 is new collection, refused outright until cross-slot
   * scheduling lands (Phase-5 precondition): the per-slot flow builds
   * both branch records from one scheduled case, which the gate rejects.
   */
  readonly treatmentBatchVersion: 1 | 2;
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
  if (input.treatmentBatchVersion === 2) {
    fail('new collection requires cross-slot scheduling (Phase-5 precondition): '
      + 'the per-slot flow builds both branch records from one scheduled case, '
      + 'which the collection-entry gate rejects; refusing before anything is written');
  }
  await appendAlaJournal(input.evidenceRoot, {
    at: new Date().toISOString(),
    cohort: input.cohort,
    kind: input.kind,
    index: input.index,
    state: resuming ? 'resumed' : 'attempted',
  });

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
  const seeds = alaDyadSeeds(input.cohort, input.kind, slotLabel);
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

  const studyCounts = alaStudyCounts(join(input.root, 'protocols/ala-lv-study.v1.json'));
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
        designCommitmentHash: manifest.protocols['protocols/ala-lv-study.v1.json']!.sha256,
        analysisCommitmentHash: manifest.protocols['protocols/ala-lv-analysis.v1.json']!.sha256,
        seedResourceCommitmentHash: manifest.protocols['protocols/ala-lv-scope-and-resource-amendment.v1.json']!.sha256,
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

    // 8. Paired cases: rotation serves 120 cases per role over 240 slots.
    const caseCommitments: string[] = [];
    let verificationMilliseconds = 0;
    let peakBytes = Math.max(rssStart, process.memoryUsage().rss);
    const slotCount = 2 * workload.withinSupportTestCases;
    for (let slot = 1; slot <= slotCount; slot += 1) {
      const receiver = slot % 2 === 1 ? 'baby-b' : 'baby-a';
      const caseIndex = (slot - 1) % workload.withinSupportTestCases;
      const scheduled = scheduledLv01BranchTarget({
        schedule,
        partition: 'within-support-test',
        caseIndex,
        receiver,
      });
      const childRunIdPrefix = `ala-${input.cohort}-${input.kind}-${slotLabel}-s${pad4(slot)}`;
      const treatmentCases = buildLv01TreatmentCases(scheduled, childRunIdPrefix);
      const derangementSeed = deriveLv01DerangementSeed({
        root: ALA_SEED_ROOT,
        studyId: ALA_STUDY_ID,
        stage: `ala-${input.cohort}`,
        policyVersion: ALA_POLICY_VERSION,
        slotKind: input.kind,
        slotIndex: engineSlotLabel,
        partition: 'within-support',
      });
      const recordsA = mapLv01LedgerAssociations(parentDocs.ledgerA as never, 'baby-a', authenticate);
      const recordsB = mapLv01LedgerAssociations(parentDocs.ledgerB as never, 'baby-b', authenticate);
      const { plan, batch } = prepareLv01PairedCase({
        parent,
        parentCheckpointHash,
        babyAInitialPolicyRef: 'policies/baby-a-latest.json',
        babyBInitialPolicyRef: 'policies/baby-b-latest.json',
        // Explicit historical replay: the retained per-slot audit shape.
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
      const caseDir = join(dyadDir, `slot-${pad4(slot)}`);
      await mkdir(caseDir, { recursive: true });
      // Treatment batch commits BEFORE any branch runs.
      await writeFile(join(caseDir, 'treatment-batch.json'), `${JSON.stringify(batch, null, 2)}\n`, 'utf8');
      await writeFile(
        join(caseDir, 'schedule.json'),
        `${JSON.stringify({ commitment: schedule.commitment, executedCaseId: scheduled.scheduledCaseId }, null, 2)}\n`,
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

    const scheduleCommitment = verifyLv01ScheduleCoverage(
      schedule,
      caseCommitments.map((_, i) => {
        const slot = i + 1;
        const receiverRole = slot % 2 === 1 ? 'baby-b' : 'baby-a';
        return `within-support-test:${receiverRole}:${String((slot - 1) % workload.withinSupportTestCases).padStart(4, '0')}`;
      }),
    );
    const cpu = process.cpuUsage(cpuStart);
    const receipt: AlaDyadReceipt = {
      cohort: input.cohort,
      kind: input.kind,
      index: input.index,
      state: 'valid',
      reason: 'all paired cases executed and audited',
      runIdPrefix: `ala-${input.cohort}-${input.kind}-${slotLabel}`,
      manifestSha256,
      sourceCommit: manifest.protocolCommit,
      freezeDigests,
      scheduleCommitment,
      caseCommitments,
      casesAttempted: slotCount,
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
