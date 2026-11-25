use vt_canvas::width;

/// Widest line of a wrapped label: longer text wraps at spaces, a longer word is cut.
const MAX_WRAPPED_COLUMNS: usize = 30;
/// Widest one-line label: an edge's or a group's.
const MAX_LINE_COLUMNS: usize = 60;
const BREAKS: [&str; 4] = ["<br>", "<br/>", "<br />", "\n"];

/// Text drawn in a node or on an edge, already cut into the lines it is drawn on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Label {
    pub(super) lines: Vec<String>,
    /// Columns of the widest line, at most the label kind's limit.
    pub(super) width: u16,
}

impl Label {
    /// Lines break where the source asks (`<br>`) and wrap at spaces past [`MAX_WRAPPED_COLUMNS`].
    pub(super) fn wrapped(text: &str) -> Self {
        let lines: Vec<String> = explicit_lines(text).iter().flat_map(|line| wrap(line, MAX_WRAPPED_COLUMNS)).collect();
        Self::new(lines, MAX_WRAPPED_COLUMNS)
    }

    /// One line: the source's line breaks become spaces.
    pub(super) fn single_line(text: &str) -> Self {
        Self::new(vec![explicit_lines(text).join(" ")], MAX_LINE_COLUMNS)
    }

    pub(super) fn height(&self) -> u16 {
        u16::try_from(self.lines.len()).unwrap_or(u16::MAX)
    }

    fn new(lines: Vec<String>, max_width: usize) -> Self {
        let widest = lines.iter().map(|line| width(line)).max().unwrap_or(0).min(max_width);
        Self { lines, width: u16::try_from(widest).unwrap_or(u16::MAX) }
    }
}

/// The source's lines, quotes around the whole text left out, spaces collapsed.
fn explicit_lines(text: &str) -> Vec<String> {
    let text = text.trim();
    let text = text.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).unwrap_or(text);
    let unified = BREAKS.iter().fold(text.to_owned(), |text, line_break| text.replace(line_break, "\n"));
    let lines: Vec<String> = unified.lines().map(|line| line.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    if lines.is_empty() { vec![String::new()] } else { lines }
}

fn wrap(line: &str, max_width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in line.split(' ') {
        match lines.last_mut() {
            Some(last) if width(last) + 1 + width(word) <= max_width => {
                last.push(' ');
                last.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
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
    fn a_word_longer_than_the_limit_keeps_its_line_and_caps_the_width() {
        let label = Label::wrapped("state-diagram-new-arrow-florbs-and-more");

        assert_eq!(label.lines, ["state-diagram-new-arrow-florbs-and-more"]);
        assert_eq!(label.width, 30);
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
    fn a_single_line_label_joins_its_lines_with_spaces() {
        assert_eq!(Label::single_line("Push to develop<br/>(e.g. a PR merge)").lines, ["Push to develop (e.g. a PR merge)"]);
    }
}
