import { describe, expect, it } from 'vitest';

import {
  ALA_SEED_ROOT,
  ALA_STUDY_ID,
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

  it('shares paired-branch seeds across branches but not cases', () => {
    const shared = { cohort: 'A' as const, slotKind: 'primary' as const, slotIndex: '0000', purpose: 'action-draw' as const, role: 'shared' as const, partition: 'within-support-test' as const, caseId: 'within-support-test:baby-a:0007' };
    const withBranch = deriveAlaSeed({ ...shared });
    const otherCase = deriveAlaSeed({ ...shared, caseId: 'within-support-test:baby-a:0008' });
    expect(withBranch).toMatch(/^[0-9a-f]{64}$/u);
    expect(otherCase).not.toBe(withBranch);
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
});
