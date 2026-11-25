// Parses every corpus input with mermaid and writes what it accepts (level A) and, for flowchart and sequence,
// the diagram db (level B) to oracle/results/. It only parses: nothing is rendered, fetched or written elsewhere.
// Usage: node oracle/oracle.mjs <corpus-dir> <results-dir>
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { detectedType, mermaid, version } from './mermaid.mjs';

const LONGEST_ERROR = 300;

const [corpus, results] = process.argv.slice(2);
if (!corpus || !results) {
  console.error('usage: node oracle/oracle.mjs <corpus-dir> <results-dir>');
  process.exit(2);
}

const MEANINGS = { flowchart: flowchartMeaning, sequence: sequenceMeaning };
const parse = {};
const meanings = Object.fromEntries(Object.keys(MEANINGS).map((kind) => [kind, {}]));
for (const path of corpusFiles()) {
  const text = readFileSync(join(corpus, path), 'utf8');
  const result = await parseCase(text);
  const kind = path.split('/')[0];
  parse[path] = result;
  if (result.accepted && MEANINGS[kind]) meanings[kind][path] = await meaningOf(text, MEANINGS[kind]);
}

mkdirSync(results, { recursive: true });
writeJson('parse.json', { mermaid: version, cases: parse });
for (const [kind, cases] of Object.entries(meanings)) writeJson(`${kind}.json`, { mermaid: version, cases });
console.log(`${Object.keys(parse).length} cases, ${Object.values(parse).filter((r) => r.accepted).length} accepted`);

async function parseCase(text) {
  try {
    const { diagramType } = await mermaid.parse(text);
    return { accepted: true, kind: diagramType };
  } catch (error) {
    return { accepted: false, kind: detectedType(text), error: summary(error) };
  }
}

// A second parse: mermaid.parse applies front matter and directives but returns no db.
async function meaningOf(text, meaning) {
  try {
    const { db } = await mermaid.mermaidAPI.getDiagramFromText(text);
    return meaning(db);
  } catch (error) {
    return { error: summary(error) };
  }
}

function flowchartMeaning(db) {
  return {
    direction: db.getDirection(),
    nodes: [...db.getVertices().values()].map(({ id, text, type, labelType }) => ({ id, text, shape: type ?? null, labelType })),
    edges: db.getEdges().map(({ start, end, type, stroke, length, text, labelType }) => ({ start, end, type, stroke, length, text, labelType })),
    subgraphs: db.getSubGraphs().map(({ id, title, nodes, dir }) => ({ id, title, nodes, dir: dir ?? null })),
  };
}

function sequenceMeaning(db) {
  const lineTypes = Object.fromEntries(Object.entries(db.LINETYPE).map(([name, value]) => [value, name]));
  const placements = Object.fromEntries(Object.entries(db.PLACEMENT).map(([name, value]) => [value, name]));
  return {
    actors: [...db.getActors().entries()].map(([id, { name, description, type, box }]) => ({ id, name, description, type, box: box?.name ?? null })),
    messages: db.getMessages().map(({ from, to, message, type, placement, activate }) => ({
      from: from ?? null,
      to: to ?? null,
      message: message ?? null,
      type: lineTypes[type] ?? type ?? null,
      placement: placements[placement] ?? null,
      activate: activate ?? false,
    })),
    boxes: db.getBoxes().map(({ name, actorKeys }) => ({ name: name ?? null, actors: actorKeys })),
  };
}

function summary(error) {
  return String(error?.message ?? error).replace(/\s+/g, ' ').trim().slice(0, LONGEST_ERROR);
}

function corpusFiles() {
  return readdirSync(corpus, { recursive: true })
    .filter((path) => path.endsWith('.mmd'))
    .sort();
}

function writeJson(name, value) {
  writeFileSync(join(results, name), `${JSON.stringify(value, null, 1)}\n`);
}
