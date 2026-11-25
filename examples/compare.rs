//! Each spike flowchart drawn by mermaid-text, then by vt-mermaid: `cargo run --example compare [-- <width>]`.
//!
//! The flowcharts are listed in `tests/spike/flowcharts.txt`; the width defaults to 100 columns.
//! mermaid-text refuses a front matter block (`---` … `---`), so it gets the source without it.

use std::error::Error;
use std::fs;
use std::path::Path;

use vt_mermaid::GlyphSet;

const DEFAULT_WIDTH: u16 = 100;

fn main() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let width = std::env::args().nth(1).map(|width| width.parse()).transpose()?.unwrap_or(DEFAULT_WIDTH);
    for path in fs::read_to_string(root.join("tests/spike/flowcharts.txt"))?.lines() {
        let source = fs::read_to_string(root.join(path))?;
        println!("━━━ {path}\n\n{}\n", source.trim_end());
        println!("─── mermaid-text\n\n{}\n", mermaid_text(&source, width));
        println!("─── vt-mermaid\n\n{}\n", vt_mermaid(&source, width));
    }
    Ok(())
}

fn mermaid_text(source: &str, width: u16) -> String {
    let options = mermaid_text::RenderOptions { max_width: Some(usize::from(width)), max_width_strict: true, ..Default::default() };
    mermaid_text::render_with_options(without_front_matter(source), &options).unwrap_or_else(|error| format!("error: {error}"))
}

fn without_front_matter(source: &str) -> &str {
    source.strip_prefix("---\n").and_then(|rest| rest.split_once("\n---\n")).map_or(source, |(_, body)| body)
}

fn vt_mermaid(source: &str, width: u16) -> String {
    match vt_mermaid::render(source, width) {
        Ok(canvas) => canvas.lines(GlyphSet::Unicode).collect::<Vec<_>>().join("\n"),
        Err(error) => format!("error: {error}"),
    }
}
