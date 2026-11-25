//! Reading Mermaid source text before mmdr does, for the syntax mmdr 0.3.1 misreads.

/// Headers that open a flowchart without naming its direction.
const BARE_HEADERS: [&str; 3] = ["flowchart", "graph", "flowchart-elk"];

/// The source with ` TD` after a header that names no direction: mmdr 0.3.1 reads a bare `flowchart` as a node.
pub(super) fn with_direction(source: &str) -> String {
    let (front_matter, body) = front_matter(source);
    let header = body.split_inclusive('\n').position(|line| !is_blank_or_comment(line));
    let lines = body.split_inclusive('\n').enumerate().map(|(index, line)| match header {
        Some(header) if index == header && is_bare_header(line) => directed(line),
        _ => line.to_owned(),
    });
    std::iter::once(front_matter.to_owned()).chain(lines).collect()
}

fn is_blank_or_comment(line: &str) -> bool {
    let line = line.trim();
    line.is_empty() || line.starts_with("%%")
}

fn is_bare_header(line: &str) -> bool {
    BARE_HEADERS.iter().any(|header| line.trim().eq_ignore_ascii_case(header))
}

fn directed(header: &str) -> String {
    let line_end = if header.ends_with('\n') { "\n" } else { "" };
    format!("{} TD{line_end}", header.trim_end())
}

/// The front matter, its `---` lines included, and the text after it.
pub(super) fn front_matter(source: &str) -> (&str, &str) {
    let closing = source.strip_prefix("---\n").and_then(|rest| rest.find("\n---\n"));
    closing.map_or(("", source), |end| source.split_at("---\n".len() + end + "\n---\n".len()))
}

/// The characters of `text` outside quotes and brackets, with their byte index: the statement's syntax, not its labels.
pub(super) fn syntax(text: &str) -> impl Iterator<Item = (usize, char)> + '_ {
    let mut depth = 0_usize;
    let mut quoted = false;
    text.char_indices().filter(move |&(_, char)| {
        let outside = !quoted && depth == 0;
        match char {
            '"' => quoted = !quoted,
            '[' | '(' | '{' if !quoted => depth += 1,
            ']' | ')' | '}' if !quoted => depth = depth.saturating_sub(1),
            _ => {}
        }
        outside && !matches!(char, '"' | '[' | '(' | '{')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_front_matter_ends_on_its_closing_line() {
        assert_eq!(front_matter("---\ntitle: a\n---\nflowchart LR"), ("---\ntitle: a\n---\n", "flowchart LR"));
        assert_eq!(front_matter("flowchart LR\n---\n"), ("", "flowchart LR\n---\n"));
    }

    #[test]
    fn a_header_without_a_direction_goes_down() {
        assert_eq!(with_direction("flowchart\n  A --> B"), "flowchart TD\n  A --> B");
        assert_eq!(with_direction("Graph  \nA"), "Graph TD\nA");
    }

    #[test]
    fn the_header_is_found_after_the_front_matter_and_comments() {
        let source = "---\ntitle: graph\n---\n%% flowchart\n\n  flowchart-elk\nA";

        assert_eq!(with_direction(source), "---\ntitle: graph\n---\n%% flowchart\n\n  flowchart-elk TD\nA");
    }

    #[test]
    fn a_header_with_a_direction_or_a_later_line_is_left_as_written() {
        assert_eq!(with_direction("flowchart LR\n  graph"), "flowchart LR\n  graph");
    }

    #[test]
    fn syntax_leaves_out_quoted_and_bracketed_text() {
        let kept: String = syntax(r#"A["x [y]"] --> B(z) & "w" C"#).map(|(_, char)| char).collect();

        assert_eq!(kept, "A --> B &  C");
    }
}
