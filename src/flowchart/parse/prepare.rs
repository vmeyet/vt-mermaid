//! What mermaid does to the text before its flowchart lexer reads it, step for step, each in one pass over the text.

use super::is_space;

/// Characters mermaid puts in place of the `&` an entity such as `#quot;` starts with, and of its `;`.
pub(super) const ENTITY_START: &str = "ﬂ°";
pub(super) const ENTITY_END: &str = "¶ß";

/// The text the lexer reads and what the configuration changes in its meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Prepared {
    pub(super) text: String,
    pub(super) config: Config,
}

/// The settings of a front matter or a directive that change what a flowchart means.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Config {
    /// `inheritDir`: a subgraph without its own direction takes the flowchart's.
    pub(super) inherits_direction: bool,
}

/// Line ends unified, quotes in HTML tags made single, the front matter, directives and comment lines left out,
/// entities marked, a newline added and the spaces after a closing brace cut to one line end.
pub(super) fn prepare(source: &str) -> Prepared {
    let unified = with_single_quoted_attributes(&source.replace("\r\n", "\n").replace('\r', "\n"));
    let (front_matter, body) = unified.split_at(front_matter_length(&unified).unwrap_or(0));
    let (directives, code) = split_directives(body);
    let code = without_comment_lines(&code);
    let styled = without_last_semicolon(&without_last_semicolon(&code, "style"), "classDef");
    let text = with_braces_ending_lines(&format!("{}\n", with_entities_marked(&styled)));
    Prepared { text, config: Config { inherits_direction: inherits_direction(front_matter) || inherits_direction(&directives) } }
}

/// `inheritDir: true` in a front matter's YAML or a directive's JSON, the one setting that changes a flowchart's meaning.
fn inherits_direction(config: &str) -> bool {
    config.match_indices("inheritDir").any(|(at, word)| {
        let after = config[at + word.len()..].strip_prefix(['"', '\'']).unwrap_or(&config[at + word.len()..]);
        let value = after.trim_start_matches(is_space).strip_prefix(':').map(|value| value.trim_start_matches(is_space));
        value.is_some_and(|value| value.starts_with("true"))
    })
}

/// Mermaid's `matchFrontMatter`: the opening `---`, indented or not, ends at the last line break of the space after
/// it that still leaves a closing fence behind; the body ends at the first closing fence after that.
fn front_matter_length(text: &str) -> Option<usize> {
    let indent_end = text.find(|char: char| !is_space(char) || char == '\n').unwrap_or(text.len());
    let fence_end = indent_end + text[indent_end..].starts_with("---").then_some(3)?;
    let closing_fence = format!("{}---", &text[..indent_end]);
    let closing_end = |at: usize| {
        let after = text[at..].strip_prefix('\n').filter(|rest| rest.starts_with(&closing_fence))?;
        last_line_break(text, text.len() - after.len() + closing_fence.len())
    };
    let opening_ends: Vec<usize> = line_breaks_of_space(text, fence_end).collect();
    let earliest_body = opening_ends.first()? + 1;
    let last_closing = (earliest_body..text.len()).rev().find(|&at| text.is_char_boundary(at) && closing_end(at).is_some())?;
    let opening_end = opening_ends.iter().copied().rfind(|&end| end < last_closing).unwrap_or(opening_ends[0]);
    (opening_end + 1..=last_closing).find_map(|at| if text.is_char_boundary(at) { closing_end(at) } else { None }).map(|end| end + 1)
}

/// Where the line breaks are in the run of spaces starting at `from`.
fn line_breaks_of_space(text: &str, from: usize) -> impl Iterator<Item = usize> + '_ {
    text[from..].char_indices().take_while(|&(_, char)| is_space(char)).filter(|&(_, char)| char == '\n').map(move |(at, _)| from + at)
}

fn last_line_break(text: &str, from: usize) -> Option<usize> {
    line_breaks_of_space(text, from).last()
}

/// Mermaid's `cleanupText`: inside an HTML tag, `="value"` becomes `='value'`.
fn with_single_quoted_attributes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        let name = rest[open + 1..].find(|char: char| !is_word(char)).unwrap_or(rest.len() - open - 1);
        let close = rest[open + 1 + name..].find('>');
        let Some(close) = close.filter(|_| name > 0).map(|close| open + 1 + name + close) else {
            out.push_str(&rest[..=open]);
            rest = &rest[open + 1..];
            continue;
        };
        out.push_str(&rest[..open + 1 + name]);
        out.push_str(&single_quoted(&rest[open + 1 + name..close]));
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

fn single_quoted(attributes: &str) -> String {
    let mut out = String::with_capacity(attributes.len());
    let mut rest = attributes;
    while let Some(at) = rest.find("=\"") {
        let Some(end) = rest[at + 2..].find('"') else { break };
        out.push_str(&rest[..at]);
        out.push_str(&format!("='{}'", &rest[at + 2..at + 2 + end]));
        rest = &rest[at + 2 + end + 1..];
    }
    out.push_str(rest);
    out
}

/// Mermaid's directives, `%%{init: {…}}%%`, as its `directiveRegex` finds them, and the text without them.
fn split_directives(text: &str) -> (String, String) {
    let (mut directives, mut code) = (String::new(), String::with_capacity(text.len()));
    let mut rest = text;
    while let Some(start) = rest.find("%%{") {
        let length = directive_length(&rest[start..]);
        code.push_str(&rest[..start + usize::from(length.is_none())]);
        directives.push_str(&rest[start..start + length.unwrap_or(0)]);
        rest = &rest[start + length.unwrap_or(1)..];
    }
    code.push_str(rest);
    (directives, code)
}

/// `%%{`, a name with or without a `:`, then a word or anything up to `}%%`, then `}%%` if it is there.
fn directive_length(text: &str) -> Option<usize> {
    let name_start = skip_space(text, 3);
    let name_end = name_start + text[name_start..].find(|char: char| !is_word(char)).unwrap_or(text.len() - name_start);
    if name_end == name_start {
        return None;
    }
    let after_colon = skip_space(text, name_end);
    let value_start = skip_space(text, if text[after_colon..].starts_with(':') { after_colon + 1 } else { name_end });
    let value = &text[value_start..];
    let value_end = if value.starts_with(is_word) {
        value_start + value.find(|char: char| !is_word(char)).unwrap_or(value.len())
    } else {
        value_start + value.find(['\u{2028}', '\u{2029}']).unwrap_or(value.len()).min(value.find("}%%").unwrap_or(value.len()))
    };
    let end = skip_space(text, value_end);
    Some(if text[end..].starts_with("}%%") { end + 3 } else { end })
}

/// Mermaid's `cleanupComments`: a line holding only a `%%` comment goes with the blank lines before it, then the
/// spaces the text starts with.
fn without_comment_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut line = 0;
    while line < text.len() {
        let marker = skip_space(text, line);
        let comment = text[marker..].strip_prefix("%%").filter(|rest| rest.starts_with(|char| char != '{' && char != '\n'));
        if let Some(rest) = comment {
            let end = text.len() - rest.len() + rest.find('\n').map_or(rest.len(), |at| at + 1);
            out.push_str(&text[copied..line]);
            (copied, line) = (end, end);
        } else {
            let Some(next) = text[marker..].find('\n') else { break };
            line = marker + next + 1;
        }
    }
    out.push_str(&text[copied..]);
    out.trim_start_matches(is_space).to_owned()
}

/// Mermaid's `encodeEntities` first step: on a line with `keyword`, then `:`, `#` in the same word, then a `;`, the
/// last `;` goes.
fn without_last_semicolon(text: &str, keyword: &str) -> String {
    let lines = text.split_inclusive(['\n', '\u{2028}', '\u{2029}']);
    lines
        .map(|line| match line.find(keyword).zip(line.rfind(';')) {
            Some((start, end)) if start + keyword.len() <= end && colon_then_hash(&line[start + keyword.len()..end]) => {
                format!("{}{}", &line[..end], &line[end + 1..])
            }
            _ => line.to_owned(),
        })
        .collect()
}

fn colon_then_hash(text: &str) -> bool {
    let mut colon = false;
    text.chars().any(|char| {
        colon = match char {
            ':' => true,
            _ if is_space(char) => false,
            _ => colon,
        };
        char == '#' && colon
    })
}

/// Mermaid's `encodeEntities`: `#quot;` becomes `ﬂ°quot¶ß`, `#35;` becomes `ﬂ°°35¶ß`.
fn with_entities_marked(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(hash) = rest.find('#') {
        let name = &rest[hash + 1..];
        let length = name.find(|char: char| !is_word(char)).unwrap_or(name.len());
        out.push_str(&rest[..hash]);
        if length > 0 && name[length..].starts_with(';') {
            let numeric = name[..length].bytes().all(|byte| byte.is_ascii_digit());
            out.push_str(&format!("{ENTITY_START}{}{}{ENTITY_END}", if numeric { "°" } else { "" }, &name[..length]));
            rest = &name[length + 1..];
        } else {
            out.push('#');
            rest = name;
        }
    }
    out.push_str(rest);
    out
}

/// Mermaid's flowchart parser cuts the spaces after a `}` up to the last line end among them to that line end.
fn with_braces_ending_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(brace) = rest.find('}') {
        out.push_str(&rest[..=brace]);
        rest = &rest[brace + 1..];
        if let Some(line_end) = last_line_break(rest, 0) {
            out.push('\n');
            rest = &rest[line_end + 1..];
        }
    }
    out.push_str(rest);
    out
}

fn skip_space(text: &str, from: usize) -> usize {
    from + text[from..].find(|char| !is_space(char)).unwrap_or(text.len() - from)
}

/// A character of a JavaScript regex `\w`.
fn is_word(char: char) -> bool {
    char.is_ascii_alphanumeric() || char == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_ends_are_unified_and_a_newline_is_added() {
        assert_eq!(prepare("graph TD\r\nA\rB").text, "graph TD\nA\nB\n");
    }

    #[test]
    fn inherit_dir_is_read_from_the_front_matter_or_a_directive() {
        let inherits = |source: &str| prepare(source).config.inherits_direction;

        assert!(inherits("---\nconfig:\n  flowchart:\n    inheritDir: true\n---\ngraph TD"));
        assert!(inherits("%%{init: {\"flowchart\": { \"inheritDir\": true }}}%%\ngraph TD"));
        assert!(!inherits("graph TD\n  A[inheritDir: true]"));
    }

    #[test]
    fn double_quotes_inside_a_tag_become_single() {
        assert_eq!(with_single_quoted_attributes(r#"A["<b class="x" id="y">t</b>"] "q""#), r#"A["<b class='x' id='y'>t</b>"] "q""#);
        assert_eq!(with_single_quoted_attributes(r#"a < b="c""#), r#"a < b="c""#);
    }

    #[test]
    fn the_front_matter_is_left_out() {
        assert_eq!(prepare("---\ntitle: a\n---\ngraph TD").text, "graph TD\n");
        assert_eq!(prepare("  ---\n  title: a\n  ---\n\ngraph TD").text, "graph TD\n");
        assert_eq!(front_matter_length("---\ntitle: a\ngraph TD"), None);
    }

    #[test]
    fn the_longest_opening_that_leaves_a_closing_fence_is_taken() {
        assert_eq!(prepare("---\n\n---\n\nMORE\n---\nrest").text, "rest\n");
    }

    #[test]
    fn directives_are_left_out_up_to_their_closing_marks() {
        assert_eq!(split_directives("%%{init: {\"theme\": \"dark\"}}%%\ngraph TD").1, "\ngraph TD");
        assert_eq!(split_directives("%%{wrap}%%graph"), ("%%{wrap}%%".to_owned(), "graph".to_owned()));
        assert_eq!(split_directives("%%{ }%% x").1, "%%{ }%% x");
    }

    #[test]
    fn a_word_after_the_name_ends_a_directive() {
        assert_eq!(split_directives("%%{init: abc def}%%").1, "def}%%");
    }

    #[test]
    fn comment_lines_go_with_the_blank_lines_before_them() {
        assert_eq!(without_comment_lines("%% one\ngraph TD\n\n  %% two\nA %% three\n%%\n"), "graph TD\nA %% three\n%%\n");
    }

    #[test]
    fn the_last_semicolon_of_a_style_line_with_a_colour_goes() {
        assert_eq!(without_last_semicolon("style A fill:#f9f;stroke:#333;\nB;", "style"), "style A fill:#f9f;stroke:#333\nB;");
        assert_eq!(without_last_semicolon("style A fill: #f9f;", "style"), "style A fill: #f9f;");
    }

    #[test]
    fn entities_are_marked_as_mermaid_marks_them() {
        assert_eq!(with_entities_marked("a #quot; b #35; #x #;"), "a ﬂ°quot¶ß b ﬂ°°35¶ß #x #;");
    }

    #[test]
    fn spaces_after_a_brace_end_at_their_last_line_end() {
        assert_eq!(with_braces_ending_lines("}  \n \n  A} B\n"), "}\n  A} B\n");
    }
}
