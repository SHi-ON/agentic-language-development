/**
 * Per-dyad live-output census (B2, reused in C4): pure reductions over
 * records the census script loads from a complete v2 dyad.
 *
 * - blockAccuracy: parent training accuracy per turn block (learning
 *   sanity; final block reads against 0.25 chance).
 * - deliveryJoin: planned shuffled tokens (block files) vs actual
 *   shuffled-branch delivered tokens, keyed by scheduled case.
 * - distinctShare: variation census (random-branch deliveries,
 *   per-case draw seeds).
 * - coverageFromExecuted: executed-slot census per block.
 *
 * Loading (bundles, schedules, blocks) lives in the script; only
 * these reductions are unit-tested. Findings (mismatches, low
 * variation) are REPORTED, never thrown: the caller decides gates.
 */

export interface SplitTrajectory {
  /** Full-size blocks only; the learning-sanity read is the last of these. */
  readonly full: readonly number[];
  /** Ragged tail (fewer than blockSize turns), or null when turns divide evenly. */
  readonly tail: number | null;
  readonly tailTurns: number;
}

/** Split a block trajectory into full blocks plus the ragged tail. */
export function splitTrajectory(trajectory: readonly number[], totalTurns: number, blockSize: number): SplitTrajectory {
  if (!Number.isInteger(totalTurns) || totalTurns < 1) throw new Error('dyad census: total turns must be positive');
  if (!Number.isInteger(blockSize) || blockSize < 1) throw new Error('dyad census: block size must be a positive integer');
  const fullCount = Math.floor(totalTurns / blockSize);
  if (trajectory.length !== fullCount + (totalTurns % blockSize === 0 ? 0 : 1)) {
    throw new Error('dyad census: trajectory length disagrees with turns and block size');
  }
  return {
    full: trajectory.slice(0, fullCount),
    tail: totalTurns % blockSize === 0 ? null : (trajectory[fullCount] as number),
    tailTurns: totalTurns % blockSize,
  };
}

export function blockAccuracy(outcomes: readonly boolean[], blockSize: number): number[] {
  if (!Number.isInteger(blockSize) || blockSize < 1) {
    throw new Error('dyad census: block size must be a positive integer');
  }
  if (outcomes.length === 0) throw new Error('dyad census: no outcomes to block');
  const blocks: number[] = [];
  for (let start = 0; start < outcomes.length; start += blockSize) {
    const block = outcomes.slice(start, start + blockSize);
    blocks.push(block.filter(Boolean).length / block.length);
  }
  return blocks;
}

export interface DeliveryJoinResult {
  readonly matched: number;
  readonly mismatched: readonly string[];
}

export function deliveryJoin(
  plannedByCase: ReadonlyMap<string, string>,
  actualByCase: ReadonlyMap<string, string>,
): DeliveryJoinResult {
  const mismatched: string[] = [];
  let matched = 0;
  for (const [caseId, planned] of plannedByCase) {
    const actual = actualByCase.get(caseId);
    if (actual === planned) {
      matched += 1;
    } else {
      mismatched.push(caseId);
    }
  }
  return { matched, mismatched: mismatched.sort() };
}

export interface DistinctShare {
  readonly distinct: number;
  readonly total: number;
}

export function distinctShare(values: readonly string[]): DistinctShare {
  return { distinct: new Set(values).size, total: values.length };
}

export interface ExecutedSlot {
  readonly partition: string;
  readonly role: string;
  readonly caseId: string;
}

export interface BlockCoverage {
  readonly partition: string;
  readonly role: string;
  readonly executed: number;
  readonly distinctCases: number;
}

export function coverageFromExecuted(slots: readonly ExecutedSlot[]): BlockCoverage[] {
  const groups = new Map<string, { partition: string; role: string; cases: string[] }>();
  for (const slot of slots) {
    const key = `${slot.partition}:${slot.role}`;
    const group = groups.get(key) ?? { partition: slot.partition, role: slot.role, cases: [] };
    group.cases.push(slot.caseId);
    groups.set(key, group);
  }
  return [...groups.values()]
    .map((group) => ({
      partition: group.partition,
      role: group.role,
      executed: group.cases.length,
      distinctCases: new Set(group.cases).size,
    }))
    .sort((a, b) => (a.partition < b.partition ? -1 : a.partition > b.partition ? 1 : a.role < b.role ? -1 : 1));
}

export function histogram(values: readonly number[]): Readonly<Record<string, number>> {
  const counts: Record<string, number> = {};
  for (const value of values) {
    const key = String(value);
    counts[key] = (counts[key] ?? 0) + 1;
  }
  return counts;
}
