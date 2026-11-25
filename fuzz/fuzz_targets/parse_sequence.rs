//! Any source, valid Mermaid or not: the sequence parser returns an error or a diagram within the limits, never
//! panics, and stays within libFuzzer's timeout on any input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vt_mermaid::{MAX_MESSAGES, MAX_PARTICIPANTS};

fuzz_target!(|source: &str| {
    let Ok(counts) = vt_mermaid::read_sequence(source) else { return };
    assert!(counts.participants <= MAX_PARTICIPANTS && counts.messages <= MAX_MESSAGES, "{counts:?}");
});
