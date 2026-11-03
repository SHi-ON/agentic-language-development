/** Loader tests on synthetic parent/branch shapes (live-shape fixtures). */
import { describe, expect, it } from 'vitest';
import {
  analysisRowFromDocs,
  branchAnalysisRow,
  loadAnalysisTrainingRows,
} from '../../src/experiments/lv01-analysis-loader.js';
import type { Lv01BundleDocs } from '../../src/experiments/lv01-audit-loader.js';

const HASH = `sha256:${'a'.repeat(64)}`;
const SIG = 'ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==';
const AT = '2026-11-03T00:00:00Z';

const turnRecord = (turn: number, phase: string, receiver: 'baby-a' | 'baby-b', selectedRef: string, selectedType: number) => ({
  version: 1,
  runId: 'parent',
  sequence: turn + 1,
  turn,
  phase,
  roles: { sender: receiver === 'baby-a' ? 'baby-b' : 'baby-a', receiver },
  communicationCondition: 'normal',
  scenarioRef: `scn:${turn}`,
  scenarioStateHash: HASH,
  observationHashes: { babyA: HASH, babyB: HASH },
  babyProposalHash: HASH,
  deliveredArtifactHash: HASH,
  channelEventHash: HASH,
  actionHash: HASH,
  outcomeHash: HASH,
  outcome: { details: { selectedRef, selectedTypeCode: selectedType }, reward: 1, success: true },
  previousEntryHash: HASH,
  recordedAt: AT,
  writerKeyId: 'nursery',
  entryHash: HASH,
  writerSignature: SIG,
});

const ledgerEvent = (babyId: 'A' | 'B', sequence: number, turn: number, eventType: string, content: Record<string, unknown>) => ({
  version: 1,
  runId: 'parent',
  babyId,
  sequence,
  turn,
  eventType,
  contentSchema: 'agent-native-ledger',
  subjectId: 'symbol:S01',
  content,
  blindingNonce: 'nonce',
  previousEntryHash: HASH,
  recordedAt: AT,
  writerKeyId: 'writer',
  entryHash: HASH,
  writerSignature: 'signature-that-is-long-enough',
});

function parentDocs(): Lv01BundleDocs {
  return {
    runManifest: {},
    runConfig: { runId: 'parent' },
    interventions: [],
    turns: [
      turnRecord(0, 'running', 'baby-b', 'o:sel0', 1),
      turnRecord(1, 'running', 'baby-a', 'o:sel1', 2),
      turnRecord(2, 'evaluating', 'baby-b', 'o:sel2', 3),
    ],
    ledgerA: [
      ledgerEvent('A', 1, 1, 'intention.recorded', { symbols: ['S01'], selection: 'o:sel1' }),
      ledgerEvent('A', 2, 1, 'interpretation.recorded', { symbols: ['S01'], candidateTypeCodes: [2, 3, 4, 6] }),
    ],
    ledgerB: [
      ledgerEvent('B', 1, 0, 'intention.recorded', { symbols: ['S02'], selection: 'o:sel0' }),
      ledgerEvent('B', 2, 0, 'interpretation.recorded', { symbols: ['S02'], candidateTypeCodes: [1, 2, 3, 4] }),
      ledgerEvent('B', 3, 2, 'intention.recorded', { symbols: ['S03'], selection: 'o:sel2' }),
      ledgerEvent('B', 4, 2, 'interpretation.recorded', { symbols: ['S03'], candidateTypeCodes: [3, 4, 6, 7] }),
    ],
    policies: {},
    checkpoints: [],
  };
}

describe('analysis training rows', () => {
  it('loads one row per running turn and excludes the evaluation turn', () => {
    const rows = loadAnalysisTrainingRows(parentDocs(), 1);
    expect(rows).toHaveLength(2);
    expect(rows).toContainEqual({
      caseId: 'parent:turn:0',
      receiverRole: 'baby-b',
      candidateTypeCodes: [1, 2, 3, 4],
      deliveredToken: 'S02',
      actualSelectedCandidateIndex: 0,
    });
    expect(rows).toContainEqual({
      caseId: 'parent:turn:1',
      receiverRole: 'baby-a',
      candidateTypeCodes: [2, 3, 4, 6],
      deliveredToken: 'S01',
      actualSelectedCandidateIndex: 0,
    });
  });

  it('fails closed on count mismatch and broken outcome agreement', () => {
    expect(() => loadAnalysisTrainingRows(parentDocs(), 2)).toThrow(/want 2/);
    const broken = parentDocs();
    (broken.turns[0] as { outcome: { details: { selectedRef: string } } }).outcome.details.selectedRef = 'o:other';
    expect(() => loadAnalysisTrainingRows(broken, 1)).toThrow(/outcome agreement/);
  });
});

describe('branch analysis rows', () => {
  const audited = (branch: string) => ({
    branch,
    runId: 'branch',
    payload: {
      receiver: 'baby-b',
      candidateRefs: ['o:a', 'o:b', 'o:c', 'o:d'],
      candidateTypeCodes: [4, 1, 6, 9],
      deliveredToken: 'S07',
      native: { distribution: [0.25, 0.25, 0.25, 0.25], source: 'training-ledger' },
      replay: { distribution: [0.1, 0.1, 0.1, 0.7] },
    },
    transcript: [{ sequence: 4, eventType: 'receiver-action-recorded', entryHash: HASH, previousEntryHash: HASH, body: { selectedCandidateRef: 'o:d', deliveredToken: 'S07', candidateRefs: ['o:a', 'o:b', 'o:c', 'o:d'] } }],
  });

  const branchTurn = () => turnRecord(0, 'evaluating', 'baby-b', 'o:d', 9);

  it('projects native/replay/actual/target from recorded fields', () => {
    const row = branchAnalysisRow(audited('normal') as never, { ...branchTurn(), outcome: { details: { selectedRef: 'o:d', selectedTypeCode: 9, targetTypeCode: 1 } } }, 'sched-1');
    expect(row.record).toMatchObject({ caseId: 'sched-1', receiverRole: 'baby-b', actualSelectedCandidateIndex: 3 });
    expect(row.targetIndex).toBe(1);
    expect(row.nativeDistribution).toEqual([0.25, 0.25, 0.25, 0.25]);
    expect(row.nativeSource).toBe('training-ledger');
  });

  it('refuses non-normal branches, missing replay, and off-order targets', () => {
    const turn = branchTurn();
    const targeted = { ...turn, outcome: { details: { selectedRef: 'o:d', selectedTypeCode: 9, targetTypeCode: 1 } } };
    expect(() => branchAnalysisRow(audited('random') as never, turn, 's:1')).toThrow(/normal branches/);
    const noReplay = audited('normal') as unknown as { payload: { replay: null } };
    noReplay.payload.replay = null;
    expect(() => branchAnalysisRow(noReplay as never, targeted, 's:1')).toThrow(/no replay/);
    expect(() => branchAnalysisRow(audited('normal') as never, { ...turn, outcome: { details: { selectedRef: 'o:d', selectedTypeCode: 9, targetTypeCode: 14 } } }, 's:1')).toThrow(/outside the committed candidate order/);
  });

  it('analysisRowFromDocs audits through the loader path', () => {
    const docs: Lv01BundleDocs = {
      runManifest: {},
      runConfig: { runId: 'x', lv01PairedCase: { branch: 'normal' } },
      interventions: [],
      turns: [branchTurn()],
      ledgerA: [],
      ledgerB: [],
      policies: {},
      checkpoints: [],
    };
    // Full-bundle audit needs the intervention stream; the shape check runs first.
    expect(() => analysisRowFromDocs({ ...docs, turns: [] }, 's:1')).toThrow(/single turn-0/);
  });
});
