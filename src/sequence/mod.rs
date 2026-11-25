//! Sequence diagrams: participants side by side, what happens between them from top to bottom.

mod diagram;
mod draw;
mod layout;
mod parse;

use vt_canvas::Canvas;

use crate::error::Error;
use crate::label::Wrap;
use crate::settings::Settings;
use diagram::Diagram;
use layout::{Fit, LOOSE, TIGHT};

/// Widths labels wrap to at spaces, from not at all to narrow, tried in turn until the drawing fits.
const WRAPS: [usize; 12] = [usize::MAX, 60, 48, 40, 32, 26, 20, 16, 12, 10, 8, 6];
/// Widths labels wrap to when a word wider is broken or cut, column by column so no more is cut than needs be.
const CUTS: std::ops::RangeInclusive<usize> = 6..=30;

/// Draws `source` in the first form that fits: labels as written, then wrapped narrower and narrower at spaces, then
/// the same with heads closer together, then with words wider than the lines broken or cut as `settings` say.
pub(crate) fn render_with(source: &str, settings: Settings) -> Result<Canvas, Error> {
    let diagram = Diagram::read(source)?;
    let max_width = settings.max_width;
    let wraps = |gap| WRAPS.map(move |columns| Fit { wrap: Wrap::to(columns, settings.long_words), gap });
    let tight = CUTS.rev().map(|columns| Fit { wrap: Wrap::to(columns, settings.long_words).tight(), gap: TIGHT });
    let mut narrowest = usize::MAX;
    for fit in wraps(LOOSE).into_iter().chain(wraps(TIGHT)).chain(tight) {
        match draw::draw(&layout::place(&diagram, fit), max_width) {
            Err(Error::TooWide { width, .. }) => narrowest = narrowest.min(width),
            drawn => return drawn,
        }
    }
    Err(Error::TooWide { width: narrowest, max_width })
}

/// How much a sequence diagram holds once read.
#[cfg(feature = "fuzz")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Participants.
    pub participants: usize,
    /// Messages, notes, activations and the starts, sections and ends of blocks.
    pub messages: usize,
}

/// Reads `source` within the limits `render` draws.
#[cfg(feature = "fuzz")]
pub fn read(source: &str) -> Result<Counts, Error> {
    let sequence = parse::parse(source, parse::Limits::DRAWN)?;
    Ok(Counts { participants: sequence.participants.len(), messages: sequence.items.len() })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use vt_canvas::GlyphSet;

    use super::*;
    use crate::settings::LongWords;

    fn rows(canvas: &Canvas, glyphs: GlyphSet) -> Vec<String> {
        canvas.rows(glyphs).map(|runs| runs.into_iter().map(|run| run.text).collect()).collect()
    }

    fn render(source: &str, max_width: u16) -> Result<Canvas, Error> {
        render_with(source, Settings::new(max_width))
    }

    fn listed() -> Vec<(String, String)> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let list = fs::read_to_string(root.join("tests/sequence/diagrams.txt")).unwrap();
        list.lines()
            .map(|path| {
                let name = Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
                (name, fs::read_to_string(root.join(path)).unwrap())
            })
            .collect()
    }

    #[test]
    fn listed_diagrams_match_their_snapshots_at_100_columns() {
        for (name, source) in listed() {
            insta::assert_snapshot!(name, rows(&render(&source, 100).unwrap(), GlyphSet::Unicode).join("\n"));
        }
    }

    #[test]
    fn a_diagram_draws_in_ascii_too() {
        let canvas = render("sequenceDiagram\n  A->>+B: hi\n  B-->>-A: yo\n  A-xB: no", 40).unwrap();

        insta::assert_snapshot!(rows(&canvas, GlyphSet::Ascii).join("\n"));
    }

    /// Every corpus diagram mermaid accepts is drawn within the width, but two with eight and ten participants side by side.
    #[test]
    fn corpus_diagrams_are_drawn_within_80_columns() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/sequence");
        let mut too_wide = Vec::new();
        for entry in fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            match render(&fs::read_to_string(&path).unwrap(), 80) {
                Ok(canvas) => assert!(rows(&canvas, GlyphSet::Unicode).iter().all(|row| vt_canvas::width(row) <= 80), "{path:?}"),
                Err(Error::TooWide { .. }) => too_wide.push(path.file_name().unwrap().to_string_lossy().into_owned()),
                Err(_) => {}
            }
        }
        too_wide.sort();

        assert_eq!(
            too_wide,
            [
                "e2e--diagrams-sequence-nightly-check-v2-classic-all-participant-types.mmd",
                "e2e--diagrams-sequence-nightly-check-v2-classic-participant-creation-and-destruction.mmd"
            ]
        );
    }

    #[test]
    fn labels_wrap_at_spaces_before_heads_close_up() {
        let source = "sequenceDiagram\n  Alice->>Bob: a message far too long to stand on one line here";

        let wide = rows(&render(source, 100).unwrap(), GlyphSet::Unicode);
        let narrow = rows(&render(source, 30).unwrap(), GlyphSet::Unicode);

        assert_eq!(wide.len(), 10);
        assert!(narrow.len() > 10 && narrow.iter().all(|row| vt_canvas::width(row) <= 30), "{narrow:#?}");
        assert!(narrow.iter().any(|row| row.contains("Alice")), "{narrow:#?}");
    }

    #[test]
    fn a_long_word_is_broken_or_cut_as_the_settings_say_once_nothing_else_fits() {
        let source = "sequenceDiagram\n  A->>B: Supercalifragilisticexpialidocious";

        let broken = rows(&render(source, 20).unwrap(), GlyphSet::Unicode);
        let cut = rows(&render_with(source, Settings::new(20).long_words(LongWords::Cut)).unwrap(), GlyphSet::Unicode);

        assert!(broken.iter().any(|row| row.contains("Supercal")) && broken.iter().any(|row| row.contains("docious")), "{broken:#?}");
        assert!(cut.iter().any(|row| row.contains('⋯')), "{cut:#?}");
    }

    #[test]
    fn a_diagram_that_cannot_fit_is_refused_with_its_narrowest_width() {
        let source = "sequenceDiagram\n  participant A\n  participant B\n  participant C\n  participant D";

        assert_eq!(render(source, 10).err(), Some(Error::TooWide { width: 23, max_width: 10 }));
    }

    #[test]
    fn the_label_of_a_message_to_itself_starts_every_line_by_the_loop() {
        let drawn = rows(&render("sequenceDiagram\n  A->>A: one two three<br>four", 40).unwrap(), GlyphSet::Unicode);

        let starts: Vec<usize> = ["one", "four"].iter().map(|word| drawn.iter().find_map(|row| row.find(word)).unwrap()).collect();
        assert_eq!(starts[0], starts[1], "{drawn:#?}");
    }

    #[test]
    fn a_lifeline_passes_behind_a_message_going_over_it() {
        let drawn =
            rows(&render("sequenceDiagram\n  participant A\n  participant B\n  participant C\n  A->>C: x", 40).unwrap(), GlyphSet::Unicode);

        let arrow = drawn.iter().find(|row| row.contains('▸')).unwrap();
        assert!(!arrow.contains('┼') && arrow.trim().starts_with("│─"), "{arrow}");
    }
}
