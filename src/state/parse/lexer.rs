//! Mermaid's state lexer (`stateDiagram.jison`): its rules in its order, the first that matches wins, each start
//! condition a [`Condition`] on a stack.
//!
//! The rules are case-insensitive, and jison appends `\b` to a rule ending in a word character, so `click` is a
//! keyword and `clicked` a state.
//! Every pattern looking past the token at hand (a `direction` or a `<<fork>>` later on the line, a `{` after two
//! words) reads an index of the text built once, so a long line is never scanned again for each token on it.

use super::Invalid;
use crate::source::{is_space, is_word};

/// A token kind, named after the grammar's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Newline,
    Click,
    Href,
    String,
    Default,
    Direction(Direction),
    Fork,
    Join,
    Choice,
    StateDescr,
    As,
    Id,
    CompositState,
    StructStart,
    StructStop,
    Header,
    HideEmpty,
    EdgeState,
    Descr,
    Arrow,
    Concurrent,
    StyleSeparator,
    Invalid,
    Eof,
}

/// The direction a `direction` statement names, in the order the rules try them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::state) enum Direction {
    TB,
    BT,
    RL,
    LR,
}

const DIRECTIONS: [(&str, Direction); 4] = [("tb", Direction::TB), ("bt", Direction::BT), ("rl", Direction::RL), ("lr", Direction::LR)];

/// The marks that make a `state` a fork, a join or a choice, in the rules' order.
const MARKS: [(&str, Kind); 6] = [
    ("<<fork>>", Kind::Fork),
    ("<<join>>", Kind::Join),
    ("<<choice>>", Kind::Choice),
    ("[[fork]]", Kind::Fork),
    ("[[join]]", Kind::Join),
    ("[[choice]]", Kind::Choice),
];

/// A token and where its text is: the text the grammar reads, which a rule may trim from its match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) kind: Kind,
    pub(super) start: usize,
    pub(super) end: usize,
}

/// A start condition of the lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Condition {
    Initial,
    Struct,
    State,
    StateString,
    StateId,
}

/// What a rule does to the stack of states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Stay,
    Push(Condition),
    Pop,
    /// Pops, then pushes.
    Swap(Condition),
}

/// A rule that matched: how far the match goes, the token it gives if any, where the token's text starts and ends
/// after the match's start, and its step.
#[derive(Debug, Clone, Copy)]
struct Matched {
    length: usize,
    kind: Option<Kind>,
    text: (usize, usize),
    step: Step,
}

impl Matched {
    fn token(length: usize, kind: Kind, step: Step) -> Self {
        Self { length, kind: Some(kind), text: (0, length), step }
    }

    fn skip(length: usize, step: Step) -> Self {
        Self { length, kind: None, text: (0, 0), step }
    }

    /// Mermaid's `processId`: a name holding `%%` ends before it, the comment read next; one starting with it gives
    /// no token and leaves the state as it is.
    fn name(length: usize, match_text: &str, kind: Kind, step: Step) -> Self {
        match match_text[..length].find("%%") {
            Some(0) => Self::skip(length, Step::Stay),
            Some(comment) => Self::token(comment, kind, step),
            None => Self::token(length, kind, step),
        }
    }
}

/// Every token of `text`, ending with one `Eof`; text no rule matches is an error.
///
/// As jison does, the rules are tried once more at the end of the text, where only those matching nothing can match.
pub(super) fn tokens(text: &str) -> Result<Vec<Token>, Invalid> {
    let index = Index::of(text);
    let mut states = vec![Condition::Initial];
    let mut position = 0;
    let mut tokens = Vec::new();
    let mut done = false;
    while !done {
        done = position == text.len();
        let state = *states.last().unwrap_or(&Condition::Initial);
        let Some(matched) = rule(state, &index, position)? else {
            if done {
                break;
            }
            return Err(Invalid::Unrecognized { line: line_of(text, position) });
        };
        tokens.extend(matched.kind.map(|kind| Token { kind, start: position + matched.text.0, end: position + matched.text.1 }));
        position += matched.length;
        step(&mut states, matched.step);
    }
    tokens.push(Token { kind: Kind::Eof, start: text.len(), end: text.len() });
    Ok(tokens)
}

pub(super) fn line_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

/// Jison never pops the initial state.
fn step(states: &mut Vec<Condition>, step: Step) {
    let pop = |states: &mut Vec<Condition>| {
        if states.len() > 1 {
            states.pop();
        }
    };
    match step {
        Step::Stay => {}
        Step::Push(state) => states.push(state),
        Step::Pop => pop(states),
        Step::Swap(state) => {
            pop(states);
            states.push(state);
        }
    }
}

/// Where the text holds what some rules look for ahead of the token at hand, each list in order.
#[derive(Debug)]
struct Index<'a> {
    text: &'a str,
    /// Where each line ends for a regex `.`: at `\n`, `\r`, ` ` or ` `.
    line_ends: Vec<usize>,
    braces: Vec<usize>,
    /// Where each mark of [`MARKS`] starts.
    marks: [Vec<usize>; 6],
    /// Each `direction`, spaces and two letters naming a direction: where it starts, and where the line of its
    /// letters ends.
    directions: Vec<(usize, Direction, usize)>,
}

impl<'a> Index<'a> {
    fn of(text: &'a str) -> Self {
        let lower = text.to_ascii_lowercase();
        let line_ends = text.match_indices(['\n', '\r', '\u{2028}', '\u{2029}']).map(|(at, _)| at).collect();
        let braces = text.match_indices('{').map(|(at, _)| at).collect();
        let marks = MARKS.map(|(mark, _)| lower.match_indices(mark).map(|(at, _)| at).collect());
        let directions = lower
            .match_indices("direction")
            .filter_map(|(start, word)| {
                let after = start + word.len();
                let space = until(&lower[after..], |char| !is_space(char));
                let letters = after + space;
                let &(_, direction) = DIRECTIONS.iter().find(|(name, _)| lower[letters..].starts_with(name))?;
                let end = letters + 2 + until(&text[letters + 2..], |char| char == '\n');
                (space > 0).then_some((start, direction, end))
            })
            .collect();
        Self { text, line_ends, braces, marks, directions }
    }

    /// Where the line holding `position` ends for a regex `.`.
    fn line_end(&self, position: usize) -> usize {
        let next = self.line_ends.partition_point(|&end| end < position);
        self.line_ends.get(next).copied().unwrap_or(self.text.len())
    }

    /// `.*direction\s+XX[^\n]*`: the directions tried in the rules' order, the greedy `.*` reaching the last one on
    /// the line; the match runs to the line end after its letters.
    fn direction(&self, position: usize) -> Option<(Direction, usize)> {
        let line_end = self.line_end(position);
        let first = self.directions.partition_point(|&(start, ..)| start < position);
        let ahead = self.directions[first..].iter().take_while(|&&(start, ..)| start < line_end);
        let (_, direction) = DIRECTIONS.iter().find(|(_, direction)| ahead.clone().any(|&(_, found, _)| found == *direction))?;
        let &(_, _, end) = ahead.filter(|&&(_, found, _)| found == *direction).last()?;
        Some((*direction, end - position))
    }

    /// `.*"<<fork>>"` and the other marks, in the rules' order: the greedy `.*` reaches the last on the line.
    fn mark(&self, position: usize) -> Option<(Kind, usize, usize)> {
        let line_end = self.line_end(position);
        MARKS.iter().zip(&self.marks).find_map(|(&(mark, kind), starts)| {
            let after_line = starts.partition_point(|&start| start < line_end);
            let last = starts[..after_line].last().filter(|&&start| start >= position)?;
            Some((kind, *last - position, *last + mark.len() - position))
        })
    }

    /// `\w+\s+\w+.*?\{`: two words, the second on a line with a `{` after it.
    fn two_words_then_brace(&self, position: usize) -> bool {
        let rest = &self.text[position..];
        let word = until(rest, |char| !is_word(char));
        let space = until(&rest[word..], |char| !is_space(char));
        let second = position + word + space;
        if word == 0 || space == 0 || !self.text[second..].starts_with(is_word) {
            return false;
        }
        let line_end = self.line_end(second);
        let brace = self.braces.partition_point(|&at| at <= second);
        self.braces.get(brace).is_some_and(|&at| at < line_end)
    }
}

/// The first rule active in `state` that matches at `position`; a state name of more than one word before a `{` is
/// refused, as mermaid's lexer throws.
fn rule(state: Condition, index: &Index<'_>, position: usize) -> Result<Option<Matched>, Invalid> {
    let rest = &index.text[position..];
    Ok(match state {
        Condition::Initial => initial(index, position),
        Condition::Struct => structure(index, position),
        Condition::State => return in_state(index, position),
        Condition::StateString if rest.starts_with('"') => Some(Matched::skip(1, Step::Pop)),
        Condition::StateString => Some(Matched::token(until(rest, |char| char == '"'), Kind::StateDescr, Step::Stay)),
        Condition::StateId => Some(Matched::name(until(rest, |char| matches!(char, '\n' | '{')), rest, Kind::Id, Step::Pop)),
    })
}

fn initial(index: &Index<'_>, position: usize) -> Option<Matched> {
    let rest = &index.text[position..];
    let Some(first) = rest.chars().next() else {
        return Some(Matched::token(0, Kind::Newline, Step::Stay));
    };
    if let Some(length) = keyword(rest, "click") {
        return Some(Matched::token(length, Kind::Click, Step::Stay));
    }
    if let Some(length) = keyword(rest, "href") {
        return Some(Matched::token(length, Kind::Href, Step::Stay));
    }
    if let Some(close) = rest.strip_prefix('"').and_then(|after| after.find('"')) {
        return Some(Matched::token(close + 2, Kind::String, Step::Stay));
    }
    if let Some(length) = keyword(rest, "default") {
        return Some(Matched::token(length, Kind::Default, Step::Stay));
    }
    if let Some((direction, length)) = index.direction(position) {
        return Some(Matched::token(length, Kind::Direction(direction), Step::Stay));
    }
    if first == '\n' {
        return Some(Matched::token(until(rest, |char| char != '\n'), Kind::Newline, Step::Stay));
    }
    if is_space(first) {
        return Some(Matched::skip(until(rest, |char| !is_space(char)), Step::Stay));
    }
    if let Some(length) = comment(rest) {
        return Some(Matched::skip(length, Step::Stay));
    }
    if let Some(matched) = statement_keyword(rest) {
        return Some(matched);
    }
    if first == '{' {
        return Some(Matched::token(1, Kind::StructStart, Step::Swap(Condition::Struct)));
    }
    if let Some(length) = spaced_keyword(rest, "stateDiagram").or_else(|| spaced_keyword(rest, "stateDiagram-v2")) {
        return Some(Matched::token(length, Kind::Header, Step::Stay));
    }
    if let Some(length) = keyword(rest, "hide empty description") {
        return Some(Matched::token(length, Kind::HideEmpty, Step::Stay));
    }
    shared_tail(rest).or_else(|| Some(Matched::token(first.len_utf8(), Kind::Invalid, Step::Stay)))
}

/// The rules of a composite state's body: no line end is a token there.
fn structure(index: &Index<'_>, position: usize) -> Option<Matched> {
    let rest = &index.text[position..];
    let space = until(rest, |char| char == '\n' || !is_space(char));
    if space > 0 {
        return Some(Matched::skip(space, Step::Stay));
    }
    if let Some(length) = comment(rest) {
        return Some(Matched::skip(length, Step::Stay));
    }
    if let Some(matched) = statement_keyword(rest) {
        return Some(matched);
    }
    if let Some((direction, length)) = index.direction(position) {
        return Some(Matched::token(length, Kind::Direction(direction), Step::Stay));
    }
    match rest.as_bytes().first() {
        Some(b'}') => return Some(Matched::token(1, Kind::StructStop, Step::Pop)),
        Some(b'\n') => return Some(Matched::skip(1, Step::Stay)),
        _ => {}
    }
    shared_tail(rest).or_else(|| rest.starts_with("--").then(|| Matched::token(2, Kind::Concurrent, Step::Stay)))
}

/// `state` before a space, in both the diagram and a composite state's body.
fn statement_keyword(rest: &str) -> Option<Matched> {
    spaced_keyword(rest, "state").map(|length| Matched::skip(length, Step::Push(Condition::State)))
}

/// `[*]`, a name, a description, `-->`, then `:::`; between them, in a body, comes `--`, which [`structure`] tries
/// after the others since none of them starts with it.
fn shared_tail(rest: &str) -> Option<Matched> {
    if rest.starts_with("[*]") {
        return Some(Matched::token(3, Kind::EdgeState, Step::Stay));
    }
    let name = until(rest, |char| matches!(char, ':' | '\n' | '-' | '{') || is_space(char));
    if name > 0 {
        return Some(Matched::name(name, rest, Kind::Id, Step::Stay));
    }
    if let Some(length) = description(rest) {
        let trimmed = rest[..length].trim_matches(is_space);
        let start = length - rest[..length].trim_start_matches(is_space).len();
        return Some(Matched { length, kind: Some(Kind::Descr), text: (start, start + trimmed.len()), step: Step::Stay });
    }
    if rest.starts_with("-->") {
        return Some(Matched::token(3, Kind::Arrow, Step::Stay));
    }
    rest.starts_with(":::").then(|| Matched::token(3, Kind::StyleSeparator, Step::Stay))
}

/// `\s*":"(?:[^:\n;]|":"[^:\n;])+`.
fn description(rest: &str) -> Option<usize> {
    let colon = until(rest, |char| !is_space(char));
    if !rest[colon..].starts_with(':') {
        return None;
    }
    let plain = |char: char| !matches!(char, ':' | '\n' | ';');
    let mut end = colon + 1;
    loop {
        let mut chars = rest[end..].chars();
        match (chars.next(), chars.next()) {
            (Some(char), _) if plain(char) => end += char.len_utf8(),
            (Some(':'), Some(char)) if plain(char) => end += 1 + char.len_utf8(),
            _ => break,
        }
    }
    (end > colon + 1).then_some(end)
}

/// After `state`: a fork, a join or a choice, a quoted description, `as`, or the composite state's name.
fn in_state(index: &Index<'_>, position: usize) -> Result<Option<Matched>, Invalid> {
    let rest = &index.text[position..];
    let space = until(rest, |char| char == '\n' || !is_space(char));
    if space > 0 {
        return Ok(Some(Matched::skip(space, Step::Stay)));
    }
    if let Some(length) = comment(rest) {
        return Ok(Some(Matched::skip(length, Step::Stay)));
    }
    if let Some((kind, name_end, length)) = index.mark(position) {
        return Ok(Some(Matched { length, kind: Some(kind), text: (0, name_end), step: Step::Pop }));
    }
    if rest.starts_with('"') {
        return Ok(Some(Matched::skip(1, Step::Push(Condition::StateString))));
    }
    if let Some(length) = alias(rest) {
        return Ok(Some(Matched::token(length, Kind::As, Step::Push(Condition::StateId))));
    }
    if index.two_words_then_brace(position) {
        let name = &rest[..until(rest, |char| char == '{')];
        return Err(Invalid::StateName { name: name.trim_matches(is_space).to_owned() });
    }
    let name = until(rest, |char| char == '{' || is_space(char));
    Ok(match rest.as_bytes().first() {
        _ if name > 0 => Some(Matched::token(name, Kind::CompositState, Step::Stay)),
        Some(b'\n') => Some(Matched::skip(1, Step::Pop)),
        Some(b'{') => Some(Matched::token(1, Kind::StructStart, Step::Swap(Condition::Struct))),
        _ => None,
    })
}

/// `\s*"as"\s+`, which may run over line ends.
fn alias(rest: &str) -> Option<usize> {
    let space = until(rest, |char| !is_space(char));
    let after = rest.get(space..space + 2).filter(|word| word.eq_ignore_ascii_case("as"))?;
    let gap = until(&rest[space + after.len()..], |char| !is_space(char));
    (gap > 0).then_some(space + 2 + gap)
}

/// `\#[^\n]*` and `\%\%(?!\{)[^\n]*`.
fn comment(rest: &str) -> Option<usize> {
    let opens = rest.starts_with('#') || (rest.starts_with("%%") && !rest[2..].starts_with('{'));
    opens.then(|| until(rest, |char| char == '\n'))
}

/// `word` and a word boundary, as jison adds one after a rule ending in a word character.
fn keyword(text: &str, word: &str) -> Option<usize> {
    let boundary = text.as_bytes().get(word.len()).is_none_or(|&byte| !byte.is_ascii_alphanumeric() && byte != b'_');
    (starts_with_ignoring_case(text, word) && boundary).then_some(word.len())
}

/// `"word"\s+`.
fn spaced_keyword(text: &str, word: &str) -> Option<usize> {
    let after = text.get(word.len()..).filter(|_| starts_with_ignoring_case(text, word))?;
    let space = until(after, |char| !is_space(char));
    (space > 0).then_some(word.len() + space)
}

fn starts_with_ignoring_case(text: &str, prefix: &str) -> bool {
    text.as_bytes().get(..prefix.len()).is_some_and(|start| start.eq_ignore_ascii_case(prefix.as_bytes()))
}

/// Bytes before the first character matching `stop`, or the whole text.
fn until(text: &str, stop: impl Fn(char) -> bool) -> usize {
    text.find(stop).unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    use Kind as K;

    fn texts(text: &str) -> Vec<(Kind, &str)> {
        tokens(text).unwrap().into_iter().map(|token| (token.kind, &text[token.start..token.end])).collect()
    }

    #[test]
    fn a_transition_is_two_names_an_arrow_and_a_description() {
        assert_eq!(
            texts("stateDiagram-v2\nA --> B : go now\n"),
            [
                (K::Header, "stateDiagram-v2\n"),
                (K::Id, "A"),
                (K::Arrow, "-->"),
                (K::Id, "B"),
                (K::Descr, ": go now"),
                (K::Newline, "\n"),
                (K::Newline, ""),
                (K::Eof, "")
            ]
        );
    }

    #[test]
    fn a_composite_body_has_no_line_ends_and_reads_dividers() {
        let kinds: Vec<Kind> = texts("state A {\n  B\n  --\n  C\n}\n").into_iter().map(|(kind, _)| kind).collect();

        assert_eq!(kinds, [K::CompositState, K::StructStart, K::Id, K::Concurrent, K::Id, K::StructStop, K::Newline, K::Newline, K::Eof]);
    }

    #[test]
    fn a_mark_makes_a_fork_a_join_or_a_choice_named_by_the_text_before_it() {
        assert_eq!(texts("state f <<fork>>\n")[0], (K::Fork, "f "));
        assert_eq!(texts("state c [[CHOICE]]\n")[0], (K::Choice, "c "));
    }

    #[test]
    fn a_direction_anywhere_on_a_line_makes_the_whole_line_a_direction() {
        assert_eq!(texts("A --> B : direction LR\nC\n")[0], (K::Direction(Direction::LR), "A --> B : direction LR"));
        assert_eq!(texts("direction\n  tb x\n")[0], (K::Direction(Direction::TB), "direction\n  tb x"));
    }

    #[test]
    fn a_quoted_description_is_given_an_id_after_as() {
        assert_eq!(texts("state \"Long name\" as L\n")[..3], [(K::StateDescr, "Long name"), (K::As, "as "), (K::Id, "L")]);
    }

    #[test]
    fn a_name_ends_before_a_comment() {
        assert_eq!(texts("A%% gone\n")[..2], [(K::Id, "A"), (K::Newline, "\n")]);
    }

    #[test]
    fn a_composite_name_of_two_words_is_refused() {
        assert_eq!(tokens("state two words {\n"), Err(Invalid::StateName { name: "two words".to_owned() }));
    }

    #[test]
    fn text_no_rule_reads_is_refused_with_its_line() {
        assert_eq!(tokens("state A {\n{\n"), Err(Invalid::Unrecognized { line: 2 }));
    }
}
