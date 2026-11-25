//! Mermaid's ER lexer (`erDiagram.jison`): its rules in its order, the first that matches wins, each start condition a
//! [`State`] on a stack.
//!
//! The rules are case-insensitive, and jison appends `\b` to a rule ending in a word character, so `one` is a
//! cardinality and `ones` an entity.

use super::Invalid;
use crate::source::is_space;

/// A token kind, named after the grammar's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Direction(Direction),
    Newline,
    EntityName,
    Word,
    ErDiagram,
    BlockStart,
    BlockStop,
    Brkt,
    Comma,
    StyleSeparator,
    Colon,
    AttributeKey,
    AttributeWord,
    Comment,
    /// A character no other rule reads, its own token as jison's `yytext[0]`.
    Char(char),
    Sqs,
    Sqe,
    Style,
    ClassDef,
    Class,
    Subgraph,
    SubgraphEnd,
    Cardinality(Cardinality),
    Identifying,
    NonIdentifying,
    DecimalNum,
    EntityOne,
    Num,
    StyleText,
    Semi,
    UnicodeText,
    /// The grammar's `EOF`, given once the lexer reaches the end of the text outside any block.
    Eof,
    /// The end of the tokens, after which the parser reads nothing.
    End,
}

/// How many of one entity relate to one of the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::er) enum Cardinality {
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
    OnlyOne,
    /// `u`: the parent of a generalisation.
    MdParent,
}

/// The way the diagram's ranks go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::er) enum Direction {
    TB,
    BT,
    RL,
    LR,
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
    Block,
    BlockBq,
    Style,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Stay,
    Push(State),
    Pop,
}

#[derive(Debug, Clone, Copy)]
struct Matched {
    length: usize,
    kind: Option<Kind>,
    step: Step,
}

impl Matched {
    const fn token(length: usize, kind: Kind, step: Step) -> Self {
        Self { length, kind: Some(kind), step }
    }

    const fn skip(length: usize, step: Step) -> Self {
        Self { length, kind: None, step }
    }
}

const DIRECTIONS: [(&str, Direction); 4] = [("TB", Direction::TB), ("BT", Direction::BT), ("RL", Direction::RL), ("LR", Direction::LR)];

/// The cardinality words and marks, in the rules' order up to `only one`; a word ends at a word boundary, a mark ending
/// in a letter too.
const CARDINALITIES: [(&str, Cardinality); 16] = [
    ("one or zero", Cardinality::ZeroOrOne),
    ("one or more", Cardinality::OneOrMore),
    ("one or many", Cardinality::OneOrMore),
    ("1+", Cardinality::OneOrMore),
    ("|o", Cardinality::ZeroOrOne),
    ("zero or one", Cardinality::ZeroOrOne),
    ("zero or more", Cardinality::ZeroOrMore),
    ("zero or many", Cardinality::ZeroOrMore),
    ("0+", Cardinality::ZeroOrMore),
    ("}o", Cardinality::ZeroOrMore),
    ("many(0)", Cardinality::ZeroOrMore),
    ("many(1)", Cardinality::OneOrMore),
    ("many", Cardinality::ZeroOrMore),
    ("}|", Cardinality::OneOrMore),
    ("one", Cardinality::OnlyOne),
    ("only one", Cardinality::OnlyOne),
];

/// The marks after the numbers, up to `|{`.
const MARKS: [(&str, Cardinality); 4] =
    [("||", Cardinality::OnlyOne), ("o|", Cardinality::ZeroOrOne), ("o{", Cardinality::ZeroOrMore), ("|{", Cardinality::OneOrMore)];

/// Every token of `text`, ending with one `End`; text no rule matches is an error.
///
/// As jison does, the rules are tried once more at the end of the text, where only those matching nothing can match.
pub(super) fn tokens(text: &str) -> Result<Vec<Token>, Invalid> {
    let mut states = vec![State::Initial];
    let mut position = 0;
    let mut tokens = Vec::new();
    let mut done = false;
    while !done {
        done = position == text.len();
        let state = *states.last().unwrap_or(&State::Initial);
        let Some(matched) = rule(state, &text[position..]) else {
            if done {
                break;
            }
            return Err(Invalid::Unrecognized { line: line_of(text, position) });
        };
        tokens.extend(matched.kind.map(|kind| Token { kind, start: position, end: position + matched.length }));
        position += matched.length;
        match matched.step {
            Step::Stay => {}
            Step::Push(state) => states.push(state),
            Step::Pop if states.len() > 1 => {
                states.pop();
            }
            Step::Pop => {}
        }
    }
    tokens.push(Token { kind: Kind::End, start: text.len(), end: text.len() });
    Ok(tokens)
}

pub(super) fn line_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

fn rule(state: State, rest: &str) -> Option<Matched> {
    match state {
        State::Initial => initial(rest),
        State::Block => block(rest),
        State::BlockBq if rest.starts_with('`') => Some(Matched::skip(1, Step::Pop)),
        State::BlockBq => {
            let length = until(rest, |char| char == '`');
            (length > 0).then_some(Matched::token(length, Kind::AttributeWord, Step::Stay))
        }
        State::Style => style(rest),
        State::AccTitle => Some(Matched::token(until(rest, |char| char == '\n'), Kind::AccTitleValue, Step::Pop)),
        State::AccDescr => Some(Matched::token(until(rest, |char| char == '\n'), Kind::AccDescrValue, Step::Pop)),
        State::AccDescrMultiline if rest.starts_with('}') => Some(Matched::skip(1, Step::Pop)),
        State::AccDescrMultiline => Some(Matched::token(until(rest, |char| char == '}'), Kind::AccDescrMultilineValue, Step::Stay)),
    }
}

fn initial(rest: &str) -> Option<Matched> {
    if let Some(length) = keyword_then(rest, "accTitle", ':') {
        return Some(Matched::token(length, Kind::AccTitle, Step::Push(State::AccTitle)));
    }
    if let Some(length) = keyword_then(rest, "accDescr", ':') {
        return Some(Matched::token(length, Kind::AccDescr, Step::Push(State::AccDescr)));
    }
    if let Some(length) = keyword_then(rest, "accDescr", '{') {
        return Some(Matched::skip(length, Step::Push(State::AccDescrMultiline)));
    }
    if let Some(matched) = direction(rest) {
        return Some(matched);
    }
    let blank = until(rest, |char| !matches!(char, ' ' | '\t' | '\r'));
    if blank > 0 {
        return Some(Matched::skip(blank, Step::Stay));
    }
    if rest.starts_with('\n') {
        return Some(Matched::token(until(rest, |char| char != '\n'), Kind::Newline, Step::Stay));
    }
    if let Some(after) = rest.strip_prefix('"') {
        let name = until(after, |char| matches!(char, '"' | '%' | '\r' | '\n' | '\u{b}' | '\u{8}' | '\\'));
        if name > 0 && after[name..].starts_with('"') {
            return Some(Matched::token(name + 2, Kind::EntityName, Step::Stay));
        }
        if let Some(close) = after.find('"') {
            return Some(Matched::token(close + 2, Kind::Word, Step::Stay));
        }
    }
    if let Some(length) = keyword(rest, "erDiagram") {
        return Some(Matched::token(length, Kind::ErDiagram, Step::Stay));
    }
    let symbol = match rest.as_bytes() {
        [b'{', ..] => Some(Matched::token(1, Kind::BlockStart, Step::Push(State::Block))),
        [b'#', ..] => Some(Matched::token(1, Kind::Brkt, Step::Stay)),
        [b',', ..] => Some(Matched::token(1, Kind::Comma, Step::Stay)),
        [b':', b':', b':', ..] => Some(Matched::token(3, Kind::StyleSeparator, Step::Stay)),
        [b':', ..] => Some(Matched::token(1, Kind::Colon, Step::Stay)),
        [b'[', ..] => Some(Matched::token(1, Kind::Sqs, Step::Stay)),
        [b']', ..] => Some(Matched::token(1, Kind::Sqe, Step::Stay)),
        _ => None,
    };
    symbol.or_else(|| keyword_rule(rest)).or_else(|| relation_rule(rest)).or_else(|| text_rule(rest))
}

/// `style`, `classDef`, `class`, `subgraph` and `end`.
fn keyword_rule(rest: &str) -> Option<Matched> {
    if let Some(length) = keyword(rest, "style") {
        return Some(Matched::token(length, Kind::Style, Step::Push(State::Style)));
    }
    if let Some(length) = keyword(rest, "classDef") {
        return Some(Matched::token(length, Kind::ClassDef, Step::Push(State::Style)));
    }
    if let Some(length) = keyword(rest, "class") {
        return Some(Matched::token(length, Kind::Class, Step::Stay));
    }
    if let Some(length) = keyword(rest, "subgraph") {
        return Some(Matched::token(length, Kind::Subgraph, Step::Stay));
    }
    keyword(rest, "end")
        .map(|length| Matched::token(length + until(&rest[length..], |char| !is_space(char)), Kind::SubgraphEnd, Step::Stay))
}

/// From `one or zero` to `-.`: cardinalities, numbers and the lines between entities.
fn relation_rule(rest: &str) -> Option<Matched> {
    if let Some((length, cardinality)) =
        CARDINALITIES.iter().find_map(|&(word, cardinality)| mark(rest, word).map(|length| (length, cardinality)))
    {
        return Some(Matched::token(length, Kind::Cardinality(cardinality), Step::Stay));
    }
    let digits = until(rest, |char| !char.is_ascii_digit());
    let fraction = rest[digits..].strip_prefix('.').map_or(0, |after| until(after, |char| !char.is_ascii_digit()));
    if digits > 0 && fraction > 0 {
        return Some(Matched::token(digits + 1 + fraction, Kind::DecimalNum, Step::Stay));
    }
    if let Some(after) = rest.strip_prefix('1') {
        if one_before_a_name_or_a_line(after) {
            return Some(Matched::token(1, Kind::Cardinality(Cardinality::OnlyOne), Step::Stay));
        }
        if after.chars().next().is_none_or(|char| !is_word(char)) {
            return Some(Matched::token(1, Kind::EntityOne, Step::Stay));
        }
    }
    if digits > 0 {
        return Some(Matched::token(digits, Kind::Num, Step::Stay));
    }
    if let Some((length, cardinality)) =
        MARKS.iter().find_map(|&(mark, cardinality)| starts_with_ignoring_case(rest, mark).then_some((mark.len(), cardinality)))
    {
        return Some(Matched::token(length, Kind::Cardinality(cardinality), Step::Stay));
    }
    if starts_with_ignoring_case(rest, "u") && rest[1..].starts_with(['.', '-', '|']) {
        return Some(Matched::token(1, Kind::Cardinality(Cardinality::MdParent), Step::Stay));
    }
    match rest.as_bytes() {
        [b'.', b'.', ..] => Some(Matched::token(2, Kind::NonIdentifying, Step::Stay)),
        [b'-', b'-', ..] => Some(Matched::token(2, Kind::Identifying, Step::Stay)),
        _ if keyword(rest, "to").is_some() => Some(Matched::token(2, Kind::Identifying, Step::Stay)),
        _ if keyword(rest, "optionally to").is_some() => Some(Matched::token(13, Kind::NonIdentifying, Step::Stay)),
        [b'.', b'-', ..] | [b'-', b'.', ..] => Some(Matched::token(2, Kind::NonIdentifying, Step::Stay)),
        _ => None,
    }
}

/// `1(?=\s+[A-Za-z_"'])`, `1(?=\s+[0-9])` and `1(?=(\-\-|\.\.|\.\-|\-\.))`.
fn one_before_a_name_or_a_line(after: &str) -> bool {
    let blank = after.len() - after.trim_start_matches(is_space).len();
    let next = after[blank..].chars().next();
    let named = blank > 0 && next.is_some_and(|char| char.is_ascii_alphanumeric() || matches!(char, '_' | '"' | '\''));
    named || ["--", "..", ".-", "-."].iter().any(|line| after.starts_with(line))
}

/// `UNICODE_TEXT`, then any one character, then the end of the text.
fn text_rule(rest: &str) -> Option<Matched> {
    let length = until(rest, |char| !(!char.is_ascii() || is_word(char) || matches!(char, '-' | '*' | '.')));
    if length > 0 {
        return Some(Matched::token(length, Kind::UnicodeText, Step::Stay));
    }
    match rest.chars().next() {
        None => Some(Matched::token(0, Kind::Eof, Step::Stay)),
        Some(char) if is_line_end(char) => None,
        Some(char) => Some(Matched::token(char.len_utf8(), Kind::Char(char), Step::Stay)),
    }
}

/// An entity's block: attributes, keys, comments.
fn block(rest: &str) -> Option<Matched> {
    let blank = rest.len() - rest.trim_start_matches(is_space).len();
    if blank > 0 {
        return Some(Matched::skip(blank, Step::Stay));
    }
    if ["PK", "FK", "UK"].iter().any(|key| keyword(rest, key).is_some()) {
        return Some(Matched::token(2, Kind::AttributeKey, Step::Stay));
    }
    if let Some(length) = tilde_word(rest).or_else(|| word(rest)) {
        return Some(Matched::token(length, Kind::AttributeWord, Step::Stay));
    }
    if rest.starts_with('`') {
        return Some(Matched::skip(1, Step::Push(State::BlockBq)));
    }
    if let Some(close) = rest.strip_prefix('"').and_then(|after| after.find('"')) {
        return Some(Matched::token(close + 2, Kind::Comment, Step::Stay));
    }
    match rest.chars().next() {
        Some('}') => Some(Matched::token(1, Kind::BlockStop, Step::Pop)),
        Some(char) if !is_line_end(char) => Some(Matched::token(char.len_utf8(), Kind::Char(char), Step::Stay)),
        _ => None,
    }
}

/// `([^\s]*)[~].*[~]([^\s]*)`: a run without spaces holding a `~`, then the line up to its last `~`, then the run
/// without spaces after it.
fn tilde_word(rest: &str) -> Option<usize> {
    let run = until(rest, is_space);
    let first = rest[..run].find('~')?;
    let line = until(rest, is_line_end);
    let last = rest[..line].rfind('~').filter(|&last| last > first)?;
    Some(last + 1 + until(&rest[last + 1..], is_space))
}

/// `[\*A-Za-z_À-￿][A-Za-z0-9\-\_\[\]\(\)\.,À-￿\*]*`.
fn word(rest: &str) -> Option<usize> {
    let first = rest.chars().next().filter(|&char| char == '*' || char.is_ascii_alphabetic() || char == '_' || char >= '\u{c0}')?;
    let tail = until(&rest[first.len_utf8()..], |char| {
        !(char.is_ascii_alphanumeric() || matches!(char, '-' | '_' | '[' | ']' | '(' | ')' | '.' | ',' | '*') || char >= '\u{c0}')
    });
    Some(first.len_utf8() + tail)
}

/// A style's or a `classDef`'s rest of line.
fn style(rest: &str) -> Option<Matched> {
    if rest.starts_with('\n') {
        return Some(Matched::token(until(rest, |char| char != '\n'), Kind::Newline, Step::Pop));
    }
    let blank = rest.len() - rest.trim_start_matches(is_space).len();
    if blank > 0 {
        return Some(Matched::skip(blank, Step::Stay));
    }
    let symbol = match rest.as_bytes().first() {
        Some(b':') => Some(Kind::Colon),
        Some(b',') => Some(Kind::Comma),
        Some(b'#') => Some(Kind::Brkt),
        _ => None,
    };
    if let Some(kind) = symbol {
        return Some(Matched::token(1, kind, Step::Stay));
    }
    let length = until(rest, |char| !(!char.is_ascii() || is_word(char) || matches!(char, '-' | '*')));
    if length > 0 {
        return Some(Matched::token(length, Kind::StyleText, Step::Stay));
    }
    rest.starts_with(';').then_some(Matched::token(1, Kind::Semi, Step::Stay))
}

/// `.*direction\s+XX[^\n]*`, `TB` first, ignoring case.
fn direction(rest: &str) -> Option<Matched> {
    let line = &rest[..until(rest, is_line_end)];
    let lower = line.to_ascii_lowercase();
    DIRECTIONS.iter().find_map(|&(name, direction)| {
        let found = lower.rmatch_indices("direction").find_map(|(at, word)| {
            let after = &rest[at + word.len()..];
            let name_at = after.len() - after.trim_start_matches(is_space).len();
            (name_at > 0 && starts_with_ignoring_case(&after[name_at..], name)).then_some(at + word.len() + name_at + name.len())
        })?;
        let length = found + until(&rest[found..], |char| char == '\n');
        Some(Matched::token(length, Kind::Direction(direction), Step::Stay))
    })
}

/// `text` written as `word` in any case, then a word boundary if `word` ends with a letter or a digit.
fn mark(text: &str, word: &str) -> Option<usize> {
    let ends_in_word = word.chars().next_back().is_some_and(is_word);
    let boundary = || text[word.len()..].chars().next().is_none_or(|char| !is_word(char));
    (starts_with_ignoring_case(text, word) && (!ends_in_word || boundary())).then_some(word.len())
}

fn keyword(text: &str, word: &str) -> Option<usize> {
    mark(text, word)
}

/// `word\s*"mark"\s*`, ignoring case.
fn keyword_then(text: &str, word: &str, mark: char) -> Option<usize> {
    let after = text.get(word.len()..).filter(|_| starts_with_ignoring_case(text, word))?;
    let space = until(after, |char| !is_space(char));
    let after_mark = after[space..].strip_prefix(mark)?;
    Some(text.len() - after_mark.len() + until(after_mark, |char| !is_space(char)))
}

fn is_word(char: char) -> bool {
    char.is_ascii_alphanumeric() || char == '_'
}

/// A character JavaScript's `.` does not match.
fn is_line_end(char: char) -> bool {
    matches!(char, '\n' | '\r' | '\u{2028}' | '\u{2029}')
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
    fn a_relationship_reads_its_cardinalities_line_and_role() {
        assert_eq!(
            texts("CUSTOMER ||--o{ ORDER : places\n")[..7],
            [
                (K::UnicodeText, "CUSTOMER"),
                (K::Cardinality(Cardinality::OnlyOne), "||"),
                (K::Identifying, "--"),
                (K::Cardinality(Cardinality::ZeroOrMore), "o{"),
                (K::UnicodeText, "ORDER"),
                (K::Colon, ":"),
                (K::UnicodeText, "places")
            ]
        );
    }

    #[test]
    fn cardinality_words_ignore_case_and_end_at_a_word_boundary() {
        assert_eq!(
            texts("A One or MORE to ones\n")[1..4],
            [(K::Cardinality(Cardinality::OneOrMore), "One or MORE"), (K::Identifying, "to"), (K::UnicodeText, "ones")]
        );
    }

    #[test]
    fn a_one_is_a_cardinality_before_a_name_or_a_line_and_an_entity_otherwise() {
        assert_eq!(texts("1 B\n")[0], (K::Cardinality(Cardinality::OnlyOne), "1"));
        assert_eq!(texts("1--\n")[0], (K::Cardinality(Cardinality::OnlyOne), "1"));
        assert_eq!(texts("1:\n")[0], (K::EntityOne, "1"));
    }

    #[test]
    fn a_block_reads_types_names_keys_and_comments() {
        assert_eq!(
            texts("A {\n  string id PK, FK \"the id\"\n}\n")[1..9],
            [
                (K::BlockStart, "{"),
                (K::AttributeWord, "string"),
                (K::AttributeWord, "id"),
                (K::AttributeKey, "PK"),
                (K::Char(','), ","),
                (K::AttributeKey, "FK"),
                (K::Comment, "\"the id\""),
                (K::BlockStop, "}")
            ]
        );
    }

    #[test]
    fn a_word_with_tildes_runs_to_the_last_tilde_of_the_line() {
        assert_eq!(texts("A {\n List~string~ names\n}\n")[2], (K::AttributeWord, "List~string~"));
    }

    #[test]
    fn a_style_line_ends_at_its_line_end() {
        let kinds: Vec<Kind> = texts("classDef red fill:#f00\nA\n").into_iter().map(|(kind, _)| kind).collect();

        assert_eq!(kinds[..8], [K::ClassDef, K::StyleText, K::StyleText, K::Colon, K::Brkt, K::StyleText, K::Newline, K::UnicodeText]);
    }
}
