#!/usr/bin/env tsx
/**
 * Pre-push check for `pin/*` seal tags (see docs/seal-tag-index.md).
 *
 * Every `pin/*` tag must either be named `pin/<first-9-hex-chars of
 * its peeled commit>` or be a grandfathered tag listed below pointing
 * at its recorded commit. New seal tags must satisfy the naming rule
 * at creation time; history is immutable and stays listed, never
 * rewritten. Run before pushing tags: `tsx scripts/check-pin-tags.ts`.
 */
import { execSync } from 'node:child_process';

/** Grandfathered tags: exact name -> peeled commit it must still point at. */
const GRANDFATHERED: Readonly<Record<string, string>> = {
  'pin/1137e1a9': '1137e1a879bf0b4f8c78ae563684315ebfbb79c5',
  'pin/1df2177': '1137e1a879bf0b4f8c78ae563684315ebfbb79c5',
  'pin/f0b6e9f': 'ad2c528fb056046cc7cfc39deefb8be24119f87c',
  'pin/c15e5317e': 'c15e5312016bda3dc176ff13a2b48f8752e1cadf',
  'pin/d6e17e2d9': 'd6e17e2673c676e19f2106207bb0c8c879381abf',
  'pin/f8547844de': 'f8547844de73d11193191295b3bceee5d2ef051c',
};

const git = (args: string): string =>
  execSync(`git ${args}`, { encoding: 'utf8' }).trim();

const tags = git(`tag --list 'pin/*'`).split('\n').map((t) => t.trim()).filter((t) => t.length > 0);
const failures: string[] = [];
let canonical = 0;
let grandfathered = 0;
for (const tag of tags) {
  const peeled = git(`rev-parse '${tag}^{}'`);
  if (tag === `pin/${peeled.slice(0, 9)}`) {
    canonical += 1;
    continue;
  }
  const recorded = GRANDFATHERED[tag];
  if (recorded === undefined) {
    failures.push(`FAIL: ${tag} peels to ${peeled} but is not pin/${peeled.slice(0, 9)} and is not grandfathered`);
  } else if (recorded !== peeled) {
    failures.push(`FAIL: ${tag} moved: recorded ${recorded}, now peels to ${peeled}`);
  } else {
    grandfathered += 1;
  }
}
if (failures.length > 0) {
  for (const failure of failures) console.error(failure);
  process.exit(1);
}
console.log(`pin-tag check passed: ${canonical} canonical, ${grandfathered} grandfathered, ${tags.length} total.`);
