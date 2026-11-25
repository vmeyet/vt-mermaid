//! Mermaid's ER grammar (`erDiagram.jison`), read top-down: accepting what its LALR parser accepts, and calling the db
//! in the order its actions do.

use super::Invalid;
use super::db::{Attribute, Db, Ends, Held};
use super::lexer::{Kind, Token, line_of};
use crate::error::Error;
use crate::source::is_space;

/// Reads `tokens` from `text` into `db`.
pub(super) fn parse(text: &str, tokens: &[Token], db: Db) -> Result<Db, Error> {
    let mut parser = Parser { text, tokens, at: 0, db, depth: 0 };
    parser.diagram()?;
    Ok(parser.db)
}

#[derive(Debug)]
struct Parser<'a> {
    text: &'a str,
    tokens: &'a [Token],
    at: usize,
    db: Db,
    /// Subgraphs open around the statement being read.
    depth: usize,
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

    /// `ER_DIAGRAM document EOF`.
    fn diagram(&mut self) -> Result<(), Error> {
        self.expect(Kind::ErDiagram)?;
        self.document()?;
        self.expect(Kind::Eof)?;
        self.expect(Kind::End)?;
        Ok(())
    }

    /// Statements one after the other, up to the end of the text, or to `end` inside a subgraph; what each hands a
    /// subgraph, in order.
    fn document(&mut self) -> Result<Vec<Held>, Error> {
        let mut held = Vec::new();
        loop {
            match self.peek() {
                Kind::Newline => {
                    self.advance();
                }
                Kind::Eof if self.depth == 0 => return Ok(held),
                Kind::SubgraphEnd if self.depth > 0 => return Ok(held),
                _ => held.extend(self.statement()?),
            }
        }
    }

    fn statement(&mut self) -> Result<Vec<Held>, Error> {
        let name = |text: &str| vec![Held::Name(text.to_owned())];
        match self.peek() {
            Kind::Direction(direction) => {
                self.advance();
                if self.depth == 0 {
                    self.db.set_direction(direction);
                    return Ok(Vec::new());
                }
                Ok(vec![Held::Direction(direction)])
            }
            Kind::AccTitle => {
                self.advance();
                Ok(name(self.expect(Kind::AccTitleValue)?.trim_matches(is_space)))
            }
            Kind::AccDescr => {
                self.advance();
                Ok(name(self.expect(Kind::AccDescrValue)?.trim_matches(is_space)))
            }
            Kind::AccDescrMultilineValue => Ok(name(self.advance().trim_matches(is_space))),
            Kind::ClassDef | Kind::Style => {
                let keyword = self.advance();
                self.id_list()?;
                self.styles()?;
                self.separator()?;
                Ok(name(keyword))
            }
            Kind::Class => {
                let keyword = self.advance();
                self.id_list()?;
                self.id_list()?;
                Ok(name(keyword))
            }
            Kind::Subgraph => self.subgraph().map(|id| name(&id)),
            _ => self.entity_statement(),
        }
    }

    /// `subgraphHeader document END`.
    fn subgraph(&mut self) -> Result<String, Error> {
        self.advance();
        let id = self.entity_name()?;
        let title = if self.take(Kind::Sqs).is_some() {
            let mut title = self.entity_name()?;
            while starts_entity_name(self.peek()) {
                title.push(' ');
                title.push_str(&self.entity_name()?);
            }
            self.expect(Kind::Sqe)?;
            title
        } else {
            id.clone()
        };
        self.separator()?;
        self.depth += 1;
        let held = self.document()?;
        self.expect(Kind::SubgraphEnd)?;
        self.depth -= 1;
        self.db.add_subgraph(&id, &held, &title)
    }

    /// A statement opening with an entity's name: an entity alone, with an alias, a style class or attributes, or a
    /// relationship to another.
    fn entity_statement(&mut self) -> Result<Vec<Held>, Error> {
        let name = self.entity_name()?;
        if self.take(Kind::Sqs).is_some() {
            let alias = self.entity_name()?;
            self.expect(Kind::Sqe)?;
            self.style_class()?;
            let attributes = self.block()?;
            self.db.add_entity(&name, &alias)?;
            if let Some(attributes) = attributes {
                self.db.add_attributes(&name, attributes)?;
            }
            return Ok(vec![Held::Name(name)]);
        }
        self.style_class()?;
        if matches!(self.peek(), Kind::Cardinality(_)) {
            return self.relationship(name);
        }
        let attributes = self.block()?;
        self.db.add_entity(&name, "")?;
        if let Some(attributes) = attributes {
            self.db.add_attributes(&name, attributes)?;
        }
        Ok(vec![Held::Name(name)])
    }

    /// `relSpec entityName (STYLE_SEPARATOR idList)? COLON role`.
    fn relationship(&mut self, from: String) -> Result<Vec<Held>, Error> {
        let before = self.cardinality()?;
        let identifying = match self.peek() {
            Kind::Identifying => true,
            Kind::NonIdentifying => false,
            _ => return Err(self.unexpected().into()),
        };
        self.advance();
        let after = self.cardinality()?;
        let to = self.entity_name()?;
        self.style_class()?;
        self.expect(Kind::Colon)?;
        let role = match self.peek() {
            Kind::Word | Kind::EntityName => self.advance().replace('"', ""),
            Kind::UnicodeText => self.advance().to_owned(),
            _ => return Err(self.unexpected().into()),
        };
        self.db.add_entity(&from, "")?;
        self.db.add_entity(&to, "")?;
        self.db.add_relationship(&from, &role, &to, Ends { before, after, identifying })?;
        Ok(vec![Held::Name(from), Held::Name(to)])
    }

    fn cardinality(&mut self) -> Result<super::lexer::Cardinality, Invalid> {
        match self.peek() {
            Kind::Cardinality(cardinality) => {
                self.advance();
                Ok(cardinality)
            }
            _ => Err(self.unexpected()),
        }
    }

    /// `STYLE_SEPARATOR idList`, when there.
    fn style_class(&mut self) -> Result<(), Invalid> {
        if self.take(Kind::StyleSeparator).is_some() {
            self.id_list()?;
        }
        Ok(())
    }

    /// `BLOCK_START attributes? BLOCK_STOP`, when there.
    fn block(&mut self) -> Result<Option<Vec<Attribute>>, Invalid> {
        if self.take(Kind::BlockStart).is_none() {
            return Ok(None);
        }
        let mut attributes = Vec::new();
        while self.take(Kind::BlockStop).is_none() {
            attributes.push(self.attribute()?);
        }
        Ok(Some(attributes))
    }

    /// `attributeType attributeName attributeKeyTypeList? attributeComment?`.
    fn attribute(&mut self) -> Result<Attribute, Invalid> {
        let mut kind = self.expect(Kind::AttributeWord)?.to_owned();
        if self.take(Kind::Char('?')).is_some() {
            kind.push('?');
        }
        let name = self.expect(Kind::AttributeWord)?.to_owned();
        let mut keys = Vec::new();
        if let Some(key) = self.take(Kind::AttributeKey) {
            keys.push(key.to_owned());
            while self.take(Kind::Char(',')).is_some() {
                keys.push(self.expect(Kind::AttributeKey)?.to_owned());
            }
        }
        let comment = self.take(Kind::Comment).map(|comment| comment.replace('"', "")).unwrap_or_default();
        Ok(Attribute { kind, name, keys, comment })
    }

    /// `entityName`: a name in quotes, a word or a number.
    fn entity_name(&mut self) -> Result<String, Invalid> {
        match self.peek() {
            Kind::EntityName => Ok(self.advance().replace('"', "")),
            kind if starts_entity_name(kind) => Ok(self.advance().to_owned()),
            _ => Err(self.unexpected()),
        }
    }

    /// `idList`: names apart by commas.
    fn id_list(&mut self) -> Result<(), Invalid> {
        loop {
            if !matches!(self.peek(), Kind::UnicodeText | Kind::StyleText) {
                return Err(self.unexpected());
            }
            self.advance();
            if self.take(Kind::Comma).is_none() {
                return Ok(());
            }
        }
    }

    /// `stylesOpt`: styles apart by commas, each one or more pieces.
    fn styles(&mut self) -> Result<(), Invalid> {
        let is_piece = |kind: Kind| matches!(kind, Kind::StyleText | Kind::Num | Kind::Colon | Kind::Brkt);
        loop {
            if !is_piece(self.peek()) {
                return Err(self.unexpected());
            }
            while is_piece(self.peek()) {
                self.advance();
            }
            if self.take(Kind::Comma).is_none() {
                return Ok(());
            }
        }
    }

    /// `SEMI`, `NEWLINE` or `EOF`.
    fn separator(&mut self) -> Result<(), Invalid> {
        if matches!(self.peek(), Kind::Semi | Kind::Newline | Kind::Eof) {
            self.advance();
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }
}

fn starts_entity_name(kind: Kind) -> bool {
    matches!(kind, Kind::EntityName | Kind::UnicodeText | Kind::Num | Kind::DecimalNum | Kind::EntityOne)
}

#[cfg(test)]
mod tests {
    use super::super::{Limits, parse as read};

    #[test]
    fn a_relationship_reads_both_cardinalities_its_line_and_its_role() {
        let diagram = read("erDiagram\n  CUSTOMER |o..|{ ORDER : \"places many\"\n", Limits::DRAWN).unwrap();

        let relationship = &diagram.relationships[0];
        assert_eq!((relationship.from.as_str(), relationship.to.as_str()), ("entity-CUSTOMER-0", "entity-ORDER-1"));
        assert_eq!((relationship.role.as_str(), relationship.identifying), ("places many", false));
    }

    #[test]
    fn statements_may_share_a_line() {
        let diagram = read("erDiagram\n  A B\n", Limits::DRAWN).unwrap();

        assert_eq!(diagram.entities.len(), 2);
    }

    #[test]
    fn an_entity_reads_its_alias_and_attributes() {
        let diagram = read("erDiagram\n  p[Person] {\n    string name PK \"full\"\n  }\n", Limits::DRAWN).unwrap();

        let entity = &diagram.entities[0];
        assert_eq!((entity.name.as_str(), entity.alias.as_str()), ("p", "Person"));
        assert_eq!((entity.attributes[0].keys.as_slice(), entity.attributes[0].comment.as_str()), (["PK".to_owned()].as_slice(), "full"));
    }
}
