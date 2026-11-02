#!/usr/bin/env tsx
/**
 * P5 admission-record writer.
 *
 * Usage:
 *   tsx scripts/ala-admission-entry.ts --manifest <path> --evidence-root <dir>
 *     --cohort A|B --primaries <n> --reserves <n>
 *
 * Verifies the manifest first (drift fails closed), then writes
 * admission.json into the evidence root, bound to the exact manifest
 * bytes. One single-cohort record per evidence root; refusing to
 * overwrite an existing record. Run once per host x cohort root at
 * gate G2, after the final manifest rebind.
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { hashCanonical } from '../packages/hashing/src/index.js';
import {
  buildAlaAdmissionRecord,
  verifyAlaExecutionManifest,
} from '../packages/orchestrator/src/ala/ala-manifest.js';

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

const manifestPath = arg('--manifest');
const evidenceRoot = arg('--evidence-root');
const cohort = arg('--cohort');
if (cohort !== 'A' && cohort !== 'B') {
  console.error('FAIL: --cohort must be A|B');
  process.exit(11);
}
const primaries = Number.parseInt(arg('--primaries'), 10);
const reserves = Number.parseInt(arg('--reserves'), 10);

verifyAlaExecutionManifest(ROOT, manifestPath);
const manifestBytes = readFileSync(join(ROOT, manifestPath), 'utf8');
const record = buildAlaAdmissionRecord({
  manifestSha256: hashCanonical('ala-manifest-identity/v1', manifestBytes),
  cohort,
  primaries,
  reserves,
});
mkdirSync(evidenceRoot, { recursive: true });
const outPath = join(evidenceRoot, 'admission.json');
if (existsSync(outPath)) {
  console.error(`FAIL: admission record already exists at ${outPath}; records are write-once`);
  process.exit(11);
}
writeFileSync(outPath, `${JSON.stringify(record, null, 2)}\n`, 'utf8');
console.log(`wrote ${outPath} for cohort ${cohort} (${primaries} primaries, ${reserves} reserves)`);
