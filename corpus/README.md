# Corpus

Mermaid inputs harvested from [mermaid-js/mermaid](https://github.com/mermaid-js/mermaid) at tag `mermaid@12.3.0`, commit `2086904e76c692331e34a45d7cc3bc30860d040e`.
They are © Knut Sveidqvist and the mermaid contributors, under the MIT license copied in [`LICENSE-mermaid`](LICENSE-mermaid).
The files are generated: regenerate them instead of editing them by hand.

The corpus feeds the [oracle](../oracle/README.md), the parsers' agreement tests, the snapshot tests and the [scoreboard](../scoreboard/README.md).

## Layout

Each file is `<kind>/<origin>--<source>[--L<line>].mmd`: one unique input.

- `kind` is the diagram kind the pinned mermaid reports when it parses the input, or detects when it rejects it, under a short folder name (`classDiagram` is `class`, `flowchart-v2` is `flowchart`…).
  `zenuml` is told apart by its header, since mermaid does not load that package; `unknown/` holds the inputs mermaid finds no kind in.
- `origin` is where it came from:

  | Origin | Source in the mermaid repo |
  |---|---|
  | `spec` | A string or template literal in a `*.spec.*` or `*.test.*` file under `packages/`, kept when mermaid detects a diagram kind in it; test titles and lone keywords are skipped |
  | `e2e` | A `.mmd` file under `e2e/`, the Playwright suite that replaced `cypress/integration/rendering/` |
  | `docs` | A `mermaid`, `mermaid-example` or `mermaid-nocode` fence in `packages/mermaid/src/docs/` |

- [`sources.json`](sources.json) lists, for each file, every place in the mermaid repo the same input appears.
- Inputs are dedented, stripped of leading blank lines and trailing spaces, and deduplicated on that text: 4777 found, 4588 unique.
- Template literals with `${…}` and strings built by concatenation are skipped; `e2e` sidecar configs are not used.

## Counts

| Kind | Cases |
|---|--:|
| agentflow | 216 |
| architecture | 58 |
| block | 121 |
| c4 | 61 |
| class | 372 |
| cynefin | 32 |
| er | 315 |
| eventmodeling | 35 |
| flowchart | 946 |
| gantt | 75 |
| git | 258 |
| info | 8 |
| ishikawa | 18 |
| journey | 24 |
| kanban | 45 |
| mindmap | 89 |
| packet | 26 |
| pie | 66 |
| quadrant | 49 |
| radar | 36 |
| railroad | 112 |
| requirement | 186 |
| sankey | 30 |
| sequence | 394 |
| state | 279 |
| swimlane | 117 |
| timeline | 55 |
| tree-view | 65 |
| treemap | 52 |
| unknown | 10 |
| usecase | 204 |
| venn | 48 |
| wardley | 49 |
| xychart | 118 |
| zenuml | 19 |
| total | 4588 |

By origin: 1649 `spec`, 2364 `e2e`, 575 `docs`.

## Regenerate

```sh
git clone --depth 1 --branch mermaid@12.3.0 https://github.com/mermaid-js/mermaid.git /tmp/mermaid
npm ci --prefix oracle --ignore-scripts
node oracle/harvest.mjs /tmp/mermaid corpus
```

Then rerun the oracle ([`oracle/README.md`](../oracle/README.md)).
