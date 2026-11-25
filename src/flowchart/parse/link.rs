//! What a link's characters mean (`destructLink` in mermaid's db): its heads, its stroke and its length.

use super::is_space;

/// Mermaid stores no longer link.
const MAX_LENGTH: usize = 10;

/// A link as mermaid reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flowchart) struct Link {
    pub(in crate::flowchart) arrow: Arrow,
    pub(in crate::flowchart) stroke: Stroke,
    /// Extra ranks the link spans plus one; `None` on an invalid link.
    pub(in crate::flowchart) length: Option<usize>,
}

/// The heads of a link; `Double` ones have the same head at the start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flowchart) enum Arrow {
    Open,
    Point,
    Cross,
    Circle,
    DoublePoint,
    DoubleCross,
    DoubleCircle,
    /// Text links whose two ends disagree, as `<-- text --x`.
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flowchart) enum Stroke {
    Normal,
    Thick,
    Dotted,
    Invisible,
    Invalid,
}

impl Arrow {
    /// Mermaid's name, as its db holds it.
    #[cfg(test)]
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Open => "arrow_open",
            Self::Point => "arrow_point",
            Self::Cross => "arrow_cross",
            Self::Circle => "arrow_circle",
            Self::DoublePoint => "double_arrow_point",
            Self::DoubleCross => "double_arrow_cross",
            Self::DoubleCircle => "double_arrow_circle",
            Self::Invalid => "INVALID",
        }
    }

    fn doubled(self) -> Self {
        match self {
            Self::Point => Self::DoublePoint,
            Self::Cross => Self::DoubleCross,
            Self::Circle => Self::DoubleCircle,
            other => other,
        }
    }
}

impl Stroke {
    #[cfg(test)]
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Thick => "thick",
            Self::Dotted => "dotted",
            Self::Invisible => "invisible",
            Self::Invalid => "INVALID",
        }
    }
}

/// The link `end` closes, opened by `start` when it has text in the middle (`-- text -->`).
pub(super) fn read(end: &str, start: Option<&str>) -> Link {
    let end_link = ending(end);
    let Some(start) = start else { return end_link };
    let (head, stroke) = opening(start);
    if stroke != end_link.stroke {
        return Link { arrow: Arrow::Invalid, stroke: Stroke::Invalid, length: None };
    }
    let arrow = match head {
        None => end_link.arrow,
        Some(head) if head == end_link.arrow => head.doubled(),
        Some(_) => return Link { arrow: Arrow::Invalid, stroke: Stroke::Invalid, length: None },
    };
    Link { arrow, ..end_link }
}

/// `destructEndLink`: the last character is the head, a matching first one doubles it, the rest is the line.
fn ending(text: &str) -> Link {
    let text = text.trim_matches(is_space);
    let (body, last) = text.split_at(text.char_indices().next_back().map_or(0, |(at, _)| at));
    let (arrow, line) = match last {
        "x" => doubled(Arrow::Cross, body, 'x'),
        ">" => doubled(Arrow::Point, body, '<'),
        "o" => doubled(Arrow::Circle, body, 'o'),
        _ => (Arrow::Open, body),
    };
    let dots = line.matches('.').count();
    let stroke = if dots > 0 {
        Stroke::Dotted
    } else if line.starts_with('~') {
        Stroke::Invisible
    } else if line.starts_with('=') {
        Stroke::Thick
    } else {
        Stroke::Normal
    };
    let length = if dots > 0 { dots } else { line.chars().count().saturating_sub(1) };
    Link { arrow, stroke, length: Some(length.min(MAX_LENGTH)) }
}

fn doubled(head: Arrow, body: &str, first: char) -> (Arrow, &str) {
    body.strip_prefix(first).map_or((head, body), |line| (head.doubled(), line))
}

/// `destructStartLink`: the head of `<-- text`, if any, and the stroke.
fn opening(text: &str) -> (Option<Arrow>, Stroke) {
    let text = text.trim_matches(is_space);
    let (head, line) = match text.chars().next() {
        Some('<') => (Some(Arrow::Point), &text[1..]),
        Some('x') => (Some(Arrow::Cross), &text[1..]),
        Some('o') => (Some(Arrow::Circle), &text[1..]),
        _ => (None, text),
    };
    let stroke = if line.contains('.') {
        Stroke::Dotted
    } else if line.contains('=') {
        Stroke::Thick
    } else {
        Stroke::Normal
    };
    (head, stroke)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(arrow: Arrow, stroke: Stroke, length: usize) -> Link {
        Link { arrow, stroke, length: Some(length) }
    }

    #[test]
    fn the_last_character_is_the_head_and_a_matching_first_doubles_it() {
        assert_eq!(read(" --> ", None), link(Arrow::Point, Stroke::Normal, 1));
        assert_eq!(read("<-->", None), link(Arrow::DoublePoint, Stroke::Normal, 1));
        assert_eq!(read("o--o", None), link(Arrow::DoubleCircle, Stroke::Normal, 1));
        assert_eq!(read("x--x", None), link(Arrow::DoubleCross, Stroke::Normal, 1));
        assert_eq!(read("---", None), link(Arrow::Open, Stroke::Normal, 1));
        assert_eq!(read("x-->", None), link(Arrow::Point, Stroke::Normal, 2));
    }

    #[test]
    fn dashes_or_dots_beyond_the_shortest_link_add_length() {
        assert_eq!(read("---->", None), link(Arrow::Point, Stroke::Normal, 3));
        assert_eq!(read("-...->", None), link(Arrow::Point, Stroke::Dotted, 3));
        assert_eq!(read("====", None), link(Arrow::Open, Stroke::Thick, 2));
        assert_eq!(read(&format!("{}>", "-".repeat(20)), None), link(Arrow::Point, Stroke::Normal, 10));
    }

    #[test]
    fn tildes_are_invisible() {
        assert_eq!(read(" ~~~ ", None), link(Arrow::Open, Stroke::Invisible, 1));
    }

    #[test]
    fn a_link_with_text_takes_its_head_from_both_ends() {
        assert_eq!(read("-->", Some("-- ")), link(Arrow::Point, Stroke::Normal, 1));
        assert_eq!(read("-->", Some("<--")), link(Arrow::DoublePoint, Stroke::Normal, 1));
        assert_eq!(read(".->", Some("-.")), link(Arrow::Point, Stroke::Dotted, 1));
        assert_eq!(read("--x", Some("<--")), Link { arrow: Arrow::Invalid, stroke: Stroke::Invalid, length: None });
    }
}
