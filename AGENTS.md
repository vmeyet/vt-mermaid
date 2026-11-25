# Working in this repo

`vt-mermaid` is a Rust library: Mermaid diagrams drawn as text on a vt-canvas grid.
Read [`ARCHITECTURE.md`](ARCHITECTURE.md) first.
The code is the contract; `ARCHITECTURE.md` describes it, and a change to a module or a boundary updates it in the same change.

## Ground rules

- One change a reviewer can read in one sitting, under 500 changed lines excluding fixtures and snapshots.
- Branches: `<scope>/<kebab-title>` with scope in `feat fix chore refactor docs test ci`.
- Worktrees live in `.claude/worktrees/<scope>+<kebab-title>`, share the repo's build dir through `CARGO_TARGET_DIR=<repo>/target`, and are removed once merged.
- Commits follow conventional commits; body in two short imperative sentences at most. No co-author trailers.
- Markdown files put one sentence per line.
- `scripts/check` runs what CI runs; a new CI step lands in both.

## Code

- Rust 2024, `rustfmt.toml` as committed (140 columns, `use_small_heuristics = "Max"`).
- Lints live in `[lints]` in `Cargo.toml`: `cargo clippy --all-targets -- -D warnings` must be clean, `unwrap`/`expect` only live in tests, `unsafe` is forbidden.
- Never allow a lint crate-wide: put `#[expect(lint, reason = "…")]` on the one expression.
- `pub` is the crate's API: keep it to what a caller needs; `missing_docs` holds every `pub` item to a doc comment.
- No public signature names a dependency's type; the crate owns its dependency.
- No comment restates the code; a `//` comment says why, and only when no name can.
- Immutability across function boundaries: pass helpers values or `&` borrows and take back a new value.
- Flat bodies of named steps at one level of abstraction; signatures designed from the call site.
- No `bool` parameter on a function with more than one caller; no tuple for data that crosses a module: a struct with named fields.
- One crate per job: prefer std or a dependency already in `Cargo.toml`; a new one needs a line saying why, and is added by its first user.

## Purity and bounds

- The library is pure: no I/O, no printing, no environment, no process exit.
- Output is styled cells whose colours are *roles* (node, edge, label, series…), never ANSI escapes; the caller maps roles to a palette and writes the terminal.
- Input is untrusted, so work is bounded before it starts: every size has an upper limit, tested at the limit and over it.
- Errors are typed; a limit hit is an error the caller can match, never a panic.

## Tests

```sh
scripts/check          # everything CI runs
cargo test             # unit and snapshots
cargo insta review     # after a deliberate rendering change
```

- Every module ships its tests in the same file, one behaviour per test, named as a sentence that says it.
- Rendering tests are `insta` snapshots of the drawn grid.
- Tests are deterministic: no terminal, no clock, no machine-dependent fonts.
- A test written for a branch proves it reaches that branch; a regression test fails on the old code.
