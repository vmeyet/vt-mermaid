//! Mermaid's state grammar (`stateDiagram.jison`), read top-down: accepting what its LALR parser accepts, and building
//! the document its actions build, each composite state's body a document of its own.

use super::Invalid;
use super::db::{Description, Kind as StateKind, Note, Side, State, Statement};
use super::lexer::{Kind, Token, line_of};
use crate::error::Error;
use crate::source::is_space;

/// The root document of `tokens`, read from `text`; refused once more than `nodes` composite states open, each
/// opening counted as a node, so nesting stays bounded.
pub(super) fn parse(text: &str, tokens: &[Token], nodes: usize) -> Result<Vec<Statement>, Error> {
    let mut parser = Parser { text, tokens, at: 0, dividers: 0 };
    parser.diagram(nodes)
}

#[derive(Debug)]
struct Parser<'a> {
    text: &'a str,
    tokens: &'a [Token],
    at: usize,
    /// Dividers read so far, which name each one as mermaid's `getDividerId` does.
    dividers: usize,
}

/// A composite state whose body is being read: the state it is, and its statements so far.
#[derive(Debug)]
struct Open {
    state: State,
    body: Vec<Statement>,
}

/// What a statement adds to the document it is in, or the composite state it opens.
#[derive(Debug)]
enum Read {
    Statement(Statement),
    Opens(State),
    Nothing,
}

impl Parser<'_> {
    fn peek(&self) -> Kind {
        self.tokens.get(self.at).map_or(Kind::Eof, |token| token.kind)
    }

    /// The token at hand's text; the last token is always `Eof`, so reading never runs past it.
    fn advance(&mut self) -> &str {
        let token = self.tokens.get(self.at).copied().unwrap_or(Token { kind: Kind::Eof, start: self.text.len(), end: self.text.len() });
        self.at = (self.at + 1).min(self.tokens.len().saturating_sub(1));
        &self.text[token.start..token.end]
    }

    fn expect(&mut self, kind: Kind) -> Result<&str, Invalid> {
        if self.peek() == kind { Ok(self.advance()) } else { Err(self.unexpected()) }
    }

    fn unexpected(&self) -> Invalid {
        let token = self.tokens.get(self.at).copied().unwrap_or(Token { kind: Kind::Eof, start: self.text.len(), end: self.text.len() });
        let found = if token.kind == Kind::Eof { "end of text".to_owned() } else { format!("{:?}", &self.text[token.start..token.end]) };
        Invalid::Unexpected { line: line_of(self.text, token.start), found }
    }

    /// `start: NL* SD document`: the composite states open at a point are a stack, so nesting never deepens the call
    /// stack.
    fn diagram(&mut self, nodes: usize) -> Result<Vec<Statement>, Error> {
        while self.peek() == Kind::Newline {
            self.advance();
        }
        self.expect(Kind::Header)?;
        let mut root = Vec::new();
        let mut open: Vec<Open> = Vec::new();
        let mut opened = 0;
        loop {
            match (self.peek(), open.pop()) {
                (Kind::Eof, None) => return Ok(root),
                (Kind::StructStop, Some(Open { state, body })) => {
                    self.advance();
                    let statement = Statement::State(State { doc: Some(body), ..state });
                    open.last_mut().map_or(&mut root, |parent| &mut parent.body).push(statement);
                }
                (_, innermost) => {
                    open.extend(innermost);
                    match self.statement()? {
                        Read::Statement(statement) => open.last_mut().map_or(&mut root, |parent| &mut parent.body).push(statement),
                        Read::Opens(state) => {
                            opened += 1;
                            if opened > nodes {
                                return Err(Error::TooManyNodes { nodes: opened });
                            }
                            open.push(Open { state, body: Vec::new() });
                        }
                        Read::Nothing => {}
                    }
                }
            }
        }
    }

    /// One `line` of a document: a statement, or a line end, which adds nothing.
    fn statement(&mut self) -> Result<Read, Invalid> {
        let statement = match self.peek() {
            Kind::Newline => {
                self.advance();
                return Ok(Read::Nothing);
            }
            Kind::ClassDef => self.tokens_then(&[Kind::ClassDefId, Kind::ClassDefStyleOpts])?,
            Kind::Style => self.tokens_then(&[Kind::StyleIds, Kind::StyleDefStyleOpts])?,
            Kind::Class => self.tokens_then(&[Kind::ClassEntityIds, Kind::StyleClass])?,
            Kind::Id | Kind::EdgeState => self.state_or_relation()?,
            Kind::HideEmpty | Kind::AccDescrMultilineValue => self.tokens_then(&[])?,
            Kind::Scale => self.tokens_then(&[Kind::Width])?,
            Kind::AccTitle => self.tokens_then(&[Kind::AccTitleValue])?,
            Kind::AccDescr => self.tokens_then(&[Kind::AccDescrValue])?,
            Kind::CompositState => {
                let id = self.advance().to_owned();
                if self.peek() != Kind::StructStart {
                    return Ok(Read::Statement(Statement::Other));
                }
                self.advance();
                return Ok(Read::Opens(State::named(id)));
            }
            Kind::StateDescr => return self.described_state(),
            Kind::Fork | Kind::Join | Kind::Choice => {
                let kind = match self.peek() {
                    Kind::Fork => StateKind::Fork,
                    Kind::Join => StateKind::Join,
                    _ => StateKind::Choice,
                };
                let id = self.advance().trim_matches(is_space).to_owned();
                Statement::State(State { kind: Some(kind), ..State::named(id) })
            }
            Kind::Concurrent => {
                self.advance();
                self.dividers += 1;
                Statement::State(State { kind: Some(StateKind::Divider), ..State::named(format!("divider-id-{}", self.dividers)) })
            }
            Kind::Note => self.note()?,
            Kind::Direction(direction) => {
                self.advance();
                Statement::Direction(direction)
            }
            Kind::Click => self.click()?,
            _ => return Err(self.unexpected()),
        };
        Ok(Read::Statement(statement))
    }

    /// A statement mermaid's db does nothing with: its first token, then `kinds` in order.
    fn tokens_then(&mut self, kinds: &[Kind]) -> Result<Statement, Invalid> {
        self.advance();
        for &kind in kinds {
            self.expect(kind)?;
        }
        Ok(Statement::Other)
    }

    /// `idStatement`, then a description or `-->`, another `idStatement` and its description.
    fn state_or_relation(&mut self) -> Result<Statement, Invalid> {
        let from = self.id_statement()?;
        match self.peek() {
            Kind::Descr => {
                let description = trim_colon(self.advance());
                Ok(Statement::State(State { description: Description::text(description), ..from }))
            }
            Kind::Arrow => {
                self.advance();
                let to = self.id_statement()?;
                let description = if self.peek() == Kind::Descr { trim_colon(self.advance()) } else { String::new() };
                Ok(Statement::Relation { from, to, description })
            }
            _ => Ok(Statement::State(from)),
        }
    }

    /// `ID`, `[*]`, either followed by `:::` and a class name.
    fn id_statement(&mut self) -> Result<State, Invalid> {
        if !matches!(self.peek(), Kind::Id | Kind::EdgeState) {
            return Err(self.unexpected());
        }
        let id = self.advance().trim_matches(is_space).to_owned();
        if self.peek() == Kind::StyleSeparator {
            self.advance();
            self.expect(Kind::Id)?;
        }
        Ok(State::named(id))
    }

    /// `state "description" as id`, with a body or not; a body-less one's id splits at its first `:`, the part after
    /// it a second description.
    fn described_state(&mut self) -> Result<Read, Invalid> {
        let description = self.advance().to_owned();
        self.expect(Kind::As)?;
        let id = self.expect(Kind::Id)?.to_owned();
        if self.peek() == Kind::StructStart {
            self.advance();
            return Ok(Read::Opens(State { description: Description::text(description), ..State::named(id) }));
        }
        let trimmed = description.trim_matches(is_space).to_owned();
        let state = match id.split(':').collect::<Vec<_>>().as_slice() {
            [single] => State { description: Description::text(trimmed), ..State::named((*single).to_owned()) },
            [first, second, ..] => {
                State { description: Description::Many(vec![trimmed, (*second).to_owned()]), ..State::named((*first).to_owned()) }
            }
            [] => State::named(id.clone()),
        };
        Ok(Read::Statement(Statement::State(state)))
    }

    /// `note left of id: text`, `note right of id … end note`, or a floating note, which mermaid drops.
    fn note(&mut self) -> Result<Statement, Invalid> {
        self.advance();
        let side = match self.peek() {
            Kind::LeftOf => Side::Left,
            Kind::RightOf => Side::Right,
            _ => {
                self.expect(Kind::NoteText)?;
                self.expect(Kind::As)?;
                self.expect(Kind::Id)?;
                return Ok(Statement::Other);
            }
        };
        self.advance();
        let id = self.expect(Kind::Id)?.trim_matches(is_space).to_owned();
        let text = self.expect(Kind::NoteText)?.trim_matches(is_space).to_owned();
        Ok(Statement::State(State { kind: None, note: Some(Note { side, text }), ..State::named(id) }))
    }

    /// `click id "url" "tooltip"` or `click id href "url"`, then a line end.
    fn click(&mut self) -> Result<Statement, Invalid> {
        self.advance();
        self.id_statement()?;
        if self.peek() == Kind::Href {
            self.advance();
        } else {
            self.expect(Kind::String)?;
        }
        self.expect(Kind::String)?;
        self.expect(Kind::Newline)?;
        Ok(Statement::Other)
    }
}

/// Mermaid's `trimColon`: a leading `:` and the spaces around the rest go.
fn trim_colon(text: &str) -> String {
    text.strip_prefix(':').unwrap_or(text).trim_matches(is_space).to_owned()
}

#[cfg(test)]
mod tests {
    use super::super::lexer::tokens;
    use super::*;

    fn read(text: &str) -> Result<Vec<Statement>, Error> {
        parse(text, &tokens(text).map_err(Error::from)?, 10)
    }

    #[test]
    fn a_relation_keeps_both_states_and_its_description() {
        let doc = read("stateDiagram\nA --> B : go\n").unwrap();

        assert_eq!(
            doc,
            [Statement::Relation { from: State::named("A".to_owned()), to: State::named("B".to_owned()), description: "go".to_owned() }]
        );
    }

    #[test]
    fn a_composite_state_holds_its_body() {
        let doc = read("stateDiagram\nstate A {\n  B\n}\n").unwrap();

        assert_eq!(
            doc,
            [Statement::State(State { doc: Some(vec![Statement::State(State::named("B".to_owned()))]), ..State::named("A".to_owned()) })]
        );
    }

    #[test]
    fn a_described_state_splits_its_id_at_a_colon() {
        let doc = read("stateDiagram\nstate \"one\" as A:two\n").unwrap();

        let described = State { description: Description::Many(vec!["one".to_owned(), "two".to_owned()]), ..State::named("A".to_owned()) };
        assert_eq!(doc, [Statement::State(described)]);
    }

    #[test]
    fn dividers_are_named_in_the_order_they_come() {
        let doc = read("stateDiagram\nstate A {\nB\n--\nC\n--\nD\n}\n").unwrap();

        let Statement::State(State { doc: Some(body), .. }) = &doc[0] else { panic!("{doc:?}") };
        let dividers: Vec<&str> = body
            .iter()
            .filter_map(|statement| match statement {
                Statement::State(State { kind: Some(StateKind::Divider), id, .. }) => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(dividers, ["divider-id-1", "divider-id-2"]);
    }

    #[test]
    fn a_chain_of_transitions_is_refused() {
        assert!(matches!(read("stateDiagram\nA --> B --> C\n"), Err(Error::Invalid { .. })));
    }

    #[test]
    fn an_unclosed_composite_state_is_refused() {
        assert!(matches!(read("stateDiagram\nstate A {\nB\n"), Err(Error::Invalid { .. })));
    }

    #[test]
    fn composite_states_opened_past_the_limit_are_refused() {
        let nested = format!("stateDiagram\n{}{}", "state A {\n".repeat(11), "}\n".repeat(11));

        assert_eq!(read(&nested), Err(Error::TooManyNodes { nodes: 11 }));
    }
}
