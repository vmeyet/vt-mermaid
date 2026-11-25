# Architecture

`vt-mermaid` is one library crate.
Mermaid diagrams drawn as text on a vt-canvas grid.
This page says how the code is laid out today; when a module or a boundary changes, change this page in the same change.

## Owns

- Mermaid source to a text drawing, one diagram kind at a time; flowcharts today.
- Laying a flowchart out in layers on the grid, then routing its edges on the grid.
- Handing chart kinds (xychart, pie, sankey…) to vt-graph.

## Never

- Reads files, the environment or the clock; prints; exits the process.
- Writes ANSI escapes or picks colours: it hands back cells with colour roles, the caller maps them and writes the terminal.
- Lets a dependency's type into a public signature, except vt-canvas's `Canvas`, `GlyphSet`, `Role` and `Run`: the cell format every vt crate hands back, re-exported so a caller needs no second dependency.

## Contract

- `render(source, max_width)` returns the drawing as a `Canvas`, at most `max_width` columns wide.
  A flowchart too wide, or with an edge left without a path, is laid out again until it fits: tight, then with the ranks going down folded to 8, 7, 6, 5 and 4 eighths of the width, then packed to 8, 6 and 4 eighths without room for the links passing a rank; each as written, then turned top-down when it or one of its groups flows sideways.
  That is at most twenty placements, each routed only when its blocks fit; one still too wide is `Error::TooWide`, so the caller can fall back.
- Limits, each an `Error` the caller can match: 64 KiB of source (`MAX_SOURCE_BYTES`), 200 nodes and subgraphs together (`MAX_NODES`), 150 edges (`MAX_EDGES`), invisible links included, 250000 cells on the routing grid (`MAX_AREA`).
- Time, measured on an Apple M5 in a release build with `cargo bench --bench limits` and `cargo bench --bench corpus`, the fastest of a few runs on a shared machine:
  - The corpus's 946 flowcharts at 100 columns: median 12 µs, p95 0.6 ms, p99 1.7 ms, slowest 8 ms (`e2e--platform-dev-diagrams-tmp-v12-0-0-faulty-diags-elk-legacy-preset-regression`, packed after eleven placements too wide).
  - The layout takes microseconds; the grid routing takes nearly all the time, more with every placement tried.
  - At each limit: 64 KiB of edges is refused by the counts after 6 ms of parsing; 200 nodes with 150 edges take 12 ms; 150 edges between 20 nodes 54 ms; 51 links passing 98 ranks each, about 5000 places kept for links, 8 ms; a sideways tree routed on 233000 cells 43 ms.
  - Fuzzing found a 27 KiB flowchart, 19 subgraphs and 107 edges, that took mermaid-rs-renderer 35 s to lay out; it is drawn in 0.27 s, packed after twelve placements (`benches/inputs/fuzzed-subgraphs.mmd`).
- The limits bound the layout before it runs: at most 200 blocks on 200 ranks, a layer for each rank and one for the labels after it, and a place on each layer for each of 150 links, so 60000 places at most; ordering sweeps them 24 times at most and placing 16.
  The library has no clock, so a deadline belongs to the caller; none of the inputs above needs one.
- No font is ever loaded: a node is as wide as its label's columns, so the drawing is the same on every machine.
- mermaid-rs-renderer may panic on input it misreads; the caller catches it, as mrk does with `mermaid::catch::caught`.
  Thirty minutes of fuzzing found no such panic in a release build.
- The `measure` feature adds `measure(source, max_width)`: crossings, bends, edge cells and level C violations, read from where each part went.
  It exists for the [scoreboard](scoreboard/README.md), is hidden from the docs and is not part of the API.

## Flowcharts

`render` runs six steps, one module each under `src/flowchart/`.

| Module | Step |
|---|---|
| `diagram` | Parses with mermaid-rs-renderer, the only module that sees it; returns nodes, groups (subgraphs), edges and invisible links with their text |
| `layout` | Nodes and groups to cells, in layers; each group is laid out as one block of its parent, so its nodes stay inside its frame |
| `port` | The side and the cell each edge best leaves and joins a block at, and the edges that share a trunk |
| `route` | Each edge as a cheapest right-angled path on the grid |
| `edge_label` | Where each edge's label goes |
| `draw` | Cuts the grid to what was drawn and draws it on a `Canvas` |

`label` cuts text into lines and `shape` sizes and draws a node.
`diagram` rewrites what mmdr 0.3.1 misreads before it reads the source, in one place, with `source` reading its syntax outside quotes, brackets and the front matter:
`source` adds ` TD` to a header that names no direction (`flowchart`, `graph`), which mmdr reads as a node; `flowchart-elk` too, though mmdr reads it as two nodes either way;
`attributes` turns a node's block (`A@{ shape: hex, label: "Text" }`), which mmdr reads as a node named `A@`, into its label in brackets and keeps the shape it names, and leaves out an edge's or a group's block with its id;
`invisible` cuts invisible links (`A ~~~ B`) out, as mmdr keeps `A` and drops the rest of the statement.
`measure`, built for tests and the `measure` feature only, counts what a drawing does and checks the level C invariants.

- Text: a node's label breaks at `<br>` and wraps at spaces past 30 columns, a longer word is cut after the last of `. / - _ : , = &` in the second half of a line, else at 30 columns; an edge label or a group title is one line of at most 60 columns, cut with `⋯`; quotes around a label are left out, then the backticks of a Markdown string, whose markup is drawn as written.
- Invisible links: `diagram` finds the nodes each one joins; the layout ranks them as it does an edge's ends, and it is never routed or drawn.
- Shapes: vt-canvas draws each outline (`Canvas::shape`), so every glyph follows the glyph set; mmdr's shape maps to the vt-canvas one of the same name, mindmap and text nodes to a rectangle.
  A shape an attribute block names maps to the closest one, `doc`, `flag` or any name without a close match to a rectangle.
  mmdr reads both `((text))` and `(((text)))` as a double circle; it is drawn as a circle, the one people write.
  A node is its label plus a space and the shape's inset on each side.
- Layout, a container at a time, the innermost first: `layout/acyclic` turns round the links closing a cycle, the first block the source names staying first; `rank` gives each block the earliest rank its links allow, then moves a block with more links leaving than arriving next to the nearest block it leads to; `order` sweeps down and up 24 times at most, each vertex at the average place of its neighbours, then swaps neighbours while that uncrosses links, and keeps the order with the fewest crossings; `coordinate` gives each layer the places closest to what its vertices ask, the middle of their neighbours, that keep their order and their distance, and keeps of 8 sweeps the placement with the straightest links and the best centred blocks; `layered` runs them and gives every link a dummy on each layer it passes.
  A link from one rank to the next weighs 8, a long link 16 between its dummies and 1 at its ends, a link closing a cycle 1, so a chain of blocks is one line and a long link bends at its ends.
  A link to a node inside a group is in line with the node, not with the middle of the group.
  Labels: a link's label has a layer of its own between two ranks, in line with the block the link leads to: a row going down, the label's columns and one more going sideways; the router keeps that room for the edge.
  Gaps: 3 rows between ranks and 4 columns between blocks when the flow goes down or up, 2 between links, or 2 and 1 when tight; 6 columns between ranks, or a label's columns plus 5, and 1 row between blocks when it goes sideways.
  Folded: a rank going down wider than the columns allowed keeps the blocks that fit, with 2 columns for each block leading to the others, which move to a new rank after it; they stand where they take the least room and the edges to them share a trunk through the ranks they pass.
  Packed: folded, and neither the links passing a rank nor the labels keep room, so blocks stand closer and edges find their way round them; edges then cross.
  A group has 4 columns fewer than its parent; its frame keeps 2 cells around what it holds and is wide enough for its title; the grid keeps 3 empty cells around everything.
- Ports: an edge leaves and joins a block through the side facing the other end, along the flow of the innermost group holding both; the edges of a side each get a cell, in the order of where they go, 2 cells apart while the side has room, the one going straight on the middle.
  A side without a cell for each, as the 1-cell side of a 3-row node, gives one cell and one trunk to its edges that draw alike; a folded or packed layout does so on every side.
- Route: edges between close blocks first, those crossing group frames later; a step costs 1, a bend 4, crossing another line 6, another cell of the side than the edge's own 2 a cell, another side 12, turning on the arrow's cell 3 more, a cell kept for another edge's label 3, passing behind a frame's title 4 more.
  Equal paths are told apart by where they turn, right after the source, or right before the target for an edge that shares only its way in, and by not running right beside a node.
  Trunks: an edge may leave an earlier edge of its trunk between that one's first two turns or up to 2 cells before, and join one between its last two turns or up to 2 cells after, each edge leaving before any joins, so no line reads as going from one source to another's target; twin edges are one line.
  A group's title moves along its frame off the column an edge comes straight down through; when the frame is no wider than its title, the edge passes behind it.
  Lines cross only at a right angle, never where one turns or ends, and never run along a node's border or another line they do not share a trunk with.
  The arrow sits on the cell before the target; a line may turn there into `▾` or `▴`, never into `▸` or `◂`.
- Edge labels go across the straight run through the room the layout kept, else the longest that has room, centred, or slid sideways on a vertical run as long as they still cover the line; else beside the run; else the same anywhere along a run, the nearest to its middle first; else nowhere.
  A label slid along a horizontal run keeps a line cell on each side, so it never touches a crossing.
- Two lines crossing draw `┼` (vt-canvas).

## Dependency direction

```
vt-canvas ← vt-graph ← vt-mermaid
```

vt-mermaid depends on vt-canvas and vt-graph, and on `mermaid-rs-renderer` for parsing; the layout is its own.

## Test corpus

- [`corpus/`](corpus/README.md): Mermaid inputs harvested from the mermaid repo at a pinned release, one file per case, grouped by diagram kind.
- [`oracle/`](oracle/README.md): a Node script that records what the pinned `mermaid` accepts (level A) and its flowchart and sequence dbs (level B), committed as JSON.
- `examples/agreement.rs`: compares mermaid-rs-renderer's parser with the oracle per kind; a dev-only program, so the library's API never sees the dependency.
- `tests/spike/flowcharts.txt`: the 20 flowcharts of the spike, 19 from the corpus and `tests/spike/cached.mmd`; each is an insta snapshot and passes the level C checks of `measure` (every node and label drawn whole, every edge joining both ends, nothing overlapping).
- [`scoreboard/`](scoreboard/README.md): every corpus flowchart drawn by vt-mermaid and three other text renderers, measured the same way, results committed; a dev-only example, run by hand.
- `fuzz/`: a cargo-fuzz crate, its own workspace, out of the library's build and of `scripts/check`; `cargo +nightly fuzz run render` feeds any source to `render` and checks it never panics and every row fits, seeded from `corpus/flowchart`.
- `benches/`: `limits` times a typical flowchart and inputs built at each limit with divan, `corpus` times every corpus flowchart and prints the spread; both run with `cargo bench` only.
- `examples/compare.rs`: prints each spike flowchart drawn by mermaid-text, then by vt-mermaid; mermaid-text is a dev-dependency only.
