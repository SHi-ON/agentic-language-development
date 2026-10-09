#!/usr/bin/env tsx
/**
 * Build protocols/ala-lv-execution-manifest.v1.json from the live tree.
 * Run once after the protocol-freeze commit; the manifest binds that
 * commit and is itself committed separately. Never hand-edit the output.
 */
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

import {
  ALA_MANIFEST_PATH,
  buildAlaExecutionManifest,
} from '../packages/orchestrator/src/ala/ala-manifest.js';

const root = join(import.meta.dirname, '..');
const manifest = buildAlaExecutionManifest(root);
writeFileSync(join(root, ALA_MANIFEST_PATH), `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
console.log(`wrote ${ALA_MANIFEST_PATH} at protocol commit ${manifest.protocolCommit}`);
console.log(`execution closure: ${manifest.executionClosure.fileCount} files ${manifest.executionClosure.sha256}`);
