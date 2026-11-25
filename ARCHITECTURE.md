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
  A flowchart too wide, or with an edge left without a path, is drawn again until it fits: turned top-down when it or one of its groups flows sideways, then with half the room the layout leaves between blocks, then with none and folded into rows of 8, 7 and 6 eighths of the width, each as written before turned.
  That is at most ten placements, each routed only when its blocks fit; one still too wide is `Error::TooWide`, so the caller can fall back.
- Limits, each an `Error` the caller can match: 64 KiB of source (`MAX_SOURCE_BYTES`), 200 nodes and subgraphs together (`MAX_NODES`), 150 edges (`MAX_EDGES`), invisible links included, 250000 cells on the routing grid (`MAX_AREA`).
- Time, measured on an Apple M5 in a release build with `cargo bench --bench limits` and `cargo bench --bench corpus`, the fastest of a few runs on a shared machine:
  - The corpus's 946 flowcharts at 100 columns: median 1 ms, p95 143 ms, p99 295 ms, slowest 3.1 s (`e2e--platform-dev-diagrams-tmp-v12-0-0-faulty-diags-elk-legacy-preset-regression`, too wide, so laid out twice).
  - mermaid-rs-renderer's parse and layout take 99% of that time, nearly all of it in its own edge routing and path cleanup, whose paths vt-mermaid does not use; the grid routing takes 1% (95 ms at most), snapping and drawing under 1 ms.
  - At each limit: 64 KiB of edges is refused by the counts after 14 ms of parsing; 200 nodes with 150 edges take 0.8 s; 150 edges between 20 nodes 4.4 s; a sideways tree routed on 233000 cells 2.5 s, 0.25 s of it routing.
  - Peak memory stays under 24 MB, the most on the 233000-cell grid.
  - Fuzzing found a 27 KiB flowchart, 19 subgraphs and 107 edges, that takes 35 s to lay out before the area limit refuses it (`benches/inputs/fuzzed-subgraphs.mmd`).
- No count bounds mermaid-rs-renderer's time: it grows with edges, subgraphs and label sizes together, and 25 edges between 20 nodes already take 0.9 s.
  The library has no clock, so the deadline belongs to the caller: run `render` on a thread of its own and give up on it after 2 s, which spares all but one corpus flowchart, then fall back.
- mermaid-rs-renderer never loads a font: it lays out a stand-in graph whose labels are `n`s, measured with its fast metrics, so the drawing is the same on every machine.
- mermaid-rs-renderer may panic on input it misreads; the caller catches it, as mrk does with `mermaid::catch::caught`.
  Thirty minutes of fuzzing found no such panic in a release build, only its debug assertions, which grade its own layout: the dev profile turns them off, and a caller's debug build should too.
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
Two modules rewrite what mmdr 0.3.1 misreads before it reads the source, with `source` reading its syntax outside quotes, brackets and the front matter:
`attributes` turns a node's block (`A@{ shape: hex, label: "Text" }`), which mmdr reads as a node named `A@`, into its label in brackets and keeps the shape it names, and leaves out an edge's or a group's block with its id;
`invisible` cuts invisible links (`A ~~~ B`) out, as mmdr keeps `A` and drops the rest of the statement.
`measure`, built for tests and the `measure` feature only, counts what a drawing does and checks the level C invariants.

- Text: a node's label breaks at `<br>` and wraps at spaces past 30 columns, a longer word is cut after the last of `. / - _ : , = &` in the second half of a line, else at 30 columns; an edge label or a group title is one line of at most 60 columns, cut with `⋯`; quotes around a label are left out.
- Invisible links: `diagram` finds the nodes each one joins and adds it to mmdr's stand-in graph, so it shapes the layout; it is never routed or drawn.
- Shapes: vt-canvas draws each outline (`Canvas::shape`), so every glyph follows the glyph set; mmdr's shape maps to the vt-canvas one of the same name, mindmap and text nodes to a rectangle.
  A shape an attribute block names maps to the closest one, `doc`, `flag` or any name without a close match to a rectangle.
  mmdr reads both `((text))` and `(((text)))` as a double circle; it is drawn as a circle, the one people write.
  A node is its label plus a space and the shape's inset on each side.
- Snap: one column is 8.192 layout pixels and one row 24, the size of the stand-in `n` and its line; a rank keeps the layout's start along the flow and centres its nodes across it.
  Blocks of one rank, or one within another's extent, move apart across the flow; a rank too close to the one before moves along, with every later rank.
  Gaps: 3 rows between ranks and 4 columns between siblings when the flow goes down or up; 6 columns, or the widest edge label plus 4, between ranks and 1 row between siblings when it goes sideways.
  Room: the snap keeps the room the layout leaves between blocks from side to side of the screen, half of it, or none; the gaps stay.
  With half, a rank going down keeps the middles of its blocks in proportion and a rank going sideways its start; a pulled rank still comes a rank gap after the blocks its edges come from.
  With none, every block starts on the left and rows going down are folded: a block ending past the columns allowed moves a rank gap under the blocks it stands beside, with the rest of its rank and every later rank; a group has 4 columns fewer than its parent.
  A group's frame keeps 2 cells around what it holds and is wide enough for its title; the grid keeps 3 empty cells around everything.
- Route: shortest edges first; a step costs 1, a bend 4, crossing another line 6, and leaving or entering a node through a side other than the flow's 4, plus half the distance from the middle of that side.
  Lines cross only at a right angle, never where one turns or ends, and never run along a node's border or another line.
  The arrow sits on the cell before the target; a line may turn there into `▾` or `▴`, never into `▸` or `◂`.
- Edge labels go across the longest straight run that has room, centred, or slid sideways on a vertical run as long as they still cover the line; else beside the run; else the same anywhere along a run, the nearest to its middle first; else nowhere.
  A label slid along a horizontal run keeps a line cell on each side, so it never touches a crossing.
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
- `fuzz/`: a cargo-fuzz crate, its own workspace, out of the library's build and of `scripts/check`; `cargo +nightly fuzz run render` feeds any source to `render` and checks it never panics and every row fits, seeded from `corpus/flowchart`.
- `benches/`: `limits` times a typical flowchart and inputs built at each limit with divan, `corpus` times every corpus flowchart and prints the spread; both run with `cargo bench` only.
- `examples/compare.rs`: prints each spike flowchart drawn by mermaid-text, then by vt-mermaid; mermaid-text is a dev-dependency only.
