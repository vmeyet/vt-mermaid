//! Reading Mermaid source text before mmdr does, for the syntax mmdr 0.3.1 misreads.

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
    fn syntax_leaves_out_quoted_and_bracketed_text() {
        let kept: String = syntax(r#"A["x [y]"] --> B(z) & "w" C"#).map(|(_, char)| char).collect();

        assert_eq!(kept, "A --> B &  C");
    }
}
