//! Mermaid's class lexer (`classDiagram.jison`): its rules in its order, the first that matches wins, each start
//! condition a [`State`] on a stack.
//!
//! Jison appends `\b` to a rule ending in a word character, so `class` is a keyword and `classes` a name, and `o`
//! an aggregation only before a character that ends a word.

use super::Invalid;
use crate::source::{is_space, unicode};

/// A token kind, named after the grammar's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Newline,
    Direction(Direction),
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    ClassDiagram,
    EdgeState,
    CallbackName,
    CallbackArgs,
    Str,
    Style,
    ClassDef,
    Namespace,
    StructStart,
    StructStop,
    EofInStruct,
    Class,
    OpenInStruct,
    Member,
    CssClass,
    Callback,
    Link,
    Click,
    NoteFor,
    Note,
    AnnotationStart,
    AnnotationEnd,
    Href,
    GenericType,
    BquoteStr,
    LinkTarget,
    Relation(End),
    Line,
    DottedLine,
    Label,
    StyleSeparator,
    Minus,
    Dot,
    Plus,
    Colon,
    Comma,
    Brkt,
    Pct,
    Equals,
    Alpha,
    Sqs,
    Sqe,
    Punctuation,
    UnicodeText,
    Space,
    /// The grammar's `EOF`, which the lexer gives once it reaches the end of the text in most states.
    Eof,
    /// The end of the tokens, after which the parser reads nothing.
    End,
}

/// What a relation draws at one of its ends, mermaid's `relationType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::class) enum End {
    Aggregation,
    Extension,
    Composition,
    Dependency,
    Lollipop,
}

/// The way the diagram's ranks go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::class) enum Direction {
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
    Str,
    BqString,
    Generic,
    Namespace,
    NamespaceBody,
    Class,
    ClassBody,
    CallbackName,
    CallbackArgs,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

/// What a rule does to the stack of states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Stay,
    Push(State),
    Pop,
    PopTwice,
    /// Pops, then pushes.
    Swap(State),
    /// Pops and reads the same text again in the state below.
    PopAndRead,
}

/// A rule that matched: how far it goes, the token it gives if any, and its step.
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

/// The keywords every state reads, in the rules' order, before the generic and backquoted rules.
const KEYWORDS: [(&str, Kind); 6] = [
    ("cssClass", Kind::CssClass),
    ("callback", Kind::Callback),
    ("link", Kind::Link),
    ("click", Kind::Click),
    ("note for", Kind::NoteFor),
    ("note", Kind::Note),
];

const LINK_TARGETS: [&str; 4] = ["_self", "_blank", "_parent", "_top"];

const DIRECTIONS: [(&str, Direction); 4] = [("TB", Direction::TB), ("BT", Direction::BT), ("RL", Direction::RL), ("LR", Direction::LR)];

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
        let Some(matched) = rule(state, text, position) else {
            if done {
                break;
            }
            return Err(Invalid::Unrecognized { line: line_of(text, position) });
        };
        tokens.extend(matched.kind.map(|kind| Token { kind, start: position, end: position + matched.length }));
        position += matched.length;
        step(&mut states, matched.step);
    }
    tokens.push(Token { kind: Kind::End, start: text.len(), end: text.len() });
    Ok(tokens)
}

pub(super) fn line_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

fn step(states: &mut Vec<State>, step: Step) {
    let pop = |states: &mut Vec<State>| {
        if states.len() > 1 {
            states.pop();
        }
    };
    match step {
        Step::Stay => {}
        Step::Push(state) => states.push(state),
        Step::Pop | Step::PopAndRead => pop(states),
        Step::PopTwice => {
            pop(states);
            pop(states);
        }
        Step::Swap(state) => {
            pop(states);
            states.push(state);
        }
    }
}

/// The first rule active in `state` that matches at `position`.
fn rule(state: State, text: &str, position: usize) -> Option<Matched> {
    let rest = &text[position..];
    match state {
        State::Initial => initial(rest),
        State::Str => Some(if rest.starts_with('"') {
            Matched::skip(1, Step::Pop)
        } else {
            Matched::token(until(rest, |char| char == '"'), Kind::Str, Step::Stay)
        }),
        State::Generic => quote(rest).or_else(|| keyword_rule(rest)).or_else(|| {
            Some(if rest.starts_with('~') {
                Matched::skip(1, Step::Pop)
            } else {
                Matched::token(until(rest, |char| char == '~'), Kind::GenericType, Step::Stay)
            })
        }),
        State::BqString => quote(rest).or_else(|| keyword_rule(rest)).or_else(|| bq_string(rest)).or_else(|| shared(rest, state)),
        State::Namespace => quote(rest).or_else(|| namespace(rest)).or_else(|| shared(rest, state)),
        State::NamespaceBody => quote(rest).or_else(|| namespace_body(rest)).or_else(|| shared(rest, state)),
        State::Class => quote(rest).or_else(|| class(rest)).or_else(|| shared(rest, state)),
        State::ClassBody => quote(rest).or_else(|| Some(class_body(rest))),
        State::CallbackName => Some(callback_name(rest)),
        State::CallbackArgs => Some(if rest.starts_with(')') {
            Matched::skip(1, Step::Pop)
        } else {
            Matched::token(until(rest, |char| char == ')'), Kind::CallbackArgs, Step::Stay)
        }),
        State::AccTitle => Some(Matched::token(until(rest, |char| char == '\n'), Kind::AccTitleValue, Step::Pop)),
        State::AccDescr => Some(Matched::token(until(rest, |char| char == '\n'), Kind::AccDescrValue, Step::Pop)),
        State::AccDescrMultiline if rest.starts_with('}') => Some(Matched::skip(1, Step::Pop)),
        State::AccDescrMultiline => Some(Matched::token(until(rest, |char| char == '}'), Kind::AccDescrMultilineValue, Step::Stay)),
    }
}

/// The rules of `INITIAL` up to the keywords every state shares.
fn initial(rest: &str) -> Option<Matched> {
    if let Some(matched) = direction(rest) {
        return Some(matched);
    }
    if rest.starts_with("%%") {
        let line = until(rest, |char| char == '\n');
        return Some(Matched::skip(line + until(&rest[line..], |char| char != '\n' && char != '\r'), Step::Stay));
    }
    if let Some(length) = keyword_then(rest, "accTitle", ':') {
        return Some(Matched::token(length, Kind::AccTitle, Step::Push(State::AccTitle)));
    }
    if let Some(length) = keyword_then(rest, "accDescr", ':') {
        return Some(Matched::token(length, Kind::AccDescr, Step::Push(State::AccDescr)));
    }
    if let Some(length) = keyword_then(rest, "accDescr", '{') {
        return Some(Matched::skip(length, Step::Push(State::AccDescrMultiline)));
    }
    if let Some(length) = newline(rest) {
        return Some(Matched::token(length, Kind::Newline, Step::Stay));
    }
    if let Some(length) = spaces(rest) {
        return Some(Matched::skip(length, Step::Stay));
    }
    if let Some(length) = keyword(rest, "classDiagram-v2").or_else(|| keyword(rest, "classDiagram")) {
        return Some(Matched::token(length, Kind::ClassDiagram, Step::Stay));
    }
    if rest.starts_with("[*]") {
        return Some(Matched::token(3, Kind::EdgeState, Step::Stay));
    }
    if let Some(after) = rest.strip_prefix("call").filter(|after| after.starts_with(is_space)) {
        return Some(Matched::skip(rest.len() - after.trim_start_matches(is_space).len(), Step::Push(State::CallbackName)));
    }
    if rest.starts_with('"') {
        return Some(Matched::skip(1, Step::Push(State::Str)));
    }
    if let Some(length) = keyword(rest, "style") {
        return Some(Matched::token(length, Kind::Style, Step::Stay));
    }
    if let Some(length) = keyword(rest, "classDef") {
        return Some(Matched::token(length, Kind::ClassDef, Step::Stay));
    }
    if let Some(length) = keyword(rest, "namespace") {
        return Some(Matched::token(length, Kind::Namespace, Step::Push(State::Namespace)));
    }
    if let Some(length) = keyword(rest, "class") {
        return Some(Matched::token(length, Kind::Class, Step::Push(State::Class)));
    }
    shared(rest, State::Initial)
}

/// `.*direction\s+XX[^\n]*`, `TB` first: the line up to a `direction`, from wherever the lexer stands, and to the end
/// of the line holding the direction's name.
fn direction(rest: &str) -> Option<Matched> {
    let line = &rest[..until(rest, |char| matches!(char, '\n' | '\r' | '\u{2028}' | '\u{2029}'))];
    DIRECTIONS.iter().find_map(|&(name, direction)| {
        let found = line.rmatch_indices("direction").find_map(|(at, word)| {
            let after = &rest[at + word.len()..];
            let name_at = after.len() - after.trim_start_matches(is_space).len();
            (name_at > 0 && after[name_at..].starts_with(name)).then_some(at + word.len() + name_at + name.len())
        })?;
        let length = found + until(&rest[found..], |char| char == '\n');
        Some(Matched::token(length, Kind::Direction(direction), Step::Stay))
    })
}

/// The `namespace` state's own rules, between the opening quote and the shared ones.
fn namespace(rest: &str) -> Option<Matched> {
    if let Some(length) = keyword(rest, "namespace") {
        return Some(Matched::token(length, Kind::Namespace, Step::Push(State::Namespace)));
    }
    if let Some(length) = newline(rest) {
        return Some(Matched::token(length, Kind::Newline, Step::Pop));
    }
    if let Some(length) = spaces(rest) {
        return Some(Matched::skip(length, Step::Stay));
    }
    match rest.as_bytes().first() {
        Some(b'{') => Some(Matched::token(1, Kind::StructStart, Step::Push(State::NamespaceBody))),
        Some(b'}') => Some(Matched::skip(0, Step::PopAndRead)),
        _ => None,
    }
}

fn namespace_body(rest: &str) -> Option<Matched> {
    if let Some(length) = keyword(rest, "namespace") {
        return Some(Matched::token(length, Kind::Namespace, Step::Push(State::Namespace)));
    }
    if rest.starts_with('}') {
        return Some(Matched::token(1, Kind::StructStop, Step::Pop));
    }
    if rest.is_empty() {
        return Some(Matched::token(0, Kind::EofInStruct, Step::Stay));
    }
    if let Some(length) = newline(rest) {
        return Some(Matched::token(length, Kind::Newline, Step::Stay));
    }
    if let Some(length) = spaces(rest) {
        return Some(Matched::skip(length, Step::Stay));
    }
    if rest.starts_with("[*]") {
        return Some(Matched::token(3, Kind::EdgeState, Step::Stay));
    }
    keyword(rest, "class").map(|length| Matched::token(length, Kind::Class, Step::Push(State::Class)))
}

fn class(rest: &str) -> Option<Matched> {
    if let Some(length) = newline(rest) {
        return Some(Matched::token(length, Kind::Newline, Step::Pop));
    }
    if let Some(length) = spaces(rest) {
        return Some(Matched::skip(length, Step::Stay));
    }
    match rest.as_bytes().first() {
        Some(b'}') => Some(Matched::token(1, Kind::StructStop, Step::PopTwice)),
        Some(b'{') => Some(Matched::token(1, Kind::StructStart, Step::Push(State::ClassBody))),
        _ => None,
    }
}

/// A class's body: a member is the rest of a line, up to a brace.
fn class_body(rest: &str) -> Matched {
    match rest.as_bytes().first() {
        Some(b'}') => Matched::token(1, Kind::StructStop, Step::Pop),
        None => Matched::token(0, Kind::EofInStruct, Step::Stay),
        Some(b'[') if rest.starts_with("[*]") => Matched::token(3, Kind::EdgeState, Step::Stay),
        Some(b'{') => Matched::token(1, Kind::OpenInStruct, Step::Stay),
        Some(b'\n') => Matched::skip(1, Step::Stay),
        _ => Matched::token(until(rest, |char| matches!(char, '{' | '}' | '\n')), Kind::Member, Step::Stay),
    }
}

fn callback_name(rest: &str) -> Matched {
    if let Some(after) = rest.strip_prefix('(') {
        let blank = until(after, |char| !is_space(char));
        return if after[blank..].starts_with(')') {
            Matched::skip(blank + 2, Step::Pop)
        } else {
            Matched::skip(1, Step::Swap(State::CallbackArgs))
        };
    }
    Matched::token(until(rest, |char| char == '('), Kind::CallbackName, Step::Stay)
}

fn bq_string(rest: &str) -> Option<Matched> {
    if rest.starts_with('~') {
        return Some(Matched::skip(1, Step::Push(State::Generic)));
    }
    if rest.starts_with('`') {
        return Some(Matched::skip(1, Step::Pop));
    }
    let length = until(rest, |char| char == '`');
    (length > 0).then_some(Matched::token(length, Kind::BquoteStr, Step::Stay))
}

/// `<*>["]`, the first rule every state but a string's reads.
fn quote(rest: &str) -> Option<Matched> {
    rest.starts_with('"').then_some(Matched::skip(1, Step::Push(State::Str)))
}

/// From `cssClass` to `href`.
fn keyword_rule(rest: &str) -> Option<Matched> {
    if let Some((length, kind)) = KEYWORDS.iter().find_map(|&(word, kind)| keyword(rest, word).map(|length| (length, kind))) {
        return Some(Matched::token(length, kind, Step::Stay));
    }
    if rest.starts_with("<<") {
        return Some(Matched::token(2, Kind::AnnotationStart, Step::Stay));
    }
    if rest.starts_with(">>") {
        return Some(Matched::token(2, Kind::AnnotationEnd, Step::Stay));
    }
    keyword(rest, "href").map(|length| Matched::token(length, Kind::Href, Step::Stay))
}

/// The rules every state shares, from `cssClass` to the end of the text, with those only `INITIAL` reads.
fn shared(rest: &str, state: State) -> Option<Matched> {
    if let Some(matched) = keyword_rule(rest) {
        return Some(matched);
    }
    if rest.starts_with('~') {
        return Some(Matched::skip(1, Step::Push(State::Generic)));
    }
    if rest.starts_with('`') {
        return Some(Matched::skip(1, Step::Push(State::BqString)));
    }
    if let Some(length) = LINK_TARGETS.iter().find_map(|target| keyword(rest, target)) {
        return Some(Matched::token(length, Kind::LinkTarget, Step::Stay));
    }
    if let Some(matched) = relation(rest) {
        return Some(matched);
    }
    let first = rest.chars().next();
    let initial = state == State::Initial;
    let kind = match rest.as_bytes() {
        [b'-', b'-', ..] => Some((2, Kind::Line)),
        [b'.', b'.', ..] => Some((2, Kind::DottedLine)),
        [b':', b':', b':', ..] => Some((3, Kind::StyleSeparator)),
        [b':', next, ..] if !matches!(next, b':' | b'\n' | b';') => {
            Some((1 + until(&rest[1..], |char| matches!(char, ':' | '\n' | ';')), Kind::Label))
        }
        [b'-', ..] => Some((1, Kind::Minus)),
        [b'.', ..] => Some((1, Kind::Dot)),
        [b'+', ..] => Some((1, Kind::Plus)),
        [b':', ..] if initial => Some((1, Kind::Colon)),
        [b',', ..] if initial => Some((1, Kind::Comma)),
        [b'#', ..] if initial => Some((1, Kind::Brkt)),
        [b'%', ..] => Some((1, Kind::Pct)),
        [b'=', ..] => Some((1, Kind::Equals)),
        [byte, ..] if byte.is_ascii_alphanumeric() || *byte == b'_' => Some((until(rest, |char| !is_word(char)), Kind::Alpha)),
        [b'[', ..] => Some((1, Kind::Sqs)),
        [b']', ..] => Some((1, Kind::Sqe)),
        [b'!' | b'"' | b'#' | b'$' | b'&' | b'\'' | b'*' | b',' | b'`' | b'?' | b'\\' | b'/', ..] => Some((1, Kind::Punctuation)),
        _ => None,
    };
    if let Some((length, kind)) = kind {
        return Some(Matched::token(length, kind, Step::Stay));
    }
    match first {
        Some(letter) if unicode::is_letter(letter) => Some(Matched::token(letter.len_utf8(), Kind::UnicodeText, Step::Stay)),
        Some(space) if is_space(space) => Some(Matched::token(space.len_utf8(), Kind::Space, Step::Stay)),
        Some(_) => None,
        None => Some(Matched::token(0, Kind::Eof, Step::Stay)),
    }
}

/// `\s*<|`, `\s*|>`, `\s*>`, `\s*<`, `\s*\*`, `\s*o\b`, `\s*()`.
fn relation(rest: &str) -> Option<Matched> {
    let blank = rest.len() - rest.trim_start_matches(is_space).len();
    let after = &rest[blank..];
    let (length, kind) = match after.as_bytes() {
        [b'<', b'|', ..] | [b'|', b'>', ..] => (2, Kind::Relation(End::Extension)),
        [b'>' | b'<', ..] => (1, Kind::Relation(End::Dependency)),
        [b'*', ..] => (1, Kind::Relation(End::Composition)),
        [b'o', next, ..] if !is_word(char::from(*next)) => (1, Kind::Relation(End::Aggregation)),
        [b'o'] => (1, Kind::Relation(End::Aggregation)),
        [b'(', b')', ..] => (2, Kind::Relation(End::Lollipop)),
        _ => return None,
    };
    Some(Matched::token(blank + length, kind, Step::Stay))
}

/// `\s*(\r?\n)+`: spaces up to the last line end among them.
fn newline(rest: &str) -> Option<usize> {
    let blank = rest.len() - rest.trim_start_matches(is_space).len();
    rest[..blank].rfind('\n').map(|at| at + 1)
}

fn spaces(rest: &str) -> Option<usize> {
    let blank = rest.len() - rest.trim_start_matches(is_space).len();
    (blank > 0).then_some(blank)
}

fn is_word(char: char) -> bool {
    char.is_ascii_alphanumeric() || char == '_'
}

/// `word` and a word boundary, as jison adds one after a rule ending in a word character.
fn keyword(text: &str, word: &str) -> Option<usize> {
    text.strip_prefix(word).filter(|after| after.chars().next().is_none_or(|char| !is_word(char))).map(|_| word.len())
}

/// `word\s*"mark"\s*`.
fn keyword_then(text: &str, word: &str, mark: char) -> Option<usize> {
    let after = text.strip_prefix(word)?;
    let space = until(after, |char| !is_space(char));
    let after_mark = after[space..].strip_prefix(mark)?;
    Some(text.len() - after_mark.len() + until(after_mark, |char| !is_space(char)))
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
    fn a_relation_reads_its_ends_its_line_and_its_label() {
        assert_eq!(
            texts("A <|-- B : is\n"),
            [
                (K::Alpha, "A"),
                (K::Relation(End::Extension), "<|"),
                (K::Line, "--"),
                (K::Alpha, "B"),
                (K::Label, ": is"),
                (K::Newline, "\n"),
                (K::Eof, ""),
                (K::End, "")
            ]
        );
    }

    #[test]
    fn an_o_is_an_aggregation_only_before_the_end_of_a_word() {
        assert_eq!(texts("o--B\n")[0], (K::Relation(End::Aggregation), "o"));
        assert_eq!(texts("ox--B\n")[0], (K::Alpha, "ox"));
    }

    #[test]
    fn a_class_body_reads_each_line_as_a_member() {
        assert_eq!(
            texts("class A {\n  +int x\n}\n")[..5],
            [(K::Class, "class"), (K::Alpha, "A"), (K::StructStart, "{"), (K::Member, "  +int x"), (K::StructStop, "}")]
        );
    }

    #[test]
    fn a_keyword_inside_a_generic_type_is_still_a_keyword() {
        assert_eq!(texts("A~T~\n")[1], (K::GenericType, "T"));
        assert_eq!(texts("A~note~\n")[1], (K::Note, "note"));
    }

    #[test]
    fn a_line_naming_a_direction_is_a_direction_from_wherever_the_lexer_stands() {
        assert_eq!(texts("A direction LR x\n")[0], (K::Direction(Direction::LR), "A direction LR x"));
    }

    #[test]
    fn a_comment_takes_the_line_ends_after_it() {
        assert_eq!(texts("A %% gone\n\nB\n")[..2], [(K::Alpha, "A"), (K::Alpha, "B")]);
    }

    #[test]
    fn a_closing_brace_ends_a_nested_namespace_then_its_parent() {
        let kinds: Vec<Kind> = texts("namespace A {\nnamespace B {\nclass C\n}\n}\n").into_iter().map(|(kind, _)| kind).collect();

        assert_eq!(kinds.iter().filter(|&&kind| kind == K::StructStop).count(), 2);
    }

    #[test]
    fn text_no_rule_reads_is_refused_with_its_line() {
        assert_eq!(tokens("classDiagram\nA ; B\n"), Err(Invalid::Unrecognized { line: 2 }));
    }
}
