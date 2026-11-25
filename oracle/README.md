# Oracle

What mermaid itself makes of each [corpus](../corpus/README.md) input, recorded so the Rust side compares against it offline.
It runs by hand, never from `cargo test`.

## Run

```sh
npm ci --prefix oracle --ignore-scripts
node oracle/oracle.mjs corpus oracle/results
cargo run --release --example agreement             # level-A table per kind
cargo run --release --example agreement -- sequence # plus the disagreeing cases of one kind
```

`mermaid` is pinned to `12.3.0`, the release the corpus was harvested from, and `package-lock.json` pins the rest.
`jsdom` gives mermaid the window its label sanitizer (DOMPurify) needs; without it, flowcharts fail to parse in Node.
[`mermaid.mjs`](mermaid.mjs) loads mermaid that way and names the corpus kind of an input, for both [`harvest.mjs`](harvest.mjs) and [`oracle.mjs`](oracle.mjs).

## Results

- [`results/parse.json`](results/parse.json), level A: for every input, `accepted`, the diagram `kind` mermaid detected, and on rejection the `error`.
- [`results/flowchart.json`](results/flowchart.json), [`results/sequence.json`](results/sequence.json) and [`results/state.json`](results/state.json), level B: the diagram db of every accepted input of that kind.
  - Flowchart: direction, nodes (id, text, shape), edges (ends, arrow type, stroke, length, text), subgraphs (id, title, nodes, direction).
  - Sequence: actors (id, name, description, type, box), messages in order (ends, text, line type by name, note placement, activation) and boxes.
  - State: the direction, and the nodes and edges mermaid's renderer is handed: states, start and end dots, choices, forks, joins, notes and their groups, composites and their concurrent regions (id, shape, label, further descriptions, parent, direction, note position), then transitions and note links (ends, label, arrowhead).
    The region after a composite's last `--` has a random id in mermaid, renamed `id-<n>` in the order it appears.
- Styles, classes, links and clicks are left out.
- A run is deterministic: the same corpus gives the same files.

## Safety

The corpus is untrusted input.
The oracle only calls `mermaid.parse` and reads the diagram db: nothing is rendered, fetched or written outside `results/`.
jsdom runs no scripts and loads no resources in its default configuration, which is the one used.
`zenuml` is a separate mermaid package that is not loaded, so mermaid rejects every `zenuml` input.
