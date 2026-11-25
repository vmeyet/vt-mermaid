# Scoreboard

vt-mermaid's flowcharts measured against the other text renderers of Mermaid, on every flowchart of the [corpus](../corpus/README.md).
A PR that changes the drawing reruns it and commits the results, so the diff says whether it got better or worse, and against whom.

## Run

```sh
scoreboard/install.sh                                     # once: the competitors, outside the repo
cargo run --release --example scoreboard                  # every flowchart, every tool, about 4 minutes
cargo run --release --example scoreboard -- --tools vt-mermaid --gallery /tmp/gallery.html
```

- `--tools <a,b>` runs only those tools; the others keep their last results, read back from `results/inputs/`.
- `--sample <n>` runs one flowchart in n, for a quick look; the summary says it is a sample.
- `--gallery <file.html>` writes the gallery there, by default `target/scoreboard/gallery.html`.
- It never runs from `cargo test` and never touches the network; only `install.sh` downloads.

## Tools

| Tool | Version | How it runs |
|---|---|---|
| vt-mermaid | this commit | In-process, in a child of the runner |
| [mermaid-text](https://crates.io/crates/mermaid-text) | 0.58.0 | In-process, in a child of the runner; a dev-dependency |
| [mermaid-ascii](https://github.com/AlexanderGrooff/mermaid-ascii) | 1.6.2 | The release binary, `--max-width <width>`; its sha256 is checked |
| [beautiful-mermaid](https://github.com/lukilabs/beautiful-mermaid) | 1.1.3 | `renderMermaidASCII` in Node through [`beautiful-mermaid.mjs`](beautiful-mermaid.mjs); `node/package-lock.json` pins it |

- `install.sh` puts mermaid-ascii and beautiful-mermaid in `$XDG_CACHE_HOME/vt-scoreboard` (`~/.cache/vt-scoreboard` by default, `VT_SCOREBOARD_CACHE` overrides it); a tool not installed is skipped and keeps its last results.
- Every tool runs in a child process with a 10 second deadline per input, so a hang is killed and a crash is told apart from a refusal.
- The competitors get the source without its front matter (`---` … `---`), which they refuse; vt-mermaid gets it whole.
- mermaid-ascii only reads edge labels written `-->|label|`; a `-- label -->` edge counts against it, as it would for a user.
- mermaid-text 0.58.0 does not always draw the same thing twice: a handful of flowcharts with subgraphs change between runs.
  A PR that only touches vt-mermaid runs `--tools vt-mermaid`, so the competitors' results stay as committed.
- D2 is not Mermaid, so it is not on the board; it remains the visual bar to judge the gallery by.

## Metrics

Every metric but the exact ones is read from the tool's text output, the same way for every tool, at 80 and 100 columns.
Meaning comes from the oracle's level B results ([`oracle/results/flowchart.json`](../oracle/results/flowchart.json)); 925 of the 946 flowcharts have one, the others mermaid itself rejects.

| Metric | Definition | Proxy? |
|---|---|---|
| Rendered | Exit 0 with a non-blank drawing | No |
| Refused | The tool declined: a parse error, a limit, too wide for `--max-width` when the tool refuses rather than overflows, or nothing drawn | No |
| Crashed | Killed by a signal, a Rust panic (exit 101), a Go `panic:` or `fatal error:`, or any Node throw that is not a plain `Error` | Partly: the line between a refusal and a crash is each tool's own |
| Timed out | Still running at the 10 second deadline | No |
| Width, rows, area | Display columns of the widest row, rows, their product; trailing spaces and blank rows above and below are left out | No |
| Fits | Width at most the columns asked for | No |
| Node labels, edge labels | A label is present when every word of it appears somewhere in the output, markup removed from both the same way for every tool; the table averages the share found per input | Yes: words can match elsewhere, and a label cut with `⋯` counts as missing |
| Arrows right | Arrowhead glyphs (`►▶▸▷→◄◀◂◁←▲▴△↑▼▾▽↓`) equal the oracle's arrowheads: one per `-->`, two per `<-->`, none for open, circle or cross ends | Yes |
| Crossings | `┼`-like glyphs | Yes: an edge through a group frame draws one too |
| Bends | Corner glyphs left once every closed rectangle's four corners are taken out | Yes: merged edges and odd shapes miscount |
| Score | 0 when not rendered; else 50 × the share of labels found, 20 if it fits, 15 × how close the arrowheads are to the oracle's, 15 ÷ (1 + crossings); only for inputs whose oracle has a node | Yes: a weighting, made to rank, not to judge |
| Median ms, p95 ms | Wall time of the child process over every input, start-up included: Node and ELK add about half a second to beautiful-mermaid | Noisy: the runs share all cores |

Markup removed means: `<br>`, `<br/>` and `<br />` are a space, a tag (`<` with a letter or `/`, up to `>`) goes as the oracle's sanitised labels drop it, and so do `*` and backticks.
The oracle's escaped characters (`&lt;`, `&amp;`…) are read back; a tool's are not, as a user would see them.

The second table per width compares layout only where it can: on the inputs every tool drew.

vt-mermaid also measures itself from its own geometry (`vt_mermaid::measure`, behind the `measure` feature):

| Metric | Definition |
|---|---|
| Crossings | Cells two edge routes share; lines never run along each other, so each one is a crossing |
| Bends | Turns along every route, its ends left out |
| Edge cells | Cells every route covers |
| Violations | Level C invariants broken: a label not drawn whole or with no room, an edge not joining both ends or running through a node, overlaps, a node outside its group |

Set beside the glyph counts in the same table, they also show how far the proxies are off.

## Results

- [`results/summary.md`](results/summary.md): the tables.
- `results/inputs/<input>.json`: per input, the oracle's counts and each tool's metrics per width, one line each; times are left out, so a rerun on the same code changes nothing but mermaid-text's unstable inputs.
- The gallery is not committed: about 15 MB, past what belongs in git.
  It shows per input the source, then each tool's drawing side by side with its metrics, sortable by how far vt-mermaid's score is below the best competitor's.
