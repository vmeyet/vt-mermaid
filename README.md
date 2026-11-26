# vt-mermaid

Mermaid diagrams drawn as text: flowcharts, sequence, state, class and ER diagrams on a [vt-canvas](https://crates.io/crates/vt-canvas) grid of styled cells, never ANSI.

It is a pure library.
Each cell carries a *role*, so the caller maps roles to colours, sanitizes and writes the terminal; vt-mermaid never prints, reads files or the environment, or looks at the clock.
Its parsers are mermaid 12.3.0's own, rewritten by hand, so a diagram means here what it means to mermaid.

```sh
cargo add vt-mermaid
```

The minimum supported Rust version is 1.92.

## Example

```rust
use vt_mermaid::GlyphSet;

let canvas = vt_mermaid::render("flowchart LR\n  A[Start] --> B{Ready?} -->|yes| C[Ship]", 80)?;

let lines: Vec<String> = canvas.lines(GlyphSet::Unicode).collect();
assert_eq!(
    lines,
    [
        "┌───────┐      ╱────────╲        ┌──────┐",
        "│ Start ├─────▸│ Ready? ├──yes──▸│ Ship │",
        "└───────┘      ╲────────╱        └──────┘",
    ]
);
# Ok::<(), vt_mermaid::Error>(())
```

`GlyphSet::Ascii` draws the same in ASCII characters only.
`Canvas::rows` gives each row as runs of text sharing a `Role`, for the caller to colour.

## What it draws

| Header | Drawn as |
|---|---|
| `flowchart`, `graph` | Nodes in their shapes, subgraphs as frames, edges routed at right angles with their labels |
| `sequenceDiagram` | Participants side by side, messages, notes, activations and blocks from top to bottom |
| `stateDiagram`, `stateDiagram-v2` | States, start and end dots, choices, forks and joins, composite states as frames, concurrent regions, notes |
| `classDiagram` | Classes as boxes of compartments, relations with their UML heads and cardinalities, namespaces, notes |
| `erDiagram` | Entities as tables of attributes, relationships with crow's feet and roles, subgraphs |

Any other kind mermaid knows (`pie`, `gantt`, `xychart`…) is `Error::Unsupported`.
A drawing too wide is laid out again, tighter, its labels wrapped narrower and its ranks folded, until it fits `max_width` or no form does.
A `graph` or `flowchart` with nothing after its header is an empty canvas, 0 by 0: mermaid accepts it and draws nothing.

## Settings

`render(source, max_width)` draws with the defaults; `render_with(source, settings)` takes `Settings`:

```rust
use vt_mermaid::{LongWords, Settings};

let settings = Settings::new(60).long_words(LongWords::Cut);
let canvas = vt_mermaid::render_with("flowchart TB\n  A[a-word-far-too-long-for-one-line-of-a-node]", settings)?;
# Ok::<(), vt_mermaid::Error>(())
```

- `max_width`: the most columns the drawing may take; over vt-canvas's 1000 (`MAX_WIDTH`) it counts as 1000.
- `long_words`: a word wider than a label's line is broken over lines (`LongWords::Break`, the default) or cut with `⋯` (`LongWords::Cut`).

## Roles

| `Role` | Cells |
|---|---|
| `Node` | A node's border, a participant's box or figure |
| `Edge` | A line between nodes, a lifeline or a message, and its tips |
| `Label` | Text: a node's name, an edge's label, a message |
| `Group` | A frame around nodes, a sequence block, a note, and its title |
| `Plain` | Empty cells |

## Errors and limits

Mermaid source is untrusted input: every size is bounded before work starts, and a limit hit is an `Error` the caller can match, never a panic.

| Error | When |
|---|---|
| `TooLong` | The source is over 64 KiB (`MAX_SOURCE_BYTES`) |
| `Invalid` | Mermaid would refuse the source; `reason` says why |
| `Unsupported` | A kind mermaid knows but not drawn here; `kind` is the source's first word |
| `TooManyNodes`, `TooManyEdges` | A graph over 200 nodes (`MAX_NODES`) or 150 edges (`MAX_EDGES`); the count is where reading stopped |
| `TooManyParticipants`, `TooManyMessages` | A sequence diagram over 50 participants (`MAX_PARTICIPANTS`) or 1000 messages (`MAX_MESSAGES`) |
| `TooLarge` | The routing grid over 250000 cells (`MAX_AREA`) or the canvas over a million (`MAX_CELLS`); `max_cells` says which |
| `TooWide` | No form of the drawing fits `max_width`; `width` is the narrowest found, for a caller that falls back |
| `Unroutable` | An edge finds no path whatever the placement |
| `TooComplex` | Routing the edges took over 6 million steps (`MAX_ROUTING_STEPS`): a bound on the work, the same on every machine |

Mermaid's own corpus is drawn in tens of microseconds at the median and 20 ms at the slowest; an input spending the whole routing budget is refused after about half a second on an Apple M5.

**Sanitize before printing.**
`Invalid::reason`, `Unroutable::from` and `Unroutable::to` quote the source, and so do the labels on the canvas: pass them through your terminal sanitizer, as you would any untrusted text.
`Unsupported::kind` holds at most 32 ASCII letters, digits and `-`.

## Not part of the API

The `measure` feature adds `measure`, the exact geometry of a drawing for the [scoreboard](https://github.com/vmeyet/vt-mermaid/blob/main/scoreboard/README.md); it is unstable and may change in any release.
The parsers alone, for fuzzing, are built only with `--cfg fuzzing`.

## Learn more

- [`ARCHITECTURE.md`](https://github.com/vmeyet/vt-mermaid/blob/main/ARCHITECTURE.md): what the crate owns, its contract, limits and timings, and how each kind is read and laid out.
- [`scoreboard/`](https://github.com/vmeyet/vt-mermaid/blob/main/scoreboard/README.md): every diagram of mermaid's own corpus drawn by vt-mermaid and other text renderers, measured the same way.
- [`AGENTS.md`](https://github.com/vmeyet/vt-mermaid/blob/main/AGENTS.md): how to work on it.

## License

MIT.
The test inputs taken from mermaid are © Knut Sveidqvist and the mermaid contributors, under the MIT license in [`corpus/LICENSE-mermaid`](https://github.com/vmeyet/vt-mermaid/blob/main/corpus/LICENSE-mermaid).
