//! Mermaid's class grammar (`classDiagram.jison`), read top-down: accepting what its LALR parser accepts, and calling
//! the db in the order its actions do.

use super::Invalid;
use super::db::{Db, Written};
use super::lexer::{Kind, Token, line_of};
use crate::error::Error;
use crate::source::is_space;

/// Reads `tokens` from `text` into `db`.
pub(super) fn parse(text: &str, tokens: &[Token], db: Db) -> Result<Db, Error> {
    let mut parser = Parser { text, tokens, at: 0, db };
    parser.diagram()?;
    Ok(parser.db)
}

#[derive(Debug)]
struct Parser<'a> {
    text: &'a str,
    tokens: &'a [Token],
    at: usize,
    db: Db,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Kind {
        self.tokens.get(self.at).map_or(Kind::End, |token| token.kind)
    }

    /// The token at hand; the last one is always `End`, so reading never runs past it.
    fn advance(&mut self) -> &'a str {
        let token = self.tokens.get(self.at).copied().unwrap_or(Token { kind: Kind::End, start: self.text.len(), end: self.text.len() });
        self.at = (self.at + 1).min(self.tokens.len().saturating_sub(1));
        &self.text[token.start..token.end]
    }

    fn expect(&mut self, kind: Kind) -> Result<&'a str, Invalid> {
        if self.peek() == kind { Ok(self.advance()) } else { Err(self.unexpected()) }
    }

    fn take(&mut self, kind: Kind) -> Option<&'a str> {
        (self.peek() == kind).then(|| self.advance())
    }

    fn unexpected(&self) -> Invalid {
        let token = self.tokens.get(self.at).copied().unwrap_or(Token { kind: Kind::End, start: self.text.len(), end: self.text.len() });
        let found = match token.kind {
            Kind::End | Kind::Eof => "end of text".to_owned(),
            _ => format!("{:?}", &self.text[token.start..token.end]),
        };
        Invalid::Unexpected { line: line_of(self.text, token.start), found }
    }

    /// `CLASS_DIAGRAM NEWLINE statements EOF`: statements apart by line ends.
    fn diagram(&mut self) -> Result<(), Error> {
        self.expect(Kind::ClassDiagram)?;
        self.expect(Kind::Newline)?;
        loop {
            self.statement()?;
            if self.take(Kind::Newline).is_none() || self.peek() == Kind::Eof {
                break;
            }
        }
        self.expect(Kind::Eof)?;
        self.expect(Kind::End)?;
        Ok(())
    }

    fn statement(&mut self) -> Result<(), Error> {
        match self.peek() {
            Kind::Direction(direction) => {
                self.advance();
                self.db.set_direction(direction);
            }
            Kind::AccTitle => {
                self.advance();
                self.expect(Kind::AccTitleValue)?;
            }
            Kind::AccDescr => {
                self.advance();
                self.expect(Kind::AccDescrValue)?;
            }
            Kind::AccDescrMultilineValue | Kind::Member => {
                self.advance();
            }
            Kind::Namespace => self.namespace()?,
            Kind::Class => {
                self.class()?;
            }
            Kind::AnnotationStart => {
                self.advance();
                let annotation = self.alpha_num()?;
                self.expect(Kind::AnnotationEnd)?;
                let class = self.class_name()?;
                self.db.add_annotation(&class, annotation)?;
            }
            Kind::Callback | Kind::Link | Kind::Click => self.click()?,
            Kind::Style => {
                self.advance();
                self.expect(Kind::Alpha)?;
                self.styles()?;
            }
            Kind::CssClass => {
                self.advance();
                self.expect(Kind::Str)?;
                self.expect(Kind::Alpha)?;
            }
            Kind::NoteFor | Kind::Note => {
                self.note()?;
            }
            Kind::ClassDef => {
                self.advance();
                self.expect(Kind::Alpha)?;
                while self.take(Kind::Comma).is_some() {
                    self.expect(Kind::Alpha)?;
                }
                self.styles()?;
            }
            kind if starts_class_name(kind) => self.relation_or_member()?,
            _ => return Err(self.unexpected().into()),
        }
        Ok(())
    }

    /// A class named alone, with a member after `:`, or a relation to another.
    fn relation_or_member(&mut self) -> Result<(), Error> {
        let from = self.class_name()?;
        if let Some(member) = self.take(Kind::Label) {
            return self.db.add_member(&from, &cleaned_label(member));
        }
        let from_text = self.take(Kind::Str).map(str::to_owned);
        if from_text.is_none() && !starts_relation(self.peek()) {
            return Ok(());
        }
        let (from_end, dotted, to_end) = self.relation()?;
        let to_text = self.take(Kind::Str).map(str::to_owned);
        let to = self.class_name()?;
        let label = self.take(Kind::Label).map(cleaned_label);
        self.db.add_relation(Written { from, to, from_end, to_end, dotted, from_text, to_text, label })
    }

    /// `relationType? lineType relationType?`.
    fn relation(&mut self) -> Result<(Option<super::lexer::End>, bool, Option<super::lexer::End>), Invalid> {
        let end = |parser: &mut Self| match parser.peek() {
            Kind::Relation(end) => {
                parser.advance();
                Some(end)
            }
            _ => None,
        };
        let from_end = end(self);
        let dotted = match self.peek() {
            Kind::Line => false,
            Kind::DottedLine => true,
            _ => return Err(self.unexpected()),
        };
        self.advance();
        Ok((from_end, dotted, end(self)))
    }

    /// `className`: names, numbers and dashes, run together or joined by dots, ending with a backquoted name or a
    /// generic type.
    fn class_name(&mut self) -> Result<String, Invalid> {
        let mut name = String::new();
        loop {
            if let Some(quoted) = self.take(Kind::BquoteStr) {
                name.push_str(quoted);
                self.generic_into(&mut name);
                return Ok(name);
            }
            name.push_str(self.alpha_num()?);
            match self.peek() {
                Kind::Dot => {
                    self.advance();
                    name.push('.');
                }
                Kind::GenericType => {
                    self.generic_into(&mut name);
                    return Ok(name);
                }
                kind if starts_class_name(kind) => {}
                _ => return Ok(name),
            }
        }
    }

    fn generic_into(&mut self, name: &mut String) {
        if let Some(generic) = self.take(Kind::GenericType) {
            name.push('~');
            name.push_str(generic);
            name.push('~');
        }
    }

    fn alpha_num(&mut self) -> Result<&'a str, Invalid> {
        if matches!(self.peek(), Kind::UnicodeText | Kind::Alpha | Kind::Minus) { Ok(self.advance()) } else { Err(self.unexpected()) }
    }

    /// `classIdentifier`, then a style class, members or an annotation; its value is the class's name.
    fn class(&mut self) -> Result<String, Error> {
        self.advance();
        let name = self.class_name()?;
        self.db.add_class(&name)?;
        if let Some(label) = self.class_label()? {
            self.db.set_class_label(&name, label);
        }
        match self.peek() {
            Kind::StyleSeparator => {
                self.advance();
                self.alpha_num()?;
                if self.take(Kind::StructStart).is_some() {
                    let members = self.members()?;
                    self.expect(Kind::StructStop)?;
                    self.db.add_members(&name, &members)?;
                }
            }
            Kind::StructStart => {
                self.advance();
                let members = if self.peek() == Kind::StructStop { Vec::new() } else { self.members()? };
                self.expect(Kind::StructStop)?;
                self.db.add_members(&name, &members)?;
            }
            Kind::AnnotationStart => {
                self.advance();
                let annotation = self.alpha_num()?;
                self.expect(Kind::AnnotationEnd)?;
                if self.take(Kind::StructStart).is_some() {
                    let members = if self.peek() == Kind::StructStop { Vec::new() } else { self.members()? };
                    self.expect(Kind::StructStop)?;
                    self.db.add_annotation(&name, annotation)?;
                    self.db.add_members(&name, &members)?;
                } else {
                    self.db.add_annotation(&name, annotation)?;
                }
            }
            _ => {}
        }
        Ok(name)
    }

    /// `SQS STR SQE`.
    fn class_label(&mut self) -> Result<Option<&'a str>, Invalid> {
        if self.take(Kind::Sqs).is_none() {
            return Ok(None);
        }
        let label = self.expect(Kind::Str)?;
        self.expect(Kind::Sqe)?;
        Ok(Some(label))
    }

    fn members(&mut self) -> Result<Vec<&'a str>, Invalid> {
        let mut members = vec![self.expect(Kind::Member)?];
        while let Some(member) = self.take(Kind::Member) {
            members.push(member);
        }
        Ok(members)
    }

    /// `NAMESPACE namespaceName classLabel? STRUCT_START NEWLINE? classStatements STRUCT_STOP`: the classes and notes
    /// it holds directly move into it once it is read.
    fn namespace(&mut self) -> Result<(), Error> {
        self.advance();
        let name = self.namespace_name()?;
        let label = self.class_label()?;
        let id = self.db.add_namespace(&name, label)?;
        self.expect(Kind::StructStart)?;
        self.take(Kind::Newline);
        let (mut classes, mut notes) = (Vec::new(), Vec::new());
        loop {
            match self.peek() {
                Kind::Class => classes.push(self.class()?),
                Kind::NoteFor | Kind::Note => notes.push(self.note()?),
                Kind::Namespace => self.namespace()?,
                _ => return Err(self.unexpected().into()),
            }
            if self.take(Kind::Newline).is_none() || self.peek() == Kind::StructStop {
                break;
            }
        }
        self.expect(Kind::StructStop)?;
        self.db.add_to_namespace(&id, &classes, &notes)?;
        self.db.pop_namespace();
        Ok(())
    }

    fn namespace_name(&mut self) -> Result<String, Invalid> {
        let mut name = String::new();
        loop {
            if let Some(quoted) = self.take(Kind::BquoteStr) {
                name.push_str(quoted);
                return Ok(name);
            }
            name.push_str(self.alpha_num()?);
            match self.peek() {
                Kind::Dot => {
                    self.advance();
                    name.push('.');
                }
                kind if starts_class_name(kind) => {}
                _ => return Ok(name),
            }
        }
    }

    /// `note for className "text"` or `note "text"`; its value is the note's id.
    fn note(&mut self) -> Result<String, Error> {
        let class = if self.advance() == "note" { None } else { Some(self.class_name()?) };
        let text = self.expect(Kind::Str)?;
        self.db.add_note(text, class.as_deref())
    }

    /// `callback`, `link` and `click`, parsed and ignored.
    fn click(&mut self) -> Result<(), Invalid> {
        let keyword = self.peek();
        self.advance();
        self.class_name()?;
        match keyword {
            Kind::Callback => {
                self.expect(Kind::Str)?;
                self.take(Kind::Str);
            }
            Kind::Link => self.link()?,
            _ if self.take(Kind::Href).is_some() => self.link()?,
            _ => {
                self.expect(Kind::CallbackName)?;
                self.take(Kind::CallbackArgs);
                self.take(Kind::Str);
            }
        }
        Ok(())
    }

    /// `STR STR? LINK_TARGET?`.
    fn link(&mut self) -> Result<(), Invalid> {
        self.expect(Kind::Str)?;
        self.take(Kind::Str);
        self.take(Kind::LinkTarget);
        Ok(())
    }

    /// `stylesOpt`: styles apart by commas, each one or more pieces.
    fn styles(&mut self) -> Result<(), Invalid> {
        loop {
            if !is_style_piece(self.peek()) {
                return Err(self.unexpected());
            }
            while is_style_piece(self.peek()) {
                self.advance();
            }
            if self.take(Kind::Comma).is_none() {
                return Ok(());
            }
        }
    }
}

fn starts_class_name(kind: Kind) -> bool {
    matches!(kind, Kind::UnicodeText | Kind::Alpha | Kind::Minus | Kind::BquoteStr)
}

fn starts_relation(kind: Kind) -> bool {
    matches!(kind, Kind::Relation(_) | Kind::Line | Kind::DottedLine)
}

fn is_style_piece(kind: Kind) -> bool {
    matches!(kind, Kind::Alpha | Kind::Colon | Kind::Space | Kind::Brkt | Kind::Style | Kind::Pct | Kind::Label)
}

/// `cleanupLabel`: the text after `:`, trimmed.
fn cleaned_label(label: &str) -> String {
    label.strip_prefix(':').unwrap_or(label).trim_matches(is_space).to_owned()
}

#[cfg(test)]
mod tests {
    use super::super::{Limits, parse as read};

    #[test]
    fn a_relation_reads_both_ends_their_texts_and_its_label() {
        let diagram = read("classDiagram\n  Customer \"1\" --> \"*\" Ticket : buys\n", Limits::DRAWN).unwrap();

        let relation = &diagram.relations[0];
        assert_eq!((relation.from.as_str(), relation.to.as_str()), ("Customer", "Ticket"));
        assert_eq!((relation.from_text.as_str(), relation.to_text.as_str(), relation.label.as_deref()), ("1", "*", Some("buys")));
    }

    #[test]
    fn a_class_named_alone_adds_nothing() {
        let diagram = read("classDiagram\n  A\n", Limits::DRAWN).unwrap();

        assert_eq!(diagram.classes.len(), 0);
    }

    #[test]
    fn a_namespace_holds_the_classes_read_inside_it() {
        let diagram = read("classDiagram\nnamespace Shapes {\n  class Square\n  class Circle\n}\n", Limits::DRAWN).unwrap();

        assert_eq!(diagram.namespaces[0].classes, ["Square", "Circle"]);
        assert_eq!(diagram.classes[0].namespace.as_deref(), Some("Shapes"));
    }

    #[test]
    fn statements_on_one_line_are_refused() {
        assert!(read("classDiagram\n  A --> B C --> D\n", Limits::DRAWN).is_err());
    }
}
