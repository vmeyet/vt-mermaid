//! Mermaid's sequence lexer (`sequenceDiagram.jison`): its rules in its order, the first that matches wins, each start
//! condition a [`State`] on a stack.
//!
//! The rules are case-insensitive, and jison appends `\b` to a rule ending in a word character, so `loop` is a keyword
//! and `loops` an actor.

use super::Invalid;
use super::arrow::Arrow;
use crate::source::is_space;

/// A token kind, named after the grammar's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Newline,
    Num,
    ConfigStart,
    ConfigContent,
    ConfigEnd,
    Actor,
    Invalid,
    Box,
    Participant,
    ParticipantActor,
    Create,
    Destroy,
    As,
    Block(Block),
    Else,
    And,
    Option,
    RestOfLine,
    End,
    LeftOf,
    RightOf,
    Links,
    Link,
    Properties,
    Details,
    Over,
    Note,
    Activate,
    Deactivate,
    Title,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Header,
    Autonumber,
    Off,
    Comma,
    Arrow(Arrow),
    Text,
    Plus,
    Minus,
    Central,
    Eof,
}

/// A keyword that opens a block closed by `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::sequence) enum Block {
    Loop,
    Rect,
    Opt,
    Alt,
    Par,
    ParOver,
    Critical,
    Break,
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
    Id,
    Alias,
    Line,
    Config,
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
    /// Pops twice, then pushes.
    Leave(State),
}

/// A rule that matched: where its token's text starts and ends after the match's start, the token it gives if any,
/// how far the match goes, and its step.
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

    /// A token whose text is the match without the spaces around it, as `yytext.trim()` leaves it.
    fn trimmed(match_text: &str, kind: Kind, step: Step) -> Self {
        let start = match_text.len() - match_text.trim_start_matches(is_space).len();
        let end = match_text.trim_end_matches(is_space).len().max(start);
        Self { length: match_text.len(), kind: Some(kind), text: (start, end), step }
    }
}

/// The keywords that open a block or a section, then push `Line` to read the rest of the line, in the rules' order.
const LINE_KEYWORDS: [(&str, Kind); 11] = [
    ("loop", Kind::Block(Block::Loop)),
    ("rect", Kind::Block(Block::Rect)),
    ("opt", Kind::Block(Block::Opt)),
    ("alt", Kind::Block(Block::Alt)),
    ("else", Kind::Else),
    ("par", Kind::Block(Block::Par)),
    ("par_over", Kind::Block(Block::ParOver)),
    ("and", Kind::And),
    ("critical", Kind::Block(Block::Critical)),
    ("option", Kind::Option),
    ("break", Kind::Block(Block::Break)),
];

/// The keywords that only count before a space and a character an actor may start with.
const ACTOR_KEYWORDS: [(&str, Kind); 4] =
    [("links", Kind::Links), ("link", Kind::Link), ("properties", Kind::Properties), ("details", Kind::Details)];

/// What may follow an actor's name in a message without being read as part of it.
const ARROW_STARTS: [&str; 17] =
    ["-x", "--x", "-)", "--)", "-|\\", "-\\", "-/", "-//", "-|/", "/|-", "\\|-", "//-", "\\\\-", "/|-", "--|\\", "--", "()"];

/// Every token of `text`, ending with one `Eof`; text no rule matches is an error.
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

fn step(states: &mut Vec<State>, step: Step) {
    let pop = |states: &mut Vec<State>| {
        if states.len() > 1 {
            states.pop();
        }
    };
    match step {
        Step::Stay => {}
        Step::Push(state) => states.push(state),
        Step::Pop => pop(states),
        Step::PopTwice => {
            pop(states);
            pop(states);
        }
        Step::Swap(state) => {
            pop(states);
            states.push(state);
        }
        Step::Leave(state) => {
            pop(states);
            pop(states);
            states.push(state);
        }
    }
}

/// The first rule active in `state` that matches at `position`; at the end of the text only the rules that match
/// nothing can, and `Initial`'s `<<EOF>>` gives a last `NEWLINE`.
fn rule(state: State, text: &str, position: usize) -> Option<Matched> {
    let rest = &text[position..];
    match state {
        State::Initial => initial(text, position),
        State::Id => id(rest),
        State::Alias => Some(alias(rest)),
        State::Line => Some(line(rest)),
        State::Config => config(rest),
        State::AccTitle => Some(Matched::token(until(rest, |char| char == '\n'), Kind::AccTitleValue, Step::Pop)),
        State::AccDescr => Some(Matched::token(until(rest, |char| char == '\n'), Kind::AccDescrValue, Step::Pop)),
        State::AccDescrMultiline if rest.starts_with('}') => Some(Matched::skip(1, Step::Pop)),
        State::AccDescrMultiline => Some(Matched::token(until(rest, |char| char == '}'), Kind::AccDescrMultilineValue, Step::Stay)),
    }
}

fn initial(text: &str, position: usize) -> Option<Matched> {
    let rest = &text[position..];
    let Some(first) = rest.chars().next() else {
        return Some(Matched::token(0, Kind::Newline, Step::Stay));
    };
    if first == '\n' {
        return Some(Matched::token(until(rest, |char| char != '\n'), Kind::Newline, Step::Stay));
    }
    if is_space(first) {
        return Some(Matched::skip(until(rest, |char| !is_space(char)), Step::Stay));
    }
    if first == '#' || (first == '%' && !rest[1..].starts_with('{')) {
        return Some(Matched::skip(until(rest, |char| char == '\n'), Step::Stay));
    }
    if first != '}' && first.len_utf16() == 1 && rest[first.len_utf8()..].starts_with("%%") {
        return Some(Matched::skip(until(rest, |char| char == '\n'), Step::Stay));
    }
    if let Some(length) = number(rest) {
        return Some(Matched::token(length, Kind::Num, Step::Stay));
    }
    keyword_rule(rest).or_else(|| symbol_rule(rest)).or_else(|| Some(Matched::token(first.len_utf8(), Kind::Invalid, Step::Stay)))
}

/// The keyword rules, from `box` to `,` and `;`, then the actor rule.
fn keyword_rule(rest: &str) -> Option<Matched> {
    if let Some(length) = keyword(rest, "box") {
        return Some(Matched::token(length, Kind::Box, Step::Push(State::Line)));
    }
    let words: [(&str, Kind, Step); 4] = [
        ("participant", Kind::Participant, Step::Push(State::Id)),
        ("actor", Kind::ParticipantActor, Step::Push(State::Id)),
        ("create", Kind::Create, Step::Stay),
        ("destroy", Kind::Destroy, Step::Push(State::Id)),
    ];
    if let Some((length, kind, step)) = words.iter().find_map(|&(word, kind, step)| keyword(rest, word).map(|length| (length, kind, step)))
    {
        return Some(Matched::token(length, kind, step));
    }
    if let Some((length, kind)) = LINE_KEYWORDS.iter().find_map(|&(word, kind)| keyword(rest, word).map(|length| (length, kind))) {
        return Some(Matched::token(length, kind, Step::Push(State::Line)));
    }
    let words: [(&str, Kind); 3] = [("end", Kind::End), ("left of", Kind::LeftOf), ("right of", Kind::RightOf)];
    if let Some((length, kind)) = words.iter().find_map(|&(word, kind)| keyword(rest, word).map(|length| (length, kind))) {
        return Some(Matched::token(length, kind, Step::Stay));
    }
    if let Some((length, kind)) = ACTOR_KEYWORDS.iter().find_map(|&(word, kind)| before_actor(rest, word).map(|length| (length, kind))) {
        return Some(Matched::token(length, kind, Step::Stay));
    }
    let words: [(&str, Kind, Step); 4] = [
        ("over", Kind::Over, Step::Stay),
        ("note", Kind::Note, Step::Stay),
        ("activate", Kind::Activate, Step::Push(State::Id)),
        ("deactivate", Kind::Deactivate, Step::Push(State::Id)),
    ];
    if let Some((length, kind, step)) = words.iter().find_map(|&(word, kind, step)| keyword(rest, word).map(|length| (length, kind, step)))
    {
        return Some(Matched::token(length, kind, step));
    }
    if let Some(length) = title(rest, "title").or_else(|| title(rest, "title:")) {
        return Some(Matched::token(length, Kind::Title, Step::Stay));
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
    let words: [(&str, Kind); 3] = [("sequenceDiagram", Kind::Header), ("autonumber", Kind::Autonumber), ("off", Kind::Off)];
    if let Some((length, kind)) = words.iter().find_map(|&(word, kind)| keyword(rest, word).map(|length| (length, kind))) {
        return Some(Matched::token(length, kind, Step::Stay));
    }
    match rest.as_bytes().first() {
        Some(b',') => Some(Matched::token(1, Kind::Comma, Step::Stay)),
        Some(b';') => Some(Matched::token(1, Kind::Newline, Step::Stay)),
        _ => actor(rest).map(|length| Matched::trimmed(&rest[..length], Kind::Actor, Step::Stay)),
    }
}

/// The arrows, the message text, `+`, `-` and `()`.
fn symbol_rule(rest: &str) -> Option<Matched> {
    if let Some(arrow) = Arrow::ALL.iter().find(|arrow| starts_with_ignoring_case(rest, arrow.lexeme())) {
        return Some(Matched::token(arrow.lexeme().len(), Kind::Arrow(*arrow), Step::Stay));
    }
    if rest.starts_with(':') {
        return Some(Matched::token(until(rest, |char| matches!(char, '#' | '\n' | ';')), Kind::Text, Step::Stay));
    }
    match rest.as_bytes() {
        [b'+', ..] => Some(Matched::token(1, Kind::Plus, Step::Stay)),
        [b'-', ..] => Some(Matched::token(1, Kind::Minus, Step::Stay)),
        [b'(', b')', ..] => Some(Matched::token(2, Kind::Central, Step::Stay)),
        _ => None,
    }
}

/// `ID`: an actor's name after `participant`, `actor`, `destroy`, `activate` or `deactivate`.
fn id(rest: &str) -> Option<Matched> {
    let first = rest.chars().next()?;
    if first != '\n' && is_space(first) {
        return Some(Matched::skip(until(rest, |char| char == '\n' || !is_space(char)), Step::Stay));
    }
    if first == '#' {
        return Some(Matched::skip(until(rest, |char| char == '\n'), Step::Stay));
    }
    if rest.starts_with("@{") {
        return Some(Matched::token(2, Kind::ConfigStart, Step::Push(State::Config)));
    }
    let word = until(rest, |char| matches!(char, '<' | '>' | ':' | '\n' | ',' | ';' | '@') || is_space(char));
    let after_word = &rest[word..];
    if word > 0 && after_word.trim_start_matches(is_space).starts_with("@{") {
        return Some(Matched::trimmed(&rest[..word], Kind::Actor, Step::Stay));
    }
    if word > 0 && alias_ahead(after_word) {
        return Some(Matched::trimmed(&rest[..word], Kind::Actor, Step::Push(State::Alias)));
    }
    if let Some(length) = name_before_line_end(rest) {
        return Some(Matched::trimmed(&rest[..length], Kind::Actor, Step::Pop));
    }
    let run = until(rest, |char| matches!(char, '>' | ':' | '\n' | ',' | ';' | '@' | '<'));
    let length =
        if rest[run..].starts_with('<') { run + until(&rest[run..], |char| char == '\n') } else { until(rest, |char| char == '\n') };
    (length > 0).then(|| Matched::trimmed(&rest[..length], Kind::Invalid, Step::Pop))
}

/// `\s+as\s`, which may run over line ends.
fn alias_ahead(text: &str) -> bool {
    let after_space = text.trim_start_matches(is_space);
    after_space.len() < text.len() && starts_with_ignoring_case(after_space, "as") && after_space[2..].starts_with(is_space)
}

/// `[^<>:\n,;@]+(?=\s*[\n;#]|$)`: the longest run whose end is the end of the text or has only spaces before a line
/// end, a `;` or a `#`.
fn name_before_line_end(rest: &str) -> Option<usize> {
    let run = until(rest, |char| matches!(char, '<' | '>' | ':' | '\n' | ',' | ';' | '@'));
    let mut ends_line = rest[run..].is_empty() || rest[run..].starts_with(['\n', ';']);
    if ends_line {
        return (run > 0).then_some(run);
    }
    for (at, char) in rest[..run].char_indices().rev() {
        ends_line = if is_space(char) { ends_line } else { char == '#' };
        if ends_line && at > 0 {
            return Some(at);
        }
    }
    None
}

/// `ALIAS`: `as` before the alias, or nothing, which ends the statement.
fn alias(rest: &str) -> Matched {
    let first = rest.chars().next();
    if first.is_some_and(|char| char != '\n' && is_space(char)) {
        return Matched::skip(until(rest, |char| char == '\n' || !is_space(char)), Step::Stay);
    }
    if first == Some('#') {
        return Matched::skip(until(rest, |char| char == '\n'), Step::Stay);
    }
    if let Some(length) = keyword(rest, "as") {
        return Matched::token(length, Kind::As, Step::Leave(State::Line));
    }
    Matched::token(0, Kind::Newline, Step::PopTwice)
}

/// `LINE`: the rest of the line after a block's keyword or an alias's `as`.
fn line(rest: &str) -> Matched {
    let first = rest.chars().next();
    if first.is_some_and(|char| char != '\n' && is_space(char)) {
        return Matched::skip(until(rest, |char| char == '\n' || !is_space(char)), Step::Stay);
    }
    if first == Some('#') {
        return Matched::skip(until(rest, |char| char == '\n'), Step::Stay);
    }
    Matched::token(until(rest, |char| matches!(char, '#' | '\n' | ';')), Kind::RestOfLine, Step::Pop)
}

/// `CONFIG`: a participant's `@{ … }` block up to its `}`.
fn config(rest: &str) -> Option<Matched> {
    let content = until(rest, |char| char == '}');
    if content > 0 {
        return Some(Matched::token(content, Kind::ConfigContent, Step::Stay));
    }
    rest.starts_with('}')
        .then(|| Matched::token(1, Kind::ConfigEnd, if alias_ahead(&rest[1..]) { Step::Swap(State::Alias) } else { Step::PopTwice }))
}

/// `([0-9]+(\.[0-9]{1,2})?|\.[0-9]{1,2})(?=[ \n]+)`.
fn number(text: &str) -> Option<usize> {
    let digits = until(text, |char| !char.is_ascii_digit());
    let fraction = text[digits..].strip_prefix('.').map_or(0, |after| until(after, |char| !char.is_ascii_digit()));
    let length = match (digits, fraction) {
        (_, 1 | 2) => digits + 1 + fraction,
        (1.., 0) if !text[digits..].starts_with('.') => digits,
        _ => return None,
    };
    text[length..].starts_with([' ', '\n']).then_some(length)
}

/// The actor rule: a run of characters an actor's name may hold, its parts joined by single dashes, stopping before
/// anything that starts an arrow.
fn actor(text: &str) -> Option<usize> {
    let head = until(text, |char| matches!(char, '/' | '\\' | '+' | '(' | ')' | '<' | '-' | '>' | ':' | '\n' | ',' | ';'));
    if head == 0 {
        return None;
    }
    let mut end = head;
    loop {
        let rest = &text[end..];
        if ARROW_STARTS.iter().any(|start| starts_with_ignoring_case(rest, start)) {
            return Some(end);
        }
        let dashes = until(rest, |char| char != '-');
        let part = until(&rest[dashes..], |char| matches!(char, '+' | '<' | '-' | '>' | ':' | '\n' | ',' | ';'));
        if part == 0 {
            return Some(end);
        }
        end += dashes + part;
    }
}

/// `title\s[^#\n;]+` and `title:\s[^#\n;]+`.
fn title(text: &str, word: &str) -> Option<usize> {
    let after = text.get(word.len()..).filter(|_| starts_with_ignoring_case(text, word))?;
    let space = after.chars().next().filter(|&char| is_space(char))?;
    let value = until(&after[space.len_utf8()..], |char| matches!(char, '#' | '\n' | ';'));
    (value > 0).then_some(word.len() + space.len_utf8() + value)
}

/// `word` and a word boundary, as jison adds one after a rule ending in a word character.
fn keyword(text: &str, word: &str) -> Option<usize> {
    let boundary = text.as_bytes().get(word.len()).is_none_or(|&byte| !byte.is_ascii_alphanumeric() && byte != b'_');
    (starts_with_ignoring_case(text, word) && boundary).then_some(word.len())
}

/// `word(?=[ \t]+[^\/\\\+\(\)<>:\n,;\s\-])`.
fn before_actor(text: &str, word: &str) -> Option<usize> {
    let after = text.get(word.len()..).filter(|_| starts_with_ignoring_case(text, word))?;
    let blank = until(after, |char| char != ' ' && char != '\t');
    let next = after[blank..].chars().next()?;
    let starts_actor = !matches!(next, '/' | '\\' | '+' | '(' | ')' | '<' | '>' | ':' | ',' | ';' | '-') && !is_space(next);
    (blank > 0 && starts_actor).then_some(word.len())
}

/// `word\s*"mark"\s*`.
fn keyword_then(text: &str, word: &str, mark: char) -> Option<usize> {
    let after = text.get(word.len()..).filter(|_| starts_with_ignoring_case(text, word))?;
    let space = until(after, |char| !is_space(char));
    let after_mark = after[space..].strip_prefix(mark)?;
    Some(text.len() - after_mark.len() + until(after_mark, |char| !is_space(char)))
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
    fn keywords_ignore_case_and_end_at_a_word_boundary() {
        assert_eq!(
            texts("PARTICIPANT A\nloops->>B: x\n"),
            [
                (K::Participant, "PARTICIPANT"),
                (K::Actor, "A"),
                (K::Newline, "\n"),
                (K::Actor, "loops"),
                (K::Arrow(Arrow::Solid), "->>"),
                (K::Actor, "B"),
                (K::Text, ": x"),
                (K::Newline, "\n"),
                (K::Newline, ""),
                (K::Eof, "")
            ]
        );
    }

    #[test]
    fn an_actor_keeps_single_dashes_and_stops_before_an_arrow() {
        let kinds: Vec<(Kind, &str)> = texts("A-B-->>C D-x E\n").into_iter().take(5).collect();

        assert_eq!(
            kinds,
            [(K::Actor, "A-B"), (K::Arrow(Arrow::Dotted), "-->>"), (K::Actor, "C D"), (K::Arrow(Arrow::SolidCross), "-x"), (K::Actor, "E")]
        );
    }

    #[test]
    fn an_alias_is_the_rest_of_the_line_after_as() {
        assert_eq!(
            texts("participant A as Alice Smith\n")[..4],
            [(K::Participant, "participant"), (K::Actor, "A"), (K::As, "as"), (K::RestOfLine, "Alice Smith")]
        );
    }

    #[test]
    fn a_name_without_alias_runs_to_the_line_end_hash_included() {
        assert_eq!(texts("participant Alice Smith #c\n")[1], (K::Actor, "Alice Smith #c"));
        assert_eq!(texts("participant B ;\n")[1..3], [(K::Actor, "B"), (K::Newline, ";")]);
    }

    #[test]
    fn a_participant_block_ends_at_its_brace() {
        assert_eq!(
            texts("participant D@{ \"type\": \"database\" } as Store\n")[1..6],
            [(K::Actor, "D"), (K::ConfigStart, "@{"), (K::ConfigContent, " \"type\": \"database\" "), (K::ConfigEnd, "}"), (K::As, "as")]
        );
    }

    #[test]
    fn a_number_needs_a_space_or_a_line_end_after_it() {
        assert_eq!(number("10 "), Some(2));
        assert_eq!(number("1.25\n"), Some(4));
        assert_eq!(number(".5 "), Some(2));
        assert_eq!(number("1.255 "), None);
        assert_eq!(number("10.001Alice"), None);
    }

    #[test]
    fn a_percent_sign_not_before_a_brace_starts_a_comment() {
        assert_eq!(texts("%x\n")[0], (K::Newline, "\n"));
        assert_eq!(texts("%{x\n")[0], (K::Actor, "%{x"));
    }

    #[test]
    fn two_percent_signs_after_any_character_but_a_brace_start_a_comment() {
        assert_eq!(texts("A%% gone\n")[0], (K::Newline, "\n"));
        assert_eq!(texts("}%% kept\n")[0], (K::Actor, "}%% kept"));
    }

    #[test]
    fn a_keyword_naming_an_actor_needs_a_name_after_it() {
        assert_eq!(texts("link A: x\n")[0], (K::Link, "link"));
        assert_eq!(texts("link->>A: x\n")[0], (K::Actor, "link"));
    }

    #[test]
    fn text_no_rule_reads_is_refused_with_its_line() {
        assert_eq!(tokens("participant\n"), Err(Invalid::Unrecognized { line: 1 }));
    }
}
