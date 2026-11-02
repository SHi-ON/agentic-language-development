/**
 * V2 dyad extraction entry point (A7).
 *
 * Usage:
 *   tsx scripts/ala-v2-extract.ts --dyad <dir> --journal <journal.jsonl> --out <dir>
 *
 * Checks the dyad against the v2 retained manifest, refuses incomplete
 * dyads, and writes diagnostic-rows.json (whole-dyad batches for the
 * driver), completeness.json, and journal-export.jsonl into --out.
 * Never modifies the dyad directory: extraction is read-only.
 */
import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import {
  checkV2DyadComplete,
  exportV2DyadJournalLines,
  extractV2DiagnosticBatches,
} from '../packages/orchestrator/src/ala/ala-v2-retained.js';

function arg(name: string): string {
  const index = process.argv.indexOf(`--${name}`);
  const value = index === -1 ? undefined : process.argv[index + 1];
  if (value === undefined || value.length === 0) {
    console.error(`FAIL: --${name} <value> is required`);
    process.exit(11);
  }
  return value;
}

async function main(): Promise<number> {
  const dyadDir = arg('dyad');
  const journalPath = arg('journal');
  const outDir = arg('out');
  const report = await checkV2DyadComplete(dyadDir);
  if (!report.ok) {
    console.error(`FAIL: dyad ${report.dyad} is incomplete (${report.problems.length} defects):`);
    for (const problem of report.problems) console.error(`  - ${problem}`);
    return 1;
  }
  const rows = await extractV2DiagnosticBatches(dyadDir);
  const journalLines = await exportV2DyadJournalLines(dyadDir, journalPath);
  await mkdir(outDir, { recursive: true });
  await writeFile(join(outDir, 'diagnostic-rows.json'), `${JSON.stringify(rows, null, 2)}\n`, 'utf8');
  await writeFile(join(outDir, 'completeness.json'), `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  await writeFile(join(outDir, 'journal-export.jsonl'), journalLines.map((line) => `${line}\n`).join(''), 'utf8');
  console.log(
    `EXTRACT OK: ${rows.dyad}: ${rows.blocks.length} batches, ` +
      `${rows.blocks.reduce((sum, block) => sum + block.records, 0)} records, ` +
      `${journalLines.length} journal lines -> ${outDir}`,
  );
  return 0;
}

main()
  .then((code) => process.exit(code))
  .catch((error: unknown) => {
    console.error(`FAIL: ${(error as Error).message}`);
    process.exit(1);
  });
