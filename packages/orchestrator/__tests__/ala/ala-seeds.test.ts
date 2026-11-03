import { describe, expect, it } from 'vitest';

import {
  ALA_SEED_ROOT,
  ALA_SEED_ROOT_V2,
  ALA_STUDY_ID,
  alaChildRunIdPrefix,
  alaDyadSeeds,
  alaEngineSlotNumber,
  alaRoleStreamSeed,
  auditAlaSeedsAgainstRetained,
  deriveAlaSeed,
} from '../../src/ala/ala-seeds.js';

describe('ALA seed namespace', () => {
  it('derives distinct seeds per purpose/role/partition/case', () => {
    const base = { cohort: 'A' as const, slotKind: 'primary' as const, slotIndex: '0000' };
    const a = deriveAlaSeed({ ...base, purpose: 'scenario', role: 'shared' });
    const b = deriveAlaSeed({ ...base, purpose: 'action-draw', role: 'shared' });
    const c = deriveAlaSeed({ ...base, purpose: 'action-draw', role: 'baby-a-receiver' });
    const d = deriveAlaSeed({ ...base, purpose: 'scenario', role: 'shared', partition: 'training', caseId: 'training:shared:0001' });
    expect(new Set([a, b, c, d]).size).toBe(4);
    for (const seed of [a, b, c, d]) expect(seed).toMatch(/^[0-9a-f]{64}$/u);
  });

  it('derives distinct seeds per case', () => {
    const shared = { cohort: 'A' as const, slotKind: 'primary' as const, slotIndex: '0000', purpose: 'action-draw' as const, role: 'shared' as const, partition: 'within-support-test' as const, caseId: 'within-support-test:baby-a:0007' };
    const sameCase = deriveAlaSeed({ ...shared });
    const otherCase = deriveAlaSeed({ ...shared, caseId: 'within-support-test:baby-a:0008' });
    expect(sameCase).toMatch(/^[0-9a-f]{64}$/u);
    expect(otherCase).not.toBe(sameCase);
  });

  it('rejects malformed parts', () => {
    expect(() => deriveAlaSeed({ cohort: 'A', slotKind: 'primary', slotIndex: '00', purpose: 'scenario', role: 'shared' })).toThrowError(
      /slotIndex/,
    );
    expect(() => deriveAlaSeed({ cohort: 'A', slotKind: 'primary', slotIndex: '0000', purpose: 'scenario', role: 'shared', caseId: 'a\0b' })).toThrowError(
      /NUL/,
    );
  });

  it('maps disjoint engine slot numbers', () => {
    const numbers = new Set<number>();
    for (const cohort of ['development', 'calibration', 'A', 'B'] as const) {
      for (const index of [0, 1, 19]) {
        const n = alaEngineSlotNumber(cohort, 'primary', index);
        expect(numbers.has(n)).toBe(false);
        numbers.add(n);
      }
      for (const index of [0, 1]) {
        const n = alaEngineSlotNumber(cohort, 'reserve', index);
        expect(numbers.has(n)).toBe(false);
        numbers.add(n);
      }
    }
    expect(alaEngineSlotNumber('A', 'primary', 0)).toBe(0);
    expect(alaEngineSlotNumber('A', 'reserve', 0)).toBe(20);
    expect(alaEngineSlotNumber('B', 'primary', 0)).toBe(100);
    expect(alaEngineSlotNumber('development', 'primary', 0)).toBe(200);
    expect(alaEngineSlotNumber('calibration', 'primary', 0)).toBe(300);
  });

  it('derives five dyad seeds from the ALA root', () => {
    expect(ALA_SEED_ROOT).toBe('ala-ledger-value-v1');
    expect(ALA_STUDY_ID).toBe('ALA-LV');
    const seeds = alaDyadSeeds('A', 'primary', '0000');
    expect(new Set(Object.values(seeds)).size).toBe(5);
  });

  it('separates per-role intervention streams', () => {
    const base = deriveAlaSeed({ cohort: 'A', slotKind: 'primary', slotIndex: '0000', purpose: 'intervention-shuffle', role: 'shared' });
    expect(alaRoleStreamSeed(base, 'baby-a')).toBe(`${base}baby-a`);
    expect(alaRoleStreamSeed(base, 'baby-b')).toBe(`${base}baby-b`);
    expect(() => alaRoleStreamSeed('short', 'baby-a')).toThrowError(/sha256 hex/);
  });

  it('fails closed on retained-identity collisions', () => {
    const seed = deriveAlaSeed({ cohort: 'A', slotKind: 'primary', slotIndex: '0000', purpose: 'scenario', role: 'shared' });
    expect(() => auditAlaSeedsAgainstRetained([seed], ['deadbeef'])).not.toThrow();
    expect(() => auditAlaSeedsAgainstRetained([seed], [seed])).toThrowError(/collides/);
  });

  it('derives a disjoint v2 namespace and refuses unknown roots', () => {
    const base = { cohort: 'A' as const, slotKind: 'primary' as const, slotIndex: '0000', purpose: 'scenario' as const, role: 'shared' as const };
    const v1 = deriveAlaSeed(base);
    const v2 = deriveAlaSeed({ ...base, seedRoot: ALA_SEED_ROOT_V2 });
    expect(v1).not.toBe(v2);
    expect(alaDyadSeeds('A', 'primary', '0000', ALA_SEED_ROOT_V2).scenario).toBe(v2);
    expect(alaDyadSeeds('A', 'primary', '0000').scenario).toBe(v1);
    expect(() => deriveAlaSeed({ ...base, seedRoot: 'bogus-root' })).toThrowError(/not a known ALA namespace root/);
  });

  it('emits lowercase-slug child run prefixes for every cohort (C2: the LV01 collector rejects uppercase)', () => {
    for (const cohort of ['A', 'B', 'development', 'calibration'] as const) {
      const prefix = alaChildRunIdPrefix(cohort, 'primary', '0000', 1);
      expect(prefix).toMatch(/^[a-z0-9-]+$/u);
    }
    expect(alaChildRunIdPrefix('A', 'primary', '0000', 1)).toBe('ala-a-primary-0000-s0001');
    expect(alaChildRunIdPrefix('development', 'primary', '0000', 1)).toBe('ala-development-primary-0000-s0001');
  });
});
