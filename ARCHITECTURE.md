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
  `render_with(source, settings)` does the same as `Settings` say: `max_width`, and `long_words`, `Break` unless set.
  A flowchart too wide, or with an edge left without a path, is laid out again until it fits: tight, then with the ranks going down folded to 8, 7, 6, 5 and 4 eighths of the width, then packed to 8, 6 and 4 eighths without room for the links passing a rank; each as written, then turned top-down when it or one of its groups flows sideways.
  That is at most twenty placements, each routed only when its blocks fit; one still too wide is `Error::TooWide`, so the caller can fall back.
- Limits, each an `Error` the caller can match: 64 KiB of source (`MAX_SOURCE_BYTES`), 200 nodes and subgraphs together (`MAX_NODES`), 150 edges (`MAX_EDGES`), invisible links included, 250000 cells on the routing grid (`MAX_AREA`); a sequence diagram has at most 50 participants (`MAX_PARTICIPANTS`) and 1000 messages (`MAX_MESSAGES`), notes, activations and the starts, sections and ends of blocks included.
  The parser counts nodes and edges as it reads them, an `&` group's edges before it makes them, and stops at the first over a limit.
- Time, measured on an Apple M5 in a release build with `cargo bench --bench limits` and `cargo bench --bench corpus`, the fastest of a few runs on a shared machine:
  - The corpus's 946 flowcharts at 100 columns: median 12 µs, p95 0.6 ms, p99 1.7 ms, slowest 8 ms (`e2e--platform-dev-diagrams-tmp-v12-0-0-faulty-diags-elk-legacy-preset-regression`, packed after eleven placements too wide).
  - The layout takes microseconds; the grid routing takes nearly all the time, more with every placement tried.
  - At each limit: 64 KiB of edges is refused by the parser at the 201st node after 2.4 ms, most of it lexing the whole source; 200 nodes with 150 edges take 12 ms; 150 edges between 20 nodes 54 ms; 51 links passing 98 ranks each, about 5000 places kept for links, 8 ms; a sideways tree routed on 233000 cells 43 ms.
  - Fuzzing found a 27 KiB flowchart that took mermaid-rs-renderer 35 s to lay out; mended where the mutation broke mermaid's grammar, 19 subgraphs and 149 edges, it is drawn in 0.30 s (`benches/inputs/fuzzed-subgraphs.mmd`).
- The limits bound the layout before it runs: at most 200 blocks on 200 ranks, a layer for each rank and one for the labels after it, and a place on each layer for each of 150 links, so 60000 places at most; ordering sweeps them 24 times at most and placing 16.
  The library has no clock, so a deadline belongs to the caller; none of the inputs above needs one.
- No font is ever loaded: a node is as wide as its label's columns, so the drawing is the same on every machine.
- Ten minutes of fuzzing the parser alone, 1.8 million inputs, found no panic and no input that took a second.
- The `measure` feature adds `measure(source, max_width)`: crossings, bends, edge cells, cells beside a node and level C violations, read from where each part went.
  It exists for the [scoreboard](scoreboard/README.md), is hidden from the docs and is not part of the API.
- The `fuzz` feature adds `read_flowchart(source)` and `read_sequence(source)`, each parser alone within the limits, for `fuzz/`; hidden and not part of the API either.

## Every kind

`source` holds what every diagram kind shares before its own parser: mermaid's preparation of the text, its entities, the YAML of `@{ … }` blocks, and the kind a source is, read from its first word once the front matter, directives and comment lines are left out.
`render` checks the length, reads the kind and hands the source to that kind's module; a kind mermaid knows but not drawn here is `Error::Unsupported`.

## Flowcharts

`render` runs seven steps, one module each under `src/flowchart/`.

| Module | Step |
|---|---|
| `parse` | Reads the source as mermaid 12.3.0 does, into its flowchart db: nodes, edges, subgraphs, classes and styles |
| `diagram` | Turns the db into what is drawn: nodes, groups (subgraphs), edges and invisible links, with their text cut into lines |
| `layout` | Nodes and groups to cells, in layers; each group is laid out as one block of its parent, so its nodes stay inside its frame |
| `port` | The side and the cell each edge best leaves and joins a block at, and the edges that share a trunk |
| `route` | Each edge as a cheapest right-angled path on the grid |
| `edge_label` | Where each edge's label goes |
| `draw` | Cuts the grid to what was drawn and draws it on a `Canvas` |

`shape` sizes and draws a node; the crate's `label`, shared with sequence diagrams, cuts text into lines.
`parse` is mermaid's own flowchart parser rewritten by hand, so a flowchart means here what it means to mermaid, in four steps:
`prepare` does to the text what mermaid does before its lexer: the steps every kind shares, from `source` (line ends, the front matter, `%%{…}%%` directives, comment lines, entities marked), then the space after a `}`;
`lexer` applies the rules of `flow.jison` in their order, the first that matches winning, each start condition a state on a stack;
`grammar` reads the tokens top-down, accepting what mermaid's LALR parser accepts and calling the db in the order its actions do;
`db` does what `flowDb.ts` does with each statement: a node is created the first time it is named, `@{ … }` blocks set its shape and label, a link (`link`) becomes edges between every node on either side, a subgraph keeps only the nodes no earlier subgraph holds, a repeated id merges, an unknown shape or a `linkStyle` past the last edge is refused.
Every pattern runs in one pass over the text: no regex, no backtracking on the source.
`parse::agreement`, a test, compares it with the oracle on every corpus flowchart: it accepts and refuses the 946 as mermaid does (level A), and reads the 925 mermaid accepts into the same nodes, edges, labels and subgraphs (level B).
mermaid-rs-renderer's parser, which it replaces, accepts and refuses 908 of them as mermaid does, and reads the same nodes, edges and labels in 814 even through the pre-parse shim it needed.
`measure`, built for tests and the `measure` feature only, counts what a drawing does and checks the level C invariants.

- Text: a node's label breaks at `<br>` and at the line ends of a quoted or Markdown string, and wraps at spaces past 30 columns, a longer word is broken after the last of `. / - _ : , = &` in the second half of a line, else at 30 columns, or with `LongWords::Cut` keeps one line cut with `⋯`; an edge label or a group title is one line of at most 60 columns, cut with `⋯`; the parser leaves out the quotes and backticks around a label, a Markdown string's markup is drawn as written, and entities (`#quot;`, `&amp;`) are shown as the character they name.
- Invisible links (`~~~`): the layout ranks their ends as it does an edge's; they are never routed or drawn.
- Shapes: vt-canvas draws each outline (`Canvas::shape`), so every glyph follows the glyph set.
  A shape a node's brackets or its `@{ shape: … }` block names maps to the closest one, `doc`, `flag` or any name without a close match to a rectangle; `((text))` is a circle, `(((text)))` a double circle; only a pointed head draws an arrow.
  A node named like a subgraph is that subgraph, as mermaid merges them; a subgraph whose id an edge names inside another subgraph moves into it, as mermaid draws it.
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
  An edge that ends on a group may join its frame under the title: the arrow stands above it.
  An edge left without a path goes first on another try, three times at most.
  Lines cross only at a right angle, never where one turns or ends, and never run along a node's border or another line they do not share a trunk with.
  The arrow sits on the cell before the target; a line may turn there into `▾` or `▴`, never into `▸` or `◂`.
- Edge labels go across the straight run through the room the layout kept, else the longest that has room, centred, or slid sideways on a vertical run as long as they still cover the line; else beside the run; else the same anywhere along a run, the nearest to its middle first; else nowhere.
  A label slid along a horizontal run keeps a line cell on each side, so it never touches a crossing.
- Two lines crossing draw `┼` (vt-canvas).

## Sequence diagrams

`parse` is mermaid's own sequence parser rewritten by hand, in three steps after the shared preparation:
`lexer` applies the rules of `sequenceDiagram.jison` in their order, case-insensitive, the first that matches winning, each start condition a state on a stack;
the actor rule's lookahead (a name stops before anything that starts an arrow, `-x`, `--`, `()`…) and the `ID` rules' (a name runs to the last place followed by only spaces before a line end, a `;` or a `#`) are each one pass over the text, never a backtracking regex;
`grammar` reads the tokens top-down, accepting what mermaid's LALR parser accepts, the blocks open at a point a stack, so nesting never deepens the call stack, and flattens each statement into what jison's actions hand the db;
`db` does what `sequenceDb.ts` does once the whole source parsed: a participant is added the first time a statement names it, a later `participant` sets its alias and type again in its first place, an `@{ … }` block's `type` and `alias` count, a box keeps its participants and names itself after its first word unless that word is a CSS colour; a participant in two boxes, a `create` of one already there, a message after `create` or `destroy` that does not involve it, and a deactivation of one not active are refused.
`parse::agreement`, a test, compares it with the oracle on every corpus sequence diagram: it accepts and refuses the 394 as mermaid does (level A), and reads the 386 mermaid accepts into the same participants, messages and boxes (level B).
mermaid-rs-renderer's parser accepts and refuses 79% of them as mermaid does.
Tried against mermaid itself on 50000 inputs mutated from the corpus, it differs only where mermaid's YAML reader does: front matters it refuses, which no parser here reads, and `@{ … }` blocks running over several lines.

## Dependency direction

```
vt-canvas ← vt-graph ← vt-mermaid
```

vt-mermaid depends on vt-canvas and vt-graph; the parser and the layout are its own, and `mermaid-rs-renderer` is a dev-dependency of `examples/agreement.rs` only.

## Test corpus

- [`corpus/`](corpus/README.md): Mermaid inputs harvested from the mermaid repo at a pinned release, one file per case, grouped by diagram kind.
- [`oracle/`](oracle/README.md): a Node script that records what the pinned `mermaid` accepts (level A) and its flowchart and sequence dbs (level B), committed as JSON.
- `examples/agreement.rs`: compares mermaid-rs-renderer's parser with the oracle per kind; a dev-only program, so the library's API never sees the dependency.
  Flowcharts are compared with vt-mermaid's own parser by `parse::agreement` instead.
- `tests/spike/flowcharts.txt`: the 20 flowcharts of the spike, 19 from the corpus and `tests/spike/cached.mmd`; each is an insta snapshot and passes the level C checks of `measure` (every node and label drawn whole, every edge joining both ends, nothing overlapping).
- [`scoreboard/`](scoreboard/README.md): every corpus flowchart drawn by vt-mermaid and three other text renderers, measured the same way, results committed; a dev-only example, run by hand.
- `fuzz/`: a cargo-fuzz crate, its own workspace, out of the library's build and of `scripts/check`, seeded from `corpus/flowchart`; `cargo +nightly fuzz run render` feeds any source to `render` and checks it never panics and every row fits; `cargo +nightly fuzz run parse` feeds it to the parser alone, through `read_flowchart` behind the `fuzz` feature, and checks it never panics and keeps the limits; `parse_sequence` does the same for the sequence parser through `read_sequence`.
  On stable, build a target with `cargo build --release --target <host>` and the sanitizer-coverage `RUSTFLAGS` cargo-fuzz passes, then run the binary.
- `benches/`: `limits` times a typical flowchart and inputs built at each limit with divan, `corpus` times every corpus flowchart and prints the spread; both run with `cargo bench` only.
- `examples/compare.rs`: prints each spike flowchart drawn by mermaid-text, then by vt-mermaid; mermaid-text is a dev-dependency only.
