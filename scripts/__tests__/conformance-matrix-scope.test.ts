import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import {
  buildEvidenceIndex,
  evidencePathsFor,
} from '../lib/acceptance-evidence.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));

// Must match RETIRED_PAPER_SCOPE in scripts/build-conformance-matrix.mjs.
// Each ID's executable evidence lived exclusively in pruned trees.
const RETIRED = [
  'ALD-040', 'ALD-043', 'ALD-048', 'ALD-049', 'ALD-050', 'ALD-051',
  'ALD-052', 'ALD-063', 'ALD-067', 'ALD-068', 'ALD-070',
];

function walkTests(directory: string): string[] {
  const out: string[] = [];
  let entries;
  try {
    entries = readdirSync(join(root, directory), { withFileTypes: true });
  } catch {
    return out;
  }
  for (const entry of entries) {
    const rel = join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== 'dist' && entry.name !== 'node_modules') {
        out.push(...walkTests(rel));
      }
    } else if (rel.endsWith('.test.ts') || (rel.startsWith('scripts/') && rel.endsWith('.mjs'))) {
      out.push(rel);
    }
  }
  return out;
}

// Independent live-tree assertion surface (mirrors the generator's walk,
// minus the generator's own mapping sources which name IDs without
// asserting them).
function liveAssertionFiles(): string[] {
  return [...walkTests('packages'), ...walkTests('scripts')]
    .filter((p) => p !== 'scripts/build-conformance-matrix.mjs'
      && p !== 'scripts/lib/acceptance-evidence.mjs'
      && p !== 'scripts/__tests__/conformance-matrix-scope.test.ts');
}

describe('conformance matrix paper scope', () => {
  it('tolerates pruned trees when building the evidence index', async () => {
    const index = await buildEvidenceIndex();
    expect(index.paths.length).toBeGreaterThan(0);
    expect(index.paths.some((p) => p.startsWith('twins/'))).toBe(false);
  });

  it('lists no live-tree assertion path for any retired ID', async () => {
    const index = await buildEvidenceIndex();
    for (const id of RETIRED) {
      expect(evidencePathsFor(id, index)).toEqual([]);
    }
  });

  it('retired IDs are independently absent from live executable sources', () => {
    const files = liveAssertionFiles();
    expect(files.length).toBeGreaterThan(0);
    for (const id of RETIRED) {
      const hits = files.filter((f) => readFileSync(join(root, f), 'utf8').includes(id));
      expect(hits).toEqual([]);
    }
  });

  it('keeps live coverage for non-retired checked criteria', async () => {
    const index = await buildEvidenceIndex();
    // ALD-011 crash-safety and ALD-036 gateway conformance stay live-gated.
    expect(evidencePathsFor('ALD-011', index).length).toBeGreaterThan(0);
    expect(evidencePathsFor('ALD-036', index).length).toBeGreaterThan(0);
  });

  it('regenerates the matrix without unmapped-applicable failures', () => {
    execFileSync('node', ['scripts/build-conformance-matrix.mjs', '--check'], {
      cwd: root, stdio: 'pipe',
    });
    const matrix = JSON.parse(readFileSync(join(root, 'docs/requirement-conformance-matrix.json'), 'utf8'));
    expect(matrix.summary.retiredBacklogCriteria).toBe(RETIRED.length * 3);
  });
});
