//! Text cut into the lines it is drawn on: where the source breaks it, then wrapped at spaces.

use vt_canvas::width;

/// Widest line of a wrapped label: longer text wraps at spaces, a longer word goes on as many lines as it takes.
const MAX_WRAPPED_COLUMNS: usize = 30;
/// Widest one-line label: an edge's or a group's.
const MAX_LINE_COLUMNS: usize = 60;
/// Characters a long word is best cut after.
const SEPARATORS: [char; 8] = ['.', '/', '-', '_', ':', ',', '=', '&'];
const BREAKS: [&str; 4] = ["<br>", "<br/>", "<br />", "\n"];

/// Text drawn in a node or on an edge, already cut into the lines it is drawn on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Label {
    pub(crate) lines: Vec<String>,
    /// Columns of the widest line, at most the label kind's limit.
    pub(crate) width: u16,
}

impl Label {
    /// Lines break where the source asks (`<br>`), wrap at spaces past [`MAX_WRAPPED_COLUMNS`] and inside a word
    /// longer than that.
    pub(crate) fn wrapped(text: &str) -> Self {
        Self::wrapped_to(text, MAX_WRAPPED_COLUMNS)
    }

    /// Lines break where the source asks, wrap at spaces past `max_width` columns and inside a word longer than that.
    pub(crate) fn wrapped_to(text: &str, max_width: usize) -> Self {
        let max_width = max_width.max(1);
        let lines: Vec<String> = explicit_lines(text).iter().flat_map(|line| wrap(line, max_width)).collect();
        Self::new(lines, max_width)
    }

    /// One line: the source's line breaks become spaces.
    pub(crate) fn single_line(text: &str) -> Self {
        Self::new(vec![explicit_lines(text).join(" ")], MAX_LINE_COLUMNS)
    }

    pub(crate) fn height(&self) -> u16 {
        u16::try_from(self.lines.len()).unwrap_or(u16::MAX)
    }

    fn new(lines: Vec<String>, max_width: usize) -> Self {
        let widest = lines.iter().map(|line| width(line)).max().unwrap_or(0).min(max_width);
        Self { lines, width: u16::try_from(widest).unwrap_or(u16::MAX) }
    }
}

/// The source's lines, quotes then a Markdown string's backticks around the whole text left out, spaces collapsed.
fn explicit_lines(text: &str) -> Vec<String> {
    let text = text.trim();
    let text = text.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).unwrap_or(text);
    let text = text.strip_prefix('`').and_then(|rest| rest.strip_suffix('`')).unwrap_or(text);
    let unified = BREAKS.iter().fold(text.to_owned(), |text, line_break| text.replace(line_break, "\n"));
    let lines: Vec<String> = unified.lines().map(|line| line.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    if lines.is_empty() { vec![String::new()] } else { lines }
}

fn wrap(line: &str, max_width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for piece in line.split(' ').flat_map(|word| pieces(word, max_width)) {
        match lines.last_mut() {
            Some(last) if width(last) + 1 + width(&piece) <= max_width => {
                last.push(' ');
                last.push_str(&piece);
            }
            _ => lines.push(piece),
        }
    }
    lines
}

/// `word` whole, or cut into pieces of at most `max_width` columns when it is longer than a line.
fn pieces(word: &str, max_width: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut rest = word;
    while let Some(end) = cut(rest, max_width) {
        let (piece, after) = rest.split_at(end);
        pieces.push(piece.to_owned());
        rest = after;
    }
    pieces.push(rest.to_owned());
    pieces
}

/// Where a word too long for a line is cut, `None` for a word that fits: after the last separator in the second half
/// of the line, so a path or a dotted name breaks between its parts, else at the last character that fits.
fn cut(word: &str, max_width: usize) -> Option<usize> {
    let ends = word.char_indices().map(|(start, character)| (start + character.len_utf8(), character));
    let fitting: Vec<(usize, char)> = ends.take_while(|&(end, _)| width(&word[..end]) <= max_width).collect();
    let &(last, _) = fitting.last().filter(|&&(end, _)| end < word.len())?;
    let after_separator =
        fitting.iter().rev().find(|&&(end, character)| SEPARATORS.contains(&character) && 2 * width(&word[..end]) > max_width);
    Some(after_separator.map_or(last, |&(end, _)| end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrapped_label_breaks_at_each_br_tag() {
        assert_eq!(Label::wrapped("one<br>two<br/>three<br />four").lines, ["one", "two", "three", "four"]);
    }

    #[test]
    fn a_wrapped_label_wraps_at_spaces_past_the_limit() {
        let label = Label::wrapped("Investigate the effect of SOX2 on EMT in GBM");

        assert_eq!(label.lines, ["Investigate the effect of SOX2", "on EMT in GBM"]);
        assert_eq!(label.width, 30);
    }

    #[test]
    fn a_word_longer_than_the_limit_goes_on_as_many_lines_as_it_takes() {
        let label = Label::wrapped("see state-diagram-new-arrow-florbs-and-more now");

        assert_eq!(label.lines, ["see state-diagram-new-arrow-", "florbs-and-more now"]);
        assert_eq!(label.width, 28);
    }

    #[test]
    fn a_long_word_without_a_separator_in_the_second_half_of_the_line_is_cut_at_the_limit() {
        let word = format!("a.{}", "n".repeat(40));

        assert_eq!(Label::wrapped(&word).lines, [format!("a.{}", "n".repeat(28)), "n".repeat(12)]);
    }

    #[test]
    fn a_label_wraps_to_the_width_asked() {
        assert_eq!(Label::wrapped_to("one two three", 7).lines, ["one two", "three"]);
    }

    #[test]
    fn a_word_as_long_as_the_limit_keeps_its_line() {
        assert_eq!(Label::wrapped(&"n".repeat(30)).lines, ["n".repeat(30)]);
    }

    #[test]
    fn a_long_word_of_wide_characters_is_cut_by_columns() {
        assert_eq!(Label::wrapped(&"漢".repeat(16)).lines, ["漢".repeat(15), "漢".to_owned()]);
    }

    #[test]
    fn an_empty_wrapped_label_has_one_empty_line() {
        assert_eq!(Label::wrapped("").lines, [""]);
    }

    #[test]
    fn quotes_around_a_label_are_left_out() {
        assert_eq!(Label::single_line("\"PR → develop\"").lines, ["PR → develop"]);
    }

    #[test]
    fn the_backticks_around_a_markdown_string_are_left_out() {
        assert_eq!(Label::wrapped("\"`The dog in **the** hog`\"").lines, ["The dog in **the** hog"]);
        assert_eq!(Label::single_line("`The dog`").lines, ["The dog"]);
    }

    #[test]
    fn a_single_line_label_joins_its_lines_with_spaces() {
        assert_eq!(Label::single_line("Push to develop<br/>(e.g. a PR merge)").lines, ["Push to develop (e.g. a PR merge)"]);
    }
}
