import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import {
  buildAlaExecutionManifest,
  verifyAlaExecutionManifest,
} from '../../src/ala/ala-manifest.js';

const temporary: string[] = [];

afterEach(() => {
  for (const dir of temporary.splice(0)) rmSync(dir, { recursive: true, force: true });
});

function fixtureRoot(): string {
  const dir = `${tmpdir()}/ald-ala-manifest-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
  temporary.push(dir);
  const study = {
    task: {
      partitions: {
        training: { cases: 3000 },
        validationFit: { cases: 240 },
        validationSelection: { cases: 240 },
        withinSupportTest: { cases: 240 },
      },
    },
  };
  mkdirSync(join(dir, 'protocols'), { recursive: true });
  mkdirSync(join(dir, 'scripts'), { recursive: true });
  mkdirSync(join(dir, 'packages/demo/src'), { recursive: true });
  writeFileSync(join(dir, 'protocols/ala-lv-study.v1.json'), JSON.stringify(study));
  writeFileSync(join(dir, 'protocols/ala-lv-analysis.v1.json'), JSON.stringify({ studyId: 'ALA-LV' }));
  writeFileSync(join(dir, 'protocols/ala-lv-scope-and-resource-amendment.v1.json'), JSON.stringify({ studyIdentity: 'ALA-LV' }));
  writeFileSync(join(dir, 'packages/demo/src/index.ts'), 'export const x = 1;\n');
  writeFileSync(join(dir, 'package.json'), JSON.stringify({ name: 'fixture', version: '0.0.1' }));
  writeFileSync(join(dir, 'pnpm-lock.yaml'), 'lockfileVersion: 9.0\n');
  execFileSync('git', ['init', '-q'], { cwd: dir });
  execFileSync('git', ['-c', 'user.email=t@t', '-c', 'user.name=t', 'add', '-A'], { cwd: dir });
  execFileSync('git', ['-c', 'user.email=t@t', '-c', 'user.name=t', 'commit', '-qm', 'fixture'], { cwd: dir });
  return dir;
}

describe('ALA execution manifest', () => {
  it('builds and verifies a fresh manifest', () => {
    const root = fixtureRoot();
    const manifest = buildAlaExecutionManifest(root);
    expect(manifest.studyId).toBe('ALA-LV');
    expect(manifest.workload.trainingCases).toBe(3000);
    expect(manifest.executionClosure.fileCount).toBeGreaterThan(3);
    writeFileSync(join(root, 'protocols/ala-lv-execution-manifest.v1.json'), JSON.stringify(manifest));
    const verified = verifyAlaExecutionManifest(root, 'protocols/ala-lv-execution-manifest.v1.json');
    expect(verified.protocolCommit).toBe(manifest.protocolCommit);
  });

  it('fails closed on protocol drift', () => {
    const root = fixtureRoot();
    const manifest = buildAlaExecutionManifest(root);
    writeFileSync(join(root, 'protocols/ala-lv-execution-manifest.v1.json'), JSON.stringify(manifest));
    writeFileSync(join(root, 'protocols/ala-lv-study.v1.json'), JSON.stringify({ task: 'tampered' }));
    expect(() => verifyAlaExecutionManifest(root, 'protocols/ala-lv-execution-manifest.v1.json')).toThrowError(
      /protocol bytes drifted/,
    );
  });

  it('fails closed on execution-closure drift', () => {
    const root = fixtureRoot();
    const manifest = buildAlaExecutionManifest(root);
    writeFileSync(join(root, 'protocols/ala-lv-execution-manifest.v1.json'), JSON.stringify(manifest));
    writeFileSync(join(root, 'packages/demo/src/index.ts'), 'export const x = 2;\n');
    expect(() => verifyAlaExecutionManifest(root, 'protocols/ala-lv-execution-manifest.v1.json')).toThrowError(
      /closure bytes drifted/,
    );
  });

  it('fails closed on added closure files', () => {
    const root = fixtureRoot();
    const manifest = buildAlaExecutionManifest(root);
    writeFileSync(join(root, 'protocols/ala-lv-execution-manifest.v1.json'), JSON.stringify(manifest));
    writeFileSync(join(root, 'packages/demo/src/extra.ts'), 'export const y = 1;\n');
    expect(() => verifyAlaExecutionManifest(root, 'protocols/ala-lv-execution-manifest.v1.json')).toThrowError(
      /file set drifted/,
    );
  });

  it('fails closed on lockfile drift', () => {
    const root = fixtureRoot();
    const manifest = buildAlaExecutionManifest(root);
    writeFileSync(join(root, 'protocols/ala-lv-execution-manifest.v1.json'), JSON.stringify(manifest));
    writeFileSync(join(root, 'pnpm-lock.yaml'), 'lockfileVersion: 9.0\n# tampered\n');
    expect(() => verifyAlaExecutionManifest(root, 'protocols/ala-lv-execution-manifest.v1.json')).toThrowError(
      /lockfile drifted/,
    );
  });

  it('records protocol bytes faithfully', () => {
    const root = fixtureRoot();
    const manifest = buildAlaExecutionManifest(root);
    const studyBytes = readFileSync(join(root, 'protocols/ala-lv-study.v1.json'));
    expect(manifest.protocols['protocols/ala-lv-study.v1.json']!.bytes).toBe(studyBytes.length);
  });

  it('records an explicit VOID seed ref when no seed manifest was frozen', () => {
    const root = fixtureRoot();
    const manifest = buildAlaExecutionManifest(root);
    expect(manifest.seedManifestRef).toContain('VOID');
    expect(manifest.seedManifestRef).not.toContain('sha256:');
    writeFileSync(join(root, 'protocols/ala-lv-execution-manifest.v1.json'), JSON.stringify(manifest));
    expect(() => verifyAlaExecutionManifest(root, 'protocols/ala-lv-execution-manifest.v1.json')).not.toThrow();
  });

  it('binds a present seed manifest by digest and fails on tamper', () => {
    const root = fixtureRoot();
    writeFileSync(join(root, 'protocols/ala-lv-seed-manifest.v1.json'), JSON.stringify({ seeds: [1] }));
    const manifest = buildAlaExecutionManifest(root);
    expect(manifest.seedManifestRef).toContain('sha256:');
    writeFileSync(join(root, 'protocols/ala-lv-execution-manifest.v1.json'), JSON.stringify(manifest));
    expect(() => verifyAlaExecutionManifest(root, 'protocols/ala-lv-execution-manifest.v1.json')).not.toThrow();
    writeFileSync(join(root, 'protocols/ala-lv-seed-manifest.v1.json'), JSON.stringify({ seeds: [2] }));
    expect(() => verifyAlaExecutionManifest(root, 'protocols/ala-lv-execution-manifest.v1.json')).toThrowError(
      /seed manifest state changed/,
    );
  });

  it('fails closed when a seed manifest appears after a VOID build', () => {
    const root = fixtureRoot();
    const manifest = buildAlaExecutionManifest(root);
    writeFileSync(join(root, 'protocols/ala-lv-execution-manifest.v1.json'), JSON.stringify(manifest));
    writeFileSync(join(root, 'protocols/ala-lv-seed-manifest.v1.json'), JSON.stringify({ seeds: [1] }));
    expect(() => verifyAlaExecutionManifest(root, 'protocols/ala-lv-execution-manifest.v1.json')).toThrowError(
      /seed manifest state changed/,
    );
  });

  it('fails closed on hand-edited workload', () => {
    const root = fixtureRoot();
    const manifest = buildAlaExecutionManifest(root);
    const edited = { ...manifest, workload: { ...manifest.workload, trainingCases: 9999 } };
    writeFileSync(join(root, 'protocols/ala-lv-execution-manifest.v1.json'), JSON.stringify(edited));
    expect(() => verifyAlaExecutionManifest(root, 'protocols/ala-lv-execution-manifest.v1.json')).toThrowError(
      /workload drifted/,
    );
  });

  it('logs the amendment commit and fails closed on commit drift', () => {
    const root = fixtureRoot();
    const manifest = buildAlaExecutionManifest(root);
    const head = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
    expect(manifest.protocolCommit).toBe(head);
    expect(manifest.amendmentCommit).toBe(head);
    const edited = { ...manifest, amendmentCommit: '0'.repeat(40) };
    writeFileSync(join(root, 'protocols/ala-lv-execution-manifest.v1.json'), JSON.stringify(edited));
    expect(() => verifyAlaExecutionManifest(root, 'protocols/ala-lv-execution-manifest.v1.json')).toThrowError(
      /amendmentCommit drifted/,
    );
  });
});
