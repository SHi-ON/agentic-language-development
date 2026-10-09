/**
 * ALA study entry point (P3; study slots stay gated until P5 admission).
 *
 * Usage:
 *   tsx scripts/ala-run-entry.ts --manifest <path> --cohort A|B|development|calibration
 *     --kind primary|reserve --index <n> [--evidence-root <dir>] [--resume]
 *
 * Collects the retained-identity corpus from tracked records and runs one
 * ALA dyad through the manifest-verified runner.
 */
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

import { runAlaDyad } from '../packages/orchestrator/src/ala/ala-runner.js';
import type { AlaCohort, AlaSlotKind } from '../packages/orchestrator/src/ala/ala-seeds.js';

const ROOT = join(import.meta.dirname, '..');

function arg(name: string): string {
  const index = process.argv.indexOf(name);
  const value = index === -1 ? undefined : process.argv[index + 1];
  if (value === undefined || value.startsWith('--')) {
    console.error(`FAIL: missing required ${name}`);
    process.exit(11);
  }
  return value;
}

/** Retained LV01/E00-E50 identity corpus: every 64-hex seed in tracked records. */
function collectRetainedSeedCorpus(): string[] {
  const corpus = new Set<string>();
  const scan = (dir: string): void => {
    for (const entry of readdirSync(join(ROOT, dir), { withFileTypes: true })) {
      const rel = join(dir, entry.name);
      if (entry.isDirectory()) {
        scan(rel);
      } else if (entry.isFile() && rel.endsWith('.json')) {
        const text = readFileSync(join(ROOT, rel), 'utf8');
        for (const match of text.matchAll(/\b[0-9a-f]{64}\b/gu)) corpus.add(match[0]);
      }
    }
  };
  for (const dir of ['protocols', 'docs', 'reports']) scan(dir);
  // Exclude the runner's own freshly derived ALA seeds (none persisted yet by
  // construction); the corpus is historical identities only.
  return [...corpus];
}

const cohort = arg('--cohort') as AlaCohort;
const kind = (process.argv.includes('--kind') ? arg('--kind') : 'primary') as AlaSlotKind;
const index = Number.parseInt(arg('--index'), 10);
if (!['A', 'B', 'development', 'calibration'].includes(cohort)) {
  console.error(`FAIL: bad --cohort ${cohort}`);
  process.exit(11);
}
if (!['primary', 'reserve'].includes(kind)) {
  console.error(`FAIL: bad --kind ${kind}`);
  process.exit(11);
}
if (!Number.isInteger(index) || index < 0) {
  console.error('FAIL: --index must be a non-negative integer');
  process.exit(11);
}
const evidenceRoot = process.argv.includes('--evidence-root')
  ? arg('--evidence-root')
  : join(ROOT, 'evidence/ala-lv');

const receipt = await runAlaDyad({
  root: ROOT,
  manifestPath: arg('--manifest'),
  cohort,
  kind,
  index,
  evidenceRoot,
  retainedCorpus: collectRetainedSeedCorpus(),
  resume: process.argv.includes('--resume'),
});
console.log(JSON.stringify({ state: receipt.state, reason: receipt.reason, casesValid: receipt.casesValid }));
