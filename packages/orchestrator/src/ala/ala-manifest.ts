/**
 * ALA-LV execution manifest (P3).
 *
 * Binds the frozen protocol bytes, the exact execution source closure
 * (worktree bytes of every file that can influence a run),
 * the dependency closure, and the workload. The runner verifies the
 * manifest at startup and fails closed on any drift: stale source,
 * edited protocols, or changed dependencies never run silently.
 *
 * Docs/test dirt outside the closure (including the preserved
 * worktree edits) does not affect execution identity.
 */
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

export const ALA_MANIFEST_PATH = 'protocols/ala-lv-execution-manifest.v1.json';
export const ALA_STUDY_PATH = 'protocols/ala-lv-study.v1.json';
export const ALA_ANALYSIS_PATH = 'protocols/ala-lv-analysis.v1.json';
export const ALA_AMENDMENT_PATH = 'protocols/ala-lv-scope-and-resource-amendment.v1.json';

export interface AlaExecutionManifest {
  readonly manifestVersion: 'ala-lv-execution-manifest/v1';
  readonly studyId: 'ALA-LV';
  readonly protocolCommit: string;
  readonly sourceTree: string;
  readonly createdAtUtc: string;
  readonly protocols: Readonly<Record<string, { readonly sha256: string; readonly bytes: number }>>;
  readonly executionClosure: { readonly fileCount: number; readonly sha256: string };
  readonly closureFiles: readonly string[];
  readonly dependencies: {
    readonly node: string;
    readonly lockfileSha256: string;
    readonly packageJsonSha256: string;
  };
  readonly workload: {
    readonly trainingCases: number;
    readonly validationFitCases: number;
    readonly validationSelectionCases: number;
    readonly withinSupportTestCases: number;
  };
  readonly seedPolicy: { readonly root: string; readonly studyId: string };
  readonly seedManifestRef: string;
}

function fail(message: string): never {
  throw new Error(`ALA manifest: ${message}`);
}

function sha256Hex(data: Buffer | string): string {
  return createHash('sha256').update(data).digest('hex');
}

function walkFiles(root: string, dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(join(root, dir), { withFileTypes: true })) {
    const rel = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'dist' || entry.name === 'node_modules') continue;
      out.push(...walkFiles(root, rel));
    } else if (entry.isFile()) {
      out.push(rel);
    }
  }
  return out;
}

/**
 * Execution closure: every worktree file that can influence a run.
 * Packages sources, top-level runner scripts, protocol contracts,
 * toolchain configs, and the integrity auditor sources.
 */
export function alaExecutionClosureFiles(root: string): string[] {
  const files: string[] = [];
  for (const dir of ['packages', 'tools/integrity-auditor']) {
    if (!existsSync(join(root, dir))) continue;
    for (const rel of walkFiles(root, dir)) {
      if (rel.includes(`${'__tests__'}/`)) continue;
      if (/\.test\.(ts|js|mjs)$/u.test(rel)) continue;
      if (!/\.(ts|mjs|js|rs|toml|json)$/u.test(rel)) continue;
      files.push(rel);
    }
  }
  for (const rel of [
    'package.json',
    'pnpm-lock.yaml',
    'pnpm-workspace.yaml',
    'tsconfig.base.json',
    'tsconfig.json',
    'vitest.config.ts',
    ALA_STUDY_PATH,
    ALA_ANALYSIS_PATH,
    ALA_AMENDMENT_PATH,
    'protocols/schemas/ala-lv-study.schema.json',
    'protocols/schemas/ala-lv-analysis.schema.json',
  ]) {
    if (existsSync(join(root, rel))) files.push(rel);
  }
  for (const entry of readdirSync(join(root, 'scripts'), { withFileTypes: true })) {
    if (entry.isFile() && /\.(mjs|ts)$/u.test(entry.name)) files.push(join('scripts', entry.name));
  }
  return [...new Set(files)].sort();
}

export function alaClosureDigest(root: string, files: readonly string[]): string {
  const hash = createHash('sha256');
  for (const rel of files) {
    hash.update(rel);
    hash.update(Buffer.from([0]));
    hash.update(readFileSync(join(root, rel)));
    hash.update(Buffer.from([0]));
  }
  return `sha256:${hash.digest('hex')}`;
}

function git(root: string, ...args: string[]): string {
  return execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
}

export function buildAlaExecutionManifest(root: string): AlaExecutionManifest {
  for (const path of [ALA_STUDY_PATH, ALA_ANALYSIS_PATH, ALA_AMENDMENT_PATH]) {
    if (!existsSync(join(root, path))) fail(`missing protocol ${path}`);
  }
  const protocols: Record<string, { sha256: string; bytes: number }> = {};
  for (const path of [ALA_STUDY_PATH, ALA_ANALYSIS_PATH, ALA_AMENDMENT_PATH]) {
    const bytes = readFileSync(join(root, path));
    protocols[path] = { sha256: `sha256:${sha256Hex(bytes)}`, bytes: bytes.length };
  }
  const study = JSON.parse(readFileSync(join(root, ALA_STUDY_PATH), 'utf8')) as {
    task: { partitions: Record<string, { cases: number }> };
  };
  const closureFiles = alaExecutionClosureFiles(root);
  return {
    manifestVersion: 'ala-lv-execution-manifest/v1',
    studyId: 'ALA-LV',
    // The commit that last changed the frozen protocol bytes (stable
    // across manifest rebuilds; a protocol change requires a new version).
    protocolCommit: git(root, 'log', '-1', '--format=%H', '--', ALA_STUDY_PATH, ALA_ANALYSIS_PATH),
    sourceTree: git(root, 'rev-parse', 'HEAD^{tree}'),
    createdAtUtc: new Date().toISOString(),
    protocols,
    executionClosure: { fileCount: closureFiles.length, sha256: alaClosureDigest(root, closureFiles) },
    closureFiles,
    dependencies: {
      node: process.version,
      lockfileSha256: `sha256:${sha256Hex(readFileSync(join(root, 'pnpm-lock.yaml')))}`,
      packageJsonSha256: `sha256:${sha256Hex(readFileSync(join(root, 'package.json')))}`,
    },
    workload: {
      trainingCases: study.task.partitions['training']!.cases,
      validationFitCases: study.task.partitions['validationFit']!.cases,
      validationSelectionCases: study.task.partitions['validationSelection']!.cases,
      withinSupportTestCases: study.task.partitions['withinSupportTest']!.cases,
    },
    seedPolicy: { root: 'ala-ledger-value-v1', studyId: 'ALA-LV' },
    seedManifestRef: 'protocols/ala-lv-seed-manifest.v1.json (frozen at P6 admission)',
  };
}

/** Recompute every binding and fail closed on any drift. */
export function verifyAlaExecutionManifest(root: string, manifestPath: string): AlaExecutionManifest {
  const manifest = JSON.parse(readFileSync(join(root, manifestPath), 'utf8')) as AlaExecutionManifest;
  if (manifest.manifestVersion !== 'ala-lv-execution-manifest/v1') fail('manifest version mismatch');
  for (const [path, bound] of Object.entries(manifest.protocols)) {
    const live = readFileSync(join(root, path));
    if (`sha256:${sha256Hex(live)}` !== bound.sha256 || live.length !== bound.bytes) {
      fail(`protocol bytes drifted: ${path}`);
    }
  }
  const lockfile = readFileSync(join(root, 'pnpm-lock.yaml'));
  if (`sha256:${sha256Hex(lockfile)}` !== manifest.dependencies.lockfileSha256) {
    fail('lockfile drifted');
  }
  const pkg = readFileSync(join(root, 'package.json'));
  if (`sha256:${sha256Hex(pkg)}` !== manifest.dependencies.packageJsonSha256) {
    fail('package.json drifted');
  }
  const liveFiles = alaExecutionClosureFiles(root);
  const boundFiles = [...manifest.closureFiles].sort();
  if (JSON.stringify(liveFiles) !== JSON.stringify(boundFiles)) {
    const liveSet = new Set(liveFiles);
    const missing = boundFiles.filter((f) => !liveSet.has(f));
    const extra = liveFiles.filter((f) => !boundFiles.includes(f));
    fail(`execution closure file set drifted (missing: ${missing.join(', ') || 'none'}; extra: ${extra.join(', ') || 'none'})`);
  }
  if (alaClosureDigest(root, liveFiles) !== manifest.executionClosure.sha256) {
    fail('execution closure bytes drifted');
  }
  return manifest;
}
