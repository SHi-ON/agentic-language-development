/**
 * LV01 analysis record loader (B4): retained bundles in, analysis rows
 * out. Training rows come from the parent bundle's phase-running turns
 * joined to the receiver ledger's intention (delivery + selection, with
 * outcome agreement) and interpretation (recorded candidates) records;
 * the post-training evaluation turn is outside the registered window
 * and never a training row. Fold/test rows come from audited normal
 * branches, whose committed payloads already carry candidates and the
 * native/replay distributions. Every join is fail-closed; nothing is
 * re-derived from seeds or engines.
 */
import type { Lv01OrdinaryRecord } from '@ald/analysis';
import { LedgerEventSchema, TurnRecordSchema } from '@ald/types';

import type { Lv01BundleDocs } from './lv01-audit-loader.js';
import { loadLv01AuditedBranch } from './lv01-audit-loader.js';
import type { Lv01AuditedBranch } from './lv01-auditor.js';

function fail(message: string): never {
  throw new Error(`LV01 analysis loader: ${message}`);
}

interface TypedTurn {
  readonly turn: number;
  readonly phase: string;
  readonly receiver: 'baby-a' | 'baby-b';
  readonly selectedRef: string;
  readonly selectedTypeCode: number;
}

function typedTurns(docs: Lv01BundleDocs, where: string): TypedTurn[] {
  return docs.turns.map((entry, index) => {
    const parsed = TurnRecordSchema.safeParse(entry);
    if (!parsed.success) fail(`${where} turn record ${index} is invalid`);
    const turn = parsed.data as unknown as Record<string, unknown>;
    const roles = turn['roles'] as { receiver?: unknown } | undefined;
    const outcome = turn['outcome'] as { details?: { selectedRef?: unknown; selectedTypeCode?: unknown } } | undefined;
    if (roles?.receiver !== 'baby-a' && roles?.receiver !== 'baby-b') fail(`${where} turn record ${index} has no receiver role`);
    if (typeof outcome?.details?.selectedRef !== 'string' || outcome.details.selectedRef.length === 0) {
      fail(`${where} turn record ${index} carries no selected ref`);
    }
    if (typeof outcome.details.selectedTypeCode !== 'number') fail(`${where} turn record ${index} carries no selected type`);
    return {
      turn: turn['turn'] as number,
      phase: turn['phase'] as string,
      receiver: roles.receiver as 'baby-a' | 'baby-b',
      selectedRef: outcome.details.selectedRef,
      selectedTypeCode: outcome.details.selectedTypeCode,
    };
  });
}

interface LedgerFact {
  readonly turn: number;
  readonly symbols: readonly unknown[];
  readonly selection: unknown;
  readonly candidateTypeCodes: unknown;
}

function ledgerFacts(ledger: readonly unknown[], eventType: string, where: string): LedgerFact[] {
  const facts: LedgerFact[] = [];
  for (const entry of ledger) {
    const parsed = LedgerEventSchema.safeParse(entry);
    if (!parsed.success) continue;
    const event = parsed.data as unknown as Record<string, unknown>;
    if (event['eventType'] !== eventType) continue;
    const content = (event['content'] ?? {}) as Record<string, unknown>;
    facts.push({
      turn: event['turn'] as number,
      symbols: (content['symbols'] ?? []) as readonly unknown[],
      selection: content['selection'],
      candidateTypeCodes: content['candidateTypeCodes'],
    });
  }
  if (facts.length === 0) fail(`${where} has no ${eventType} records`);
  return facts;
}

/**
 * Ordinary training records from a parent bundle: one per
 * phase-running turn, keyed by the receiver. Asserts exactly
 * expectedPerRole rows per role (the registered window); the
 * post-training evaluation turn is excluded by the phase filter.
 */
export function loadAnalysisTrainingRows(parentDocs: Lv01BundleDocs, expectedPerRole: number): Lv01OrdinaryRecord[] {
  const turns = typedTurns(parentDocs, 'parent').filter((turn) => turn.phase === 'running');
  const runId = (parentDocs.runConfig as { runId?: unknown })?.runId;
  if (typeof runId !== 'string' || runId.length === 0) fail('parent run-config carries no run id');
  const rows: Lv01OrdinaryRecord[] = [];
  for (const role of ['baby-a', 'baby-b'] as const) {
    const ledger = role === 'baby-a' ? parentDocs.ledgerA : parentDocs.ledgerB;
    const intentions = new Map<number, LedgerFact>();
    for (const fact of ledgerFacts(ledger, 'intention.recorded', `parent ${role} ledger`)) {
      if (intentions.has(fact.turn)) fail(`parent ${role} ledger repeats an intention at turn ${fact.turn}`);
      intentions.set(fact.turn, fact);
    }
    const interpretations = new Map<number, LedgerFact>();
    for (const fact of ledgerFacts(ledger, 'interpretation.recorded', `parent ${role} ledger`)) {
      if (interpretations.has(fact.turn)) fail(`parent ${role} ledger repeats an interpretation at turn ${fact.turn}`);
      interpretations.set(fact.turn, fact);
    }
    const roleTurns = turns.filter((turn) => turn.receiver === role);
    for (const turn of roleTurns) {
      const intention = intentions.get(turn.turn);
      if (intention === undefined) fail(`parent ${role} ledger has no intention at turn ${turn.turn}`);
      const interpretation = interpretations.get(turn.turn);
      if (interpretation === undefined) fail(`parent ${role} ledger has no interpretation at turn ${turn.turn}`);
      if (intention.selection !== turn.selectedRef) {
        fail(`parent turn ${turn.turn} intention disagrees with the outcome (no outcome agreement)`);
      }
      const delivered = intention.symbols[0] ?? null;
      if (delivered !== null && typeof delivered !== 'string') fail(`parent turn ${turn.turn} delivery is malformed`);
      if (!Array.isArray(interpretation.candidateTypeCodes) || interpretation.candidateTypeCodes.length !== 4) {
        fail(`parent turn ${turn.turn} interpretation carries no four-candidate order`);
      }
      const candidates = interpretation.candidateTypeCodes as number[];
      const actual = candidates.indexOf(turn.selectedTypeCode);
      if (actual < 0) fail(`parent turn ${turn.turn} selected type is outside the recorded candidates`);
      rows.push({
        caseId: `${runId}:turn:${turn.turn}`,
        receiverRole: role,
        candidateTypeCodes: [...candidates],
        deliveredToken: delivered,
        actualSelectedCandidateIndex: actual,
      });
    }
  }
  for (const role of ['baby-a', 'baby-b'] as const) {
    const count = rows.filter((row) => row.receiverRole === role).length;
    if (count !== expectedPerRole) fail(`parent training window has ${count} ${role} rows, want ${expectedPerRole}`);
  }
  return rows;
}

export interface AnalysisBranchRow {
  readonly record: Lv01OrdinaryRecord;
  readonly nativeDistribution: readonly number[];
  readonly replayDistribution: readonly number[];
  readonly targetIndex: number;
  readonly nativeSource: string;
}

/**
 * One analysis row from an audited normal branch: the committed
 * payload carries candidates plus native/replay distributions, the
 * transcript carries the agreed action, and the turn outcome carries
 * the task target. The caller supplies the scheduled case id, which
 * must match the case the branch served.
 */
export function branchAnalysisRow(
  audited: Lv01AuditedBranch,
  turnRecord: unknown,
  scheduledCaseId: string,
): AnalysisBranchRow {
  if (audited.branch !== 'normal') fail(`analysis rows come from normal branches, not ${audited.branch}`);
  const action = audited.transcript.find((entry) => entry.eventType === 'receiver-action-recorded');
  const body = action?.body as { selectedCandidateRef?: unknown; deliveredToken?: unknown; candidateRefs?: unknown } | undefined;
  if (typeof body?.selectedCandidateRef !== 'string') fail('normal branch transcript carries no selection');
  const candidateRefs = audited.payload.candidateRefs;
  const actual = candidateRefs.indexOf(body.selectedCandidateRef);
  if (actual < 0) fail('normal branch selection is outside the committed candidate order');
  const parsed = TurnRecordSchema.safeParse(turnRecord);
  if (!parsed.success) fail('normal branch turn record is invalid');
  const turn = parsed.data as unknown as Record<string, unknown>;
  const outcome = turn['outcome'] as { details?: { targetTypeCode?: unknown } } | undefined;
  const targetType = outcome?.details?.targetTypeCode;
  if (typeof targetType !== 'number') fail('normal branch turn outcome carries no target type');
  const targetIndex = audited.payload.candidateTypeCodes.indexOf(targetType);
  if (targetIndex < 0) fail('normal branch target is outside the committed candidate order');
  if (audited.payload.replay === null) fail('normal branch payload carries no replay distribution');
  const delivered = audited.payload.deliveredToken;
  if (delivered !== null && body.deliveredToken !== delivered) fail('normal branch payload disagrees with the recorded delivery');
  return {
    record: {
      caseId: scheduledCaseId,
      receiverRole: audited.payload.receiver,
      candidateTypeCodes: [...audited.payload.candidateTypeCodes],
      deliveredToken: delivered,
      actualSelectedCandidateIndex: actual,
    },
    nativeDistribution: [...audited.payload.native.distribution],
    replayDistribution: [...audited.payload.replay.distribution],
    targetIndex,
    nativeSource: audited.payload.native.source,
  };
}

/** Audit one parsed branch bundle, then project its analysis row. */
export function analysisRowFromDocs(docs: Lv01BundleDocs, scheduledCaseId: string): AnalysisBranchRow {
  if (docs.turns.length !== 1) fail('branch bundle is not a single turn-0 record');
  return branchAnalysisRow(loadLv01AuditedBranch(docs), docs.turns[0], scheduledCaseId);
}
