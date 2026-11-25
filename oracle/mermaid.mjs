// The pinned mermaid, loaded for parsing only, and the corpus kind of an input as mermaid detects it.
import { readFileSync } from 'node:fs';
import { JSDOM } from 'jsdom';

// mermaid sanitizes labels with DOMPurify, which needs a window; jsdom runs no scripts and loads no resources by default.
const { window } = new JSDOM('');
globalThis.window = window;
globalThis.document = window.document;
const { default: mermaid } = await import('mermaid');
mermaid.initialize({ startOnLoad: false, logLevel: 'fatal' });

export { mermaid };
export const { version } = JSON.parse(readFileSync(new URL('node_modules/mermaid/package.json', import.meta.url), 'utf8'));

const FOLDERS = {
  'flowchart-v2': 'flowchart',
  'flowchart-elk': 'flowchart',
  classDiagram: 'class',
  stateDiagram: 'state',
  gitGraph: 'git',
  quadrantChart: 'quadrant',
  treeView: 'tree-view',
  railroadAbnf: 'railroad',
  railroadEbnf: 'railroad',
  railroadPeg: 'railroad',
};

// zenuml lives in a separate mermaid package that is not loaded, so mermaid does not know it.
const ZENUML = /^\s*zenuml\b/m;

// The kind mermaid.parse reports, else the one detectType guesses for an input mermaid rejects.
export async function corpusKind(text) {
  const id = (await parsedType(text)) ?? detectedType(text);
  if (id) return FOLDERS[id] ?? (/^[a-z][\w-]*$/i.test(id) ? id : 'unknown');
  return ZENUML.test(text) ? 'zenuml' : 'unknown';
}

export function looksLikeDiagram(text) {
  return detectedType(text) !== null || ZENUML.test(text);
}

async function parsedType(text) {
  try {
    return (await mermaid.parse(text)).diagramType;
  } catch {
    return null;
  }
}

export function detectedType(text) {
  try {
    return mermaid.detectType(text);
  } catch {
    return null;
  }
}
