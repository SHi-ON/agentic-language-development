/**
 * Per-dyad live-output census (B2, reused in C4).
 *
 * Usage:
 *   tsx scripts/ala-dyad-census.ts --dyad <dir> --out <report.json>
 *
 * Requires a complete dyad (A7 check first, refuses otherwise), then
 * reports: executed coverage per block, target histograms per block,
 * the planned-vs-actual shuffled-delivery join, the random-branch
 * variation census (deliveries + per-case draw seeds), and the parent
 * training accuracy trajectory. Findings are reported, never gated:
 * exit 0 with the report, exit 1 only on structural problems.
 */
import { readFile, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { checkV2DyadComplete, v2BlockFileName, V2_BLOCK_PARTITIONS, V2_BLOCK_ROLES } from '../packages/orchestrator/src/ala/ala-v2-retained.js';
import {
  blockAccuracy,
  coverageFromExecuted,
  deliveryJoin,
  distinctShare,
  histogram,
  splitTrajectory,
} from '../packages/orchestrator/src/ala/ala-dyad-census.js';
import { readLv01BundleDocs } from '../packages/orchestrator/src/experiments/lv01-collector.js';
import { loadLv01AuditedBranch } from '../packages/orchestrator/src/experiments/lv01-audit-loader.js';

function arg(name: string): string {
  const index = process.argv.indexOf(`--${name}`);
  const value = index === -1 ? undefined : process.argv[index + 1];
  if (value === undefined || value.length === 0) {
    console.error(`FAIL: --${name} <value> is required`);
    process.exit(11);
  }
  return value;
}

interface SlotCase {
  readonly slot: number;
  readonly partition: string;
  readonly executedCaseId: string;
}

async function main(): Promise<number> {
  const dyadDir = arg('dyad');
  const outPath = arg('out');
  const complete = await checkV2DyadComplete(dyadDir);
  if (!complete.ok) {
    console.error(`FAIL: dyad ${complete.dyad} is incomplete (${complete.problems.length} defects)`);
    return 1;
  }
  const slotDirs = (await readdir(dyadDir))
    .filter((entry) => /^slot-\d{4}$/.test(entry))
    .sort();
  const slots: SlotCase[] = [];
  for (const dir of slotDirs) {
    const record = JSON.parse(await readFile(join(dyadDir, dir, 'schedule.json'), 'utf8')) as {
      partition: string;
      executedCaseId: string;
    };
    slots.push({ slot: Number(dir.slice(5)), partition: record.partition, executedCaseId: record.executedCaseId });
  }
  const planned = new Map<string, string>();
  const targets: { partition: string; role: string; target: number; scheduled: string }[] = [];
  for (const partition of V2_BLOCK_PARTITIONS) {
    for (const role of V2_BLOCK_ROLES) {
      const block = JSON.parse(
        await readFile(join(dyadDir, v2BlockFileName(partition, role)), 'utf8'),
      ) as {
        cases: { caseId: string; scheduledCaseId: string; targetTypeCode: number; shuffledDeliveredToken: string }[];
      };
      for (const entry of block.cases) {
        if (entry.caseId.endsWith('-ledger-shuffled:turn:0')) {
          planned.set(entry.scheduledCaseId, entry.shuffledDeliveredToken);
        }
      }
      const seen = new Set<string>();
      for (const entry of block.cases) {
        if (seen.has(entry.scheduledCaseId)) continue;
        seen.add(entry.scheduledCaseId);
        targets.push({ partition, role, target: entry.targetTypeCode, scheduled: entry.scheduledCaseId });
      }
    }
  }
  const actual = new Map<string, string>();
  const randomDeliveries: string[] = [];
  const randomSeeds: string[] = [];
  let branches = 0;
  for (const slot of slots) {
    const branchDirs = (await readdir(join(dyadDir, `slot-${String(slot.slot).padStart(4, '0')}`, 'branches'))).sort();
    for (const runId of branchDirs) {
      const docs = await readLv01BundleDocs(join(dyadDir, `slot-${String(slot.slot).padStart(4, '0')}`, 'branches', runId));
      const audited = loadLv01AuditedBranch(docs);
      branches += 1;
      if (audited.branch === 'shuffled' || audited.branch === 'random') {
        const action = audited.transcript.find((entry) => entry.eventType === 'receiver-action-recorded');
        const body = action?.body as { deliveredToken?: unknown; selectedCandidateRef?: unknown } | undefined;
        if (typeof body?.deliveredToken !== 'string') {
          console.error(`FAIL: slot ${slot.slot} ${audited.branch} branch has no recorded delivery`);
          return 1;
        }
        if (audited.branch === 'shuffled') {
          actual.set(slot.executedCaseId, body.deliveredToken);
        } else {
          randomDeliveries.push(body.deliveredToken);
          randomSeeds.push(audited.drawSeed);
        }
      }
    }
  }
  const joinResult = deliveryJoin(planned, actual);
  const parentTurns = (await readFile(join(dyadDir, 'parent-bundle', 'turn-records.jsonl'), 'utf8'))
    .split('\n')
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as { outcome?: { success?: unknown } });
  const outcomes = parentTurns.map((turn, index) => {
    if (typeof turn.outcome?.success !== 'boolean') {
      console.error(`FAIL: parent turn record ${index} carries no boolean outcome`);
      process.exit(1);
    }
    return turn.outcome.success;
  });
  const trajectory = blockAccuracy(outcomes, 250);
  const split = splitTrajectory(trajectory, outcomes.length, 250);
  const finalFull = split.full[split.full.length - 1] as number;
  const report = {
    schema: 'ala-dyad-census/v1',
    dyad: complete.dyad,
    slots: slots.length,
    branches,
    coverage: coverageFromExecuted(
      slots.map((slot) => {
        const role = slot.executedCaseId.split(':')[1] as string;
        return { partition: slot.partition, role, caseId: slot.executedCaseId };
      }),
    ),
    targets: Object.fromEntries(
      V2_BLOCK_PARTITIONS.flatMap((partition) =>
        V2_BLOCK_ROLES.map((role) => [
          `${partition}:${role}`,
          histogram(targets.filter((entry) => entry.partition === partition && entry.role === role).map((entry) => entry.target)),
        ]),
      ),
    ),
    deliveryJoin: {
      planned: planned.size,
      actual: actual.size,
      matched: joinResult.matched,
      mismatched: joinResult.mismatched,
    },
    random: {
      deliveries: distinctShare(randomDeliveries),
      drawSeeds: distinctShare(randomSeeds),
    },
    learning: {
      turns: outcomes.length,
      blockSize: 250,
      accuracyByBlock: trajectory,
      finalFullBlockAccuracy: finalFull,
      tailAccuracy: split.tail,
      tailTurns: split.tailTurns,
      chance: 0.25,
    },
  };
  await writeFile(outPath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  console.log(
    `CENSUS OK: ${report.dyad}: ${report.slots} slots, delivery join ${joinResult.matched}/${planned.size} ` +
      `(${joinResult.mismatched.length} mismatches), random ${report.random.deliveries.distinct}/${report.random.deliveries.total} distinct, ` +
      `final-full-block accuracy ${report.learning.finalFullBlockAccuracy.toFixed(3)} vs 0.25 -> ${outPath}`,
  );
  return 0;
}

main()
  .then((code) => process.exit(code))
  .catch((error: unknown) => {
    console.error(`FAIL: ${(error as Error).message}`);
    process.exit(1);
  });
