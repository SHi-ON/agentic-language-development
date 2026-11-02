#!/usr/bin/env tsx
/**
 * Build protocols/ala-lv-execution-manifest.v<N>.json from the live tree.
 * Run once after the protocol-freeze commit; the manifest binds that
 * commit and is itself committed separately. Never hand-edit the output.
 * Usage: tsx scripts/build-ala-manifest.ts [--protocol-version 1|2]
 */
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

import {
  alaProtocolPaths,
  buildAlaExecutionManifest,
} from '../packages/orchestrator/src/ala/ala-manifest.js';

const root = join(import.meta.dirname, '..');
const flag = process.argv.indexOf('--protocol-version');
const version = flag === -1 ? '1' : process.argv[flag + 1];
if (version !== '1' && version !== '2') {
  console.error('FAIL: --protocol-version must be 1|2');
  process.exit(11);
}
const paths = alaProtocolPaths(version === '2' ? 2 : 1);
const manifest = buildAlaExecutionManifest(root, version === '2' ? 2 : 1);
writeFileSync(join(root, paths.manifest), `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
console.log(`wrote ${paths.manifest} at protocol commit ${manifest.protocolCommit}`);
console.log(`execution closure: ${manifest.executionClosure.fileCount} files ${manifest.executionClosure.sha256}`);
