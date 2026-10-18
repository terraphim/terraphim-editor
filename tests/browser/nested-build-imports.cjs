// Parse-only AST extraction using existing Acorn/Terser tooling. Never execute
// input JavaScript, install dependencies, or fall back to lexical import regexes.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const REQUIRED_ACORN_VERSION = '8.14.0';

function checkParserIdentity(parser) {
  if (parser.version !== REQUIRED_ACORN_VERSION) {
    const error = new Error('parser-version-mismatch');
    error.code = 'parser-version-mismatch';
    // Report only a bounded version identifier, never arbitrary module data.
    error.observed = typeof parser.version === 'string' &&
      /^[0-9]+\.[0-9]+\.[0-9]+(?:-[a-zA-Z0-9.-]+)?$/.test(parser.version) && parser.version.length <= 64
      ? parser.version : 'unknown';
    throw error;
  }
}

function existingAcorn() {
  try { return require('acorn'); } catch (_) { /* Try installed build tooling. */ }
  for (const directory of (process.env.PATH || '').split(path.delimiter)) {
    if (!directory) continue;
    const executable = path.join(directory, 'terser');
    try {
      const real = fs.realpathSync(executable);
      try { return createRequire(real)('acorn'); } catch (_) { /* A pnpm shell shim is not the package entry. */ }
      if (fs.statSync(real).size > 65536) continue;
      // Resolve only the literal Terser package target in its installed pnpm
      // shim; never execute/eval the shim or treat it as application JS.
      const shim = fs.readFileSync(real, 'utf8');
      const target = shim.match(/\$basedir\/([^"\r\n]+\/terser\/bin\/terser)/);
      if (target) {
        const entry = fs.realpathSync(path.resolve(path.dirname(real), target[1]));
        return createRequire(entry)('acorn');
      }
    } catch (_) { /* No install/download or guessed lexical fallback. */ }
  }
  throw new Error('parser-unavailable');
}

function literal(node) {
  if (node && node.type === 'Literal' && typeof node.value === 'string') return node.value;
  if (node && node.type === 'TemplateLiteral' && node.expressions.length === 0) {
    return node.quasis[0].value.cooked;
  }
  return null;
}

function extract(acorn, source) {
  const ast = acorn.parse(source, {
    ecmaVersion: 'latest', sourceType: 'module', allowAwaitOutsideFunction: true,
  });
  const modules = [], wasm = [], pending = [ast];
  while (pending.length) {
    const node = pending.pop();
    if (['ImportDeclaration', 'ExportNamedDeclaration', 'ExportAllDeclaration', 'ImportExpression'].includes(node.type)) {
      const value = literal(node.source);
      if (value !== null) modules.push(value);
    }
    if (node.type === 'Property' && !node.computed &&
        (node.key.type === 'Identifier' ? node.key.name : literal(node.key)) === 'module_or_path') {
      const value = literal(node.value);
      if (value !== null) wasm.push(value);
    }
    for (const child of Object.values(node)) {
      if (Array.isArray(child)) {
        for (const item of child) if (item && typeof item === 'object') pending.push(item);
      } else if (child && typeof child === 'object') pending.push(child);
    }
  }
  return { modules, wasm };
}

try {
  const chunks = [], buffer = Buffer.alloc(65536);
  let total = 0, count;
  while ((count = fs.readSync(0, buffer, 0, buffer.length, null)) > 0) {
    total += count;
    if (total > 32 * 1024 * 1024) throw new Error('input-limit');
    chunks.push(Buffer.from(buffer.subarray(0, count)));
  }
  const input = Buffer.concat(chunks).toString('utf8');
  const request = JSON.parse(input);
  if (!Array.isArray(request.sources) || request.sources.some(source => typeof source !== 'string')) {
    throw new Error('invalid-input');
  }
  const acorn = existingAcorn();
  checkParserIdentity(acorn);
  process.stdout.write(JSON.stringify({ parser: 'acorn', version: acorn.version,
    references: request.sources.map(source => extract(acorn, source)) }));
} catch (error) {
  // No source text, environment or package paths in diagnostics.
  if (error.code === 'parser-version-mismatch') {
    process.stderr.write(`AST extraction FAIL: parser-version-mismatch required=${REQUIRED_ACORN_VERSION} observed=${error.observed}; no install, upgrade or lexical fallback\n`);
  } else {
    process.stderr.write('AST extraction FAIL: parser unavailable, invalid input or JavaScript syntax; no lexical fallback\n');
  }
  process.exitCode = 1;
}
