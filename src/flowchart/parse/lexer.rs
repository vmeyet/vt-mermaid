//! Mermaid's flowchart lexer (`flow.jison`): its rules in its order, the first that matches wins, each start
//! condition a [`State`] on a stack.
//!
//! Jison appends `\b` to a rule ending in a word character, so `graph` is a keyword and `graphite` a node.

use super::{Invalid, is_space, unicode};

/// A token kind, named after the grammar's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    MdStr,
    Str,
    Style,
    Default,
    LinkStyle,
    Interpolate,
    ClassDef,
    Class,
    Graph,
    Subgraph,
    End,
    NoDir,
    Dir,
    Num,
    Brkt,
    StyleSeparator,
    Colon,
    Amp,
    Semi,
    Comma,
    Mult,
    EllipseEnd,
    Text,
    EllipseStart,
    StadiumEnd,
    StadiumStart,
    SubroutineEnd,
    SubroutineStart,
    VertexWithPropsStart,
    TagEnd,
    CylinderEnd,
    CylinderStart,
    DoubleCircleEnd,
    DoubleCircleStart,
    TrapEnd,
    InvTrapEnd,
    TrapStart,
    InvTrapStart,
    TagStart,
    Up,
    Sep,
    Down,
    NodeString,
    Minus,
    UnicodeText,
    Pipe,
    Ps,
    Pe,
    Sqs,
    Sqe,
    DiamondStart,
    DiamondStop,
    Newline,
    Space,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) kind: Kind,
    pub(super) start: usize,
    pub(super) end: usize,
}

/// A start condition of the lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Initial,
    MdString,
    String,
    Dir,
    EllipseText,
    Text,
    TrapText,
}

/// What a rule does to the stack of states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Stay,
    Push(State),
    Pop,
    /// `graph` and the like push `Dir` the first time only.
    Header,
}

/// A rule that matched: how far, the token it gives if any, and its step.
#[derive(Debug, Clone, Copy)]
struct Matched {
    length: usize,
    kind: Option<Kind>,
    step: Step,
}

/// Every token of `text`, ending with one `Eof`; a character no rule matches is an error.
pub(super) fn tokens(text: &str) -> Result<Vec<Token>, Invalid> {
    let mut lexer = Lexer { position: 0, states: vec![State::Initial], first_graph: true };
    let mut tokens = Vec::new();
    while lexer.position < text.len() {
        let start = lexer.position;
        let state = *lexer.states.last().unwrap_or(&State::Initial);
        let matched = rule(state, &text[start..]).ok_or_else(|| Invalid::Unrecognized { line: line_of(text, start) })?;
        lexer.position += matched.length;
        lexer.step(matched.step);
        tokens.extend(matched.kind.map(|kind| Token { kind, start, end: lexer.position }));
    }
    tokens.push(Token { kind: Kind::Eof, start: text.len(), end: text.len() });
    Ok(tokens)
}

pub(super) fn line_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

#[derive(Debug)]
struct Lexer {
    position: usize,
    states: Vec<State>,
    first_graph: bool,
}

impl Lexer {
    fn step(&mut self, step: Step) {
        match step {
            Step::Stay => {}
            Step::Push(state) => self.states.push(state),
            Step::Pop => self.pop(),
            Step::Header => {
                if self.first_graph {
                    self.states.push(State::Dir);
                }
                self.first_graph = false;
            }
        }
    }

    /// Jison never pops the initial state.
    fn pop(&mut self) {
        if self.states.len() > 1 {
            self.states.pop();
        }
    }
}

/// The first rule, in the grammar's order, that is active in `state` and matches at the start of `rest`.
fn rule(state: State, rest: &str) -> Option<Matched> {
    use State as S;
    let initial = state == S::Initial;
    let emit = |length, kind, step| Some(Matched { length, kind: Some(kind), step });
    let skip = |length, step| Some(Matched { length, kind: None, step });
    let first = rest.chars().next()?;
    let is = |wanted: State| state == wanted;

    if is(S::MdString) {
        let length = until(rest, |char| matches!(char, '`' | '"'));
        if length > 0 {
            return emit(length, Kind::MdStr, Step::Stay);
        }
        if rest.starts_with("`\"") {
            return skip(2, Step::Pop);
        }
    }
    if rest.starts_with("\"`") {
        return skip(2, Step::Push(S::MdString));
    }
    if is(S::String) {
        let length = until(rest, |char| char == '"');
        return if length > 0 { emit(length, Kind::Str, Step::Stay) } else { skip(1, Step::Pop) };
    }
    if first == '"' {
        return skip(1, Step::Push(S::String));
    }
    if initial {
        let keywords = [
            ("style", Kind::Style),
            ("default", Kind::Default),
            ("linkStyle", Kind::LinkStyle),
            ("interpolate", Kind::Interpolate),
            ("classDef", Kind::ClassDef),
            ("class", Kind::Class),
        ];
        if let Some(&(word, kind)) = keywords.iter().find(|(word, _)| is_keyword(rest, word)) {
            return emit(word.len(), kind, Step::Stay);
        }
    }
    if initial {
        if let Some(word) = ["flowchart-elk", "swimlane-beta", "graph", "flowchart"].iter().find(|word| is_keyword(rest, word)) {
            return emit(word.len(), Kind::Graph, Step::Header);
        }
        if is_keyword(rest, "subgraph") {
            return emit(8, Kind::Subgraph, Step::Stay);
        }
        if is_keyword(rest, "end") {
            return emit(3 + until(&rest[3..], |char| !is_space(char)), Kind::End, Step::Stay);
        }
    }
    if is(S::Dir) {
        let space = until(rest, |char| !is_space(char));
        if let Some(line_end) = rest[..space].rfind('\n') {
            return emit(line_end + 1, Kind::NoDir, Step::Pop);
        }
        let after = &rest[space..];
        let named = ["LR", "RL", "TB", "BT", "TD", "BR"].iter().find(|word| is_keyword(after, word)).map(|word| word.len());
        let arrow = after.starts_with(['<', '>', '^']).then_some(1).or_else(|| is_keyword(after, "v").then_some(1));
        if let Some(length) = named.or(arrow) {
            return emit(space + length, Kind::Dir, Step::Pop);
        }
    }
    if initial {
        let digits = until(rest, |char| !char.is_ascii_digit());
        if digits > 0 {
            return emit(digits, Kind::Num, Step::Stay);
        }
        if rest.starts_with(":::") {
            return emit(3, Kind::StyleSeparator, Step::Stay);
        }
        let single = [('#', Kind::Brkt), (':', Kind::Colon), ('&', Kind::Amp), (';', Kind::Semi), (',', Kind::Comma), ('*', Kind::Mult)];
        if let Some(&(_, kind)) = single.iter().find(|&&(char, _)| char == first) {
            return emit(1, kind, Step::Stay);
        }
    }
    if is(S::EllipseText) {
        if rest.starts_with(['-', '/', ')']) && rest[1..].starts_with(')') {
            return emit(2, Kind::EllipseEnd, Step::Pop);
        }
        if !matches!(first, '(' | ')' | '[' | ']' | '{' | '}') {
            return emit(first.len_utf8(), Kind::Text, Step::Stay);
        }
    }
    let opening = [
        ("(-", None, Kind::EllipseStart, S::EllipseText),
        ("([", Some(("])", Kind::StadiumEnd)), Kind::StadiumStart, S::Text),
        ("[[", Some(("]]", Kind::SubroutineEnd)), Kind::SubroutineStart, S::Text),
    ];
    for (open, close, kind, inside) in opening {
        if let Some((close, close_kind)) = close
            && is(S::Text)
            && rest.starts_with(close)
        {
            return emit(2, close_kind, Step::Pop);
        }
        if rest.starts_with(open) {
            return emit(2, kind, Step::Push(inside));
        }
    }
    if initial && rest.starts_with("[|") {
        return emit(2, Kind::VertexWithPropsStart, Step::Stay);
    }
    if initial && first == '>' {
        return emit(1, Kind::TagEnd, Step::Push(S::Text));
    }
    for (open, close, open_kind, close_kind) in
        [("[(", ")]", Kind::CylinderStart, Kind::CylinderEnd), ("(((", ")))", Kind::DoubleCircleStart, Kind::DoubleCircleEnd)]
    {
        if is(S::Text) && rest.starts_with(close) {
            return emit(close.len(), close_kind, Step::Pop);
        }
        if rest.starts_with(open) {
            return emit(open.len(), open_kind, Step::Push(S::Text));
        }
    }
    if is(S::TrapText)
        && let Some(matched) = trap_text(rest)
    {
        return Some(matched);
    }
    if rest.starts_with("[/") {
        return emit(2, Kind::TrapStart, Step::Push(S::TrapText));
    }
    if rest.starts_with("[\\") {
        return emit(2, Kind::InvTrapStart, Step::Push(S::TrapText));
    }
    if initial {
        if let Some(&(_, kind)) = [('<', Kind::TagStart), ('^', Kind::Up)].iter().find(|&&(char, _)| char == first) {
            return emit(1, kind, Step::Stay);
        }
        if rest.starts_with("\\|") {
            return emit(2, Kind::Sep, Step::Stay);
        }
        if is_keyword(rest, "v") {
            return emit(1, Kind::Down, Step::Stay);
        }
        let length = node_string(rest);
        if length > 0 {
            return emit(length, Kind::NodeString, Step::Stay);
        }
        if first == '-' {
            return emit(1, Kind::Minus, Step::Stay);
        }
        if unicode::is_letter(first) {
            return emit(first.len_utf8(), Kind::UnicodeText, Step::Stay);
        }
    }
    let brackets = [
        ('|', '|', Kind::Pipe, Kind::Pipe),
        ('(', ')', Kind::Ps, Kind::Pe),
        ('[', ']', Kind::Sqs, Kind::Sqe),
        ('{', '}', Kind::DiamondStart, Kind::DiamondStop),
    ];
    for (open, close, open_kind, close_kind) in brackets {
        if is(S::Text) && first == close {
            return emit(1, close_kind, Step::Pop);
        }
        if first == open {
            return emit(1, open_kind, Step::Push(S::Text));
        }
    }
    if is(S::Text) {
        let length = until(rest, |char| matches!(char, '[' | ']' | '(' | ')' | '{' | '}' | '|' | '"'));
        if length > 0 {
            return emit(length, Kind::Text, Step::Stay);
        }
    }
    if initial {
        let newlines = until(rest, |char| char != '\n');
        if newlines > 0 {
            return emit(newlines, Kind::Newline, Step::Stay);
        }
        let space = until(rest, |char| !is_space(char) || char == '\n' || char == '\r');
        if space > 0 {
            return emit(space, Kind::Space, Step::Stay);
        }
    }
    None
}

/// Bytes before the first character `stop` accepts.
fn until(text: &str, stop: impl Fn(char) -> bool) -> usize {
    text.find(stop).unwrap_or(text.len())
}

/// A JavaScript regex `\w`.
fn is_word(char: char) -> bool {
    char.is_ascii_alphanumeric() || char == '_'
}

/// `word` with no word character after it: jison's `\b` after a rule ending in a letter.
fn is_keyword(text: &str, word: &str) -> bool {
    text.strip_prefix(word).is_some_and(|after| !after.starts_with(is_word))
}

/// The trapezoid's ends and text: `\]`, `(]`, `?]`, `=]` or `]]` closes a trapezoid, `/]` an inverted one.
fn trap_text(text: &str) -> Option<Matched> {
    let closes = text.get(1..).is_some_and(|rest| rest.starts_with(']'));
    if closes && text.starts_with(['\\', '(', '?', '=', ']']) {
        return Some(Matched { length: 2, kind: Some(Kind::TrapEnd), step: Step::Pop });
    }
    if closes && text.starts_with('/') {
        return Some(Matched { length: 2, kind: Some(Kind::InvTrapEnd), step: Step::Pop });
    }
    let length = if text.starts_with(['/', '\\']) {
        usize::from(!closes)
    } else {
        until(text, |char| matches!(char, '\\' | '[' | ']' | '(' | ')' | '{' | '}' | '/'))
    };
    (length > 0).then_some(Matched { length, kind: Some(Kind::Text), step: Step::Stay })
}

/// `([A-Za-z0-9!"#$%&'*+.`?\\_/]|-(?=[^>\-.])|=(?!=))+`.
fn node_string(text: &str) -> usize {
    let bytes = text.as_bytes();
    let taken = bytes.iter().enumerate().position(|(at, &byte)| {
        let next = bytes.get(at + 1).copied();
        let taken = match byte {
            b'-' => next.is_some_and(|next| !matches!(next, b'>' | b'-' | b'.')),
            b'=' => next != Some(b'='),
            _ => byte.is_ascii_alphanumeric() || b"!\"#$%&'*+.`?\\_/".contains(&byte),
        };
        !taken
    });
    taken.unwrap_or(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<Kind> {
        tokens(text).unwrap().into_iter().map(|token| token.kind).collect()
    }

    fn texts(text: &str) -> Vec<(Kind, &str)> {
        tokens(text).unwrap().into_iter().map(|token| (token.kind, &text[token.start..token.end])).collect()
    }

    use Kind as K;

    #[test]
    fn a_header_takes_its_direction_the_first_time_only() {
        assert_eq!(
            texts("graph LR;flowchart TD\n"),
            [
                (K::Graph, "graph"),
                (K::Dir, " LR"),
                (K::Semi, ";"),
                (K::Graph, "flowchart"),
                (K::Space, " "),
                (K::NodeString, "TD"),
                (K::Newline, "\n"),
                (K::Eof, "")
            ]
        );
        assert_eq!(kinds("graph \n\nA\n"), [K::Graph, K::NoDir, K::NodeString, K::Newline, K::Eof]);
    }

    #[test]
    fn a_keyword_inside_a_word_is_a_node() {
        assert_eq!(
            texts("graphite vertex v"),
            [(K::NodeString, "graphite"), (K::Space, " "), (K::NodeString, "vertex"), (K::Space, " "), (K::Down, "v"), (K::Eof, "")]
        );
    }

    #[test]
    fn end_takes_the_spaces_and_line_ends_after_it() {
        assert_eq!(
            texts("end \n\nA end-x"),
            [(K::End, "end \n\n"), (K::NodeString, "A"), (K::Space, " "), (K::End, "end"), (K::NodeString, "-x"), (K::Eof, "")]
        );
    }

    #[test]
    fn shapes_open_and_close_text() {
        assert_eq!(
            kinds("A([a])B[[b]]C[(c)]D(((d)))E((e))F{{f}}G>g]H[/h\\]I[\\i/]J(-j-)"),
            [
                K::NodeString,
                K::StadiumStart,
                K::Text,
                K::StadiumEnd,
                K::NodeString,
                K::SubroutineStart,
                K::Text,
                K::SubroutineEnd,
                K::NodeString,
                K::CylinderStart,
                K::Text,
                K::CylinderEnd,
                K::NodeString,
                K::DoubleCircleStart,
                K::Text,
                K::DoubleCircleEnd,
                K::NodeString,
                K::Ps,
                K::Ps,
                K::Text,
                K::Pe,
                K::Pe,
                K::NodeString,
                K::DiamondStart,
                K::DiamondStart,
                K::Text,
                K::DiamondStop,
                K::DiamondStop,
                K::NodeString,
                K::TagEnd,
                K::Text,
                K::Sqe,
                K::NodeString,
                K::TrapStart,
                K::Text,
                K::TrapEnd,
                K::NodeString,
                K::InvTrapStart,
                K::Text,
                K::InvTrapEnd,
                K::NodeString,
                K::EllipseStart,
                K::Text,
                K::EllipseEnd,
                K::Eof
            ]
        );
    }

    #[test]
    fn quotes_and_markdown_give_their_inside() {
        assert_eq!(
            texts("A[\"a b\"]|\"`**c**`\"|B"),
            [
                (K::NodeString, "A"),
                (K::Sqs, "["),
                (K::Str, "a b"),
                (K::Sqe, "]"),
                (K::Pipe, "|"),
                (K::MdStr, "**c**"),
                (K::Pipe, "|"),
                (K::NodeString, "B"),
                (K::Eof, "")
            ]
        );
    }

    #[test]
    fn a_character_no_rule_reads_is_refused_with_its_line() {
        assert_eq!(tokens("graph TD\nA°"), Err(Invalid::Unrecognized { line: 2 }));
    }

    #[test]
    fn node_strings_keep_dashes_and_equals_before_another_character() {
        assert_eq!(texts("a-b=c-d"), [(K::NodeString, "a-b=c-d"), (K::Eof, "")]);
    }
}
