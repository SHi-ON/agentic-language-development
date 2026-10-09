/**
 * ALA-LV workload resolution (P3).
 *
 * Executable counts come ONLY from the frozen protocols/ala-lv-study.v1.json
 * task partitions — never silent resizes, never a switch to production
 * transport, no small-fixture study path. Development and calibration
 * attempts are full dyads charged against the amendment budget.
 */
import { readFileSync } from 'node:fs';

export interface AlaStudyCounts {
  readonly trainingCases: number;
  readonly validationFitCases: number;
  readonly validationSelectionCases: number;
  readonly withinSupportTestCases: number;
}

export function alaStudyCounts(studyProtocolPath: string): AlaStudyCounts {
  const study = JSON.parse(readFileSync(studyProtocolPath, 'utf8')) as {
    task?: { partitions?: Record<string, { cases?: unknown }> };
  };
  const at = (partition: string): number => {
    const value = study.task?.partitions?.[partition]?.cases;
    if (typeof value !== 'number' || !Number.isInteger(value) || value < 1) {
      throw new Error(`ALA workload: study protocol has no positive ${partition} cases`);
    }
    return value;
  };
  return {
    trainingCases: at('training'),
    validationFitCases: at('validationFit'),
    validationSelectionCases: at('validationSelection'),
    withinSupportTestCases: at('withinSupportTest'),
  };
}

export interface AlaCollectionWorkload {
  readonly profile: 'prototype';
  readonly trainingTurns: number;
  readonly validationFitCases: number;
  readonly validationSelectionCases: number;
  readonly withinSupportTestCases: number;
}

export function alaPrototypeWorkload(studyCounts: AlaStudyCounts): AlaCollectionWorkload {
  return {
    profile: 'prototype',
    trainingTurns: studyCounts.trainingCases,
    validationFitCases: studyCounts.validationFitCases,
    validationSelectionCases: studyCounts.validationSelectionCases,
    withinSupportTestCases: studyCounts.withinSupportTestCases,
  };
}
