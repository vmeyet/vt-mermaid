# Scoreboard

vt-mermaid's flowcharts, sequence, state, class and ER diagrams measured against the other text renderers of Mermaid, on every diagram of those kinds in the [corpus](../corpus/README.md).
A PR that changes the drawing reruns it and commits the results, so the diff says whether it got better or worse, and against whom.
The latest full run is written up in [`docs/comparison.md`](../docs/comparison.md).

- [Run](#run)
- [Tools](#tools)
- [Metrics](#metrics)
- [Results](#results)

## Run

```sh
scoreboard/install.sh                                                      # once: the competitors, outside the repo
cargo run --release --example scoreboard                                   # every flowchart, every tool, about 4 minutes
cargo run --release --example scoreboard -- --kind sequence                # every sequence diagram, about 20 seconds
cargo run --release --example scoreboard -- --markdown docs/comparison.md  # every kind, then the comparison page
```

| Option | Does |
|---|---|
| `--kind <kind>` | Measures `sequence`, `state`, `class` or `er` diagrams instead of flowcharts; results go to `results/<kind>/`, the gallery to `target/scoreboard/gallery-<kind>.html` |
| `--tools <a,b>` | Runs only those tools; the others keep their last results, read back from `results/inputs/` |
| `--sample <n>` | Runs one input in n, for a quick look; the summary says it is a sample |
| `--gallery <file.html>` | Writes the gallery there, by default `target/scoreboard/gallery.html` |
| `--markdown <file.md>` | Runs every kind, then the kinds only other tools draw, times vt-mermaid alone, and writes the comparison page; takes no `--kind`, `--sample` or `--gallery` |

- State diagrams take about 30 seconds; sequence, class and ER diagrams about 20 seconds each.
- It never runs from `cargo test` and never touches the network; only `install.sh` downloads.

## Tools

| Tool | Version | How it runs |
|---|---|---|
| vt-mermaid | this commit | In-process, in a child of the runner |
| [mermaid-text](https://crates.io/crates/mermaid-text) | 0.58.0 | In-process, in a child of the runner; a dev-dependency |
| [mermaid-ascii](https://github.com/AlexanderGrooff/mermaid-ascii) | 1.6.2 | The release binary, `--max-width <width>`; its sha256 is checked |
| [beautiful-mermaid](https://github.com/lukilabs/beautiful-mermaid) | 1.1.3 | `renderMermaidASCII` in Node through [`beautiful-mermaid.mjs`](beautiful-mermaid.mjs); `node/package-lock.json` pins it |

- `install.sh` puts mermaid-ascii and beautiful-mermaid in `$XDG_CACHE_HOME/vt-scoreboard` (`~/.cache/vt-scoreboard` by default, `VT_SCOREBOARD_CACHE` overrides it).
  A tool not installed is skipped and keeps its last results.
- Every tool runs in a child process with a 10 second deadline per input, so a hang is killed and a crash is told apart from a refusal.
- The competitors get the source without its front matter (`---` … `---`), which they refuse; vt-mermaid gets it whole.
- mermaid-ascii only reads edge labels written `-->|label|`; a `-- label -->` edge counts against it, as it would for a user.
- mermaid-ascii draws no state or class diagram, so it refuses all of them, and the table of inputs every tool drew leaves it out.
- mermaid-text 0.58.0 does not always draw the same thing twice: a handful of flowcharts with subgraphs change between runs.
  A PR that only touches vt-mermaid runs `--tools vt-mermaid`, so the competitors' results stay as committed.
- D2 is not Mermaid, so it is not on the board; it remains the visual bar to judge the gallery by.

## Metrics

Every metric but the exact ones is read from the tool's text output, the same way for every tool, at 80 and 100 columns.
What a drawing must show comes from the oracle's level B results: what mermaid itself makes of the input.

| Metric | Definition | Proxy? |
|---|---|---|
| Rendered | Exit 0 with a non-blank drawing | No |
| Refused | The tool declined: a parse error, a limit, too wide for `--max-width` when the tool refuses rather than overflows, or nothing drawn | No |
| Crashed | Killed by a signal, a Rust panic (exit 101), a Go `panic:` or `fatal error:`, or any Node throw that is not a plain `Error` | Partly: the line between a refusal and a crash is each tool's own |
| Timed out | Still running at the 10 second deadline | No |
| Width, rows, area | Display columns of the widest row, rows, their product; trailing spaces and blank rows above and below are left out | No |
| Fits | Width at most the columns asked for | No |
| Node labels, edge labels | A label is present when every word of it appears somewhere in the output, markup removed from both the same way for every tool; the table averages the share found per input | Yes: words can match elsewhere, and a label cut with `⋯` counts as missing |
| Arrows right | Arrowhead glyphs (`►▶▸▷→◄◀◂◁←▲▴△↑▼▾▽↓`), and one more for each edge sharing an arrowhead, equal the oracle's arrowheads | Yes: see [Lines followed](#lines-followed) |
| Crossings | `┼`-like glyphs, those where a trunk parts or gathers both ways left out | Yes: an edge through a group frame draws one too; see [Lines followed](#lines-followed) |
| Bends | Corner glyphs left once every closed rectangle's four corners are taken out | Yes: merged edges and odd shapes miscount |
| Score | See [Score](#score) | Yes: a weighting, made to rank, not to judge |
| Median ms, p95 ms | Wall time of the child process over every input, start-up included: Node and ELK add about half a second to beautiful-mermaid | Noisy: the runs share all cores |

### Score

Each input scores 0 when not rendered, else, rounded to a whole number:

- 50 × the share of labels found, node and edge labels together (full when the oracle expects none);
- 20 if it fits;
- 15 × how close the arrowheads are to the oracle's: 1 − min(|drawn − expected|, e) ÷ e, with e = max(expected, 1);
- 15 ÷ (1 + crossings).

The mean is only over inputs whose oracle meaning has a node.

### What each kind counts

| Kind | Oracle meanings | Node labels | Edge labels | Arrowheads |
|---|---|---|---|---|
| Flowchart | [`flowchart.json`](../oracle/results/flowchart.json), 925 of 946 | Node texts | Edge texts | One per `-->`, two per `<-->`, none for open, circle or cross ends |
| Sequence | [`sequence.json`](../oracle/results/sequence.json), 386 of 394 | Participants | Messages and notes; a block's text is left out, as each tool draws blocks its own way | One per message, two for a two-way one, none for an open (`->`) or crossed (`-x`) end |
| State | [`state.json`](../oracle/results/state.json), 276 of 279 | A state's label and descriptions, a composite state's title, a note's text; a start or end dot, a fork, a join and a choice show none | Transition labels | One per transition; transitions merged into one line before an end dot count one, the dot being no rectangle the line could end on |
| Class | [`class.json`](../oracle/results/class.json), 315 of 372 | Each class's name, each member's name, parameters and return type (a generic's `~` read as a space, as tools draw `<T>`), and the notes | A relation's label and the texts at its ends | One per end with a relation type, read from arrowheads and the triangles and diamonds class relations end on (`▵ ▹ ▿ ◃ ⬥ ⬦ ◆ ◇ ♦ ♢ ◊`); a lollipop drawn `o` is not read |
| ER | [`er.json`](../oracle/results/er.json), 312 of 315 | Each entity's alias, else its name, and each attribute's type, name, keys and comment | Relationship roles | None counted, as every tool draws a crow's foot its own way, so every drawing has its arrows right |

- The inputs without a meaning are those mermaid itself rejects.
- In ER diagrams a crossing glyph counts only where lines join it on its four sides, as a crow's foot draws its bars as `┼` across a line.
- vt-mermaid has no exact measures of a sequence diagram: its rows are read from the text like every other tool's.

### Markup

- `<br>`, `<br/>` and `<br />` are a space.
- A tag (`<` with a letter or `/`, up to `>`) goes, as the oracle's sanitised labels drop it; so do `*` and backticks.
- A Font Awesome icon (`fa:fa-car`) is no word of a label, as mermaid draws an icon there.
- The oracle's escaped characters (`&lt;`, `&amp;`…) are read back; a tool's are not, as a user would see them.

### Lines followed

Two counts follow the lines from glyph to glyph ([`lines.rs`](lines.rs)), the same way for every tool, since glyphs alone miscount merged edges.
Lines joined end to end are one network; a `┼` joins only the cells it lies between, and the sides of a closed rectangle, a node or a frame, end a line.

- Shared arrowheads: a network with an arrowhead counts an arrow for each end it has on a rectangle's side beyond its arrowheads, so a fan-in merged into one line counts one per edge.
  A line that ends only on rectangles and crosses a line with an arrowhead gathers into it: each of its ends counts one more.
- Trunk crossings: a `┼` is not a crossing when the line across it leads only to arrowheads, two or more (a trunk parting both ways), or gathers as above.
- Where it is wrong: a line cut by its label, by a frame's title or ending on a shape that is no rectangle is followed no further, so its edges count as before.
  An open link (`---`) or a `<-->` edge crossing an arrow edge reads as a trunk.
  A tool that draws a line onto a frame's side gets an arrow too many.

The second table per width compares layout only where it can: on the inputs every tool drew.

### vt-mermaid's exact measures

vt-mermaid also measures its flowcharts, state, class and ER diagrams from their own geometry (`vt_mermaid::measure`, behind the `measure` feature).
Set beside the glyph counts in the same table, they show how far the proxies are off.

| Metric | Definition |
|---|---|
| Crossings | Cells one edge route runs straight through sideways and another up or down; routes sharing a trunk run the same way there, so they never count |
| Bends | Turns along every route, its ends left out, a turn the routes of a trunk share counted once |
| Edge cells | Cells the routes cover, a cell they share counted once |
| Cells beside a node | Cells a route runs straight through right beside the border of a node it does not join |
| Violations | Level C invariants broken: a label not drawn whole or with no room, an edge not joining both ends or running through a node, overlaps, a node outside its group |

## Results

| File | Holds |
|---|---|
| [`results/summary.md`](results/summary.md) | The flowchart tables |
| [`sequence`](results/sequence/summary.md), [`state`](results/state/summary.md), [`class`](results/class/summary.md), [`er`](results/er/summary.md) summaries | The same for each kind |
| `results/inputs/`, `results/<kind>/inputs/` | One JSON file per input: the oracle's counts and each tool's metrics per width, one line each |
| [`docs/comparison.md`](../docs/comparison.md) | Every kind on one page, with speed, agreement with mermaid and drawings side by side |

- Times are left out of the per-input files, so a rerun on the same code changes nothing but mermaid-text's unstable inputs.
- The gallery is not committed: about 15 MB, past what belongs in git.
  It shows per input the source, then each tool's drawing side by side with its metrics, sortable by how far vt-mermaid's score is below the best competitor's.
