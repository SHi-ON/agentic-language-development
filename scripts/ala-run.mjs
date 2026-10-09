#!/usr/bin/env node
/** Thin launcher: paper:run -> tsx ALA entry (keeps .mjs for pnpm scripts). */
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
execFileSync('pnpm', ['tsx', join(here, 'ala-run-entry.ts'), ...process.argv.slice(2)], { stdio: 'inherit' });
