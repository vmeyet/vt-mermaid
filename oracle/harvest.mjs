// Harvests Mermaid inputs from a mermaid-js/mermaid checkout into corpus/<kind>/, one file per unique input.
// The kind is the one the pinned mermaid detects, so the corpus and the oracle never disagree on it.
// Usage: node oracle/harvest.mjs <mermaid-checkout> <corpus-dir>
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { corpusKind, looksLikeDiagram } from './mermaid.mjs';

const LONGEST_SLUG = 80;
const TEST_TITLE = /\b(describe|it|test|bench)(\.\w+)*\(\s*$/;

const [checkout, corpus] = process.argv.slice(2);
if (!checkout || !corpus) {
  console.error('usage: node oracle/harvest.mjs <mermaid-checkout> <corpus-dir>');
  process.exit(2);
}

const found = [...specCases(), ...e2eCases(), ...docsCases()].filter(({ text }) => text.trim());
const cases = await withKinds(dedupe(found));
writeCorpus(cases);
for (const [kind, count] of countByKind(cases)) console.log(`${kind}\t${count}`);
console.log(`total\t${cases.length}\t(${found.length} before dedupe)`);

// A spec literal is kept when mermaid detects a diagram in it and it is more than a lone keyword.
function specCases() {
  const files = walk(join(checkout, 'packages')).filter((path) => /\.(spec|test)\.[jt]s$/.test(path));
  return files.flatMap((path) =>
    stringLiterals(readFileSync(path, 'utf8'))
      .map((literal) => caseOf('spec', path, literal))
      .filter(({ text }) => /\s\S/.test(text.trim()) && looksLikeDiagram(text)),
  );
}

function e2eCases() {
  const files = walk(join(checkout, 'e2e')).filter((path) => path.endsWith('.mmd'));
  return files.map((path) => caseOf('e2e', path, { text: readFileSync(path, 'utf8'), line: null }));
}

function docsCases() {
  const files = walk(join(checkout, 'packages/mermaid/src/docs')).filter((path) => path.endsWith('.md'));
  return files.flatMap((path) => mermaidFences(readFileSync(path, 'utf8')).map((fence) => caseOf('docs', path, fence)));
}

function caseOf(origin, path, { text, line }) {
  const source = relative(checkout, path);
  return { text: normalize(text), origin, source, line, location: line ? `${source}:${line}` : source };
}

async function withKinds(cases) {
  const kinds = [];
  for (const { text } of cases) kinds.push(await corpusKind(text));
  return cases.map((item, at) => ({ ...item, kind: kinds[at] }));
}

function dedupe(found) {
  const byText = new Map();
  for (const item of found) {
    const seen = byText.get(item.text);
    if (seen) seen.sources.push(item.location);
    else byText.set(item.text, { ...item, sources: [item.location] });
  }
  return [...byText.values()];
}

function writeCorpus(cases) {
  const named = withFileNames(cases);
  for (const entry of readdirSync(corpus, { withFileTypes: true })) {
    if (entry.isDirectory()) rmSync(join(corpus, entry.name), { recursive: true });
  }
  for (const { kind, file, text } of named) {
    mkdirSync(join(corpus, kind), { recursive: true });
    writeFileSync(join(corpus, kind, file), text);
  }
  const sources = Object.fromEntries(named.map(({ kind, file, sources }) => [`${kind}/${file}`, sources]).sort(([a], [b]) => a.localeCompare(b)));
  writeFileSync(join(corpus, 'sources.json'), `${JSON.stringify(sources, null, 2)}\n`);
}

function withFileNames(cases) {
  const taken = new Set();
  return cases.map((item) => {
    const base = `${item.origin}--${slug(item.source)}${item.line ? `--L${item.line}` : ''}`;
    let file = `${base}.mmd`;
    for (let n = 2; taken.has(`${item.kind}/${file}`); n++) file = `${base}-${n}.mmd`;
    taken.add(`${item.kind}/${file}`);
    return { ...item, file };
  });
}

function countByKind(cases) {
  const counts = new Map();
  for (const { kind } of cases) counts.set(kind, (counts.get(kind) ?? 0) + 1);
  return [...counts].sort(([a], [b]) => a.localeCompare(b));
}

function normalize(text) {
  const lines = text.replace(/\r\n?/g, '\n').split('\n');
  const indent = Math.min(...lines.filter((line) => line.trim()).map(indentOf));
  const dedented = lines.map((line) => line.slice(Math.min(indent, indentOf(line))).trimEnd());
  return `${dedented.join('\n').replace(/^\n+/, '').trimEnd()}\n`;
}

function indentOf(line) {
  return line.match(/^[ \t]*/)[0].length;
}

function mermaidFences(markdown) {
  const fences = [];
  const lines = markdown.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const open = lines[i].match(/^(\s*)(`{3,}|~{3,})\s*mermaid(-example|-nocode)?\s*$/);
    if (!open) continue;
    const close = new RegExp(`^\\s*${open[2]}\\s*$`);
    let end = i + 1;
    while (end < lines.length && !close.test(lines[end])) end++;
    if (end === lines.length) break;
    fences.push({ text: lines.slice(i + 1, end).join('\n'), line: i + 2 });
    i = end;
  }
  return fences;
}

// Scans JS/TS source for string and template literals without interpolation, skipping test titles;
// regex literals are not recognised.
function stringLiterals(source) {
  const literals = [];
  let line = 1;
  for (let i = 0; i < source.length; i++) {
    const c = source[i];
    if (c === '\n') line++;
    else if (c === '/' && source[i + 1] === '/') {
      i = source.indexOf('\n', i) - 1;
      if (i < 0) break;
    } else if (c === '/' && source[i + 1] === '*') {
      const end = source.indexOf('*/', i + 2);
      if (end < 0) break;
      line += countNewlines(source.slice(i, end));
      i = end + 1;
    } else if (c === "'" || c === '"' || c === '`') {
      const { end, raw, interpolated } = readLiteral(source, i, c);
      if (!interpolated && !TEST_TITLE.test(source.slice(Math.max(0, i - 40), i))) literals.push({ text: unescape(raw), line });
      line += countNewlines(source.slice(i, end));
      i = end;
    }
  }
  return literals;
}

function readLiteral(source, start, quote) {
  let interpolated = false;
  for (let i = start + 1; i < source.length; i++) {
    const c = source[i];
    if (c === '\\') i++;
    else if (c === quote) return { end: i, raw: source.slice(start + 1, i), interpolated };
    else if (quote === '`' && c === '$' && source[i + 1] === '{') interpolated = true;
    else if (quote !== '`' && c === '\n') return { end: i, raw: '', interpolated: true };
  }
  return { end: source.length, raw: '', interpolated: true };
}

function unescape(raw) {
  const simple = { n: '\n', t: '\t', r: '\r', b: '\b', f: '\f', v: '\v', 0: '\0', '\n': '' };
  return raw.replace(/\\(u\{[0-9a-fA-F]+\}|u[0-9a-fA-F]{4}|x[0-9a-fA-F]{2}|[\s\S])/g, (_, code) => {
    if (code.length > 1) return String.fromCodePoint(parseInt(code.replace(/^[ux]\{?|\}$/g, ''), 16));
    return simple[code] ?? code;
  });
}

function countNewlines(text) {
  return text.split('\n').length - 1;
}

function slug(path) {
  const short = path
    .replace(/^packages\/mermaid\/src\/(diagrams\/|docs\/)?/, '')
    .replace(/^e2e\//, '')
    .replace(/\.(spec|test)?\.?[jt]s$|\.mmd$|\.md$/, '');
  return short.replace(/[^A-Za-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, LONGEST_SLUG);
}

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true, recursive: true })
    .filter((entry) => entry.isFile() && !entry.parentPath.includes('node_modules'))
    .map((entry) => join(entry.parentPath, entry.name))
    .sort();
}
