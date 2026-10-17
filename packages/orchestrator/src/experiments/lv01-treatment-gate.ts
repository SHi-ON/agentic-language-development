/** Collection-entry gate for ledger-treatment batches (promoted from the
 * offline test helper `__tests__/ala/helpers/treatment-diagnostic.ts`).
 *
 * `diagnoseLedgerBatch` is byte-for-byte the helper logic: scheduled
 * identity is an explicit recorded input, never inferred from branch
 * names; selections are trusted records, never re-derived from policy
 * ledgers; malformed input returns a rejection verdict, never throws
 * (matching the Python port). The historical v1 collector never calls
 * this gate, preserving the retained sham audit; repaired construction
 * ({@link buildValidLedgerTreatmentBatch} in
 * `./lv01-ledger-treatments.js`) must pass {@link assertValidLedgerBatch}
 * before a batch is collected.
 */
export interface AlaTreatmentDiagnosticCase {
  readonly caseId: string;
  readonly scheduledCaseId: string;
  readonly receiverRole: 'baby-a' | 'baby-b';
  readonly target: number;
  readonly candidates: readonly number[];
  readonly selection: string;
  readonly shuffledSource: string;
  readonly shuffledDelivered: string;
}

export interface AlaTreatmentVerdict {
  readonly ok: boolean;
  readonly reasons: string[];
  readonly genuineCollisions: number;
  readonly repeatedContent: number;
}

export function diagnoseLedgerBatch(cases: readonly AlaTreatmentDiagnosticCase[]): AlaTreatmentVerdict {
  const reasons: string[] = [];
  let genuineCollisions = 0;
  let repeatedContent = 0;
  const result = (): AlaTreatmentVerdict => ({ ok: reasons.length === 0, reasons, genuineCollisions, repeatedContent });
  if (!Array.isArray(cases) || cases.length === 0) {
    reasons.push('batch is empty or not a list');
    return result();
  }
  const keyOf = (c: AlaTreatmentDiagnosticCase): string => JSON.stringify([c.receiverRole, c.target, c.candidates]);
  const identities = new Map<string, string>();
  const recordIds = new Set<string>();
  for (const c of cases) {
    if (c === null || typeof c !== 'object' || Array.isArray(c)
      || ['caseId', 'scheduledCaseId', 'selection', 'shuffledSource', 'shuffledDelivered'].some(
      (field) => typeof c[field as keyof AlaTreatmentDiagnosticCase] !== 'string' || c[field as keyof AlaTreatmentDiagnosticCase] === '',
    ) || !['baby-a', 'baby-b'].includes(c.receiverRole)
      || !Number.isInteger(c.target) || !Array.isArray(c.candidates)
      || c.candidates.length === 0 || !c.candidates.every(Number.isInteger)) {
      reasons.push('case has malformed recorded fields');
      continue;
    }
    if (recordIds.has(c.caseId)) reasons.push(`${c.caseId}: duplicate branch record id`);
    recordIds.add(c.caseId);
    const previous = identities.get(c.scheduledCaseId);
    if (previous !== undefined && previous !== keyOf(c)) {
      reasons.push(`${c.scheduledCaseId}: scheduled identity has inconsistent conditioning content`);
    }
    identities.set(c.scheduledCaseId, keyOf(c));
  }
  if (reasons.length) return result();
  const byId = new Map(cases.map((c) => [c.caseId, c]));
  const donors = cases.map((c) => c.shuffledSource);
  if (new Set(donors).size !== donors.length) {
    reasons.push('donor assignment is not injective (two cases share one shuffle target)');
  }
  for (const role of ['baby-a', 'baby-b'] as const) {
    const roleCases = cases.filter((c) => c.receiverRole === role);
    if (roleCases.length && new Set(roleCases.map((c) => c.scheduledCaseId)).size < 2) {
      reasons.push(`role ${role}: one distinct scheduled case across ${roleCases.length} cases (branch records share one scheduled identity)`);
    }
  }
  for (const c of cases) {
    const donor = byId.get(c.shuffledSource);
    if (donor === undefined) {
      reasons.push(`${c.caseId}: donor ${c.shuffledSource} is not in the batch`);
      continue;
    }
    let constructionOk = true;
    if (donor.caseId === c.caseId) {
      reasons.push(`${c.caseId}: donor is self (derangement has a fixed point)`);
      constructionOk = false;
    }
    if (donor.receiverRole !== c.receiverRole) {
      reasons.push(`${c.caseId}: donor role ${donor.receiverRole} mismatches ${c.receiverRole}`);
      constructionOk = false;
    }
    if (donor.scheduledCaseId === c.scheduledCaseId) {
      reasons.push(`${c.caseId}: donor is the recipient's own scheduled case (sham shuffle)`);
      constructionOk = false;
    }
    if (constructionOk && keyOf(donor) === keyOf(c)) repeatedContent += 1;
    if (c.shuffledDelivered !== donor.selection) {
      reasons.push(`${c.caseId}: delivery mismatch: donor ${donor.caseId} selected ${donor.selection} but ${c.shuffledDelivered} was delivered`);
      constructionOk = false;
    }
    if (!constructionOk) continue;
    if (c.shuffledDelivered === c.selection) genuineCollisions += 1;
  }
  return result();
}

function gateFail(message: string): never {
  throw new Error(`LV01 treatment gate: ${message}`);
}

/** Fail-closed collection-entry gate: returns the accept verdict, or
 * throws naming every rejection reason. Malformed input is refused with
 * the diagnostic's own reasons, never with a TypeError.
 */
export function assertValidLedgerBatch(
  cases: readonly AlaTreatmentDiagnosticCase[],
): AlaTreatmentVerdict {
  const verdict = diagnoseLedgerBatch(cases);
  if (!verdict.ok) {
    gateFail(`refused batch (${verdict.reasons.join('; ')})`);
  }
  return verdict;
}
