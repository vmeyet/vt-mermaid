# Architecture

`vt-mermaid` is one library crate: Mermaid diagrams drawn as text on a vt-canvas grid.
This page says how the code is laid out today.
When a module or a boundary changes, change this page in the same change.

- [Overview](#overview)
- [Contract](#contract)
- [Limits and how work is bounded](#limits-and-how-work-is-bounded)
- [Step 1: prepare the text and find the kind](#step-1-prepare-the-text-and-find-the-kind)
- [Step 2: parse as mermaid does](#step-2-parse-as-mermaid-does)
- [Step 3: lay out and draw a graph](#step-3-lay-out-and-draw-a-graph)
- [Step 3, sequence diagrams: lay out and draw](#step-3-sequence-diagrams-lay-out-and-draw)
- [How each graph kind is drawn](#how-each-graph-kind-is-drawn)
- [Time](#time)
- [Tests, corpus and tools](#tests-corpus-and-tools)
- [Dependencies](#dependencies)

## Overview

The path from source to cells, drawn by vt-mermaid itself:

```text
                                           ┌────────┐
                                           │ source │
                                           └───┬────┘
                                               │
                                               │
                                               ▾
                                      ┌─────────────────┐
                                      │ source: prepare │
                                      └────────┬────────┘
                                               │
                                               │
                                               ▾
                                            ╱──────╲
                                            │ kind │
                                            ╲┬─┬─┬─╱
               ┌─────────────────────────────┘ │ └──────────────────────────┐
  flowchart, state, class, ER              sequence                     any other
               ▾                               ▾                            ▾
       ┌────────────────┐         ┌──────────────────────────┐    ┌────────────────────┐
       │ parse, diagram │         │ sequence: parse, diagram │    │ Error::Unsupported │
       └───────┬────────┘         └────────────┬─────────────┘    └────────────────────┘
               │                               │
               │                               │
               ▾                               ▾
┌─────────────────────────────┐    ┌────────────────────────┐
│ graph: layout, port, route, │ ┌──┤ sequence: layout, draw │
│      edge_label, draw       ├┐│  └────────────────────────┘
└─────────────────────────────┘││
                               ││
                               ││
                               ▾▾
                    ┌──────────────────────┐
                    │ Canvas of Role cells │
                    └──────────────────────┘
```

### What the crate owns

- Mermaid source to a text drawing, one diagram kind at a time: flowcharts, sequence, state, class and ER diagrams today.
- Laying a graph (a flowchart, a state, class or ER diagram) out in layers on the grid, then routing its edges on the grid.
- Laying a sequence diagram out: each participant's column, then each event on rows of its own.
- Chart kinds (xychart, pie, sankey…) are planned to go to vt-graph, a crate not published yet; today they are `Error::Unsupported`.

### What it never does

- Read files, the environment or the clock; print; exit the process.
- Write ANSI escapes or pick colours: it hands back cells with colour roles, and the caller maps them and writes the terminal.
- Let a dependency's type into a public signature, except vt-canvas's `Canvas`, `GlyphSet` and `Run`.
  They are the cell format every vt crate hands back, re-exported so a caller needs no second dependency.
- Load a font: a node is as wide as its label's columns, so the drawing is the same on every machine.

### Modules

| Module | Job |
|---|---|
| `lib` | `render`, `render_with`, and the re-exports |
| `error` | `Error` and every limit constant |
| `settings` | `Settings` and `LongWords` |
| `role` | `Role`, what each cell is drawn for: `Node`, `Edge`, `Label`, `Group`, and `Plain` for empty cells |
| `source` | What every kind shares before its own parser: `prepare`, `entity`, `yaml` (`@{ … }` blocks), `unicode` (letters the lexers read) |
| `kind` | One table of every header mermaid knows, and how each kind is drawn |
| `label` | Text cut into lines: where the source breaks it, then wrapped at spaces |
| `flowchart`, `state`, `class`, `er` | Each kind's `parse` (and its `agreement` test) and `diagram`, which turns mermaid's db into the graph drawn |
| `graph` | What the four graph kinds share: `diagram`, `layout`, `port`, `route`, `budget`, `edge_label`, `shape`, `draw`, `measure` |
| `sequence` | Sequence diagrams, end to end: `parse`, `diagram`, `layout`, `draw` |

## Contract

### Calls

- `render(source, max_width)` returns the drawing as a `Canvas`, at most `max_width` columns wide.
  A `max_width` over vt-canvas's 1000 columns (`MAX_WIDTH`) counts as 1000, in `Settings::new` and again in `render_with`, as the field is public.
- `render_with(source, settings)` does the same as `Settings` say: `max_width`, and `long_words`, `Break` unless set.
- A flowchart with nothing after its header (`graph`) is an empty canvas, 0 by 0: mermaid accepts it and draws nothing.

### The fit ladder

A drawing too wide is laid out again, tighter, until it fits.

**Flowcharts, state, class and ER diagrams** (`graph`).
A drawing too wide, or with an edge left without a path, is tried in this order:

1. Loose, then tight.
2. Tight with node labels wrapped at 20, then 14 columns.
3. The ranks going down folded to 8, 7, 6, 5 and 4 eighths of the width.
4. Folded again to 8 down to 2 eighths, with labels wrapped at 14.
5. Packed to 8, 6 and 4 eighths, without room for the links passing a rank.

Each is tried as written, then turned top-down when it or one of its groups flows sideways.
That is at most thirty-eight placements, each routed only when its blocks fit.

- One still too wide is `Error::TooWide`, so the caller can fall back.
- The ladder stops once the same edge found no path in two placements (`Error::Unroutable`).
- It also stops once the path searches of every placement together took 6 million cells off their queues (`MAX_ROUTING_STEPS`, `Error::TooComplex`).
  The searches share their memory (`graph::budget`), a stamp per state telling one search's costs from the last.

**Sequence diagrams.**

1. Labels as written, then wrapped at spaces to 60, 48, 40, 32, 26, 20, 16, 12, 10, 8 and 6 columns, a word up to 30 columns kept whole.
2. The same with heads 1 space apart instead of 3.
3. Wrapped to 30 columns down to 6, one at a time, every word wider than the lines broken or cut as `long_words` says.

That is at most forty-nine placements, each a few passes over the events.
One still too wide is `Error::TooWide`.

### Hidden extras

- The `measure` feature adds `measure(source, max_width)` for flowcharts, state, class and ER diagrams: crossings, bends, edge cells, cells beside a node and level C violations, read from where each part went.
  It exists for the [scoreboard](scoreboard/README.md), is hidden from the docs, unstable and not part of the API.
- Built with `--cfg fuzzing`, as cargo-fuzz builds, the crate adds each parser alone within the limits, for `fuzz/`:
  `read_flowchart`, `read_state`, `read_class` and `read_er` return `GraphCounts`; `read_sequence` returns `SequenceCounts`.
  No feature turns them on, so they are no part of the API; `scripts/check` lints them with `cargo clippy --lib -- --cfg fuzzing`.

## Limits and how work is bounded

Every limit is an `Error` the caller can match.

| Constant | Value | Counts |
|---|--:|---|
| `MAX_SOURCE_BYTES` | 64 KiB | Bytes of source |
| `MAX_NODES` | 200 | Nodes and subgraphs together |
| `MAX_EDGES` | 150 | Edges, invisible links included |
| `MAX_AREA` | 250000 | Cells of the routing grid |
| `MAX_CELLS` | 1000000 | Cells of the canvas |
| `MAX_PARTICIPANTS` | 50 | Participants of a sequence diagram |
| `MAX_MESSAGES` | 1000 | Messages of a sequence diagram, notes, activations and the starts, sections and ends of blocks included |
| `MAX_ROUTING_STEPS` | 6000000 | Cells the path searches take off their queues, over every placement tried |
| `MAX_WIDTH` | 1000 | Columns; a wider `max_width` counts as this |

- `TooLarge` names the one of `MAX_AREA` and `MAX_CELLS` it is over.
- A state diagram counts towards `MAX_NODES` the nodes mermaid's renderer gets: start and end dots, notes, their groups and concurrent regions included, and each composite state's opening while it is read.
  It counts towards `MAX_EDGES` its transitions and the links of its notes.
- A class diagram counts its classes, lollipops, notes and namespaces against `MAX_NODES`, its relations and the links of notes to classes against `MAX_EDGES`.
- An ER diagram counts its entities and subgraphs against `MAX_NODES`, its relationships against `MAX_EDGES`.
- The parser counts nodes and edges as it reads them, an `&` group's edges before it makes them, and stops at the first over a limit.
  The count an error carries is where reading stopped: a lower bound of what the source holds.

The limits bound the layout before it runs:

- At most 200 blocks on 200 ranks, a layer for each rank and one for the labels after it, and a place on each layer for each of 150 links: 60000 places at most.
- Ordering sweeps them 24 times at most, placing 16.
- The library has no clock: the routing budget bounds the work instead, the same on every machine.
- The corpus's drawings take at most 263000 steps at 40 to 1000 columns; the inputs built at the limits take 3 million at most, `fuzzed-subgraphs` 5.3 million.
- An input spending the whole budget is refused after about half a second, as `fuzzed-subgraphs` is at 80 columns.

## Step 1: prepare the text and find the kind

- `source` holds what every kind shares before its own parser: mermaid's preparation of the text, its entities, the YAML of `@{ … }` blocks, and the word a source opens with once the front matter, directives and comment lines are left out.
- `kind` holds one table of every header mermaid knows, each a prefix of that word, with the kind it names:
  - a graph kind gives the function reading the prepared text into the graph drawn, which `graph` lays out, routes and draws;
  - a sequence diagram draws its own way;
  - a kind mermaid knows but not drawn here has none.
- `render` checks the length, prepares the text once, looks the header up and hands the prepared text to that kind.
- A kind mermaid knows but not drawn here is `Error::Unsupported`, quoting the source's first word, at most 32 ASCII letters, digits and `-`.
- Every graph kind's parser reads within one `graph::Limits`, and `graph::measured` measures any of them.

## Step 2: parse as mermaid does

Each parser is mermaid 12.3.0's own, rewritten by hand, so a diagram means here what it means to mermaid.
They share one shape, after the shared preparation:

| Part | Does |
|---|---|
| `lexer` | Applies the rules of the kind's `.jison` grammar in their order, the first that matches winning, each start condition a state on a stack |
| `grammar` | Reads the tokens top-down, accepting what mermaid's LALR parser accepts, and calls the db in the order its actions do |
| `db` | Does what the kind's `*Db.ts` does with each statement |
| `agreement` | A test comparing the parser with the [oracle](oracle/README.md) on every corpus input of the kind |

- Every pattern runs in one pass over the text: no regex, no backtracking on the source.
- Nesting (blocks, composite states) is a stack, so it never deepens the call stack.

`agreement` checks two levels on the corpus, and every case agrees today:

| Kind | Level A: accepts and refuses as mermaid | Level B: same meaning, on the inputs mermaid accepts |
|---|--:|--:|
| Flowchart | 946 | 925: nodes, edges, labels and subgraphs |
| Sequence | 394 | 386: participants, messages and boxes |
| State | 279 | 276: nodes (id, shape, label, descriptions, parent, direction, note side) and edges (ends, label, head) |
| Class | 372 | 315: classes, members, namespaces, relations and notes |
| ER | 315 | 312: entities, attributes, relationships and subgraphs |

### Flowchart parser

- `prepare` does to the text what mermaid does before its lexer: the steps every kind shares, from `source` (line ends, the front matter, `%%{…}%%` directives, comment lines, entities marked), then the space after a `}`.
- `lexer` follows `flow.jison`.
- `db` follows `flowDb.ts`:
  - a node is created the first time it is named, and `@{ … }` blocks set its shape and label;
  - a link (`link`) becomes edges between every node on either side;
  - a subgraph keeps only the nodes no earlier subgraph holds;
  - a repeated id merges;
  - an unknown shape or a `linkStyle` past the last edge is refused.
- mermaid-rs-renderer's parser, which this one replaces, accepts and refuses 908 of the 946 as mermaid does, and reads the same nodes, edges and labels in 814, even through the pre-parse shim it needed.

### Sequence parser

- `lexer` follows `sequenceDiagram.jison`, case-insensitive.
  The actor rule's lookahead (a name stops before anything that starts an arrow, `-x`, `--`, `()`…) and the `ID` rules' (a name runs to the last place followed by only spaces before a line end, a `;` or a `#`) are each one pass over the text.
- `grammar` keeps the blocks open at a point on a stack, and flattens each statement into what jison's actions hand the db.
- `db` does what `sequenceDb.ts` does once the whole source parsed:
  - a participant is added the first time a statement names it;
  - a later `participant` sets its alias and type again in its first place;
  - an `@{ … }` block's `type` and `alias` count;
  - a box keeps its participants and names itself after its first word unless that word is a CSS colour (`colour`);
  - a participant in two boxes, a `create` of one already there, a message after `create` or `destroy` that does not involve it, and a deactivation of one not active are refused.
- `arrow` says how mermaid writes each arrow, its name in the db, and what its ends are drawn with.
- mermaid-rs-renderer's parser accepts and refuses 79% of them as mermaid does.
- Tried against mermaid itself on 50000 inputs mutated from the corpus, it differs only where mermaid's YAML reader does: front matters it refuses, which no parser here reads, and `@{ … }` blocks running over several lines.

### State parser

- `lexer` follows `stateDiagram.jison`, case-insensitive.
  The rules looking past the token at hand (a `direction` or a `<<fork>>` later on the line, a `{` after two words) read an index of the text built once, so a long line is never scanned again for each token on it.
  A note's text up to `end note` is one pass, each run of spaces before a line read once.
- `grammar` builds the document mermaid's actions build; each composite state's opening counts towards `MAX_NODES`.
- `db` does what `stateDb.ts` hands mermaid's renderer:
  - `docTranslator` names each `[*]` after the document holding it, `_start` where it leads and `_end` where it is led to;
  - it splits a composite state's body at its dividers into regions, the statements after the last a region of its own unless the body ends with a divider;
  - `dataFetcher` walks the documents into nodes and edges, a node made the first time a statement names it and changed by each later one: its descriptions (a state described twice draws them as lines under its label), whether it is a group, its parent where its last naming statement gives one;
  - a note becomes a group of its own holding the note, and a link without a head to its state;
  - a composite state with more than its label is refused.
- Nodes, note groups included, count towards `MAX_NODES` and edges towards `MAX_EDGES` as they are made, each found through an index, so the work stays linear in the statements.

### Class parser

- `lexer` follows `classDiagram.jison`, so a keyword inside a generic type or backquotes is still a keyword, and a line naming a `direction` is one, wherever the lexer stands.
- `db` does what `classDb.ts` does:
  - a class is added the first time a relation, a `class` statement or a member names it, its generic type read from its name;
  - a class named alone adds nothing;
  - an end written `()` facing no other end is a lollipop;
  - a namespace's dotted name makes its parents;
  - an annotation on a class not there yet, and a method with no `(` after its name, are refused as mermaid fails on them.
- `member` reads an attribute or a method as `ClassMember` does: the method's pattern is one pass over the text.

### ER parser

- `lexer` follows `erDiagram.jison`, case-insensitive, so `one`, `many` or `to` naming an entity is a cardinality or a line, and `1` a cardinality before a name or a line.
  A type with tildes runs to the last tilde of its line in one pass.
- `grammar` lets statements share a line, and a subgraph hands its document's names and directions to the db once it ends.
- `db` does what `erDb.ts` does:
  - an entity is added the first time a statement names it, named `entity-<name>-<place>`;
  - an alias given later counts while it has none;
  - a relationship's end named like a subgraph closed before is that subgraph;
  - a subgraph holds each name its document hands it once, but those a subgraph closed before holds.

## Step 3: lay out and draw a graph

Flowcharts, state, class and ER diagrams are drawn by `graph`, with the same fit ladder.
Each kind's `diagram` first turns mermaid's db into the graph drawn (`graph::diagram`): nodes, groups, edges and invisible links, with their text cut into lines.

| Module | Step |
|---|---|
| `layout` | Nodes and groups to cells, in layers; each group is laid out as one block of its parent, so its nodes stay inside its frame |
| `port` | The side and the cell each edge best leaves and joins a block at, and the edges that share a trunk |
| `route` | Each edge as a cheapest right-angled path on the grid |
| `edge_label` | Where each edge's label goes |
| `draw` | Cuts the grid to what was drawn and draws it on a `Canvas` |

`graph::shape` sizes and draws a node; `measure`, built for tests and the `measure` feature only, counts what a drawing does and checks the level C invariants.

### Text

- A node's label breaks at `<br>`, in any case and with spaces before an optional `/`, as mermaid's `lineBreakRegex` reads it, and at the line ends of a quoted or Markdown string.
- It wraps at spaces past 30 columns, or past 20 or 14 when the drawing is too wide; a word of up to 30 columns then keeps its line.
- A longer word is broken after the last of `. / - _ : , = &` in the second half of a line, else at 30 columns; with `LongWords::Cut` it keeps one line cut with `⋯`.
- An edge label or a group title is one line of at most 60 columns, cut with `⋯`.
- The parser leaves out the quotes and backticks around a label; a Markdown string's markup is drawn as written.
- A Font Awesome icon (`fa:fa-car`) is left out with the spaces after it, as no cell draws the icon mermaid shows.
- Entities (`#quot;`, `&amp;`) are shown as the character they name.

### Nodes and groups

- vt-canvas draws each outline (`Canvas::shape`), so every glyph follows the glyph set.
- A node is its label plus a space and the shape's inset on each side.
- A node is a figure (`graph::diagram::Figure`): an outline around its label, a symbol on one cell (`Canvas::symbol`), or a heavy bar across the flow of its group, both without their label.
  Every edge joins a symbol's cell, from any side, and a bar's cells each way across it.
- Compartments: a node may hold lines under its label, each list in a compartment below a rule (`├───┤`), from the left.
  Flowcharts have none; class and ER diagrams draw their members and attributes so.
  A line past 60 columns, as wide as an edge label gets, is cut with `⋯`.
- A group is framed with its title, a concurrent region without a frame (dashed lines between regions), or bare, without a frame or room around, only keeping its blocks together (`graph::diagram::GroupKind`).
- Invisible links (`~~~`): the layout ranks their ends as it does an edge's; they are never routed or drawn.

### Layout

A container at a time, the innermost first:

| Step | Does |
|---|---|
| `layout/acyclic` | Turns round the links closing a cycle, the first block the source names staying first |
| `rank` | Gives each block the earliest rank its links allow, then moves a block with more links leaving than arriving next to the nearest block it leads to |
| `order` | Sweeps down and up 24 times at most, each vertex at the average place of its neighbours, then swaps neighbours while that uncrosses links; keeps the order with the fewest crossings |
| `coordinate` | Gives each layer the places closest to what its vertices ask (the middle of their neighbours) that keep their order and their distance; 8 sweeps down and up, then up to 8 passes pulling each layer towards both neighbours; keeps the placement with the straightest links and the best centred blocks |
| `layered` | Runs them and gives every link a dummy on each layer it passes |

- Weights: a link from one rank to the next weighs 8, a long link 16 between its dummies and 1 at its ends, a link closing a cycle 1.
  So a chain of blocks is one line, and a long link bends at its ends.
- A link to a node inside a group is in line with the node, not with the middle of the group.
- Labels: a link's label has a layer of its own between two ranks, in line with the block the link leads to: a row going down, the label's columns and one more going sideways.
  The router keeps that room for the edge.
- Gaps when the flow goes down or up: 3 rows between ranks, 4 columns between blocks, 2 between links; 2 and 1 when tight.
- Gaps when it goes sideways: 6 columns between ranks, or a label's columns plus 5, and 1 row between blocks.
- Folded: a rank going down wider than the columns allowed keeps the blocks that fit, with 2 columns for each block leading to the others, which move to a new rank after it.
  They stand where they take the least room, and the edges to them share a trunk through the ranks they pass.
- Packed: folded, and neither the links passing a rank nor the labels keep room, so blocks stand closer and edges find their way round them; edges then cross.
- A group has 4 columns fewer than its parent.
  Its frame keeps 2 cells around what it holds, or one more than an edge's tips take at one end, so the edge turns before it crosses the frame; it is wide enough for its title.
- The grid keeps 3 empty cells around everything.

### Ports

- No edge joins a node's side on the row of a compartment's rule.
- An edge leaves and joins a block through the side facing the other end, along the flow of the innermost group holding both.
- The edges of a side each get a cell, in the order of where they go, 2 cells apart while the side has room, the one going straight on the middle.
- A side without a cell for each, as the 1-cell side of a 3-row node, gives one cell and one trunk to its edges that draw alike; a folded or packed layout does so on every side.
- Edges with a hollow head at a side (inheritance) share one cell, one trunk and one head there whatever the room, as UML draws them; an edge leaving that trunk leaves it past the head.
- An edge going down or up through the room kept for its label joins both blocks under that room, as near its middle as each side has a free cell, so labels side by side each stand on their edge.

### Route

Edges between close blocks go first.
Each path is the cheapest on the grid, priced in steps:

| Move | Cost |
|---|--:|
| A step | 1 |
| A bend | 4 |
| Crossing another line | 6 |
| Leaving or joining through another cell of the side than the edge's own, per cell away | 2 |
| Leaving or joining through another side than the one facing the other end | 12, plus 1 per cell off its middle |
| Turning on the arrow's cell, on top of the bend | 3 |
| Running through the room kept for another edge's label | 3 |
| Leaving or joining a trunk where it runs straight | 3 |
| A cell right beside the border of a node the edge does not join | 2.5 |
| Passing behind a frame's title, on top of crossing the frame | 4 |

- Less than a step tells equal paths apart: where they turn, right after the source, or right before the target for an edge that shares only its way in.
- Trunks: an edge may leave an earlier edge of its trunk between that one's first two turns or up to 2 cells before, and join one between its last two turns or up to 2 cells after.
  Each edge leaves before any joins, so no line reads as going from one source to another's target; twin edges are one line.
- An edge left without a path goes first on another try, three times at most.
- Lines cross only at a right angle, never where one turns or ends, and never run on a node's border or along another line they do not share a trunk with.
  They keep a cell off a node they pass wherever going round costs less.
- Two lines crossing draw `┼` (vt-canvas).
- The arrow sits on the cell before the target; a line may turn there into `▾` or `▴`, never into `▸` or `◂`.
- An edge ending on tips (a class relation's head, a crow's foot) runs straight through as many cells next to that end as it has tips, never turning or crossing there, and no later edge crosses them.
  Ranks stand apart by the tips of an edge at both ends and one cell more.
  Tips of two cells or more at one end (crow's feet) keep a line cell beside them before a label or the other end's tips, so the line still shows whether it is dashed.

#### Groups and titles

- A group's title moves along its frame off the column an edge comes straight down through; when the frame is no wider than its title, the edge passes behind it.
- An edge that ends on a group may join its frame under the title: the arrow stands above it.

### Edge labels

- Edge labels go across the straight run through the room the layout kept.
- Else across the longest run that has room, centred, or slid sideways on a vertical run as long as they still cover the line.
- Else beside the run; else the same anywhere along a run, the nearest to its middle first; else nowhere.
- A label slid along a horizontal run keeps a line cell on each side, so it never touches a crossing.
- Texts at an edge's ends (a class relation's cardinalities) stand beside the straight run leaving each end, as near the node as there is room.
  Right of a vertical run, else left; above a horizontal run, else below; a cell clear on each side, after every edge label.

## Step 3, sequence diagrams: lay out and draw

| Module | Step |
|---|---|
| `diagram` | Turns the db into what is drawn: participants by their place, with their figure, and events (messages, notes, activations, block borders and dividers) |
| `layout` | Each participant's column (`layout::columns`), then each event on rows of its own, top to bottom |
| `draw` | Lifelines, boxes of participants, block frames, heads, notes, then messages, on a `Canvas` |

### Diagram

- A participant is a box, a stick figure for `actor`, or a cylinder for `database`.
- Texts are decoded, and messages numbered as mermaid counts them.
- Each creation is tied to the first message another participant sends it, each destruction to the first message involving it.
- Boxes without participants are left out.

### Columns

- Heads stand 3 spaces apart, or 1 when tight, 5 more where a box of participants starts or ends.
- Then each event asks for room:
  - a message, its label's columns plus 3 between its two lifelines;
  - a note beside a lifeline, its width plus 3 to the next one;
  - a message to itself, its label plus 4 to the right;
  - a block, 2 columns more on each side for each level of block it holds.
- A need spanning several gaps is shared among them, the narrowest spans first.

### Rows

- One empty row between two events.
- A message is its label's lines, then its arrow's row; a message to itself is its label, then a loop of two rows.
- A note is its box; a block's start, divider and end are a row each.
- A participant created later has its head where the message creating it arrives.
- One destroyed has its lifeline end on the message destroying it, with a cross and no foot.

### Drawing

- A label goes centred in the gap next to the sender when it fits there, else centred over the whole span.
- A block's title and its dividers' widen its frame up to the width labels wrap to, and are cut with `⋯` past that.
- Lifelines are light, heavy where the participant is active (`activate`, `+`), from the message that starts it to the one that ends it; they join their heads' borders.
- A message, a block's border or divider and a note pass in front of a lifeline: the lifeline stops there instead of crossing.
- Arrows: solid `─` or dotted `╌`.
  A filled, open or half arrow head draws `▸`, an open one (`->`) none, a cross (`-x`) `╳`, and a two-way message has a head at each end.
  A half arrow written head first (`/|-`) points back at the sender.
- Boxes of participants frame their heads, with the box's name, and their feet; block frames are square, notes rounded, and `rect` dashed without a title.

## How each graph kind is drawn

### Flowcharts

- `flowchart::shape` maps mermaid's shape names to vt-canvas shapes.
  A shape a node's brackets or its `@{ shape: … }` block names maps to the closest one; `doc`, `flag` or any name without a close match is a rectangle.
- `((text))` is a circle, `(((text)))` a double circle; only a pointed head draws an arrow.
- A node named like a subgraph is that subgraph, as mermaid merges them.
- A subgraph whose id an edge names inside another subgraph moves into it, as mermaid draws it.

### State diagrams

- A state is a rectangle, its descriptions under its label below a rule (a compartment, `Node::compartments`), as mermaid draws them.
- A start is a dot `⬤`, an end a ringed dot `◉`, a choice a diamond `⬥`, each on one cell.
  `●` and `◇` would be East Asian ambiguous; `⬤ ◉ ⬥` are one column wide everywhere.
- A fork or a join is a heavy bar with a cell for each edge of its busier side, two apart, five cells at the least.
- A composite state is a frame with its title, flowing its own `direction`, top-down when it has none, as mermaid lays it out; a transition naming it ends on its frame.
- Transitions point at their target; their labels are one line of at most 60 columns.
- Concurrent regions follow one another along their composite state's flow, linked unseen in turn, each a group without a frame (`GroupKind::Region`).
  A dashed line runs across the frame in the middle of the gap between two of them, meeting its sides (`├╌╌┤`); edges cross it only at a right angle.
- A note is rounded, beside its state: the state and its notes stand in a group without a frame or room around it (`GroupKind::Bare`), flowing right.
  The note is linked to its state by a dashed line without a head, on the left for `left of`.
  When the drawing is too wide, the ladder turns that group top-down and the note goes under its state.

### Class diagrams

- A class is a box: its first annotation as `«interface»` over its name and generic type (`Square<Shape>`), centred, then its attributes, then its methods.
  Each list is in a compartment under a rule when it has any, from the left, as mermaid shows them: visibility, name, a method's parameters and ` : ` return type, `~T~` as `<T>`.
  The classifier `$` or `*` is kept at the end, as no cell is underlined or italic.
- A namespace is a group framed with its label, a lollipop a stadium with its name, a note a rounded box linked to its class by a dashed line without heads.
- A relation is an edge, dashed when written `..`, from its first class to its second, so the first ranks above.
  Its label goes on it as a flowchart edge's, and the texts in quotes at its ends stand beside each end.
- Edges with a hollow head at one side of a class share it, so a parent's children hang from one `▵` on one trunk.
- Heads stand on the cell next to the class, the line straight through it but for an arrow, which may stand where the line turns as a flowchart's does; none is East Asian ambiguous:

  | Written | Means | Drawn (ASCII) |
  |---|---|---|
  | `<\|`, `\|>` | inheritance, realization when dotted | `▵ ▹ ▿ ◃` (`^ > v <`) pointing at the parent |
  | `*` | composition | `⬥` (`#`) |
  | `o` | aggregation | `⬦` (`o`) |
  | `<`, `>` | dependency, association | `▴ ▸ ▾ ◂` (`^ > v <`), the flowchart arrow |
  | `()` | lollipop | `o` |

### ER diagrams

- An entity is a table: its alias, else its name, centred, over one row per attribute.
  Its type, name, keys (`PK,FK`) and comment each stand in a column as wide as its widest cell, two spaces apart; a column no attribute fills is left out.
  A comment wraps at spaces onto rows of its own, so a row keeps within 60 columns while the comment keeps 12.
- A subgraph is a group framed with its title, flowing its own `direction`, else its parent's; an entity named like a subgraph is that subgraph, and a relationship may end on it.
- A relationship is an edge from its first entity to its second, solid when identifying (`--`), dashed when not (`..`), its role on it as a flowchart edge's label.
- Crow's feet take two cells next to each entity, the line straight through them and a line cell beside them before a label; none is East Asian ambiguous outside box drawing.
  Drawn with the entity on the right of a line going right; the fork opens onto the entity whichever side it joins (`< > Ʌ V`):

  | Written | Means | Drawn (ASCII) |
  |---|---|---|
  | `\|\|` | exactly one | `┼┼` (`++`) |
  | `\|o`, `o\|` | zero or one | `o┼` (`o+`) |
  | `}\|`, `\|{` | one or more | `┼<` (`+<`) |
  | `}o`, `o{` | zero or more | `o<` (`o<`) |
  | `u` | parent of a generalisation | nothing, as mermaid draws none |

## Time

Measured on an Apple M5 in a release build with `cargo bench --bench limits` and `cargo bench --bench corpus`, the fastest of a few runs on a shared machine.
The layout takes microseconds; the grid routing takes nearly all the time, more with every placement tried.
The scoreboard times the corpus again on each full run, in [`docs/comparison.md`](docs/comparison.md#speed).

### The corpus, at 100 columns

| Kind | Inputs | Median | p95 | p99 | Slowest |
|---|--:|--:|--:|--:|--:|
| Flowchart | 946 | 15 µs | 0.6 ms | 1.4 ms | 19 ms |
| Sequence | 394 | 12 µs | 0.18 ms | 0.33 ms | 0.42 ms |
| State | 279 | 50 µs | 0.32 ms | 0.45 ms | 5.6 ms |
| Class | 372 | 16 µs | 0.18 ms | 1.8 ms | 8.8 ms |
| ER | 315 | 21 µs | 0.16 ms | 1.1 ms | 1.2 ms |

The slowest of each:

- Flowchart: `e2e--platform-dev-diagrams-tmp-v12-0-0-faulty-diags-elk-legacy-preset-regression`, packed after every other placement was too wide, and routed again with a stuck edge first.
- Sequence: `e2e--diagrams-sequence-nightly-check-should-render-all-message-types-and-arrow-types-`, 88 messages.
- State: `e2e--diagrams-state-diagram-v2-pr-check-should-render-edge-labels-correctly-with-mult`, nine states with long names and seventeen labelled transitions.
- Class: `e2e--diagrams-class-diagram-visual-check-forest-theme`, eight pairs of classes side by side, packed; `tests/class/library.mmd` takes 0.23 ms.
- ER: `e2e--diagrams-er-diagram-visual-check-er-diagram-with-all-relationship-types-neo-look-2`, every kind of crow's foot; `tests/er/shop.mmd` takes 0.16 ms.

### At the limits

| Input | Result | Time |
|---|---|--:|
| 64 KiB of flowchart edges | Refused by the parser at the 201st node, most of it lexing the whole source | 2.4 ms |
| 200 nodes with 150 edges | Drawn | 12 ms |
| 150 edges between 20 nodes | Drawn | 54 ms |
| 51 links passing 98 ranks each, about 5000 places kept for links | Drawn | 8 ms |
| A sideways tree routed on 233000 cells | Drawn | 43 ms |
| 50 participants and 1000 messages, each crossing every lifeline | `TooLarge`, over the canvas's million cells | 7.6 ms |
| The same at 80 columns | Every placement tried, all too wide | 37 ms |
| 499 blocks nested around one message | Too wide | 9 ms |
| 64 KiB of messages | Refused at the message limit | 1.8 ms |
| 200 described states and 150 labelled transitions down a tree | Drawn | 50 ms |
| 199 composite states nested around one state | `TooLarge`, over the routing grid's cells | 0.85 ms |
| 64 KiB of transitions | Refused at the 151st edge, most of it reading the whole source | 2.3 ms |
| 200 classes with 150 inheritance relations | Drawn | 64 ms |
| 64 KiB of attributes in one class, a box of 2982 rows | Drawn | 2.4 ms |
| 200 entities with 150 relationships, 245 rows tall | Drawn | 204 ms |
| 64 KiB of attributes in one entity, a table of nearly 3000 rows | Drawn | 3.3 ms |

### Found by fuzzing

- A 27 KiB flowchart took mermaid-rs-renderer 35 s to lay out.
  Mended where the mutation broke mermaid's grammar, 19 subgraphs and 149 edges, it is drawn in 0.33 s, 5.3 million routing steps at 1000 columns.
  At 80 columns it spends the whole budget and is refused as `TooComplex` after 0.49 s (`benches/inputs/fuzzed-subgraphs.mmd`).
- A 2 KiB ER diagram, subgraphs named alike around a relationship repeated between two entities, is refused as `Unroutable` once the same edge is stuck in two placements: after 0.07 s at 200 columns, 0.19 s at most (`benches/inputs/fuzzed-er.mmd`).

## Tests, corpus and tools

### Fuzzing

`fuzz/` is seeded from `corpus/flowchart`, and from `corpus/sequence`, `corpus/class`, `corpus/state` and `corpus/er` for those kinds.

| Target | Feeds | Checks |
|---|---|---|
| `render` | Any source to `render` | It never panics and every row fits |
| `parse` | The flowchart parser alone, through `read_flowchart` | It never panics and keeps the limits |
| `parse_sequence`, `parse_class`, `parse_state`, `parse_er` | Those parsers, through `read_sequence`, `read_class`, `read_state` and `read_er` | The same |

Run one with `cargo +nightly fuzz run <target>`.
On stable, build a target with `cargo build --release --target <host>` and the sanitizer-coverage `RUSTFLAGS` cargo-fuzz passes, then run the binary.

Runs so far:


| Target | Time | Inputs | Found |
|---|---|--:|---|
| Flowchart parser | 10 min | 1.8 million | No panic, no input that took a second |
| Sequence parser | 10 min | 9.2 million | None |
| `render`, seeded with sequence diagrams | 20 min | 4.9 million | None |
| State parser | 10 min | 11.4 million | None |
| `render`, seeded with state diagrams | 20 min | 0.55 million | None |
| Class parser | 10 min | 14.8 million | None |
| ER parser | two runs | 7.5 million | None |
| `render`, seeded with class diagrams | 20 min | 0.24 million | None |
| `render`, seeded with ER diagrams | 10 min | 0.29 million | Subgraphs named alike holding each other overflowed a frame, mended; `fuzzed-er.mmd`, above |

Read by mermaid itself, the 2785 state diagrams the state parser's fuzzing kept are accepted and refused alike, except where mermaid's YAML reader refuses a front matter, or over the node limit; the 1130 both read hold as many nodes and edges.

### Where things are

- [`corpus/`](corpus/README.md): Mermaid inputs harvested from the mermaid repo at a pinned release, one file per case, grouped by diagram kind.
- [`oracle/`](oracle/README.md): a Node script that records what the pinned `mermaid` accepts (level A) and its flowchart, sequence, state, class and ER dbs (level B), committed as JSON.
- Snapshot lists, each an insta snapshot per diagram:
  - `tests/spike/flowcharts.txt`: the 20 flowcharts of the spike, 19 from the corpus and `tests/spike/cached.mmd`; each passes the level C checks of `measure` (every node and label drawn whole, every edge joining both ends, nothing overlapping).
  - `tests/sequence/diagrams.txt`: sequence diagrams from the corpus and `tests/sequence/checkout.mmd`, at 100 columns; every corpus diagram mermaid accepts is drawn within 80 columns but two, with eight and ten participants side by side.
  - `tests/state/diagrams.txt`: state diagrams from the corpus and `tests/state/`, at 100 columns; every corpus diagram mermaid accepts is drawn within 80 columns and passes the level C checks, but for labels the layout leaves without room.
  - `tests/class/diagrams.txt`: class diagrams from the corpus and `tests/class/library.mmd`, at 100 columns, keeping the level C invariants; every corpus diagram mermaid accepts is drawn within 80 columns, the four packing eight pairs of classes side by side with 16 labels left out.
  - `tests/er/diagrams.txt`: ER diagrams from the corpus and `tests/er/shop.mmd`, at 100 columns, keeping the level C invariants; every corpus diagram mermaid accepts is drawn within 80 columns, those breaking the invariants only with wide characters the measures miscount, a comment cut past 60 columns or a relationship of an entity to itself.
- [`scoreboard/`](scoreboard/README.md): every corpus flowchart, sequence, state, class and ER diagram drawn by vt-mermaid and three other text renderers, measured the same way, results committed; a dev-only example, run by hand.
  It also writes [`docs/comparison.md`](docs/comparison.md).
- `benches/`: `limits` times a typical diagram of each kind and inputs built at each limit with divan; `corpus` times every corpus diagram of the five kinds and prints the spread of each.
  Both run with `cargo bench` only.
- `fuzz/`: a cargo-fuzz crate, its own workspace, out of the library's build and of `scripts/check`; see [Fuzzing](#fuzzing).
- `examples/agreement.rs`: compares mermaid-rs-renderer's parser with the oracle per kind; a dev-only program, so the library's API never sees the dependency.
- `examples/compare.rs`: prints each spike flowchart drawn by mermaid-text, then by vt-mermaid; mermaid-text is a dev-dependency only.

### The published package

The package holds `src/`, the README, the licence, this page, and only the inputs and snapshots the library's own tests read, with `corpus/LICENSE-mermaid` for the mermaid inputs among them.
`scripts/check` packs it and runs its tests, except those reading the corpus, the oracle or a list of diagrams, whose names say `corpus`, `listed` or `spike`.

## Dependencies

```text
vt-canvas ← vt-mermaid
```

- vt-mermaid depends on vt-canvas alone, from crates.io.
- vt-graph, the chart crate planned for chart kinds, is not published yet and depends on vt-canvas too.
- The parser and the layout are its own.
- `mermaid-rs-renderer` is a dev-dependency of `examples/agreement.rs` only.
