//! Any source, valid Mermaid or not: `render` returns a drawing or an error, never panics, and a drawing fits.

#![no_main]

use libfuzzer_sys::fuzz_target;
use unicode_width::UnicodeWidthStr;
use vt_mermaid::GlyphSet;

/// The width mrk usually gets, narrow enough to reach the top-down retry.
const MAX_WIDTH: u16 = 100;

fuzz_target!(|source: &str| {
    let Ok(canvas) = vt_mermaid::render(source, MAX_WIDTH) else { return };
    let widths: Vec<usize> = canvas.rows(GlyphSet::Unicode).map(|runs| runs.iter().map(|run| run.text.width()).sum()).collect();
    assert!(widths.iter().all(|&width| width == widths[0] && width <= usize::from(MAX_WIDTH)), "row widths {widths:?}");
});
