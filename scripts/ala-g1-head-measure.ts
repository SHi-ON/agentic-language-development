/** G1 datum: embedded-head vs digest-head bytes per payload instance.
 *
 * Builds both state shapes exactly as the nursery call site does (see
 * nursery-runtime.ts lv01PredictionFor), with real-length entry hashes,
 * and reports JSON bytes. Ledger length L varies by training; the
 * report carries L symbolically and evaluates it at the v1-observed
 * scale for the projection. Usage: tsx scripts/ala-g1-head-measure.ts
 */
import { digestLv01LedgerHeads } from '../packages/orchestrator/src/experiments/lv01-paired-predictions.js';

const hash = (role: string, index: number): string =>
  `sha256:${(role + String(index).padStart(63, '0')).slice(0, 64)}`;

const measure = (length: number) => {
  const babyA = Array.from({ length }, (_, i) => hash('a', i));
  const babyB = Array.from({ length }, (_, i) => hash('b', i));
  if (babyA[0]!.length !== 71 || babyB[0]!.length !== 71) throw new Error('hash length drifted');
  const embedded = { babyA, babyB };
  const cutoff = { sequence: length, turn: length };
  const digest = { cutoff: { ...cutoff }, headsDigest: digestLv01LedgerHeads(babyA, babyB) };
  const sidecar = { version: 1, cutoffBabyA: { ...cutoff }, cutoffBabyB: { ...cutoff }, babyA, babyB, headsDigest: digest.headsDigest };
  return {
    length,
    embeddedBytes: JSON.stringify(embedded).length,
    digestBytes: JSON.stringify(digest).length,
    sidecarBytes: JSON.stringify(sidecar).length,
  };
};

const INSTANCES = 720 * 7;
const points = [100, 1000, 6600].map(measure);
for (const point of points) {
  console.log(
    `L=${point.length}: embedded=${point.embeddedBytes}B digest=${point.digestBytes}B sidecar=${point.sidecarBytes}B`,
  );
}
const at = points[2]!;
const v1rate = at.embeddedBytes * INSTANCES;
const v2total = at.sidecarBytes + at.digestBytes * INSTANCES;
console.log(`instances per v2 dyad: ${INSTANCES}`);
console.log(`v1-rate head share at L=6600: ${(v1rate / 2 ** 30).toFixed(2)} GiB`);
console.log(`v2 head share at L=6600: ${(v2total / 2 ** 20).toFixed(2)} MiB`);
