# Architecture

`vt-mermaid` is one library crate.
Mermaid diagrams drawn as text on a vt-canvas grid.
This page says how the code is laid out today; when a module or a boundary changes, change this page in the same change.

## Owns

- Mermaid source to a text drawing, one diagram kind at a time; flowcharts today.
- Snapping a real layout to the grid, then routing edges on the grid.
- Handing chart kinds (xychart, pie, sankey…) to vt-graph.

## Never

- Reads files, the environment or the clock; prints; exits the process.
- Writes ANSI escapes or picks colours: it hands back cells with colour roles, the caller maps them and writes the terminal.
- Lets a dependency's type into a public signature, except vt-canvas's `Canvas`, `GlyphSet`, `Role` and `Run`: the cell format every vt crate hands back, re-exported so a caller needs no second dependency.
- Writes its own layout engine: mermaid-rs-renderer lays out, vt-mermaid snaps to the grid.

## Contract

- `render(source, max_width)` returns the drawing as a `Canvas`, at most `max_width` columns wide.
  A flowchart flowing sideways that is too wide is laid out again top-down; one still too wide is `Error::TooWide`, so the caller can fall back.
- Limits, each an `Error` the caller can match: 64 KiB of source (`MAX_SOURCE_BYTES`), 200 nodes and subgraphs together (`MAX_NODES`), 150 edges (`MAX_EDGES`), 250000 cells on the routing grid (`MAX_AREA`).
- mermaid-rs-renderer never loads a font: it lays out a stand-in graph whose labels are `n`s, measured with its fast metrics, so the drawing is the same on every machine.
- mermaid-rs-renderer may panic on input it misreads; the caller catches it, as mrk does with `mermaid::catch::caught`.
- The `measure` feature adds `measure(source, max_width)`: crossings, bends, edge cells and level C violations, read from where each part went.
  It exists for the [scoreboard](scoreboard/README.md), is hidden from the docs and is not part of the API.

## Flowcharts

`render` runs five steps, one module each under `src/flowchart/`.

| Module | Step |
|---|---|
| `diagram` | Parses and lays out with mermaid-rs-renderer, the only module that sees it; returns nodes, groups (subgraphs) and edges with their text and layout pixels |
| `snap` | Pixels to cells; each group is placed as one block inside its parent, so its nodes stay inside its frame |
| `route` | Each edge as a cheapest right-angled path on the grid |
| `edge_label` | Where each edge's label goes |
| `draw` | Cuts the grid to what was drawn and draws it on a `Canvas` |

`label` cuts text into lines and `shape` sizes and draws a node.
`measure`, built for tests and the `measure` feature only, counts what a drawing does and checks the level C invariants.

- Text: a node's label breaks at `<br>` and wraps at spaces past 30 columns, a longer word is cut with `⋯`; an edge label or a group title is one line of at most 60 columns; quotes around a label are left out.
- Shapes: rectangle, rounded (round, stadium, circle), diamond (`╱ ╲` corners), cylinder (a line under the top), subroutine (an inner line down each side); any other shape is a rectangle.
  A node is its label plus a space and a border on each side.
- Snap: one column is 8.192 layout pixels and one row 24, the size of the stand-in `n` and its line; a rank keeps the layout's start along the flow and centres its nodes across it.
  Blocks of one rank, or one within another's extent, move apart across the flow; a rank too close to the one before moves along, with every later rank.
  Gaps: 3 rows between ranks and 4 columns between siblings when the flow goes down or up; 6 columns, or the widest edge label plus 4, between ranks and 1 row between siblings when it goes sideways.
  A group's frame keeps 2 cells around what it holds and is wide enough for its title; the grid keeps 3 empty cells around everything.
- Route: shortest edges first; a step costs 1, a bend 4, crossing another line 6, and leaving or entering a node through a side other than the flow's 4, plus half the distance from the middle of that side.
  Lines cross only at a right angle, never where one turns or ends, and never run along a node's border or another line.
  The arrow sits on the cell before the target; a line may turn there into `▾` or `▴`, never into `▸` or `◂`.
- Edge labels go across the longest straight run that has room, centred, or slid sideways on a vertical run as long as they still cover the line; else beside the run; else nowhere.
- Two lines crossing draw `┼` (vt-canvas).

## Dependency direction

```
vt-canvas ← vt-graph ← vt-mermaid
```

vt-mermaid depends on vt-canvas and vt-graph, and on `mermaid-rs-renderer` for parsing and layout; no layout engine is written here.

## Test corpus

- [`corpus/`](corpus/README.md): Mermaid inputs harvested from the mermaid repo at a pinned release, one file per case, grouped by diagram kind.
- [`oracle/`](oracle/README.md): a Node script that records what the pinned `mermaid` accepts (level A) and its flowchart and sequence dbs (level B), committed as JSON.
- `examples/agreement.rs`: compares mermaid-rs-renderer's parser with the oracle per kind; a dev-only program, so the library's API never sees the dependency.
- `tests/spike/flowcharts.txt`: the 20 flowcharts of the spike, 19 from the corpus and `tests/spike/cached.mmd`; each is an insta snapshot and passes the level C checks of `measure` (every node and label drawn whole, every edge joining both ends, nothing overlapping), but for the violations the test lists.
- [`scoreboard/`](scoreboard/README.md): every corpus flowchart drawn by vt-mermaid and three other text renderers, measured the same way, results committed; a dev-only example, run by hand.
- `examples/compare.rs`: prints each spike flowchart drawn by mermaid-text, then by vt-mermaid; mermaid-text is a dev-dependency only.
