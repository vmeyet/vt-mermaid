//! Mermaid's sequence grammar (`sequenceDiagram.jison`), read top-down: accepting what its LALR parser accepts, and
//! handing the db the statements its actions build, in the order `apply` gets them.

use super::Invalid;
use super::arrow::Arrow;
use super::db::{Placement, Statement, Text};
use super::lexer::{Block, Kind, Token, line_of};
use crate::source::is_space;

/// The statements of `tokens`, read from `text`, flattened in the order mermaid applies them.
pub(super) fn parse(text: &str, tokens: &[Token]) -> Result<Vec<Statement>, Invalid> {
    let mut parser = Parser { text, tokens, at: 0, statements: Vec::new() };
    parser.diagram()?;
    Ok(parser.statements)
}

#[derive(Debug)]
struct Parser<'a> {
    text: &'a str,
    tokens: &'a [Token],
    at: usize,
    statements: Vec<Statement>,
}

impl Parser<'_> {
    fn peek(&self) -> Kind {
        self.peek_at(0)
    }

    fn peek_at(&self, ahead: usize) -> Kind {
        self.tokens.get(self.at + ahead).map_or(Kind::Eof, |token| token.kind)
    }

    /// The token at hand; the last one is always `Eof`, so reading never runs past it.
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

    /// `start: NEWLINE* SD document`: the blocks a document opens are a stack, so nesting never deepens the call stack.
    fn diagram(&mut self) -> Result<(), Invalid> {
        while self.peek() == Kind::Newline {
            self.advance();
        }
        self.expect(Kind::Header)?;
        let mut open: Vec<Block> = Vec::new();
        loop {
            let innermost = open.last().copied();
            match (self.peek(), innermost) {
                (Kind::Eof, None) => return Ok(()),
                (Kind::End, Some(block)) => {
                    self.advance();
                    open.pop();
                    self.statements.push(Statement::Close { block });
                }
                (Kind::Else, Some(Block::Alt)) | (Kind::And, Some(Block::Par | Block::ParOver)) | (Kind::Option, Some(Block::Critical)) => {
                    self.advance();
                    let text = self.rest_of_line()?;
                    self.statements.push(Statement::Section { block: innermost.unwrap_or(Block::Alt), text });
                }
                (Kind::Block(block), _) => {
                    self.advance();
                    let text = self.rest_of_line()?;
                    self.statements.push(Statement::Open { block, text });
                    open.push(block);
                }
                _ => self.line()?,
            }
        }
    }

    /// A line of a document that neither opens nor closes a block.
    fn line(&mut self) -> Result<(), Invalid> {
        match self.peek() {
            Kind::Newline | Kind::Invalid | Kind::Title | Kind::AccDescrMultilineValue => {
                self.advance();
                Ok(())
            }
            Kind::Participant | Kind::ParticipantActor | Kind::Destroy => self.participant(false),
            Kind::Create => {
                self.advance();
                self.participant(true)
            }
            Kind::Box => self.group(),
            Kind::Link if matches!(self.peek_at(1), Kind::Actor | Kind::Link) => self.actor_link(),
            Kind::Actor | Kind::Link => self.signal(),
            Kind::Autonumber => self.autonumber(),
            Kind::Activate | Kind::Deactivate => self.activation(),
            Kind::Note => self.note(),
            Kind::Links | Kind::Properties | Kind::Details => self.actor_link(),
            Kind::AccTitle => self.advance_then(Kind::AccTitleValue),
            Kind::AccDescr => self.advance_then(Kind::AccDescrValue),
            _ => Err(self.unexpected()),
        }
    }

    fn advance_then(&mut self, kind: Kind) -> Result<(), Invalid> {
        self.advance();
        self.expect(kind).map(|_| ())
    }

    /// `participant`, `actor` or `destroy`, after `create` when `created`.
    fn participant(&mut self, created: bool) -> Result<(), Invalid> {
        let keyword = self.peek();
        self.advance();
        let id = self.expect(Kind::Actor)?.to_owned();
        if keyword == Kind::Destroy {
            self.expect(Kind::Newline)?;
            self.statements.push(Statement::Destroy { id });
            return Ok(());
        }
        let config = if self.peek() == Kind::ConfigStart {
            self.advance();
            let content = self.expect(Kind::ConfigContent)?.trim_matches(is_space).to_owned();
            self.expect(Kind::ConfigEnd)?;
            Some(content)
        } else {
            None
        };
        let description = if self.peek() == Kind::As {
            self.advance();
            Some(self.rest_of_line()?)
        } else {
            None
        };
        self.expect(Kind::Newline)?;
        let figure = Some(if keyword == Kind::ParticipantActor { "actor" } else { "participant" });
        self.statements.push(Statement::Participant { id, figure, description, config, created });
        Ok(())
    }

    /// `box restOfLine box_section end`, a box holding only participants.
    fn group(&mut self) -> Result<(), Invalid> {
        self.advance();
        let text = self.expect(Kind::RestOfLine)?.to_owned();
        self.statements.push(Statement::GroupStart { text });
        loop {
            match self.peek() {
                Kind::Newline => {
                    self.advance();
                }
                Kind::Participant | Kind::ParticipantActor | Kind::Destroy => self.participant(false)?,
                Kind::End => break,
                _ => return Err(self.unexpected()),
            }
        }
        self.advance();
        self.statements.push(Statement::GroupEnd);
        Ok(())
    }

    /// `links`, `link`, `properties` or `details`, an actor and a text: only the actor counts.
    fn actor_link(&mut self) -> Result<(), Invalid> {
        self.advance();
        let id = self.actor()?;
        self.text()?;
        self.expect(Kind::Newline)?;
        self.statements.push(Statement::named(id));
        Ok(())
    }

    /// A message, `+` activating its receiver, `-` deactivating its sender, `()` on either side a central connection.
    fn signal(&mut self) -> Result<(), Invalid> {
        let from = self.actor()?;
        let reverse = self.peek() == Kind::Central;
        if reverse {
            self.advance();
        }
        let arrow = self.arrow()?;
        let mark = self.peek();
        if matches!(mark, Kind::Plus | Kind::Minus | Kind::Central) && !(reverse && mark != Kind::Central) {
            self.advance();
        }
        let to = self.actor()?;
        let text = self.text()?;
        self.expect(Kind::Newline)?;
        let activate = matches!(mark, Kind::Plus | Kind::Central);
        self.statements.extend([Statement::named(from.clone()), Statement::named(to.clone())]);
        self.statements.push(Statement::Message { from: from.clone(), to: to.clone(), text, arrow, activate });
        let after = match mark {
            Kind::Plus => Some(Statement::Activation { id: to, start: true }),
            Kind::Minus => Some(Statement::Activation { id: from.clone(), start: false }),
            Kind::Central => Some(Statement::Central { id: to, reverse: false }),
            _ => None,
        };
        self.statements.extend(after);
        if reverse {
            self.statements.push(Statement::Central { id: from, reverse: true });
        }
        Ok(())
    }

    fn arrow(&mut self) -> Result<Arrow, Invalid> {
        match self.peek() {
            Kind::Arrow(arrow) => {
                self.advance();
                Ok(arrow)
            }
            _ => Err(self.unexpected()),
        }
    }

    /// `autonumber`, then a start and a step, a start, `off` or nothing.
    fn autonumber(&mut self) -> Result<(), Invalid> {
        self.advance();
        let statement = match self.peek() {
            Kind::Num => {
                let start = number(self.advance());
                let step = if self.peek() == Kind::Num { number(self.advance()) } else { 1.0 };
                Statement::Autonumber { start: Some(start), step: Some(step), visible: true }
            }
            Kind::Off => {
                self.advance();
                Statement::Autonumber { start: None, step: None, visible: false }
            }
            _ => Statement::Autonumber { start: None, step: None, visible: true },
        };
        self.expect(Kind::Newline)?;
        self.statements.push(statement);
        Ok(())
    }

    fn activation(&mut self) -> Result<(), Invalid> {
        let start = self.peek() == Kind::Activate;
        self.advance();
        let id = self.expect(Kind::Actor)?.to_owned();
        self.expect(Kind::Newline)?;
        self.statements.push(Statement::Activation { id, start });
        Ok(())
    }

    /// `note left of A`, `note right of A`, `note over A` or `note over A, B`.
    fn note(&mut self) -> Result<(), Invalid> {
        self.advance();
        let placement = match self.peek() {
            Kind::LeftOf => Placement::LeftOf,
            Kind::RightOf => Placement::RightOf,
            Kind::Over => Placement::Over,
            _ => return Err(self.unexpected()),
        };
        self.advance();
        let from = self.actor()?;
        let to = if placement == Placement::Over && self.peek() == Kind::Comma {
            self.advance();
            self.actor()?
        } else {
            from.clone()
        };
        let text = self.text()?;
        self.expect(Kind::Newline)?;
        self.statements.push(Statement::named(from.clone()));
        if to != from {
            self.statements.push(Statement::named(to.clone()));
        }
        self.statements.push(Statement::Note { from, to, placement, text });
        Ok(())
    }

    /// `actor: ACTOR | 'link'`.
    fn actor(&mut self) -> Result<String, Invalid> {
        match self.peek() {
            Kind::Actor | Kind::Link => Ok(self.advance().to_owned()),
            _ => Err(self.unexpected()),
        }
    }

    /// `text2`: the text after a `:`.
    fn text(&mut self) -> Result<Text, Invalid> {
        let written = self.expect(Kind::Text)?;
        let without_colon = written.trim_matches(is_space).get(1..).unwrap_or_default();
        Ok(Text::parsed(without_colon))
    }

    fn rest_of_line(&mut self) -> Result<Text, Invalid> {
        self.expect(Kind::RestOfLine).map(Text::parsed)
    }
}

/// JavaScript's `Number()` of a `NUM` token: digits with at most two decimals.
fn number(written: &str) -> f64 {
    written.parse().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::super::lexer::tokens;
    use super::*;

    fn statements(text: &str) -> Result<Vec<Statement>, Invalid> {
        parse(text, &tokens(text)?)
    }

    fn text(value: &str) -> Text {
        Text { value: value.to_owned(), wrap: None }
    }

    #[test]
    fn a_plus_activates_the_receiver_after_the_message() {
        assert_eq!(
            statements("sequenceDiagram\nA->>+B: hi\n"),
            Ok(vec![
                Statement::named("A".to_owned()),
                Statement::named("B".to_owned()),
                Statement::Message { from: "A".to_owned(), to: "B".to_owned(), text: text("hi"), arrow: Arrow::Solid, activate: true },
                Statement::Activation { id: "B".to_owned(), start: true },
            ])
        );
    }

    #[test]
    fn central_connections_follow_the_message_receiver_first() {
        let read = statements("sequenceDiagram\nA()->>()B: hi\n").unwrap();

        assert_eq!(
            read[3..],
            [Statement::Central { id: "B".to_owned(), reverse: false }, Statement::Central { id: "A".to_owned(), reverse: true }]
        );
    }

    #[test]
    fn blocks_close_innermost_first_and_sections_only_divide_their_own_block() {
        let read = statements("sequenceDiagram\nalt yes\nloop again\nend\nelse no\nend\n").unwrap();
        let blocks: Vec<&Statement> = read.iter().filter(|statement| !matches!(statement, Statement::Message { .. })).collect();

        assert_eq!(blocks.len(), 5);
        assert_eq!(*blocks[2], Statement::Close { block: Block::Loop });
        assert!(statements("sequenceDiagram\nalt yes\nloop again\nelse no\nend\nend\n").is_err());
    }

    #[test]
    fn a_block_left_open_or_an_end_too_many_is_refused() {
        assert!(statements("sequenceDiagram\nloop\n").is_err());
        assert!(statements("sequenceDiagram\nend\n").is_err());
    }

    #[test]
    fn a_note_over_two_participants_names_both() {
        let read = statements("sequenceDiagram\nNote over A,B: both\n").unwrap();

        assert_eq!(read[2], Statement::Note { from: "A".to_owned(), to: "B".to_owned(), placement: Placement::Over, text: text("both") });
    }

    #[test]
    fn a_message_needs_a_text() {
        assert!(matches!(statements("sequenceDiagram\nA->>B\n"), Err(Invalid::Unexpected { line: 2, .. })));
    }

    #[test]
    fn a_box_holds_only_participants() {
        assert!(statements("sequenceDiagram\nbox Group\nparticipant A\nend\n").is_ok());
        assert!(statements("sequenceDiagram\nbox Group\nA->>B: hi\nend\n").is_err());
    }

    #[test]
    fn autonumber_reads_its_start_and_step() {
        assert_eq!(
            statements("sequenceDiagram\nautonumber 10 5\n"),
            Ok(vec![Statement::Autonumber { start: Some(10.0), step: Some(5.0), visible: true }])
        );
    }
}
