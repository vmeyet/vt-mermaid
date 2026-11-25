# Architecture

`vt-mermaid` is one library crate.
Mermaid diagrams drawn as text on a vt-canvas grid.
This page says how the code is laid out today; when a module or a boundary changes, change this page in the same change.

## Owns

- Mermaid source to a text drawing, one diagram kind at a time.
- Snapping a real layout to the grid, then routing edges on the grid.
- Handing chart kinds (xychart, pie, sankey…) to vt-graph.

## Never

- Reads files, the environment or the clock; prints; exits the process.
- Writes ANSI escapes or picks colours: it hands back cells with colour roles, the caller maps them and writes the terminal.
- Lets a dependency's type into a public signature.
- Writes its own layout engine: mermaid-rs-renderer lays out, vt-mermaid snaps to the grid.

## Dependency direction

```
vt-canvas ← vt-graph ← vt-mermaid
```

vt-mermaid depends on vt-canvas and vt-graph, and on `mermaid-rs-renderer` for parsing and layout; no layout engine is written here.

## Test corpus

- [`corpus/`](corpus/README.md): Mermaid inputs harvested from the mermaid repo at a pinned release, one file per case, grouped by diagram kind.
- [`oracle/`](oracle/README.md): a Node script that records what the pinned `mermaid` accepts (level A) and its flowchart and sequence dbs (level B), committed as JSON.
- `examples/agreement.rs`: compares mermaid-rs-renderer's parser with the oracle per kind; a dev-only program, so the library's API never sees the dependency.
