//! Text cut into the lines it is drawn on: where the source breaks it, then wrapped at spaces.

use vt_canvas::text_width;

use crate::settings::LongWords;

/// Widest line of a wrapped label: longer text wraps at spaces, a longer word goes on as many lines as it takes.
const MAX_WRAPPED_COLUMNS: usize = 30;
/// Widest one-line label: an edge's or a group's.
const MAX_LINE_COLUMNS: usize = 60;
/// Characters a long word is best cut after.
const SEPARATORS: [char; 8] = ['.', '/', '-', '_', ':', ',', '=', '&'];

/// Text drawn in a node or on an edge, already cut into the lines it is drawn on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Label {
    pub(crate) lines: Vec<String>,
    /// Columns of the widest line, at most the label kind's limit.
    pub(crate) width: u16,
}

/// How a label's text is cut into lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wrap {
    /// Columns of the widest line.
    pub(crate) columns: usize,
    /// Columns of the widest word that keeps its line whole, however narrow the lines.
    pub(crate) word: usize,
    pub(crate) long_words: LongWords,
}

impl Wrap {
    /// Lines as wide as a label gets, [`MAX_WRAPPED_COLUMNS`].
    pub(crate) const fn wide(long_words: LongWords) -> Self {
        Self { columns: MAX_WRAPPED_COLUMNS, word: MAX_WRAPPED_COLUMNS, long_words }
    }

    /// The same with lines of at most `columns`, never wider than a wide one's.
    pub(crate) fn narrowed(self, columns: usize) -> Self {
        Self { columns: columns.min(MAX_WRAPPED_COLUMNS), ..self }
    }

    /// Lines of at most `columns`, as wide as that may be; a word keeps its line while it is no wider than the lines
    /// or a wide label's.
    pub(crate) fn to(columns: usize, long_words: LongWords) -> Self {
        Self { columns, word: columns.max(MAX_WRAPPED_COLUMNS), long_words }
    }

    /// The same with no word wider than the lines kept whole.
    pub(crate) fn tight(self) -> Self {
        Self { word: self.columns, ..self }
    }
}

impl Label {
    /// Lines break where the source asks (`<br>`), wrap at spaces past [`MAX_WRAPPED_COLUMNS`] and inside a word
    /// longer than that.
    pub(crate) fn wrapped(text: &str) -> Self {
        Self::wrapped_as(text, Wrap::wide(LongWords::Break))
    }

    /// Lines break where the source asks and wrap at spaces past the columns of `wrap`; a word keeps its line while
    /// it is no wider than the word `wrap` allows, then is broken over lines, or keeps one line the drawing cuts, as
    /// `wrap` says.
    pub(crate) fn wrapped_as(text: &str, wrap: Wrap) -> Self {
        let word = wrap.word.max(1);
        let whole = |piece: &str| vec![piece.to_owned()];
        let broken = |piece: &str| if wrap.long_words == LongWords::Break { pieces(piece, word) } else { whole(piece) };
        let lines: Vec<String> =
            explicit_lines(text).iter().flat_map(|line| filled(line.split(' ').flat_map(broken), wrap.columns)).collect();
        Self::new(lines, word.max(wrap.columns))
    }

    /// One line: the source's line breaks become spaces.
    pub(crate) fn single_line(text: &str) -> Self {
        Self::new(vec![explicit_lines(text).join(" ")], MAX_LINE_COLUMNS)
    }

    pub(crate) fn height(&self) -> u16 {
        u16::try_from(self.lines.len()).unwrap_or(u16::MAX)
    }

    fn new(lines: Vec<String>, max_width: usize) -> Self {
        let widest = lines.iter().map(|line| text_width(line)).max().unwrap_or(0).min(max_width);
        Self { lines, width: u16::try_from(widest).unwrap_or(u16::MAX) }
    }
}

/// The source's lines, quotes then a Markdown string's backticks around the whole text left out, spaces collapsed.
fn explicit_lines(text: &str) -> Vec<String> {
    let text = text.trim();
    let text = text.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).unwrap_or(text);
    let text = text.strip_prefix('`').and_then(|rest| rest.strip_suffix('`')).unwrap_or(text);
    let unified = with_breaks_as_line_ends(text);
    let lines: Vec<String> = unified.lines().map(|line| line.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    if lines.is_empty() { vec![String::new()] } else { lines }
}

/// The pieces on lines of at most `max_width` columns, a space between two on a line; a piece wider has its own.
/// Mermaid's `lineBreakRegex`, `<br\s*/?>` in any case, as a line end.
fn with_breaks_as_line_ends(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let tag = &rest[open..];
        let length = tag.get(..3).filter(|name| name.eq_ignore_ascii_case("<br")).and_then(|_| {
            let after_space = tag[3..].trim_start_matches(char::is_whitespace);
            let after_slash = after_space.strip_prefix('/').unwrap_or(after_space);
            after_slash.starts_with('>').then(|| tag.len() - after_slash.len() + 1)
        });
        out.push_str(if length.is_some() { "\n" } else { "<" });
        rest = &tag[length.unwrap_or(1)..];
    }
    out.push_str(rest);
    out
}

fn filled(pieces: impl Iterator<Item = String>, max_width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for piece in pieces {
        match lines.last_mut() {
            Some(last) if text_width(last) + 1 + text_width(&piece) <= max_width => {
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
    let mut columns = 0;
    let mut fitting: Vec<(usize, char, usize)> = Vec::new();
    for (start, character) in word.char_indices() {
        let end = start + character.len_utf8();
        columns += text_width(&word[start..end]);
        if columns > max_width {
            let &(last, ..) = fitting.last()?;
            let after_separator =
                fitting.iter().rev().find(|&&(_, character, columns)| SEPARATORS.contains(&character) && 2 * columns > max_width);
            return Some(after_separator.map_or(last, |&(end, ..)| end));
        }
        fitting.push((end, character, columns));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrapped_label_breaks_at_each_br_tag() {
        assert_eq!(Label::wrapped("one<br>two<br/>three<br />four").lines, ["one", "two", "three", "four"]);
    }

    #[test]
    fn a_break_tag_in_any_spelling_breaks_a_line_and_another_tag_stays() {
        assert_eq!(Label::wrapped("a<BR>b<br \t/>c<br/ >d<b>e").lines, ["a", "b", "c<br/ >d<b>e"]);
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
    fn a_label_wraps_to_any_width_and_keeps_a_word_up_to_that_width_whole() {
        let label = Label::wrapped_as("one two three a-word-of-forty-columns-going-on-and-on", Wrap::to(40, LongWords::Break));

        assert_eq!(label.lines, ["one two three", "a-word-of-forty-columns-going-on-and-on"]);
        assert_eq!(Label::wrapped_as("one two three", Wrap::to(usize::MAX, LongWords::Break)).lines, ["one two three"]);
    }

    #[test]
    fn a_tight_wrap_breaks_or_cuts_a_word_wider_than_its_lines() {
        let tight = Wrap::to(6, LongWords::Break).tight();

        assert_eq!(Label::wrapped_as("Notification", tight).lines, ["Notifi", "cation"]);
        assert_eq!(Label::wrapped_as("Notification", Wrap { long_words: LongWords::Cut, ..tight }).width, 6);
    }

    #[test]
    fn a_long_word_cut_keeps_one_line_as_wide_as_the_limit() {
        let label = Label::wrapped_as("see state-diagram-new-arrow-florbs-and-more now", Wrap::wide(LongWords::Cut));

        assert_eq!(label.lines, ["see", "state-diagram-new-arrow-florbs-and-more", "now"]);
        assert_eq!(label.width, 30);
    }

    #[test]
    fn a_narrowed_label_wraps_sooner_and_keeps_a_word_no_longer_than_a_wide_line_whole() {
        let narrowed = Wrap::wide(LongWords::Break).narrowed(14);

        let label = Label::wrapped_as("Sleep and chill at home SAM.CommonFA.FMESummary", narrowed);

        assert_eq!(label.lines, ["Sleep and", "chill at home", "SAM.CommonFA.FMESummary"]);
        assert_eq!(label.width, 23);
    }

    #[test]
    fn a_word_of_any_length_is_cut_in_one_pass_over_it() {
        let word = "n".repeat(60_000);

        assert_eq!(Label::wrapped_as(&word, Wrap::to(usize::MAX, LongWords::Break)).lines, std::slice::from_ref(&word));
        assert_eq!(Label::wrapped_as(&word, Wrap::to(6, LongWords::Break).tight()).lines.len(), 10_000);
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
