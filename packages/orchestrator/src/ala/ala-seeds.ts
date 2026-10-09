/**
 * ALA-LV seed namespace (P3).
 *
 * Disjoint from every LV01/E00-E50 identity by construction: a distinct
 * root (`ala-ledger-value-v1`) plus the `ALA-LV` study part. Engine-internal
 * LV01-family calls receive disjoint engine slot numbers; external
 * identity is always cohort/slotKind/slotIndex in this namespace.
 * Every materialized seed is audited against the retained-identity corpus
 * before execution; any collision fails closed.
 */
import { deriveSeedHex } from '@ald/hashing';

export const ALA_SEED_ROOT = 'ala-ledger-value-v1' as const;
export const ALA_STUDY_ID = 'ALA-LV' as const;
export const ALA_POLICY_VERSION = 'v1' as const;

export type AlaCohort = 'development' | 'calibration' | 'A' | 'B';
export type AlaSlotKind = 'primary' | 'reserve';
export type AlaSeedRole = 'baby-a-receiver' | 'baby-b-receiver' | 'shared';
export type AlaSeedPartition = 'training' | 'validation-fit' | 'validation-selection' | 'within-support-test';

export const ALA_SEED_PURPOSES = [
  'scenario',
  'learner/baby-a',
  'learner/baby-b',
  'channel',
  'action-draw',
  'intervention-shuffle',
  'branch-order',
  'analysis',
] as const;

function fail(message: string): never {
  throw new Error(`ALA seeds: ${message}`);
}

function assertPart(value: string, name: string): void {
  if (value.length === 0) fail(`${name} is empty`);
  if (value.includes('\0')) fail(`${name} contains NUL`);
}

/**
 * Derive one ALA seed. Parts: root/studyId/cohort/slotKind/slotIndex/
 * purpose/role/partition/case/branch — branch omitted for paired seeds so
 * sibling branches share scenario, observation, pre-state and draw.
 */
export function deriveAlaSeed(input: {
  readonly cohort: AlaCohort;
  readonly slotKind: AlaSlotKind;
  readonly slotIndex: string;
  readonly purpose: (typeof ALA_SEED_PURPOSES)[number];
  readonly role: AlaSeedRole;
  readonly partition?: AlaSeedPartition;
  readonly caseId?: string;
  readonly branch?: string;
}): string {
  for (const [name, value] of Object.entries(input)) assertPart(String(value), name);
  if (!/^\d{4}$/u.test(input.slotIndex)) fail('slotIndex must be zero-padded');
  const parts = [
    ALA_SEED_ROOT,
    ALA_STUDY_ID,
    input.cohort,
    input.slotKind,
    input.slotIndex,
    input.purpose,
    input.role,
  ];
  if (input.partition !== undefined) parts.push(input.partition);
  if (input.caseId !== undefined) parts.push(input.caseId);
  if (input.branch !== undefined) parts.push(input.branch);
  return deriveSeedHex(...parts);
}

/** Slot-scoped seeds for one dyad (scenario/learners/channel/analysis). */
export function alaDyadSeeds(cohort: AlaCohort, slotKind: AlaSlotKind, slotIndex: string): {
  readonly scenario: string;
  readonly babyA: string;
  readonly babyB: string;
  readonly channel: string;
  readonly analysis: string;
} {
  const seed = (purpose: (typeof ALA_SEED_PURPOSES)[number]): string =>
    deriveAlaSeed({ cohort, slotKind, slotIndex, purpose, role: 'shared' });
  return {
    scenario: seed('scenario'),
    babyA: seed('learner/baby-a'),
    babyB: seed('learner/baby-b'),
    channel: seed('channel'),
    analysis: seed('analysis'),
  };
}

/**
 * Engine-internal LV01-family slot numbers. Disjoint across cohorts and
 * dev/calibration; the adapter documents the mapping and the manifest
 * records it. External identity stays cohort/slotKind/slotIndex.
 */
export function alaEngineSlotNumber(cohort: AlaCohort, slotKind: AlaSlotKind, index: number): number {
  if (!Number.isInteger(index) || index < 0) fail('dyad index must be a non-negative integer');
  const reserve = slotKind === 'reserve' ? 20 : 0;
  switch (cohort) {
    case 'A': return index + reserve;
    case 'B': return 100 + index + reserve;
    case 'development': return 200 + index + reserve;
    case 'calibration': return 300 + index + reserve;
  }
}

/** Per-role intervention-shuffle stream separation (hex base + role code). */
export function alaRoleStreamSeed(baseSeed: string, role: 'baby-a' | 'baby-b'): string {
  if (!/^[0-9a-f]{64}$/u.test(baseSeed)) fail('role stream base seed is not sha256 hex');
  return `${baseSeed}${role}`;
}

/**
 * Audit materialized ALA seeds against the retained-identity corpus.
 * Any exact match fails closed: ALA identities must never collide with
 * retained LV01/E00-E50 identities.
 */
export function auditAlaSeedsAgainstRetained(
  seeds: readonly string[],
  retainedCorpus: readonly string[],
): void {
  const corpus = new Set(retainedCorpus);
  for (const seed of seeds) {
    if (corpus.has(seed)) fail(`seed ${seed} collides with a retained identity`);
  }
}
