//! Invisible links (`A ~~~ B`): mmdr 0.3.1 does not read them, it keeps `A` and drops the rest of the statement.

use super::source;

/// The source with each statement holding `~~~` cut into the statements on either side, and the links cut out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Split {
    pub(super) source: String,
    pub(super) links: Vec<Link>,
}

/// An invisible link: from the last node `before` names to the first node `after` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Link {
    pub(super) before: String,
    pub(super) after: String,
}

/// Cuts every statement at its `~~~` links, leaving the front matter and quoted or bracketed text as written.
pub(super) fn split(source: &str) -> Split {
    let (front_matter, body) = source::front_matter(source);
    let mut lines: Vec<String> = front_matter.lines().map(str::to_owned).collect();
    let mut links = Vec::new();
    for line in body.lines() {
        let statements = statements(line);
        links.extend(statements.windows(2).map(|pair| Link { before: pair[0].to_owned(), after: pair[1].to_owned() }));
        lines.extend(statements.into_iter().map(str::to_owned));
    }
    let source = if links.is_empty() { source.to_owned() } else { lines.join("\n") };
    Split { source, links }
}

/// The parts of `line` between its `~~~` links, the whole line when it has none.
fn statements(line: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (index, _) in source::syntax(line).filter(|&(_, char)| char == '~') {
        if index >= start && line[index..].starts_with("~~~") {
            parts.push(line[start..index].trim());
            start = index + line[index..].chars().take_while(|&char| char == '~').count();
        }
    }
    parts.push(line[start..].trim());
    if parts.len() == 1 { vec![line] } else { parts }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(before: &str, after: &str) -> Link {
        Link { before: before.to_owned(), after: after.to_owned() }
    }

    #[test]
    fn a_link_is_cut_into_the_statements_on_either_side() {
        let split = split("flowchart LR\n  A ~~~ B[Bee]");

        assert_eq!(split.source, "flowchart LR\nA\nB[Bee]");
        assert_eq!(split.links, [link("A", "B[Bee]")]);
    }

    #[test]
    fn a_chain_is_cut_at_every_link_of_any_length() {
        let split = split("flowchart LR\n  A ~~~~ B --> C ~~~ D");

        assert_eq!(split.links, [link("A", "B --> C"), link("B --> C", "D")]);
    }

    #[test]
    fn tildes_inside_quotes_or_brackets_are_text() {
        let source = "flowchart LR\n  A[\"x ~~~ y\"] --> B(~~~) --> C";

        assert_eq!(split(source), Split { source: source.to_owned(), links: Vec::new() });
    }

    #[test]
    fn the_front_matter_is_left_as_written() {
        let source = "---\ntitle: a ~~~ b\n---\nflowchart LR\n  A --> B";

        assert_eq!(split(source), Split { source: source.to_owned(), links: Vec::new() });
    }
}
