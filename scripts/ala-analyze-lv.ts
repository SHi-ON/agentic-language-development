/**
 * ALA-LV analysis driver (B4/Phase D): retained dyads in, cohort
 * estimates out, per the frozen v2 analysis contract.
 *
 * Usage:
 *   tsx scripts/ala-analyze-lv.ts --dyad <dir> [--dyad <dir>...]
 *     --cohort <name> --out <report.json> [--coverage 0.975]
 *
 * Per dyad: ordinary training rows from the parent bundle, fold rows
 * from audited normal branches (validation-fit, validation-selection,
 * within-support-test), five-candidate selection, refit, test scoring,
 * and the dyad primary. Across dyads: the Student-t cohort interval.
 * Fold counts come from the frozen v2 study protocol, never literals.
 * Refuses incomplete dyads and empty or miscounted folds.
 */
import { readFile, readdir, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  alaLvCohortInterval,
  analyzeAlaLvDyad,
  type AlaLvRoleFolds,
  type AlaLvTestRow,
} from '../packages/analysis/src/ala-lv-dyad.js';
import type { Lv01OrdinaryRecord } from '../packages/analysis/src/ledger-value.js';
import { checkV2DyadComplete } from '../packages/orchestrator/src/ala/ala-v2-retained.js';
import { alaProtocolPaths } from '../packages/orchestrator/src/ala/ala-manifest.js';
import { alaPrototypeWorkload, alaStudyCounts } from '../packages/orchestrator/src/ala/ala-workload.js';
import { analysisRowFromDocs, loadAnalysisTrainingRows } from '../packages/orchestrator/src/experiments/lv01-analysis-loader.js';
import { readLv01BundleDocs } from '../packages/orchestrator/src/experiments/lv01-collector.js';
import { fixedTokenInventory } from '../packages/types/src/domains.js';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

function arg(name: string): string {
  const index = process.argv.indexOf(`--${name}`);
  const value = index === -1 ? undefined : process.argv[index + 1];
  if (value === undefined || value.length === 0) {
    console.error(`FAIL: --${name} <value> is required`);
    process.exit(11);
  }
  return value;
}

function args(name: string): string[] {
  const out: string[] = [];
  for (let index = 0; index < process.argv.length; index += 1) {
    if (process.argv[index] === `--${name}`) {
      const value = process.argv[index + 1];
      if (value === undefined || value.length === 0) {
        console.error(`FAIL: --${name} <value> is required`);
        process.exit(11);
      }
      out.push(value);
    }
  }
  return out;
}

async function main(): Promise<number> {
  const dyadDirs = args('dyad');
  if (dyadDirs.length === 0) {
    console.error('FAIL: at least one --dyad <dir> is required');
    return 11;
  }
  const cohort = arg('cohort');
  const outPath = arg('out');
  const coverage = process.argv.includes('--coverage') ? Number(arg('coverage')) : 0.975;
  if (!Number.isFinite(coverage) || coverage <= 0 || coverage >= 1) {
    console.error('FAIL: --coverage must be strictly between 0 and 1');
    return 11;
  }
  const paths = alaProtocolPaths(2);
  const workload = alaPrototypeWorkload(alaStudyCounts(join(ROOT, paths.study)));
  if (workload.trainingTurns % 2 !== 0) {
    console.error('FAIL: training turns do not split evenly across roles');
    return 1;
  }
  const want = {
    training: workload.trainingTurns / 2,
    fit: workload.validationFitCases,
    selection: workload.validationSelectionCases,
    test: workload.withinSupportTestCases,
  };
  const inventory = fixedTokenInventory(32);
  const dyads = [];
  for (const dyadDir of dyadDirs) {
    const complete = await checkV2DyadComplete(dyadDir);
    if (!complete.ok) {
      console.error(`FAIL: dyad ${complete.dyad} is incomplete (${complete.problems.length} defects)`);
      return 1;
    }
    const training = loadAnalysisTrainingRows(await readLv01BundleDocs(join(dyadDir, 'parent-bundle')), want.training);
    const slotDirs = (await readdir(dyadDir)).filter((entry) => /^slot-\d{4}$/.test(entry)).sort();
    const fit: AlaLvTestRow[] = [];
    const selection: AlaLvTestRow[] = [];
    const test: AlaLvTestRow[] = [];
    for (const dir of slotDirs) {
      const record = JSON.parse(await readFile(join(dyadDir, dir, 'schedule.json'), 'utf8')) as {
        partition: string;
        executedCaseId: string;
      };
      const branches = await readdir(join(dyadDir, dir, 'branches'));
      const normal = branches.filter((runId) => runId.endsWith('-normal')).sort();
      if (normal.length !== 1) {
        console.error(`FAIL: ${dir} has ${normal.length} normal branch bundles`);
        return 1;
      }
      const row = analysisRowFromDocs(
        await readLv01BundleDocs(join(dyadDir, dir, 'branches', normal[0] as string)),
        record.executedCaseId,
      );
      if (record.partition === 'validation-fit') fit.push(row);
      else if (record.partition === 'validation-selection') selection.push(row);
      else if (record.partition === 'within-support-test') test.push(row);
      else {
        console.error(`FAIL: ${dir} names partition ${record.partition}`);
        return 1;
      }
    }
    const byRole = (role: 'baby-a' | 'baby-b'): AlaLvRoleFolds => ({
      role,
      training: training.filter((row) => row.receiverRole === role),
      validationFit: fit.filter((row) => row.record.receiverRole === role).map((row) => row.record),
      validationSelection: selection.filter((row) => row.record.receiverRole === role).map((row) => row.record),
      test: test.filter((row) => row.record.receiverRole === role),
    });
    const folds = [byRole('baby-a'), byRole('baby-b')] as [AlaLvRoleFolds, AlaLvRoleFolds];
    for (const entry of folds) {
      const counts = { training: entry.training.length, fit: entry.validationFit.length, selection: entry.validationSelection.length, test: entry.test.length };
      if (counts.training !== want.training || counts.fit !== want.fit || counts.selection !== want.selection || counts.test !== want.test) {
        console.error(`FAIL: dyad ${complete.dyad} role ${entry.role} folds ${JSON.stringify(counts)}, want ${JSON.stringify(want)}`);
        return 1;
      }
    }
    dyads.push(analyzeAlaLvDyad(complete.dyad, folds, inventory));
  }
  const interval = alaLvCohortInterval(cohort, dyads.map((dyad) => dyad.primary), coverage);
  const startedAt = new Date().toISOString();
  const report = {
    schema: 'ala-lv-analysis/v1',
    cohort,
    coverage,
    dyads,
    interval,
    inputs: dyadDirs,
    producedAt: startedAt,
  };
  await writeFile(outPath, JSON.stringify(report, null, 2).concat('\n'), 'utf8');
  const flag = interval.degenerate === null
    ? `interval [${interval.lower?.toFixed(4)}, ${interval.upper?.toFixed(4)}]`
    : `DEGENERATE (${interval.degenerate})`;
  console.log(`ANALYZE OK: cohort ${cohort}: n=${interval.n} mean=${interval.mean.toFixed(4)} ${flag} -> ${outPath}`);
  return 0;
}

main()
  .then((code) => process.exit(code))
  .catch((error: unknown) => {
    console.error(`FAIL: ${(error as Error).message}`);
    process.exit(1);
  });
