#!/usr/bin/env node
/**
 * Strict ALA-LV protocol checker (P3).
 *
 * Validates protocols/ala-lv-study.v1.json and
 * protocols/ala-lv-analysis.v1.json against their JSON schemas using a
 * strict-subset interpreter (type/required/const/enum/properties/items/
 * additionalProperties/minItems/maxItems/minLength), then verifies the
 * exact-byte binding against protocols/ala-lv-execution-manifest.v1.json
 * when it exists. Any deviation fails closed with a named error.
 */
import { createHash, timingSafeEqual } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';

const ROOT = new URL('../', import.meta.url);

function fail(code, message) {
  console.error(`FAIL [${code}]: ${message}`);
  process.exit(2);
}

function deepEqual(left, right) {
  if (Object.is(left, right)) return true;
  if (typeof left !== typeof right) return false;
  if (left === null || right === null) return left === right;
  if (Array.isArray(left) || Array.isArray(right)) {
    if (!Array.isArray(left) || !Array.isArray(right)) return false;
    if (left.length !== right.length) return false;
    return left.every((entry, index) => deepEqual(entry, right[index]));
  }
  if (typeof left === 'object') {
    const lkeys = Object.keys(left).sort();
    const rkeys = Object.keys(right).sort();
    if (!deepEqual(lkeys, rkeys)) return false;
    return lkeys.every((key) => deepEqual(left[key], right[key]));
  }
  return false;
}

function checkType(value, expected, path) {
  const actual = Array.isArray(value) ? 'array' : value === null ? 'null' : typeof value;
  const want = Array.isArray(expected) ? expected : [expected];
  if (!want.includes(actual)) {
    fail('SCHEMA-TYPE', `${path}: expected ${want.join('/')} but found ${actual}`);
  }
}

function validate(value, schema, path) {
  if (schema === true) return;
  if (schema === false) fail('SCHEMA-FALSE', `${path}: value forbidden by schema`);
  if (schema.type !== undefined) checkType(value, schema.type, path);
  if (schema.const !== undefined && !deepEqual(value, schema.const)) {
    fail('SCHEMA-CONST', `${path}: value does not match the frozen constant`);
  }
  if (schema.enum !== undefined && !schema.enum.some((entry) => deepEqual(value, entry))) {
    fail('SCHEMA-ENUM', `${path}: value is not an allowed enum member`);
  }
  if (typeof value === 'string') {
    if (schema.minLength !== undefined && value.length < schema.minLength) {
      fail('SCHEMA-MIN-LENGTH', `${path}: string shorter than ${schema.minLength}`);
    }
  }
  if (Array.isArray(value)) {
    if (schema.minItems !== undefined && value.length < schema.minItems) {
      fail('SCHEMA-MIN-ITEMS', `${path}: fewer than ${schema.minItems} items`);
    }
    if (schema.maxItems !== undefined && value.length > schema.maxItems) {
      fail('SCHEMA-MAX-ITEMS', `${path}: more than ${schema.maxItems} items`);
    }
    if (schema.items !== undefined) {
      value.forEach((entry, index) => validate(entry, schema.items, `${path}[${index}]`));
    }
  }
  if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
    for (const key of schema.required ?? []) {
      if (!(key in value)) fail('SCHEMA-REQUIRED', `${path}: missing required key '${key}'`);
    }
    const props = schema.properties ?? {};
    for (const [key, entry] of Object.entries(value)) {
      if (key in props) {
        validate(entry, props[key], `${path}.${key}`);
      } else if (schema.additionalProperties === false) {
        fail('SCHEMA-ADDITIONAL', `${path}: forbidden additional key '${key}'`);
      }
    }
  }
}

function sha256Hex(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function checkBinding(protocolPath, manifest) {
  const bytes = readFileSync(new URL(protocolPath, ROOT));
  const actual = `sha256:${sha256Hex(bytes)}`;
  const bound = manifest?.protocols?.[protocolPath]?.sha256;
  if (bound === undefined) {
    console.log(`NOTE: ${protocolPath} has no execution-manifest binding yet (pre-P3-commit).`);
    return;
  }
  const left = Buffer.from(actual);
  const right = Buffer.from(bound);
  if (left.length !== right.length || !timingSafeEqual(left, right)) {
    fail('BINDING-MISMATCH', `${protocolPath}: live bytes ${actual} do not match bound ${bound}`);
  }
  console.log(`bound OK: ${protocolPath} ${actual}`);
}

const pairs = [
  ['protocols/ala-lv-study.v1.json', 'protocols/schemas/ala-lv-study.schema.json'],
  ['protocols/ala-lv-analysis.v1.json', 'protocols/schemas/ala-lv-analysis.schema.json'],
];
for (const [docPath, schemaPath] of pairs) {
  if (!existsSync(new URL(docPath, ROOT))) fail('MISSING-PROTOCOL', docPath);
  const doc = JSON.parse(readFileSync(new URL(docPath, ROOT), 'utf8'));
  const schema = JSON.parse(readFileSync(new URL(schemaPath, ROOT), 'utf8'));
  validate(doc, schema, docPath);
  console.log(`schema OK: ${docPath}`);
}

const manifestPath = 'protocols/ala-lv-execution-manifest.v1.json';
const manifest = existsSync(new URL(manifestPath, ROOT))
  ? JSON.parse(readFileSync(new URL(manifestPath, ROOT), 'utf8'))
  : undefined;
for (const [docPath] of pairs) checkBinding(docPath, manifest);
console.log('ALA-LV protocol check passed.');
