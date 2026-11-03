/**
 * ALA-LV dyad estimation (B4/Phase D): per-role ordinary
 * selection/refit/test scoring plus the dyad primary statistic and
 * dyad-unit Student-t intervals, exactly per the frozen v2 analysis
 * contract. Estimation-only: no supported labels, no power claims.
 *
 * Per dyad: equal average over receiver roles of mean
 * selected-ordinary Brier minus native Brier on normal-branch test
 * cases; positive favors native. Per cohort: two-sided Student-t
 * interval across dyads (97.5% individual for Bonferroni 95% joint
 * over the two primary cohort intervals). Degenerate inputs (n < 2,
 * zero variance) are flagged, never a finding.
 */
import { summarize } from './descriptive.js';
import { AnalysisError } from './errors.js';
import {
  scoreLv01OrdinaryFit,
  scoreLv01Prediction,
  selectLv01OrdinaryPredictor,
  type Lv01OrdinaryPredictorId,
  type Lv01OrdinaryRecord,
} from './ledger-value.js';
import { studentTQuantile } from './special.js';

function fail(message: string): never {
  throw new AnalysisError('domain', message);
}

export interface AlaLvTestRow {
  readonly record: Lv01OrdinaryRecord;
  readonly nativeDistribution: readonly number[];
  readonly replayDistribution: readonly number[];
  readonly targetIndex: number;
  readonly nativeSource: string;
}

export interface AlaLvRoleFolds {
  readonly role: 'baby-a' | 'baby-b';
  readonly training: readonly Lv01OrdinaryRecord[];
  readonly validationFit: readonly Lv01OrdinaryRecord[];
  readonly validationSelection: readonly Lv01OrdinaryRecord[];
  readonly test: readonly AlaLvTestRow[];
}

export interface AlaLvRoleResult {
  readonly role: 'baby-a' | 'baby-b';
  readonly selectedPredictorId: Lv01OrdinaryPredictorId;
  readonly validationBrierByPredictor: Readonly<Record<Lv01OrdinaryPredictorId, number>>;
  readonly testCases: number;
  readonly ordinaryBrier: number;
  readonly nativeBrier: number;
  readonly replayBrier: number;
  readonly meanExcessReplayBrier: number;
  /** Mean selected-ordinary Brier minus native Brier; positive favors native. */
  readonly difference: number;
  readonly nativeMissingTokens: number;
}

export interface AlaLvDyadResult {
  readonly dyad: string;
  readonly roles: readonly AlaLvRoleResult[];
  /** Equal average over receiver roles of the role differences. */
  readonly primary: number;
}

function brier(probabilities: readonly number[], action: number): number {
  return probabilities.reduce((sum, value, index) => sum + (value - (index === action ? 1 : 0)) ** 2, 0);
}

export function analyzeAlaLvRole(
  folds: AlaLvRoleFolds,
  inventory: readonly string[],
): AlaLvRoleResult {
  if (folds.test.length === 0) fail(`role ${folds.role} has no test rows`);
  const selection = selectLv01OrdinaryPredictor({
    inventory: [...inventory],
    training: [...folds.training],
    validationFit: [...folds.validationFit],
    validationSelection: [...folds.validationSelection],
  });
  const refit = selection.refitPredictor;
  let ordinary = 0;
  let native = 0;
  let replay = 0;
  let excess = 0;
  let missing = 0;
  for (const row of folds.test) {
    if (row.record.receiverRole !== folds.role) fail(`role ${folds.role} test set carries a ${row.record.receiverRole} row`);
    // scoreLv01OrdinaryFit revalidates the record shape (assertRecord);
    // scoreLv01Prediction validates distributions and labels.
    const ordinaryDistribution = scoreLv01OrdinaryFit(refit.id, refit.fit, row.record, inventory);
    const scored = scoreLv01Prediction({
      candidateTypeCodes: [...row.record.candidateTypeCodes],
      nativeDistribution: [...row.nativeDistribution],
      replayDistribution: [...row.replayDistribution],
      label: {
        actualSelectedCandidateIndex: row.record.actualSelectedCandidateIndex,
        taskTargetCandidateIndex: row.targetIndex,
      },
    });
    ordinary += brier(ordinaryDistribution, row.record.actualSelectedCandidateIndex);
    native += scored.nativeBrier;
    replay += scored.replayBrier;
    excess += scored.expectedExcessReplayBrier;
    if (row.nativeSource !== 'training-ledger') missing += 1;
  }
  const n = folds.test.length;
  return {
    role: folds.role,
    selectedPredictorId: selection.selectedPredictorId,
    validationBrierByPredictor: selection.validationBrierByPredictor,
    testCases: n,
    ordinaryBrier: ordinary / n,
    nativeBrier: native / n,
    replayBrier: replay / n,
    meanExcessReplayBrier: excess / n,
    difference: (ordinary - native) / n,
    nativeMissingTokens: missing,
  };
}

export function analyzeAlaLvDyad(
  dyad: string,
  roles: readonly [AlaLvRoleFolds, AlaLvRoleFolds],
  inventory: readonly string[],
): AlaLvDyadResult {
  const roleIds = roles.map((folds) => folds.role).sort();
  if (roleIds[0] !== 'baby-a' || roleIds[1] !== 'baby-b') fail(`dyad ${dyad} folds do not cover both receiver roles`);
  const results = roles.map((folds) => analyzeAlaLvRole(folds, inventory));
  return {
    dyad,
    roles: results,
    primary: (results[0]!.difference + results[1]!.difference) / 2,
  };
}

export interface AlaLvCohortInterval {
  readonly cohort: string;
  readonly n: number;
  readonly mean: number;
  readonly sd: number;
  readonly se: number;
  readonly coverage: number;
  readonly lower: number | null;
  readonly upper: number | null;
  /** True when n < 2 or variance is zero: reported, never a finding. */
  readonly degenerate: string | null;
}

/** Two-sided Student-t interval across dyad primaries (B4 n=1: degenerate by rule). */
export function alaLvCohortInterval(cohort: string, primaries: readonly number[], coverage: number): AlaLvCohortInterval {
  if (!(coverage > 0 && coverage < 1)) fail('coverage must be strictly between 0 and 1');
  const stats = summarize([...primaries]);
  const base = { cohort, n: stats.n, mean: stats.mean, sd: stats.sd, se: stats.n > 0 ? stats.sd / Math.sqrt(stats.n) : NaN, coverage };
  if (stats.n < 2) {
    return { ...base, lower: null, upper: null, degenerate: `n=${stats.n}: no variance estimate` };
  }
  if (!(stats.sd > 0)) {
    return { ...base, lower: null, upper: null, degenerate: 'zero variance across dyads' };
  }
  const critical = studentTQuantile(1 - (1 - coverage) / 2, stats.n - 1);
  const halfWidth = critical * (stats.sd / Math.sqrt(stats.n));
  return { ...base, lower: stats.mean - halfWidth, upper: stats.mean + halfWidth, degenerate: null };
}
