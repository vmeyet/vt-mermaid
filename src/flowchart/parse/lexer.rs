//! Mermaid's flowchart lexer (`flow.jison`): its rules in its order, the first that matches wins, each start
//! condition a [`State`] on a stack.
//!
//! Jison appends `\b` to a rule ending in a word character, so `graph` is a keyword and `graphite` a node.

use super::{Invalid, is_space};
use crate::source::unicode;

/// A token kind, named after the grammar's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    /// `@{` and every part of the block after it, all `SHAPE_DATA` to the grammar.
    ShapeData,
    /// Text inside quotes in a block, whose line ends become `<br/>`.
    ShapeDataString,
    CallbackName,
    CallbackArgs,
    MdStr,
    Str,
    Style,
    Default,
    LinkStyle,
    Interpolate,
    ClassDef,
    Class,
    Href,
    Click,
    Graph,
    Subgraph,
    End,
    LinkTarget,
    NoDir,
    Dir,
    Direction(Direction),
    LinkId,
    Num,
    Brkt,
    StyleSeparator,
    Colon,
    Amp,
    Semi,
    Comma,
    Mult,
    Link,
    StartLink,
    EdgeText,
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

/// The direction a `direction` statement names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flowchart) enum Direction {
    TB,
    BT,
    RL,
    LR,
    TD,
}

/// The rules try the directions in this order.
const DIRECTIONS: [(&str, Direction); 5] =
    [("TB", Direction::TB), ("BT", Direction::BT), ("RL", Direction::RL), ("LR", Direction::LR), ("TD", Direction::TD)];

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
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    ShapeData,
    ShapeDataStr,
    CallbackName,
    CallbackArgs,
    MdString,
    String,
    Click,
    Dir,
    EdgeText,
    ThickEdgeText,
    DottedEdgeText,
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
    Swap(State),
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
    let mut ahead = Ahead { text, directions: directions(text), run: None };
    let mut tokens = Vec::new();
    while lexer.position < text.len() {
        let start = lexer.position;
        let state = *lexer.states.last().unwrap_or(&State::Initial);
        let matched = rule(state, &text[start..], &mut ahead).ok_or_else(|| Invalid::Unrecognized { line: line_of(text, start) })?;
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

/// What two rules look for beyond the token at hand: a `direction` ahead on the line, the end of a link id.
#[derive(Debug)]
struct Ahead<'a> {
    text: &'a str,
    directions: Vec<Occurrence>,
    run: Option<Run>,
}

/// A `direction XX` in the text: where it starts, its direction and where its two letters end.
#[derive(Debug, Clone, Copy)]
struct Occurrence {
    start: usize,
    direction: Direction,
    end: usize,
}

/// A run of characters a link id may hold, and its last `@` a link id may end on.
#[derive(Debug, Clone, Copy)]
struct Run {
    start: usize,
    end: usize,
    last_at: Option<usize>,
}

impl Lexer {
    fn step(&mut self, step: Step) {
        match step {
            Step::Stay => {}
            Step::Push(state) => self.states.push(state),
            Step::Pop => self.pop(),
            Step::Swap(state) => {
                self.pop();
                self.states.push(state);
            }
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

impl Ahead<'_> {
    /// `.*direction\s+XX[^\n]*`: a `direction` anywhere ahead on the line, the directions tried in the rules' order;
    /// the greedy `.*` reaches the last one, and the match runs to the line end after it.
    fn direction(&self, rest: &str) -> Option<(Direction, usize)> {
        let position = self.text.len() - rest.len();
        let line_end = position + until(rest, |char| matches!(char, '\n' | '\r' | '\u{2028}' | '\u{2029}'));
        let first = self.directions.partition_point(|occurrence| occurrence.start < position);
        let ahead = self.directions[first..].iter().take_while(|occurrence| occurrence.start < line_end);
        let (_, direction) = DIRECTIONS.iter().find(|(_, direction)| ahead.clone().any(|occurrence| occurrence.direction == *direction))?;
        let last = ahead.filter(|occurrence| occurrence.direction == *direction).last()?;
        let end = last.end + until(&self.text[last.end..], |char| char == '\n');
        Some((*direction, end - position))
    }

    /// `[^\s"[\](){}|]+@(?=[^{"])`: the run ahead, up to its last `@` followed by a character other than `{` or `"`.
    fn link_id(&mut self, rest: &str) -> Option<usize> {
        let position = self.text.len() - rest.len();
        let run = match self.run {
            Some(run) if run.start <= position && position < run.end => run,
            _ => {
                let end = position + until(rest, |char| is_space(char) || matches!(char, '"' | '[' | ']' | '(' | ')' | '{' | '}' | '|'));
                let last_at = self.text[position..end]
                    .match_indices('@')
                    .map(|(at, _)| position + at)
                    .rfind(|&at| self.text[at + 1..].starts_with(|char| char != '{' && char != '"'));
                let run = Run { start: position, end, last_at };
                self.run = Some(run);
                run
            }
        };
        run.last_at.filter(|&at| at >= position).map(|at| at + 1 - position)
    }
}

/// The first rule, in the grammar's order, that is active in `state` and matches at the start of `rest`.
fn rule(state: State, rest: &str, ahead: &mut Ahead<'_>) -> Option<Matched> {
    use State as S;
    let initial = state == S::Initial;
    let emit = |length, kind, step| Some(Matched { length, kind: Some(kind), step });
    let skip = |length, step| Some(Matched { length, kind: None, step });
    let first = rest.chars().next()?;
    let is = |wanted: State| state == wanted;

    if initial && let Some(length) = keyword_then(rest, "accTitle", ':') {
        return emit(length, Kind::AccTitle, Step::Push(S::AccTitle));
    }
    if is(S::AccTitle) {
        return emit(until(rest, |char| char == '\n'), Kind::AccTitleValue, Step::Pop);
    }
    if initial && let Some(length) = keyword_then(rest, "accDescr", ':') {
        return emit(length, Kind::AccDescr, Step::Push(S::AccDescr));
    }
    if is(S::AccDescr) {
        return emit(until(rest, |char| char == '\n'), Kind::AccDescrValue, Step::Pop);
    }
    if initial && let Some(length) = keyword_then(rest, "accDescr", '{') {
        return skip(length, Step::Push(S::AccDescrMultiline));
    }
    if is(S::AccDescrMultiline) {
        return if first == '}' {
            skip(1, Step::Pop)
        } else {
            emit(until(rest, |char| char == '}'), Kind::AccDescrMultilineValue, Step::Stay)
        };
    }
    if initial && rest.starts_with("@{") {
        return emit(2, Kind::ShapeData, Step::Push(S::ShapeData));
    }
    if is(S::ShapeData) && first == '"' {
        return emit(1, Kind::ShapeData, Step::Push(S::ShapeDataStr));
    }
    if is(S::ShapeDataStr) {
        return if first == '"' {
            emit(1, Kind::ShapeData, Step::Pop)
        } else {
            emit(until(rest, |char| char == '"'), Kind::ShapeDataString, Step::Stay)
        };
    }
    if is(S::ShapeData) {
        let length = until(rest, |char| matches!(char, '}' | '^' | '"'));
        return if length > 0 {
            emit(length, Kind::ShapeData, Step::Stay)
        } else if first == '}' {
            skip(1, Step::Pop)
        } else {
            None
        };
    }
    if initial && let Some(after) = rest.strip_prefix("call") {
        let space = until(after, |char| !is_space(char));
        if space > 0 {
            return skip(4 + space, Step::Push(S::CallbackName));
        }
    }
    if is(S::CallbackName) {
        if let Some(after) = rest.strip_prefix('(') {
            let space = until(after, |char| !is_space(char));
            return if after[space..].starts_with(')') { skip(space + 2, Step::Pop) } else { skip(1, Step::Swap(S::CallbackArgs)) };
        }
        return emit(until(rest, |char| char == '('), Kind::CallbackName, Step::Stay);
    }
    if is(S::CallbackArgs) {
        return if first == ')' { skip(1, Step::Pop) } else { emit(until(rest, |char| char == ')'), Kind::CallbackArgs, Step::Stay) };
    }
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
        if let Some(space) = rest.strip_prefix("href").and_then(|after| after.chars().next()).filter(|&char| is_space(char)) {
            return emit(4 + space.len_utf8(), Kind::Href, Step::Stay);
        }
        if let Some(after) = rest.strip_prefix("click") {
            let space = until(after, |char| !is_space(char));
            if space > 0 {
                return skip(5 + space, Step::Push(S::Click));
            }
        }
    }
    if is(S::Click) {
        return if is_space(first) { skip(first.len_utf8(), Step::Pop) } else { emit(until(rest, is_space), Kind::Click, Step::Stay) };
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
        if let Some(word) = ["_self", "_blank", "_parent", "_top"].iter().find(|word| is_keyword(rest, word)) {
            return emit(word.len(), Kind::LinkTarget, Step::Stay);
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
        if let Some((direction, length)) = ahead.direction(rest) {
            return emit(length, Kind::Direction(direction), Step::Stay);
        }
        if let Some(length) = ahead.link_id(rest) {
            return emit(length, Kind::LinkId, Step::Stay);
        }
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
    for (line, text_state) in [('-', S::EdgeText), ('=', S::ThickEdgeText)] {
        if (initial || is(text_state))
            && let Some(length) = link_end(rest, line)
        {
            return emit(length, Kind::Link, Step::Pop);
        }
        if initial && let Some(length) = link_start(rest, line) {
            return emit(length, Kind::StartLink, Step::Push(text_state));
        }
        if is(text_state) && (first != line || !rest[1..].starts_with(line)) {
            return emit(first.len_utf8(), Kind::EdgeText, Step::Stay);
        }
    }
    if (initial || is(S::DottedEdgeText))
        && let Some(length) = dotted_link_end(rest)
    {
        return emit(length, Kind::Link, Step::Pop);
    }
    if initial && let Some(length) = dotted_link_start(rest) {
        return emit(length, Kind::StartLink, Step::Push(S::DottedEdgeText));
    }
    if is(S::DottedEdgeText) && (first != '.' || !rest[1..].starts_with('-')) {
        return emit(first.len_utf8(), Kind::EdgeText, Step::Stay);
    }
    if let Some(length) = invisible_link(rest) {
        return emit(length, Kind::Link, Step::Stay);
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

/// Every `direction` followed by spaces and a direction's two letters, in order.
fn directions(text: &str) -> Vec<Occurrence> {
    text.match_indices("direction")
        .filter_map(|(start, word)| {
            let after = start + word.len();
            let space = until(&text[after..], |char| !is_space(char));
            let letters = &text[after + space..];
            let &(name, direction) = DIRECTIONS.iter().find(|(name, _)| letters.starts_with(name))?;
            (space > 0).then_some(Occurrence { start, direction, end: after + space + name.len() })
        })
        .collect()
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

/// `accTitle\s*:\s*` and the like.
fn keyword_then(text: &str, word: &str, mark: char) -> Option<usize> {
    let after = text.strip_prefix(word)?;
    let before_mark = until(after, |char| !is_space(char));
    let after_mark = after[before_mark..].strip_prefix(mark)?;
    Some(text.len() - after_mark.len() + until(after_mark, |char| !is_space(char)))
}

/// `\s*[xo<]?` then `body`'s length, then `\s*`.
fn spaced(text: &str, body: impl Fn(&str) -> Option<usize>) -> Option<usize> {
    let space = until(text, |char| !is_space(char));
    let after_space = &text[space..];
    let head = usize::from(after_space.starts_with(['x', 'o', '<']));
    let length = body(&after_space[head..])?;
    let end = space + head + length;
    Some(end + until(&text[end..], |char| !is_space(char)))
}

/// `\s*[xo<]?--+[-xo>]\s*`, and the same with `=`: the line gives back its last character to the head if it must.
fn link_end(text: &str, line: char) -> Option<usize> {
    spaced(text, |body| {
        let run = until(body, |char| char != line);
        let head = body[run..].starts_with(['x', 'o', '>']);
        if run >= 2 && head {
            Some(run + 1)
        } else if run >= 3 {
            Some(run)
        } else {
            None
        }
    })
}

/// `\s*[xo<]?--\s*`, and the same with `=`.
fn link_start(text: &str, line: char) -> Option<usize> {
    spaced(text, |body| body.chars().take(2).all(|char| char == line).then_some(2).filter(|_| body.len() >= 2))
}

/// `\s*[xo<]?-?\.+-[xo>]?\s*`.
fn dotted_link_end(text: &str) -> Option<usize> {
    spaced(text, |body| {
        let dash = usize::from(body.starts_with('-'));
        let dots = until(&body[dash..], |char| char != '.');
        let after = body[dash + dots..].strip_prefix('-').filter(|_| dots > 0)?;
        Some(dash + dots + 1 + usize::from(after.starts_with(['x', 'o', '>'])))
    })
}

/// `\s*[xo<]?-\.\s*`.
fn dotted_link_start(text: &str) -> Option<usize> {
    spaced(text, |body| body.starts_with("-.").then_some(2))
}

/// `\s*~~~+\s*`.
fn invisible_link(text: &str) -> Option<usize> {
    let space = until(text, |char| !is_space(char));
    let tildes = until(&text[space..], |char| char != '~');
    (tildes >= 3).then(|| space + tildes + until(&text[space + tildes..], |char| !is_space(char)))
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
    fn links_take_the_spaces_around_them() {
        assert_eq!(
            texts("A --> B---C x--o D<==>E -.-> F ~~~ G"),
            [
                (K::NodeString, "A"),
                (K::Link, " --> "),
                (K::NodeString, "B"),
                (K::Link, "---"),
                (K::NodeString, "C"),
                (K::Link, " x--o "),
                (K::NodeString, "D"),
                (K::Link, "<==>"),
                (K::NodeString, "E"),
                (K::Link, " -.-> "),
                (K::NodeString, "F"),
                (K::Link, " ~~~ "),
                (K::NodeString, "G"),
                (K::Eof, "")
            ]
        );
    }

    #[test]
    fn a_link_with_text_is_read_one_character_at_a_time() {
        assert_eq!(
            texts("A-- a-b -->B -. c .->D"),
            [
                (K::NodeString, "A"),
                (K::StartLink, "-- "),
                (K::EdgeText, "a"),
                (K::EdgeText, "-"),
                (K::EdgeText, "b"),
                (K::Link, " -->"),
                (K::NodeString, "B"),
                (K::StartLink, " -. "),
                (K::EdgeText, "c"),
                (K::Link, " .->"),
                (K::NodeString, "D"),
                (K::Eof, "")
            ]
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
            texts("A[\"a b\"]-->|\"`**c**`\"|B"),
            [
                (K::NodeString, "A"),
                (K::Sqs, "["),
                (K::Str, "a b"),
                (K::Sqe, "]"),
                (K::Link, "-->"),
                (K::Pipe, "|"),
                (K::MdStr, "**c**"),
                (K::Pipe, "|"),
                (K::NodeString, "B"),
                (K::Eof, "")
            ]
        );
    }

    #[test]
    fn a_direction_anywhere_ahead_on_the_line_takes_the_line() {
        assert_eq!(kinds("A[direction LR] direction TB\nB"), [K::Direction(Direction::TB), K::Newline, K::NodeString, K::Eof]);
        assert_eq!(kinds("  direction\n  RL\n"), [K::Direction(Direction::RL), K::Newline, K::Eof]);
    }

    #[test]
    fn a_link_id_ends_on_the_last_at_sign_of_its_run() {
        assert_eq!(
            texts("A e1@--> B@C"),
            [
                (K::NodeString, "A"),
                (K::Space, " "),
                (K::LinkId, "e1@"),
                (K::Link, "--> "),
                (K::LinkId, "B@"),
                (K::NodeString, "C"),
                (K::Eof, "")
            ]
        );
        assert_eq!(kinds("A@{ shape: x }"), [K::NodeString, K::ShapeData, K::ShapeData, K::Eof]);
    }

    #[test]
    fn a_shape_data_block_keeps_its_quotes_and_marks_their_inside() {
        assert_eq!(
            texts("A@{ label: \"a\nb\" }"),
            [
                (K::NodeString, "A"),
                (K::ShapeData, "@{"),
                (K::ShapeData, " label: "),
                (K::ShapeData, "\""),
                (K::ShapeDataString, "a\nb"),
                (K::ShapeData, "\""),
                (K::ShapeData, " "),
                (K::Eof, "")
            ]
        );
    }

    #[test]
    fn statements_start_with_their_keywords() {
        assert_eq!(
            kinds("classDef c fill:#f00\nclick A call f(x)\n"),
            [
                K::ClassDef,
                K::Space,
                K::NodeString,
                K::Space,
                K::NodeString,
                K::Colon,
                K::Brkt,
                K::NodeString,
                K::Newline,
                K::Click,
                K::CallbackName,
                K::CallbackArgs,
                K::Newline,
                K::Eof
            ]
        );
        assert_eq!(
            texts("accTitle : My title\n"),
            [(K::AccTitle, "accTitle : "), (K::AccTitleValue, "My title"), (K::Newline, "\n"), (K::Eof, "")]
        );
    }

    #[test]
    fn a_character_no_rule_reads_is_refused_with_its_line() {
        assert_eq!(tokens("graph TD\nA°"), Err(Invalid::Unrecognized { line: 2 }));
    }

    #[test]
    fn node_strings_keep_dashes_and_equals_that_start_no_link() {
        assert_eq!(texts("a-b=c-->d"), [(K::NodeString, "a-b=c"), (K::Link, "-->"), (K::NodeString, "d"), (K::Eof, "")]);
    }
}
