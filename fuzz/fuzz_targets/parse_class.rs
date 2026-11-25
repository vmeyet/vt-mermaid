//! Any source, valid Mermaid or not: the class parser returns an error or a diagram within the limits, never panics,
//! and stays within libFuzzer's timeout on any input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vt_mermaid::{MAX_EDGES, MAX_NODES};

fuzz_target!(|source: &str| {
    let Ok(counts) = vt_mermaid::read_class(source) else { return };
    assert!(counts.nodes <= MAX_NODES && counts.edges <= MAX_EDGES, "{counts:?}");
});
