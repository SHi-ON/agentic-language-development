/**
 * C0/G2.5 smoke: one deterministic case through the LV01 collector
 * harness (the live-test recipe, no test runner), under a verified
 * manifest, with NO journal writes (amendment: the smoke is a
 * non-study determinism check, logged not journalled, excluded
 * from D).
 *
 * Usage:
 *   tsx scripts/ala-smoke-case.ts --manifest <v2manifest> --evidence-root <smoke-root> --out <fixture.json>
 *
 * Prints and records the case commitment plus a deterministic digest
 * over the collected case bytes. Run twice on one host for
 * self-parity, then once per host: x86 hosts must match TVPS
 * byte-for-byte; ARM divergences are reported with structural
 * detail for the C0 keep/drop decision.
 */
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { exportRunBundle, SqliteEvidenceWriter } from '../packages/evidence/src/index.js';
import { hashCanonical, InMemorySignerRegistry } from '../packages/hashing/src/index.js';
import { loadLearnerContract } from '../packages/learners/src/index.js';
import type { SignerRegistry } from '../packages/types/src/contracts.js';
import { SIGNER_DOMAINS } from '../packages/types/src/domains.js';

import { verifyAlaExecutionManifest } from '../packages/orchestrator/src/ala/ala-manifest.js';
import { compileLv01StagePacket } from '../packages/orchestrator/src/experiments/lv01-stage-packets.js';
import {
  collectLv01Slot,
  lv01ParentCheckpointHash,
  readLv01BundleDocs,
} from '../packages/orchestrator/src/experiments/lv01-collector.js';
// Live-test recipe (fixed seeds, deterministic step clock). These
// helpers are outside the execution closure; the smoke verifies the
// manifest itself but is not dyad execution.
import { buildHarness } from '../packages/orchestrator/__tests__/helpers.js';
import {
  ORDINARY,
  fixtureParent,
  packetInputWithAllocation,
} from '../packages/orchestrator/__tests__/experiments/lv01-collector-fixtures.js';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

function fail(message: string): never {
  console.error(`FAIL: ${message}`);
  process.exit(1);
}

function arg(name: string): string {
  const index = process.argv.indexOf(`--${name}`);
  const value = index === -1 ? undefined : process.argv[index + 1];
  if (value === undefined || value.length === 0) fail(`--${name} <value> is required`);
  return value as string;
}

async function treeFiles(dir: string, base = ''): Promise<string[]> {
  const out: string[] = [];
  for (const entry of (await readdir(join(dir, base), { withFileTypes: true })).sort((a, b) =>
    a.name < b.name ? -1 : 1,
  )) {
    const rel = base === '' ? entry.name : `${base}/${entry.name}`;
    if (entry.isDirectory()) out.push(...(await treeFiles(dir, rel)));
    else if (entry.isFile()) out.push(rel);
  }
  return out;
}

async function main(): Promise<void> {
  const manifestPath = arg('manifest');
  const evidenceRoot = arg('evidence-root');
  const outPath = arg('out');
  const manifest = verifyAlaExecutionManifest(ROOT, manifestPath);
  const manifestSha256 = hashCanonical('ala-manifest-identity/v1', await readFile(join(ROOT, manifestPath), 'utf8'));

  // Fixed-seed signers: production uses fresh random keys per run
  // (correct there), but the smoke needs byte-identical output, so
  // keys derive deterministically per run id. Fixture-only material.
  const registries = new Map<string, SignerRegistry>();
  const signerProvider = (runId: string): SignerRegistry => {
    const existing = registries.get(runId);
    if (existing !== undefined) return existing;
    const seeds = Object.fromEntries(
      SIGNER_DOMAINS.map((domain) => [
        domain,
        createHash('sha256').update(`ala-smoke-case/v1/${runId}/${domain}`).digest('hex'),
      ]),
    );
    const created = InMemorySignerRegistry.fromSeeds(runId, seeds);
    registries.set(runId, created);
    return created;
  };
  const harnessRoot = await mkdtemp(join(tmpdir(), 'ald-smoke-harness-'));
  const harness = buildHarness({
    root: harnessRoot,
    databasePath: join(harnessRoot, 'evidence.sqlite'),
    signerProvider,
    overrides: {
      lv01PredictionFor: (config) => (config.lv01PairedCase === undefined ? undefined : { ...ORDINARY }),
    },
  });
  const parent = fixtureParent();
  await harness.runtime.createRun(parent);
  await harness.runtime.runToCompletion(parent.runId);
  if (harness.runtime.turnRecords(parent.runId).length !== 5) fail('fixture parent did not train 5 turns');
  const policiesDir = join(harness.root, 'runs', parent.runId, 'policies');
  const policyFiles: Record<string, unknown> = {};
  for (const name of ['baby-a-latest.json', 'baby-b-latest.json']) {
    policyFiles[name] = JSON.parse(await readFile(join(policiesDir, name), 'utf8')) as unknown;
  }
  const scratch = await mkdtemp(join(tmpdir(), 'ald-smoke-'));
  const bundleDir = join(scratch, 'parent-bundle');
  const writer = new SqliteEvidenceWriter({
    database: harness.database,
    signers: harness.signerProvider(parent.runId),
  });
  await exportRunBundle(writer, parent.runId, bundleDir, {
    softwareCommit: manifest.protocolCommit,
    learnerContracts: ['scratch-rl'].map((track) => {
      const contract = loadLearnerContract(track as 'scratch-rl');
      return { track, version: contract.version, text: contract.text };
    }),
    policyFiles,
  });
  const docs = await readLv01BundleDocs(bundleDir);
  if (!/^sha256:[0-9a-f]{64}$/u.test(lv01ParentCheckpointHash(docs))) fail('fixture parent has no checkpoint hash');
  const store = { root: harness.root, databasePath: harness.databasePath };
  harness.close();

  // Fixed allocation path: the packet commitment hashes the allocation
  // identity including its path, so a scratch-dependent path would make
  // every run (and every host) disagree. Fixed content + fixed path =
  // fixed packet = fixed seeds. All fleet hosts are Linux (/tmp).
  const allocationDir = join(tmpdir(), 'ald-smoke-allocation');
  await mkdir(allocationDir, { recursive: true });
  const packet = compileLv01StagePacket(await packetInputWithAllocation(allocationDir));
  const stageDir = join(evidenceRoot, 'stage');
  const collected = await collectLv01Slot({
    packet,
    slot: 1,
    kind: 'primary',
    parentBundleDir: bundleDir,
    stageDir,
    softwareCommit: manifest.protocolCommit,
    partition: 'dev',
    ordinary: { ...ORDINARY },
    store,
    signerProvider,
  });
  const scheduleRecord = JSON.parse(await readFile(join(collected.caseDir, 'schedule.json'), 'utf8')) as {
    executedCaseId: string;
  };
  if (scheduleRecord.executedCaseId !== 'within-support-test:baby-b:0000') {
    fail(`smoke case identity drifted: ${scheduleRecord.executedCaseId}`);
  }
  const digest = createHash('sha256');
  for (const rel of await treeFiles(collected.caseDir)) {
    digest.update(rel);
    digest.update(await readFile(join(collected.caseDir, rel)));
  }
  const bundleDigest = `sha256:${digest.digest('hex')}`;
  const report = {
    schema: 'ala-smoke-fixture/v1',
    manifestSha256,
    softwareCommit: manifest.protocolCommit,
    caseCommitment: collected.caseCommitment,
    bundleDigest,
    executedCaseId: scheduleRecord.executedCaseId,
  };
  await writeFile(outPath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  console.log(`SMOKE OK: ${collected.caseCommitment} ${bundleDigest} -> ${outPath}`);
}

main().catch((error: unknown) => fail((error as Error).message));
