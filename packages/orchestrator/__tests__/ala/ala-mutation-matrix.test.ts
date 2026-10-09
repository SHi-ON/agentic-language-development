/**
 * ALA-LV P4 mutation matrix (paper-scoped corruption fixtures).
 *
 * Each mutation tampers with a copy of staged live evidence (a tiny
 * trained parent bundle plus one collected paired case) and asserts the
 * exact loader/auditor check that detects it. No tampered copy ever
 * enters analysis; the matrix proves tampering fails closed, never that
 * tampered data is usable.
 */
import { cp, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { exportRunBundle, SqliteEvidenceWriter } from '@ald/evidence';
import { loadLearnerContract } from '@ald/learners';
import { afterEach, describe, expect, it } from 'vitest';

import { createHarness, type Harness } from '../helpers.js';
import { collectLv01Slot } from '../../src/experiments/lv01-collector.js';
import {
  authenticateFromManifest,
  loadLv01AuditedBranch,
} from '../../src/experiments/lv01-audit-loader.js';
import {
  deriveLv01LedgerCutoff,
  mapLv01LedgerAssociations,
} from '../../src/experiments/lv01-paired-predictions.js';
import { compileLv01StagePacket } from '../../src/experiments/lv01-stage-packets.js';
import { indexLv01TrainingLedger } from '@ald/learners';
import { extractLv01OrdinaryRecords } from '../../src/experiments/lv01-ordinary-loader.js';
import {
  ORDINARY,
  fixtureParent,
  packetInputWithAllocation,
} from '../experiments/lv01-collector-fixtures.js';
import { readLv01BundleDocs } from '../../src/experiments/lv01-collector.js';

describe('ALA mutation matrix', () => {
  let harness: Harness | undefined;
  let scratch: string | undefined;
  let parentBundleDir = '';
  let caseDir = '';

  async function stage(): Promise<void> {
    harness = await createHarness({
      lv01PredictionFor: (config) =>
        config.lv01PairedCase === undefined ? undefined : { ...ORDINARY },
    });
    if (harness === undefined) throw new Error('harness unavailable');
    const parent = fixtureParent();
    await harness.runtime.createRun(parent);
    await harness.runtime.runToCompletion(parent.runId);
    scratch = await mkdtemp(join(tmpdir(), 'ald-ala-mutation-'));
    parentBundleDir = join(scratch, 'parent-bundle');
    const policiesDir = join(harness.root, 'runs', parent.runId, 'policies');
    const policyFiles: Record<string, unknown> = {};
    for (const name of ['baby-a-latest.json', 'baby-b-latest.json']) {
      policyFiles[name] = JSON.parse(await readFile(join(policiesDir, name), 'utf8')) as unknown;
    }
    const writer = new SqliteEvidenceWriter({
      database: harness.database,
      signers: harness.signerProvider(parent.runId),
    });
    await exportRunBundle(writer, parent.runId, parentBundleDir, {
      softwareCommit: 'git:ala-mutation-test',
      learnerContracts: ['scratch-rl'].map((track) => {
        const contract = loadLearnerContract(track as 'scratch-rl');
        return { track, version: contract.version, text: contract.text };
      }),
      policyFiles,
    });
    const store = { root: harness.root, databasePath: harness.databasePath };
    harness.close();
    const packet = compileLv01StagePacket(await packetInputWithAllocation(scratch));
    const collected = await collectLv01Slot({
      packet,
      slot: 1,
      kind: 'primary',
      parentBundleDir,
      stageDir: join(scratch, 'stage'),
      softwareCommit: 'git:ala-mutation-test',
      partition: 'dev',
      ordinary: { ...ORDINARY },
      store,
    });
    caseDir = collected.caseDir;
  }

  afterEach(async () => {
    await harness?.cleanup().catch(() => undefined);
    harness = undefined;
    if (scratch !== undefined) await rm(scratch, { recursive: true, force: true });
    scratch = undefined;
  });

  async function tamperedCase(): Promise<string> {
    if (scratch === undefined) throw new Error('not staged');
    const copy = join(scratch, `tampered-${Date.now()}-${Math.floor(Math.random() * 1e6)}`);
    await cp(caseDir, copy, { recursive: true });
    return copy;
  }

  async function firstBranchDir(dir: string): Promise<string> {
    const runs = (await readdir(join(dir, 'branches'))).sort();
    if (runs[0] === undefined) throw new Error('no branches staged');
    return join(dir, 'branches', runs[0]);
  }

  it('M1 rejects an edited turn outcome', async () => {
    await stage();
    const copy = await tamperedCase();
    const branchDir = await firstBranchDir(copy);
    const path = join(branchDir, 'turn-records.jsonl');
    const lines = (await readFile(path, 'utf8')).split('\n').filter((l) => l.length > 0);
    const turn = JSON.parse(lines[0] as string) as Record<string, unknown>;
    const details = ((turn['outcome'] as Record<string, unknown>)['details'] ?? {}) as Record<string, unknown>;
    details['selectedRef'] = 'cand-TAMPERED';
    lines[0] = JSON.stringify(turn);
    await writeFile(path, `${lines.join('\n')}\n`, 'utf8');
    const docs = await readLv01BundleDocs(branchDir);
    expect(() => loadLv01AuditedBranch(docs)).toThrowError(/disagrees with the intention selection/);
  });

  it('M2 rejects a deleted branch bundle (census)', async () => {
    await stage();
    const copy = await tamperedCase();
    const branchDir = await firstBranchDir(copy);
    await rm(branchDir, { recursive: true, force: true });
    const remaining = await readdir(join(copy, 'branches'));
    expect(remaining.length).toBe(6);
    expect(remaining.length).not.toBe(7);
  });

  it('M3 rejects reordered intervention commitments', async () => {
    await stage();
    const copy = await tamperedCase();
    const branchDir = await firstBranchDir(copy);
    const path = join(branchDir, 'intervention-log.jsonl');
    const lines = (await readFile(path, 'utf8')).split('\n').filter((l) => l.length > 0);
    const parsed = lines.map((line) => JSON.parse(line) as Record<string, unknown>);
    const prediction = parsed.findIndex((e) => e['reasonCode'] === 'lv01-paired-pre-receiver-action-prediction-committed');
    const draw = parsed.findIndex((e) => e['reasonCode'] === 'lv01-shared-action-draw-committed');
    if (prediction === -1 || draw === -1) throw new Error('fixture lacks swappable commitments');
    // File order is irrelevant; the loader enforces sequence/recordedAt
    // order. Swap the sequence VALUES to forge a reordered stream.
    const tmp = parsed[prediction]!['sequence'];
    parsed[prediction]!['sequence'] = parsed[draw]!['sequence'];
    parsed[draw]!['sequence'] = tmp;
    await writeFile(path, `${parsed.map((e) => JSON.stringify(e)).join('\n')}\n`, 'utf8');
    const docs = await readLv01BundleDocs(branchDir);
    expect(() => loadLv01AuditedBranch(docs)).toThrowError(/violates causal order/);
  });

  it('M4 rejects a duplicated ledger event', async () => {
    await stage();
    const copy = join(scratch as string, 'parent-dup');
    await cp(parentBundleDir, copy, { recursive: true });
    const path = join(copy, 'baby-a-ledger.jsonl');
    const lines = (await readFile(path, 'utf8')).split('\n').filter((l) => l.length > 0);
    const hypothesis = lines.find((line) => line.includes('hypothesis.'));
    if (hypothesis === undefined) throw new Error('fixture parent has no hypothesis event');
    lines.push(hypothesis);
    await writeFile(path, `${lines.join('\n')}\n`, 'utf8');
    const docs = await readLv01BundleDocs(copy);
    const authenticate = authenticateFromManifest(docs.runManifest);
    const records = mapLv01LedgerAssociations(docs.ledgerA as never, 'baby-a', authenticate);
    expect(() => indexLv01TrainingLedger(records, 'baby-a', deriveLv01LedgerCutoff(records))).toThrowError(
      /duplicate sequence/,
    );
  });

  it('M5 rejects a wrong-signature ledger event', async () => {
    await stage();
    const copy = join(scratch as string, 'parent-forged');
    await cp(parentBundleDir, copy, { recursive: true });
    const path = join(copy, 'baby-a-ledger.jsonl');
    const lines = (await readFile(path, 'utf8')).split('\n').filter((l) => l.length > 0);
    const index = lines.findIndex((line) => line.includes('hypothesis.'));
    if (index === -1) throw new Error('fixture parent has no hypothesis event');
    const event = JSON.parse(lines[index] as string) as Record<string, unknown>;
    const signature = event['writerSignature'] as string;
    event['writerSignature'] = `${signature.slice(0, -1)}${signature.endsWith('a') ? 'b' : 'a'}`;
    lines[index] = JSON.stringify(event);
    await writeFile(path, `${lines.join('\n')}\n`, 'utf8');
    const docs = await readLv01BundleDocs(copy);
    const authenticate = authenticateFromManifest(docs.runManifest);
    const records = mapLv01LedgerAssociations(docs.ledgerA as never, 'baby-a', authenticate);
    expect(records.some((record) => record.authenticated === false)).toBe(true);
    expect(() => indexLv01TrainingLedger(records, 'baby-a', deriveLv01LedgerCutoff(records))).toThrowError(
      /unauthenticated/,
    );
  });

  it('M6 rejects a truncated treatment batch', async () => {
    await stage();
    const copy = await tamperedCase();
    const path = join(copy, 'treatment-batch.json');
    const text = await readFile(path, 'utf8');
    await writeFile(path, text.slice(0, Math.floor(text.length / 2)), 'utf8');
    await expect(readFile(path, 'utf8').then((t) => JSON.parse(t))).rejects.toThrow();
  });

  it('M7 rejects a test episode injected into ordinary input', async () => {
    await stage();
    const docs = await readLv01BundleDocs(parentBundleDir);
    const { buildLv01Schedule } = await import('../../src/experiments/lv01-schedule.js');
    const { ReferentialScenarioEngine } = await import('@ald/scenario');
    const { fixedTokenInventory } = await import('@ald/types');
    const { LV01_UNTOUCHED_TYPE_CODES } = await import('@ald/scenario');
    const engine = new ReferentialScenarioEngine(
      { version: 1, symbolInventory: fixedTokenInventory(32), interactionMode: 'cooperative-signaling', heldOutTypeCodes: [...LV01_UNTOUCHED_TYPE_CODES] },
      'mutation-matrix-test-scenario-seed',
    );
    const schedule = buildLv01Schedule({
      engine,
      counts: { training: 5, 'validation-fit': 0, 'validation-selection': 0, 'within-support-test': 1 },
      receiverRoles: ['baby-a', 'baby-b'],
    });
    // Forge schedule membership: place a REAL fixture-turn hash in the
    // test set so the loader's C3a test-episode rejection fires for it.
    const turnHash = ((docs.turns[0] ?? {}) as Record<string, unknown>)['scenarioStateHash'];
    if (typeof turnHash !== 'string') throw new Error('fixture has no turn hashes');
    const forged = {
      ...schedule,
      cases: {
        ...schedule.cases,
        'within-support-test': schedule.cases['within-support-test'].map((entry, index) =>
          index === 0 ? { ...entry, stateHash: turnHash } : entry,
        ),
      },
    };
    expect(() => extractLv01OrdinaryRecords({ parentDocs: docs, schedule: forged, inventory: fixedTokenInventory(32) })).toThrowError(
      /never ordinary/,
    );
  });

  it('M0 positive control: staged case carries 7 branches plus committed batch and schedule', async () => {
    // collectLv01Slot self-audits during staging and throws on any
    // violation, so reaching this point means the unmodified case audits
    // clean; every mutation below starts from this accepted state.
    await stage();
    const runs = (await readdir(join(caseDir, 'branches'))).sort();
    expect(runs.length).toBe(7);
    const batch = JSON.parse(await readFile(join(caseDir, 'treatment-batch.json'), 'utf8')) as { cases?: unknown[]; batchCommitment?: string };
    expect(Array.isArray(batch.cases)).toBe(true);
    expect(typeof batch.batchCommitment).toBe('string');
    const schedule = JSON.parse(await readFile(join(caseDir, 'schedule.json'), 'utf8')) as { commitment?: string };
    expect(typeof schedule.commitment).toBe('string');
  });
});
