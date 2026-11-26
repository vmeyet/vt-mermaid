# Architecture

`vt-mermaid` is one library crate.
Mermaid diagrams drawn as text on a vt-canvas grid.
This page says how the code is laid out today; when a module or a boundary changes, change this page in the same change.

## Owns

- Mermaid source to a text drawing, one diagram kind at a time; flowcharts, sequence, state, class and ER diagrams today.
- Laying a graph (a flowchart, a state, class or ER diagram) out in layers on the grid, then routing its edges on the grid.
- Laying a sequence diagram out: each participant's column, then each event on rows of its own.
- Handing chart kinds (xychart, pie, sankey…) to vt-graph.

## Never

- Reads files, the environment or the clock; prints; exits the process.
- Writes ANSI escapes or picks colours: it hands back cells with colour roles, the caller maps them and writes the terminal.
- Lets a dependency's type into a public signature, except vt-canvas's `Canvas`, `GlyphSet` and `Run`: the cell format every vt crate hands back, re-exported so a caller needs no second dependency.
  The roles in its cells are vt-mermaid's own `Role` (`role`): `Node`, `Edge`, `Label`, `Group`, and `Plain` for empty cells.

## Contract

- `render(source, max_width)` returns the drawing as a `Canvas`, at most `max_width` columns wide; a `max_width` over vt-canvas's 1000 columns (`MAX_WIDTH`) counts as 1000, in `Settings::new` and again in `render_with`, as the field is public.
  `render_with(source, settings)` does the same as `Settings` say: `max_width`, and `long_words`, `Break` unless set.
  A flowchart or a state diagram too wide, or with an edge left without a path, is laid out again until it fits: tight; tight with node labels wrapped at 20 then 14 columns; with the ranks going down folded to 8, 7, 6, 5 and 4 eighths of the width; folded again to 8 down to 2 eighths with labels wrapped at 14; then packed to 8, 6 and 4 eighths without room for the links passing a rank; each as written, then turned top-down when it or one of its groups flows sideways.
  That is at most thirty-eight placements, each routed only when its blocks fit; one still too wide is `Error::TooWide`, so the caller can fall back.
  The ladder stops once the same edge found no path in two placements (`Error::Unroutable`), and once the path searches of every placement together took 6 million cells off their queues (`MAX_ROUTING_STEPS`, `Error::TooComplex`); the searches share their memory, a stamp per state telling one search's costs from the last.
  A sequence diagram too wide is laid out again: its labels as written, then wrapped at spaces to 60, 48, 40, 32, 26, 20, 16, 12, 10, 8 and 6 columns, a word up to 30 columns kept whole; the same with heads 1 space apart instead of 3; then wrapped to 30 columns down to 6, one at a time, every word wider than the lines broken or cut as `long_words` says.
  That is at most forty-nine placements, each a few passes over the events; one still too wide is `Error::TooWide`.
- Limits, each an `Error` the caller can match: 64 KiB of source (`MAX_SOURCE_BYTES`), 200 nodes and subgraphs together (`MAX_NODES`), 150 edges (`MAX_EDGES`), invisible links included, 250000 cells on the routing grid (`MAX_AREA`), a million on the canvas (`MAX_CELLS`), `TooLarge` naming the one it is over; a sequence diagram has at most 50 participants (`MAX_PARTICIPANTS`) and 1000 messages (`MAX_MESSAGES`), notes, activations and the starts, sections and ends of blocks included; a state diagram counts towards `MAX_NODES` the nodes mermaid's renderer gets, start and end dots, notes, their groups and concurrent regions included, and each composite state's opening while it is read, and towards `MAX_EDGES` its transitions and the links of its notes.
  The parser counts nodes and edges as it reads them, an `&` group's edges before it makes them, and stops at the first over a limit: the count an error carries is where reading stopped, a lower bound of what the source holds.
- Time, measured on an Apple M5 in a release build with `cargo bench --bench limits` and `cargo bench --bench corpus`, the fastest of a few runs on a shared machine:
  - The corpus's 946 flowcharts at 100 columns: median 15 µs, p95 0.6 ms, p99 1.4 ms, slowest 19 ms (`e2e--platform-dev-diagrams-tmp-v12-0-0-faulty-diags-elk-legacy-preset-regression`, packed after every other placement was too wide, and routed again with a stuck edge first).
  - The layout takes microseconds; the grid routing takes nearly all the time, more with every placement tried.
  - At each limit: 64 KiB of edges is refused by the parser at the 201st node after 2.4 ms, most of it lexing the whole source; 200 nodes with 150 edges take 12 ms; 150 edges between 20 nodes 54 ms; 51 links passing 98 ranks each, about 5000 places kept for links, 8 ms; a sideways tree routed on 233000 cells 43 ms.
  - The corpus's 394 sequence diagrams at 100 columns: median 12 µs, p95 0.18 ms, p99 0.33 ms, slowest 0.42 ms (`e2e--diagrams-sequence-nightly-check-should-render-all-message-types-and-arrow-types-`, 88 messages).
  - At the sequence limits: 50 participants and 1000 messages each crossing every lifeline are refused as `TooLarge`, over the canvas's million cells, after 7.6 ms; the same at 80 columns, every placement tried and all too wide, 37 ms; 499 blocks nested around one message, too wide, 9 ms; 64 KiB of messages refused at the message limit after 1.8 ms.
  - The corpus's 279 state diagrams at 100 columns: median 50 µs, p95 0.32 ms, p99 0.45 ms, slowest 5.6 ms (`e2e--diagrams-state-diagram-v2-pr-check-should-render-edge-labels-correctly-with-mult`, nine states with long names and seventeen labelled transitions).
  - At the state limits: 200 described states and 150 labelled transitions down a tree of them are drawn in 50 ms; 199 composite states nested around one state are refused as `TooLarge`, over the routing grid's cells, after 0.85 ms; 64 KiB of transitions is refused at the 151st edge after 2.3 ms, most of it reading the whole source.
  - The corpus's 372 class diagrams at 100 columns: median 16 µs, p95 0.18 ms, p99 1.8 ms, slowest 8.8 ms (`e2e--diagrams-class-diagram-visual-check-forest-theme`, eight pairs of classes side by side, packed); `tests/class/library.mmd` 0.23 ms.
  - At the class limits: 200 classes with 150 inheritance relations take 64 ms; 64 KiB of attributes in one class, a box of 2982 rows, 2.4 ms.
  - The corpus's 315 ER diagrams at 100 columns: median 21 µs, p95 0.16 ms, p99 1.1 ms, slowest 1.2 ms (`e2e--diagrams-er-diagram-visual-check-er-diagram-with-all-relationship-types-neo-look-2`, every kind of crow's foot); `tests/er/shop.mmd` 0.16 ms.
  - At the ER limits: 200 entities with 150 relationships take 204 ms, 245 rows tall; 64 KiB of attributes in one entity, a table of nearly 3000 rows, 3.3 ms.
  - Fuzzing found a 27 KiB flowchart that took mermaid-rs-renderer 35 s to lay out; mended where the mutation broke mermaid's grammar, 19 subgraphs and 149 edges, it is drawn in 0.33 s, 5.3 million routing steps at 1000 columns; at 80 columns it spends the whole budget and is refused as `TooComplex` after 0.49 s (`benches/inputs/fuzzed-subgraphs.mmd`).
- The limits bound the layout before it runs: at most 200 blocks on 200 ranks, a layer for each rank and one for the labels after it, and a place on each layer for each of 150 links, so 60000 places at most; ordering sweeps them 24 times at most and placing 16.
  The library has no clock: the routing budget bounds the work instead, deterministic on every machine.
  The corpus's drawings take at most 263000 steps at 40 to 1000 columns, the inputs above 3 million at most but `fuzzed-subgraphs` 5.3 million; an input spending the whole budget is refused after about half a second, as `fuzzed-subgraphs` is at 80 columns.
- No font is ever loaded: a node is as wide as its label's columns, so the drawing is the same on every machine.
- Ten minutes of fuzzing the parser alone, 1.8 million inputs, found no panic and no input that took a second.
  Ten minutes of the sequence parser alone, 9.2 million inputs, and twenty of `render` seeded with sequence diagrams, 4.9 million, found none either.
  Ten minutes of the state parser alone, 11.4 million inputs, and twenty of `render` seeded with state diagrams, 0.55 million, found none either.
  Ten minutes of the class parser alone, 14.8 million inputs, two runs of the ER parser, 7.5 million, and twenty minutes of `render` seeded with class diagrams, 0.24 million, found none either.
  Ten minutes of `render` seeded with ER diagrams, 0.29 million inputs, found subgraphs named alike holding each other, which overflowed a frame, mended, and a 2 KiB input, subgraphs named alike around a relationship repeated between two entities, refused as `Unroutable` once the same edge is stuck in two placements, after 0.07 s at 200 columns and 0.19 s at most (`benches/inputs/fuzzed-er.mmd`).
  Read by mermaid itself, the 2785 state diagrams the state parser's fuzzing kept are accepted and refused alike but where mermaid's YAML reader refuses a front matter, or over the node limit, and the 1130 both read hold as many nodes and edges.
- The `measure` feature adds `measure(source, max_width)` for flowcharts, state, class and ER diagrams: crossings, bends, edge cells, cells beside a node and level C violations, read from where each part went.
  It exists for the [scoreboard](scoreboard/README.md), is hidden from the docs, unstable and not part of the API.
- Built with `--cfg fuzzing`, as cargo-fuzz builds, the crate adds `read_flowchart(source)`, `read_state(source)`, `read_class(source)` and `read_er(source)`, each parser alone within the limits, returning `GraphCounts`, and `read_sequence(source)`, returning `SequenceCounts`, for `fuzz/`; no feature turns them on, so they are no part of the API; `scripts/check` lints them with `cargo clippy --lib -- --cfg fuzzing`.

## Every kind

`source` holds what every diagram kind shares before its own parser: mermaid's preparation of the text, its entities, the YAML of `@{ … }` blocks, and the word a source opens with once the front matter, directives and comment lines are left out.
`kind` holds one table of every header mermaid knows, each a prefix of that word, with the kind it names: a graph kind gives the function reading the prepared text into the graph drawn, which `graph` lays out, routes and draws; a sequence diagram draws its own way; a kind mermaid knows but not drawn here has none.
Every graph kind's parser reads within one `graph::Limits`, and `graph::measured` measures any of them.
`render` checks the length, prepares the text once, looks the header up and hands the prepared text to that kind; a kind mermaid knows but not drawn here is `Error::Unsupported`, quoting the source's first word, at most 32 ASCII letters, digits and `-`.
`role` holds `Role`, what each cell of a drawing is drawn for.

## Flowcharts

`render` runs seven steps, one module each: `parse` and `diagram` under `src/flowchart/`, the others under `src/graph/`, the layered layout, routing and drawing every kind drawn as a graph shares, with its fit ladder.

| Module | Step |
|---|---|
| `parse` | Reads the source as mermaid 12.3.0 does, into its flowchart db: nodes, edges, subgraphs, classes and styles |
| `diagram` | Turns the db into the graph drawn (`graph::diagram`): nodes, groups (subgraphs), edges and invisible links, with their text cut into lines |
| `layout` | Nodes and groups to cells, in layers; each group is laid out as one block of its parent, so its nodes stay inside its frame |
| `port` | The side and the cell each edge best leaves and joins a block at, and the edges that share a trunk |
| `route` | Each edge as a cheapest right-angled path on the grid |
| `edge_label` | Where each edge's label goes |
| `draw` | Cuts the grid to what was drawn and draws it on a `Canvas` |

`graph::shape` sizes and draws a node, `flowchart::shape` maps mermaid's shape names to vt-canvas shapes; the crate's `label`, shared with sequence diagrams, cuts text into lines.
`parse` is mermaid's own flowchart parser rewritten by hand, so a flowchart means here what it means to mermaid, in four steps:
`prepare` does to the text what mermaid does before its lexer: the steps every kind shares, from `source` (line ends, the front matter, `%%{…}%%` directives, comment lines, entities marked), then the space after a `}`;
`lexer` applies the rules of `flow.jison` in their order, the first that matches winning, each start condition a state on a stack;
`grammar` reads the tokens top-down, accepting what mermaid's LALR parser accepts and calling the db in the order its actions do;
`db` does what `flowDb.ts` does with each statement: a node is created the first time it is named, `@{ … }` blocks set its shape and label, a link (`link`) becomes edges between every node on either side, a subgraph keeps only the nodes no earlier subgraph holds, a repeated id merges, an unknown shape or a `linkStyle` past the last edge is refused.
Every pattern runs in one pass over the text: no regex, no backtracking on the source.
`parse::agreement`, a test, compares it with the oracle on every corpus flowchart: it accepts and refuses the 946 as mermaid does (level A), and reads the 925 mermaid accepts into the same nodes, edges, labels and subgraphs (level B).
mermaid-rs-renderer's parser, which it replaces, accepts and refuses 908 of them as mermaid does, and reads the same nodes, edges and labels in 814 even through the pre-parse shim it needed.
`measure`, built for tests and the `measure` feature only, counts what a drawing does and checks the level C invariants.

- Text: a node's label breaks at `<br>`, in any case and with spaces before an optional `/` as mermaid's `lineBreakRegex` reads it, and at the line ends of a quoted or Markdown string, and wraps at spaces past 30 columns, or past 20 or 14 when the drawing is too wide, a word of up to 30 columns then keeping its line, a longer word is broken after the last of `. / - _ : , = &` in the second half of a line, else at 30 columns, or with `LongWords::Cut` keeps one line cut with `⋯`; an edge label or a group title is one line of at most 60 columns, cut with `⋯`; the parser leaves out the quotes and backticks around a label, a Markdown string's markup is drawn as written, a Font Awesome icon (`fa:fa-car`) is left out with the spaces after it, as no cell draws the icon mermaid shows, and entities (`#quot;`, `&amp;`) are shown as the character they name.
- Invisible links (`~~~`): the layout ranks their ends as it does an edge's; they are never routed or drawn.
- Compartments: a node may hold lines under its label, each list in a compartment below a rule (`├───┤`), from the left; flowcharts have none, class and ER diagrams draw their members and attributes so; a line past 60 columns, as wide as an edge label gets, is cut with `⋯`.
- Shapes: vt-canvas draws each outline (`Canvas::shape`), so every glyph follows the glyph set.
  A group is framed with its title, a concurrent region without a frame (dashed lines between regions), or bare, without a frame or room around, only keeping its blocks together (`graph::diagram::GroupKind`).
  A node is a figure (`graph::diagram::Figure`): an outline around its label, a symbol on one cell (`Canvas::symbol`) or a heavy bar across the flow of its group, both without their label; every edge joins a symbol's cell, from any side, and a bar's cells each way across it.
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
  A group has 4 columns fewer than its parent; its frame keeps 2 cells around what it holds, or one more than an edge's tips take at one end so the edge turns before it crosses the frame, and is wide enough for its title; the grid keeps 3 empty cells around everything.
- Ports: no edge joins a node's side on the row of a compartment's rule. An edge leaves and joins a block through the side facing the other end, along the flow of the innermost group holding both; the edges of a side each get a cell, in the order of where they go, 2 cells apart while the side has room, the one going straight on the middle.
  A side without a cell for each, as the 1-cell side of a 3-row node, gives one cell and one trunk to its edges that draw alike; a folded or packed layout does so on every side.
  Edges with a hollow head at a side (inheritance) share one cell, one trunk and one head there whatever the room, as UML draws them; an edge leaving that trunk leaves it past the head.
  An edge going down or up through the room kept for its label joins both blocks under that room, as near its middle as each side has a free cell, so labels side by side each stand on their edge.
- Route: edges between close blocks first; a step costs 1, a bend 4, crossing another line 6, another cell of the side than the edge's own 2 a cell, another side 12, turning on the arrow's cell 3 more, a cell kept for another edge's label 3, a cell right beside the border of a node the edge does not join 2.5, passing behind a frame's title 4 more.
  Equal paths are told apart by where they turn, right after the source, or right before the target for an edge that shares only its way in.
  Trunks: an edge may leave an earlier edge of its trunk between that one's first two turns or up to 2 cells before, and join one between its last two turns or up to 2 cells after, each edge leaving before any joins, so no line reads as going from one source to another's target; twin edges are one line.
  A group's title moves along its frame off the column an edge comes straight down through; when the frame is no wider than its title, the edge passes behind it.
  An edge that ends on a group may join its frame under the title: the arrow stands above it.
  An edge left without a path goes first on another try, three times at most.
  Lines cross only at a right angle, never where one turns or ends, and never run on a node's border or along another line they do not share a trunk with; they keep a cell off a node they pass wherever going round costs less.
  The arrow sits on the cell before the target; a line may turn there into `▾` or `▴`, never into `▸` or `◂`.
  An edge ending on tips (a class relation's head, a crow's foot) runs straight through as many cells next to that end as it has tips, never turning or crossing there, and no later edge crosses them; ranks stand apart by the tips of an edge at both ends and one cell more; tips of two cells or more at one end (crow's feet) keep a line cell beside them before a label or the other end's tips, so the line still shows whether it is dashed.
- Texts at an edge's ends (a class relation's cardinalities) stand beside the straight run leaving each end, as near the node as there is room: right of a vertical run, else left; above a horizontal run, else below; a cell clear on each side, after every edge label.
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

`render` then runs three steps, one module each:

| Module | Step |
|---|---|
| `diagram` | Turns the db into what is drawn: participants by their place, with their figure (a box, a stick figure for `actor`, a cylinder for `database`), and events (messages, notes, activations, block borders and dividers); texts decoded, messages numbered as mermaid counts them, each creation tied to the first message another participant sends it and each destruction to the first message involving it, boxes without participants left out |
| `layout` | Each participant's column (`layout::columns`), then each event on rows of its own, top to bottom |
| `draw` | Lifelines, boxes of participants, block frames, heads, notes, then messages, on a `Canvas` |

- Columns: heads 3 spaces apart, or 1 when tight, 5 more where a box of participants starts or ends; then each message needs its label's columns plus 3 between its two lifelines, a note beside a lifeline its width plus 3 to the next one, a message to itself its label plus 4 to the right, and a block 2 columns more on each side for each level of block it holds; a need spanning several gaps is shared among them, the narrowest spans first.
- Rows: one empty row between two events; a message is its label's lines then its arrow's row, a message to itself its label then a loop of two rows, a note its box, a block's start, divider and end a row each; a participant created later has its head where the message creating it arrives, and one destroyed has its lifeline end on the message destroying it, with a cross and no foot.
- A label goes centred in the gap next to the sender when it fits there, else centred over the whole span; a block's title and its dividers' widen its frame up to the width labels wrap to, and are cut with `⋯` past that.
- Lifelines are light, heavy where the participant is active (`activate`, `+`), from the message that starts it to the one that ends it; they join their heads' borders.
- A message, a block's border or divider and a note pass in front of a lifeline: the lifeline stops there instead of crossing.
- Arrows: solid `─` or dotted `╌`; a filled, open or half arrow head draws `▸`, an open one (`->`) none, a cross (`-x`) `╳`, and a two-way message has a head at each end; a half arrow written head first (`/|-`) points back at the sender.
- Boxes of participants frame their heads, with the box's name, and their feet; block frames are square, notes rounded, and `rect` dashed without a title.

## State diagrams

`parse` is mermaid's own state parser rewritten by hand, its lexer first: the rules of `stateDiagram.jison` in their order, case-insensitive, the first that matches winning, each start condition a state on a stack.
The rules looking past the token at hand (a `direction` or a `<<fork>>` later on the line, a `{` after two words) read an index of the text built once, so a long line is never scanned again for each token on it; a note's text up to `end note` is one pass, each run of spaces before a line read once.
`grammar` reads the tokens top-down, accepting what mermaid's LALR parser accepts and building the document its actions build; the composite states open at a point are a stack, so nesting never deepens the call stack, and each opening counts towards `MAX_NODES`.
`db` does what `stateDb.ts` hands mermaid's renderer: `docTranslator` names each `[*]` after the document holding it, `_start` where it leads and `_end` where it is led to, and splits a composite state's body at its dividers into regions, the statements after the last a region of its own unless the body ends with a divider; then `dataFetcher` walks the documents into nodes and edges, a node made the first time a statement names it and changed by each later one: its descriptions (a state described twice draws them as lines under its label), whether it is a group, its parent where its last naming statement gives one; a note becomes a group of its own holding the note, and a link without a head to its state; a composite state with more than its label is refused.
Nodes, note groups included, count towards `MAX_NODES` and edges towards `MAX_EDGES` as they are made, each found through an index, so the work stays linear in the statements.
Every pattern runs in one pass over the text: no regex, no backtracking on the source.
`parse::agreement`, a test, compares it with the oracle on every corpus state diagram: it accepts and refuses the 279 as mermaid does (level A), and reads the 276 mermaid accepts into the same nodes (id, shape, label, descriptions, parent, direction, note side) and edges (ends, label, head) (level B).

`render` then turns the db into the graph drawn (`diagram`) and draws it as a flowchart is drawn, with the same fit ladder:

- A state is a rectangle, its descriptions under its label below a rule (a compartment, `Node::compartments`), as mermaid draws them; a start is a dot `⬤`, an end a ringed dot `◉`, a choice a diamond `⬥`, each on one cell, and a fork or a join a heavy bar with a cell for each edge of its busier side, two apart, five cells at the least.
  `●` and `◇` would be East Asian ambiguous; `⬤ ◉ ⬥` are one column wide everywhere.
- A composite state is a frame with its title, flowing its own `direction`, top-down when it has none, as mermaid lays it out; a transition naming it ends on its frame.
- Transitions point at their target; their labels are one line of at most 60 columns.
- Concurrent regions follow one another along their composite state's flow, linked unseen in turn, each a group without a frame (`GroupKind::Region`), and a dashed line runs across the frame in the middle of the gap between two of them, meeting its sides (`├╌╌┤`); edges cross it only at a right angle.
- A note is rounded, beside its state: the state and its notes stand in a group without a frame or room around it (`GroupKind::Bare`), flowing right, the note linked to its state by a dashed line without a head, on the left for `left of`; when the drawing is too wide, the ladder turns that group top-down and the note goes under its state.

## Class diagrams

`parse` is mermaid's own class parser rewritten by hand, in three steps after the shared preparation:
`lexer` applies the rules of `classDiagram.jison` in their order, the first that matches winning, each start condition a state on a stack, so a keyword inside a generic type or backquotes is still a keyword and a line naming a `direction` is one, wherever the lexer stands;
`grammar` reads the tokens top-down, accepting what mermaid's LALR parser accepts and calling the db in the order its actions do;
`db` does what `classDb.ts` does: a class is added the first time a relation, a `class` statement or a member names it, its generic type read from its name, a class named alone adds nothing, an end written `()` facing no other end is a lollipop, and a namespace's dotted name makes its parents; an annotation on a class not there yet, and a method with no `(` after its name, are refused as mermaid fails on them.
`member` reads an attribute or a method as `ClassMember` does: the method's pattern is one pass over the text, never a backtracking regex.
A class diagram counts its classes, lollipops, notes and namespaces against `MAX_NODES`, its relations and the links of notes to classes against `MAX_EDGES`.
`parse::agreement`, a test, compares it with the oracle on every corpus class diagram: it accepts and refuses the 372 as mermaid does (level A), and reads the 315 mermaid accepts into the same classes, members, namespaces, relations and notes (level B).

`diagram` turns the db into the graph drawn, which `graph` lays out, routes and draws as it does a flowchart:

- A class is a box: its first annotation as `«interface»` over its name and generic type (`Square<Shape>`), centred, then its attributes, then its methods, each list in a compartment under a rule when it has any, from the left, as mermaid shows them: visibility, name, a method's parameters and ` : ` return type, `~T~` as `<T>`, and the classifier `$` or `*` kept at the end, as no cell is underlined or italic.
- A namespace is a group framed with its label, a lollipop a stadium with its name, a note a rounded box linked to its class by a dashed line without heads.
- A relation is an edge, dashed when written `..`, from its first class to its second, so the first ranks above; its label goes on it as a flowchart edge's, and the texts in quotes at its ends stand beside each end.
- Heads, each on the cell next to the class, the line straight through it but for an arrow, which may stand where the line turns as a flowchart's does; none is East Asian ambiguous:

  | Written | Means | Drawn (ASCII) |
  |---|---|---|
  | `<\|`, `\|>` | inheritance, realization when dotted | `▵ ▹ ▿ ◃` (`^ > v <`) pointing at the parent |
  | `*` | composition | `⬥` (`#`) |
  | `o` | aggregation | `⬦` (`o`) |
  | `<`, `>` | dependency, association | `▴ ▸ ▾ ◂` (`^ > v <`), the flowchart arrow |
  | `()` | lollipop | `o` |

- Edges with a hollow head at one side of a class share it, so a parent's children hang from one `▵` on one trunk.

## ER diagrams

`parse` is mermaid's own ER parser rewritten by hand, in three steps after the shared preparation:
`lexer` applies the rules of `erDiagram.jison` in their order, case-insensitive, the first that matches winning, each start condition a state on a stack, so `one`, `many` or `to` naming an entity is a cardinality or a line, and `1` a cardinality before a name or a line; a type with tildes runs to the last tilde of its line in one pass, never a backtracking regex;
`grammar` reads the tokens top-down, accepting what mermaid's LALR parser accepts and calling the db in the order its actions do: statements may share a line, and a subgraph hands its document's names and directions to the db once it ends;
`db` does what `erDb.ts` does: an entity is added the first time a statement names it, named `entity-<name>-<place>`, an alias given later counts while it has none, a relationship's end named like a subgraph closed before is that subgraph, and a subgraph holds each name its document hands it once but those a subgraph closed before holds.
An ER diagram counts its entities and subgraphs against `MAX_NODES`, its relationships against `MAX_EDGES`.
`parse::agreement`, a test, compares it with the oracle on every corpus ER diagram: it accepts and refuses the 315 as mermaid does (level A), and reads the 312 mermaid accepts into the same entities, attributes, relationships and subgraphs (level B).

`diagram` turns the db into the graph drawn, which `graph` lays out, routes and draws as it does a flowchart:

- An entity is a table: its alias, else its name, centred, over one row per attribute, its type, name, keys (`PK,FK`) and comment each in a column as wide as its widest cell, two spaces apart, a column no attribute fills left out; a comment wraps at spaces onto rows of its own so a row keeps within 60 columns while the comment keeps 12.
- A subgraph is a group framed with its title, flowing its own `direction` else its parent's; an entity named like a subgraph is that subgraph, and a relationship may end on it.
- A relationship is an edge from its first entity to its second, solid when identifying (`--`), dashed when not (`..`), its role on it as a flowchart edge's label.
- Crow's feet, two cells next to each entity, the line straight through them and a line cell beside them before a label; none is East Asian ambiguous outside box drawing.
  Drawn with the entity on the right of a line going right; the fork opens onto the entity whichever side it joins (`< > Ʌ V`):

  | Written | Means | Drawn (ASCII) |
  |---|---|---|
  | `\|\|` | exactly one | `┼┼` (`++`) |
  | `\|o`, `o\|` | zero or one | `o┼` (`o+`) |
  | `}\|`, `\|{` | one or more | `┼<` (`+<`) |
  | `}o`, `o{` | zero or more | `o<` (`o<`) |
  | `u` | parent of a generalisation | nothing, as mermaid draws none |


## Dependency direction

```
vt-canvas ← vt-graph ← vt-mermaid
```

vt-mermaid depends on vt-canvas and vt-graph; the parser and the layout are its own, and `mermaid-rs-renderer` is a dev-dependency of `examples/agreement.rs` only.

## Test corpus

- The package published holds `src/`, the README, the licence, this page, and only the inputs and snapshots the library's own tests read, with `corpus/LICENSE-mermaid` for the mermaid inputs among them; `scripts/check` packs it and runs its tests but those reading the corpus, the oracle or a list of diagrams, whose names say `corpus`, `listed` or `spike`.
- [`corpus/`](corpus/README.md): Mermaid inputs harvested from the mermaid repo at a pinned release, one file per case, grouped by diagram kind.
- [`oracle/`](oracle/README.md): a Node script that records what the pinned `mermaid` accepts (level A) and its flowchart, sequence, state, class and ER dbs (level B), committed as JSON.
- `examples/agreement.rs`: compares mermaid-rs-renderer's parser with the oracle per kind; a dev-only program, so the library's API never sees the dependency.
  Flowcharts are compared with vt-mermaid's own parser by `parse::agreement` instead.
- `tests/class/diagrams.txt`: class diagrams from the corpus and `tests/class/library.mmd`, each an insta snapshot at 100 columns that keeps the level C invariants; every corpus diagram mermaid accepts is drawn within 80 columns, the four packing eight pairs of classes side by side with 16 labels left out.
- `tests/er/diagrams.txt`: ER diagrams from the corpus and `tests/er/shop.mmd`, each an insta snapshot at 100 columns that keeps the level C invariants; every corpus diagram mermaid accepts is drawn within 80 columns, those breaking the invariants only with wide characters the measures miscount, a comment cut past 60 columns or a relationship of an entity to itself.
- `tests/sequence/diagrams.txt`: sequence diagrams from the corpus and `tests/sequence/checkout.mmd`, each an insta snapshot at 100 columns; every corpus diagram mermaid accepts is drawn within 80 columns but two with eight and ten participants side by side.
- `tests/state/diagrams.txt`: state diagrams from the corpus and `tests/state/`, each an insta snapshot at 100 columns; every corpus diagram mermaid accepts is drawn within 80 columns and passes the level C checks, but for labels the layout leaves without room.
- `tests/spike/flowcharts.txt`: the 20 flowcharts of the spike, 19 from the corpus and `tests/spike/cached.mmd`; each is an insta snapshot and passes the level C checks of `measure` (every node and label drawn whole, every edge joining both ends, nothing overlapping).
- [`scoreboard/`](scoreboard/README.md): every corpus flowchart, sequence, state, class and ER diagram drawn by vt-mermaid and three other text renderers, measured the same way, results committed; a dev-only example, run by hand.
- `fuzz/`: a cargo-fuzz crate, its own workspace, out of the library's build and of `scripts/check`, seeded from `corpus/flowchart`, and from `corpus/sequence`, `corpus/class`, `corpus/state` and `corpus/er` for those kinds; `cargo +nightly fuzz run render` feeds any source to `render` and checks it never panics and every row fits; `cargo +nightly fuzz run parse` feeds it to the parser alone, through `read_flowchart`, and checks it never panics and keeps the limits; `parse_sequence`, `parse_class`, `parse_state` and `parse_er` do the same for those parsers through `read_sequence`, `read_class`, `read_state` and `read_er`.
  On stable, build a target with `cargo build --release --target <host>` and the sanitizer-coverage `RUSTFLAGS` cargo-fuzz passes, then run the binary.
- `benches/`: `limits` times a typical flowchart, sequence, state, class and ER diagram and inputs built at each limit with divan, `corpus` times every corpus flowchart, sequence, state, class and ER diagram and prints the spread of each kind; both run with `cargo bench` only.
- `examples/compare.rs`: prints each spike flowchart drawn by mermaid-text, then by vt-mermaid; mermaid-text is a dev-dependency only.
