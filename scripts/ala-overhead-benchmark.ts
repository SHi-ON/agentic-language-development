/**
 * ALA-LV overhead benchmark (P4 rehearsal on calibration; contractual run on
 * the first eligible cohort-A dyad in P6).
 *
 * Measures, on identical canonical content (first 1000 training turns in
 * manifest order):
 *  1. ordinary append-only path (1 warmup + 3 measured replays),
 *  2. authenticated recording path incl. export + reload + full
 *     verification (1 warmup + 3 measured replays),
 *  3. whole-bundle verification of the dyad parent, at most 3 timed runs,
 *  4. 500 fixed native-query vs exact-policy-forward calls.
 *
 * Fails closed if the benchmark itself exceeds its 0.1 CPU-hour cap.
 * All outputs are temporary; sealed originals are never modified.
 *
 * Usage: tsx scripts/ala-overhead-benchmark.ts --dyad <dir> --out <json>
 */
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { exportRunBundle, openEvidenceDatabase, SqliteEvidenceWriter } from '../packages/evidence/src/index.js';
import { InMemorySignerRegistry } from '../packages/hashing/src/index.js';
import {
  indexLv01TrainingLedger,
  loadLearnerContract,
  parseExportedRecurrentScratchPolicy,
  predictLv01NativeLedger,
  replayLv01RecurrentReceiver,
} from '../packages/learners/src/index.js';
import { fixedTokenInventory } from '../packages/types/src/index.js';

import {
  authenticateFromManifest,
  typedLedger,
  typedTurns,
} from '../packages/orchestrator/src/experiments/lv01-audit-loader.js';
import { readLv01BundleDocs } from '../packages/orchestrator/src/experiments/lv01-collector.js';
import {
  deriveLv01LedgerCutoff,
  mapLv01LedgerAssociations,
} from '../packages/orchestrator/src/experiments/lv01-paired-predictions.js';

const BENCHMARK_CPU_SECONDS_MAX = 360;

function arg(name: string): string {
  const index = process.argv.indexOf(name);
  const value = index === -1 ? undefined : process.argv[index + 1];
  if (value === undefined || value.startsWith('--')) {
    console.error(`FAIL: missing required ${name}`);
    process.exit(11);
  }
  return value;
}

function cpuSecondsSince(start: NodeJS.CpuUsage): number {
  const diff = process.cpuUsage(start);
  return (diff.user + diff.system) / 1e6;
}

function median(values: readonly number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? sorted[mid]! : (sorted[mid - 1]! + sorted[mid]!) / 2;
}

const dyadDir = arg('--dyad');
const outPath = arg('--out');
const benchStart = process.cpuUsage();
const parentBundleDir = join(dyadDir, 'parent-bundle');
const docs = await readLv01BundleDocs(parentBundleDir);
const turns = typedTurns(docs);
if (turns.length < 1000) throw new Error(`benchmark needs 1000 training turns, found ${turns.length}`);
const canonical = turns.slice(0, 1000);
const inventory = fixedTokenInventory(32);
const parentConfig = JSON.parse(readFileSync(join(parentBundleDir, 'configuration/run-config.json'), 'utf8')) as Record<
  string,
  unknown
>;

type Turn = Record<string, unknown>;

async function appendCanonical(
  writer: SqliteEvidenceWriter,
  runId: string,
): Promise<void> {
  writer.registerRun({ ...(parentConfig as object), runId } as never);
  for (const turn of canonical as Turn[]) {
    await writer.appendTurnRecord({
      runId,
      turn: turn['turn'],
      phase: turn['phase'],
      roles: turn['roles'],
      communicationCondition: turn['communicationCondition'],
      scenarioRef: turn['scenarioRef'],
      scenarioStateHash: turn['scenarioStateHash'],
      observationHashes: turn['observationHashes'],
      babyProposalHash: turn['babyProposalHash'],
      deliveredArtifactHash: turn['deliveredArtifactHash'],
      channelEventHash: turn['channelEventHash'],
      actionHash: turn['actionHash'],
      outcomeHash: turn['outcomeHash'],
      outcome: turn['outcome'],
    } as never);
  }
}

// 1-2. Warmup + 3 measured replays through both paths (identical content).
const ordinaryMs: number[] = [];
const authenticatedMs: number[] = [];
for (let repeat = 0; repeat < 4; repeat += 1) {
  const warmup = repeat === 0;
  {
    const dir = mkdtempSync(join(tmpdir(), 'ald-ala-bench-ord-'));
    try {
      const database = openEvidenceDatabase(join(dir, 'bench.sqlite'));
      try {
        const writer = new SqliteEvidenceWriter({
          database,
          signers: InMemorySignerRegistry.generate(`ala-bench-ord-r${repeat}`),
        });
        const start = Date.now();
        await appendCanonical(writer, `ala-bench-ord-r${repeat}`);
        if (!warmup) ordinaryMs.push(Date.now() - start);
      } finally {
        database.close();
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }
  {
    const dir = mkdtempSync(join(tmpdir(), 'ald-ala-bench-auth-'));
    try {
      const database = openEvidenceDatabase(join(dir, 'bench.sqlite'));
      try {
        const runId = `ala-bench-auth-r${repeat}`;
        const writer = new SqliteEvidenceWriter({ database, signers: InMemorySignerRegistry.generate(runId) });
        const start = Date.now();
        await appendCanonical(writer, runId);
        const bundleDir = join(dir, 'bundle');
        const tracks = [
          ...new Set(
            [(parentConfig as never)['babyA']?.track, (parentConfig as never)['babyB']?.track].filter(
              (track): track is string => typeof track === 'string',
            ),
          ),
        ];
        await exportRunBundle(writer, runId, bundleDir, {
          softwareCommit: 'ala-overhead-benchmark',
          learnerContracts: tracks.map((track) => {
            const contract = loadLearnerContract(track as never);
            return { track: contract.track, version: contract.version, text: contract.text };
          }),
        });
        const reloaded = await readLv01BundleDocs(bundleDir);
        const reloadedTurns = typedTurns(reloaded);
        if (reloadedTurns.length !== canonical.length) throw new Error('benchmark reload lost turns');
        for (let i = 1; i < reloadedTurns.length; i += 1) {
          const prev = reloadedTurns[i - 1] as unknown as Turn;
          const cur = reloadedTurns[i] as unknown as Turn;
          if (cur['previousEntryHash'] !== prev['entryHash']) throw new Error('benchmark turn chain broken');
        }
        const authenticate = authenticateFromManifest(reloaded.runManifest);
        for (const event of typedLedger(reloaded.ledgerA, 'baby-a-ledger.jsonl')) {
          if (!authenticate(event as never)) throw new Error('benchmark ledger auth failed');
        }
        for (const event of typedLedger(reloaded.ledgerB, 'baby-b-ledger.jsonl')) {
          if (!authenticate(event as never)) throw new Error('benchmark ledger auth failed');
        }
        if (!warmup) authenticatedMs.push(Date.now() - start);
      } finally {
        database.close();
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }
}

// 3. Whole-bundle verification of the dyad parent (at most 3 timed runs).
const verifyMs: number[] = [];
for (let repeat = 0; repeat < 3; repeat += 1) {
  const start = Date.now();
  const reloaded = await readLv01BundleDocs(parentBundleDir);
  typedTurns(reloaded);
  const authenticate = authenticateFromManifest(reloaded.runManifest);
  for (const event of typedLedger(reloaded.ledgerA, 'baby-a-ledger.jsonl')) authenticate(event as never);
  for (const event of typedLedger(reloaded.ledgerB, 'baby-b-ledger.jsonl')) authenticate(event as never);
  verifyMs.push(Date.now() - start);
}

// 4. 500 fixed native-query vs exact-policy-forward calls.
const authenticate = authenticateFromManifest(docs.runManifest);
const recordsA = mapLv01LedgerAssociations(docs.ledgerA as never, 'baby-a', authenticate);
const indexA = indexLv01TrainingLedger(recordsA, 'baby-a', deriveLv01LedgerCutoff(recordsA));
const policies = docs.policies as Record<string, unknown>;
const exported = policies['policies/baby-a-latest.json'];
if (exported === undefined) throw new Error('benchmark: no frozen baby-a policy export');
const frozenModel = parseExportedRecurrentScratchPolicy(exported).model;
const fixedToken = 'S01';
const fixedTypes = [1, 2, 3, 4];
const nativeMs: number[] = [];
const replayMs: number[] = [];
for (let call = 0; call < 500; call += 1) {
  let start = Date.now();
  predictLv01NativeLedger(indexA, fixedToken, fixedTypes);
  nativeMs.push(Date.now() - start);
  start = Date.now();
  replayLv01RecurrentReceiver(frozenModel, fixedToken, inventory, fixedTypes);
  replayMs.push(Date.now() - start);
}

const cpuSeconds = cpuSecondsSince(benchStart);
if (cpuSeconds > BENCHMARK_CPU_SECONDS_MAX) {
  console.error(`FAIL: benchmark consumed ${cpuSeconds.toFixed(1)}s CPU over the 0.1 CPU-hour cap`);
  process.exit(13);
}
const result = {
  dyadDir,
  eventCount: canonical.length,
  host: { platform: process.platform, arch: process.arch, node: process.version },
  ordinaryAppendMs: { repeats: ordinaryMs, median: median(ordinaryMs) },
  authenticatedMs: { repeats: authenticatedMs, median: median(authenticatedMs) },
  wholeBundleVerifyMs: { repeats: verifyMs, median: median(verifyMs) },
  nativeQueryMs: { calls: nativeMs.length, median: median(nativeMs) },
  replayForwardMs: { calls: replayMs.length, median: median(replayMs) },
  cpuSeconds,
  capCpuSeconds: BENCHMARK_CPU_SECONDS_MAX,
};
writeFileSync(outPath, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ medianOrdinaryMs: result.ordinaryAppendMs.median, medianAuthenticatedMs: result.authenticatedMs.median, cpuSeconds }));
