//! Mermaid's flowchart grammar (`flow.jison`), read top-down: each rule a method, accepting what its LALR parser
//! accepts and calling the db in the order its actions do.

use super::db::{Db, Flowchart, Item, LinkData, Text, TextKind};
use super::lexer::{Kind, Token, line_of};
use super::{Direction, Invalid, Limits, link};
use crate::error::Error;

/// Tokens a node id is made of.
const ID: [Kind; 11] = [
    Kind::Num,
    Kind::NodeString,
    Kind::Down,
    Kind::Minus,
    Kind::Default,
    Kind::Comma,
    Kind::Colon,
    Kind::Amp,
    Kind::Brkt,
    Kind::Mult,
    Kind::UnicodeText,
];
/// Tokens a label in brackets is made of.
const TEXT: [Kind; 4] = [Kind::Text, Kind::TagStart, Kind::TagEnd, Kind::UnicodeText];
const LINKS: [Kind; 3] = [Kind::Link, Kind::StartLink, Kind::LinkId];

/// A bracket shape: the tokens before the label, then each closing and mermaid's name for the shape it makes.
#[derive(Debug)]
struct Bracket {
    opening: &'static [Kind],
    closings: &'static [(&'static [Kind], &'static str)],
}

const BRACKETS: [Bracket; 13] = [
    Bracket { opening: &[Kind::Sqs], closings: &[(&[Kind::Sqe], "square")] },
    Bracket { opening: &[Kind::DoubleCircleStart], closings: &[(&[Kind::DoubleCircleEnd], "doublecircle")] },
    Bracket { opening: &[Kind::Ps, Kind::Ps], closings: &[(&[Kind::Pe, Kind::Pe], "circle")] },
    Bracket { opening: &[Kind::EllipseStart], closings: &[(&[Kind::EllipseEnd], "ellipse")] },
    Bracket { opening: &[Kind::StadiumStart], closings: &[(&[Kind::StadiumEnd], "stadium")] },
    Bracket { opening: &[Kind::SubroutineStart], closings: &[(&[Kind::SubroutineEnd], "subroutine")] },
    Bracket { opening: &[Kind::CylinderStart], closings: &[(&[Kind::CylinderEnd], "cylinder")] },
    Bracket { opening: &[Kind::Ps], closings: &[(&[Kind::Pe], "round")] },
    Bracket { opening: &[Kind::DiamondStart, Kind::DiamondStart], closings: &[(&[Kind::DiamondStop, Kind::DiamondStop], "hexagon")] },
    Bracket { opening: &[Kind::DiamondStart], closings: &[(&[Kind::DiamondStop], "diamond")] },
    Bracket { opening: &[Kind::TagEnd], closings: &[(&[Kind::Sqe], "odd")] },
    Bracket { opening: &[Kind::TrapStart], closings: &[(&[Kind::TrapEnd], "trapezoid"), (&[Kind::InvTrapEnd], "lean_right")] },
    Bracket { opening: &[Kind::InvTrapStart], closings: &[(&[Kind::InvTrapEnd], "inv_trapezoid"), (&[Kind::TrapEnd], "lean_left")] },
];

/// Reads `tokens` of `text` into the db; with `inherits_direction`, a subgraph without its own direction takes the
/// flowchart's.
pub(super) fn parse(text: &str, tokens: &[Token], limits: Limits, inherits_direction: bool) -> Result<Flowchart, Error> {
    let mut parser = Parser { text, tokens, at: 0, db: Db::new(limits, inherits_direction) };
    parser.header()?;
    parser.document()?;
    Ok(parser.db.into_flowchart())
}

#[derive(Debug)]
struct Parser<'a> {
    text: &'a str,
    tokens: &'a [Token],
    at: usize,
    db: Db,
}

impl Parser<'_> {
    fn peek(&self) -> Kind {
        self.peek_at(0)
    }

    /// Whether the tokens at hand are `kinds`.
    fn ahead(&self, kinds: &[Kind]) -> bool {
        kinds.iter().enumerate().all(|(ahead, &kind)| self.peek_at(ahead) == kind)
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

    fn expect(&mut self, kind: Kind) -> Result<&str, Error> {
        if self.peek() == kind { Ok(self.advance()) } else { Err(self.unexpected()) }
    }

    fn unexpected(&self) -> Error {
        let token = self.tokens.get(self.at).copied().unwrap_or(Token { kind: Kind::Eof, start: self.text.len(), end: self.text.len() });
        let found = match token.kind {
            Kind::Eof => "the end".to_owned(),
            Kind::Newline => "a line end".to_owned(),
            _ => format!("`{}`", &self.text[token.start..token.end]),
        };
        Invalid::Unexpected { line: line_of(self.text, token.start), found }.into()
    }

    /// The text of the tokens of `kinds` at hand, at least one.
    fn joined(&mut self, kinds: &[Kind]) -> Result<String, Error> {
        if !kinds.contains(&self.peek()) {
            return Err(self.unexpected());
        }
        let mut joined = String::new();
        while kinds.contains(&self.peek()) {
            joined.push_str(self.advance());
        }
        Ok(joined)
    }

    fn spaces(&mut self) -> bool {
        let spaced = self.peek() == Kind::Space;
        while self.peek() == Kind::Space {
            self.advance();
        }
        spaced
    }

    /// `graphConfig`: the header and its direction, or a line end without one.
    fn header(&mut self) -> Result<(), Error> {
        while matches!(self.peek(), Kind::Space | Kind::Newline) {
            self.advance();
        }
        self.expect(Kind::Graph)?;
        if self.peek() == Kind::NoDir {
            self.advance();
            return Ok(());
        }
        let direction = header_direction(self.expect(Kind::Dir)?);
        self.db.set_direction(direction);
        match self.peek() {
            Kind::Semi | Kind::Newline => {
                self.advance();
                Ok(())
            }
            Kind::Space => {
                self.spaces();
                self.expect(Kind::Newline).map(|_| ())
            }
            _ => Err(self.unexpected()),
        }
    }

    /// `document`: statements up to the end.
    fn document(&mut self) -> Result<(), Error> {
        let mut items: Vec<Item> = Vec::new();
        loop {
            match self.peek() {
                Kind::Eof => return Ok(()),
                Kind::Newline | Kind::Space | Kind::Semi => {
                    let name = self.advance().to_owned();
                    items.push(Item::Name(name));
                }
                Kind::AccTitle | Kind::AccDescr => {
                    self.advance();
                    let value = self.joined(&[Kind::AccTitleValue, Kind::AccDescrValue])?;
                    items.push(Item::Name(value.trim_matches(super::is_space).to_owned()));
                }
                Kind::AccDescrMultilineValue => {
                    let value = self.advance().trim_matches(super::is_space).to_owned();
                    items.push(Item::Name(value));
                }
                Kind::Direction(direction) => {
                    self.advance();
                    items.push(Item::Direction(direction));
                }
                _ => {
                    let nodes = self.vertex_statement()?;
                    self.separator()?;
                    items.push(Item::Nodes(nodes));
                }
            }
        }
    }

    /// A line end or `;` ends a statement, and so does the end of the text, left for the document to read.
    fn separator(&mut self) -> Result<(), Error> {
        match self.peek() {
            Kind::Newline | Kind::Semi => {
                self.advance();
                Ok(())
            }
            Kind::Eof => Ok(()),
            _ => Err(self.unexpected()),
        }
    }

    /// `vertexStatement`: nodes joined by links; its nodes listed the way mermaid lists them, the last group first.
    fn vertex_statement(&mut self) -> Result<Vec<String>, Error> {
        let mut group = self.node()?;
        self.trailing_shape_data(&group)?;
        self.spaces();
        let mut nodes = group.clone();
        while LINKS.contains(&self.peek()) {
            let link = self.link()?;
            let next = self.node()?;
            self.trailing_shape_data(&next)?;
            self.db.add_link(&group, &next, &link)?;
            self.spaces();
            nodes = next.iter().cloned().chain(nodes).collect();
            group = next;
        }
        Ok(nodes)
    }

    /// `node`: vertices joined by ` & `, a block before the `&` going to the vertex before it.
    fn node(&mut self) -> Result<Vec<String>, Error> {
        let mut nodes = vec![self.styled_vertex()?];
        loop {
            let before = self.at;
            let block = self.shape_data();
            if !(self.spaces() && self.peek() == Kind::Amp) {
                self.at = before;
                return Ok(nodes);
            }
            self.advance();
            if !self.spaces() {
                return Err(self.unexpected());
            }
            let next = self.styled_vertex()?;
            if let (Some(block), Some(last)) = (block, nodes.last()) {
                self.db.add_vertex(last, None, None, Some(&block))?;
            }
            nodes.push(next);
        }
    }

    fn trailing_shape_data(&mut self, nodes: &[String]) -> Result<(), Error> {
        match (self.shape_data(), nodes.last()) {
            (Some(block), Some(last)) => self.db.add_vertex(last, None, None, Some(&block)),
            _ => Ok(()),
        }
    }

    /// `shapeData`: the text of a block, quotes kept, line ends inside quotes as `<br/>`.
    fn shape_data(&mut self) -> Option<String> {
        let mut block: Option<String> = None;
        loop {
            match self.peek() {
                Kind::ShapeData => {
                    let part = self.advance();
                    let part = if part == "@{" { "" } else { part }.to_owned();
                    block.get_or_insert_default().push_str(&part);
                }
                Kind::ShapeDataString => {
                    let part = self.advance().to_owned();
                    block.get_or_insert_default().push_str(&with_line_breaks(&part));
                }
                _ => return block,
            }
        }
    }

    /// `styledVertex`: a vertex and the class after `:::`.
    fn styled_vertex(&mut self) -> Result<String, Error> {
        let id = self.vertex()?;
        if self.peek() == Kind::StyleSeparator {
            self.advance();
            let class = self.joined(&ID)?;
            self.db.set_class(&id, &class);
        }
        Ok(id)
    }

    /// `vertex`: an id and the label in the brackets of its shape.
    fn vertex(&mut self) -> Result<String, Error> {
        let id = self.joined(&ID)?;
        if self.peek() == Kind::VertexWithPropsStart {
            self.advance();
            self.expect(Kind::NodeString)?;
            self.expect(Kind::Colon)?;
            self.expect(Kind::NodeString)?;
            self.expect(Kind::Pipe)?;
            let text = self.text()?;
            self.expect(Kind::Sqe)?;
            self.db.add_vertex(&id, Some(text), Some("rect"), None)?;
            return Ok(id);
        }
        let Some(bracket) = BRACKETS.iter().find(|bracket| self.ahead(bracket.opening)) else {
            self.db.add_vertex(&id, None, None, None)?;
            return Ok(id);
        };
        self.at += bracket.opening.len();
        let text = self.text()?;
        let &(closing, name) = bracket.closings.iter().find(|(closing, _)| self.ahead(closing)).ok_or_else(|| self.unexpected())?;
        self.at += closing.len();
        self.db.add_vertex(&id, Some(text), Some(name), None)?;
        Ok(id)
    }

    /// `text`: one quoted or Markdown string, or plain text.
    fn text(&mut self) -> Result<Text, Error> {
        match self.peek() {
            Kind::Str => Ok(Text { value: self.advance().to_owned(), kind: TextKind::String }),
            Kind::MdStr => Ok(Text { value: self.advance().to_owned(), kind: TextKind::Markdown }),
            _ => Ok(Text { value: self.joined(&TEXT)?, kind: TextKind::Text }),
        }
    }

    /// `link`: an optional id, then the link with its text in pipes after it or in its middle.
    fn link(&mut self) -> Result<LinkData, Error> {
        let id = if self.peek() == Kind::LinkId { Some(self.advance().replacen('@', "", 1)) } else { None };
        if self.peek() == Kind::StartLink {
            let start = self.advance().to_owned();
            let text = match self.peek() {
                Kind::Str => Text { value: self.advance().to_owned(), kind: TextKind::String },
                Kind::MdStr => Text { value: self.advance().to_owned(), kind: TextKind::Markdown },
                _ => Text { value: self.joined(&[Kind::EdgeText, Kind::UnicodeText])?, kind: TextKind::Text },
            };
            let end = self.expect(Kind::Link)?;
            return Ok(LinkData { link: link::read(end, Some(&start)), text: Some(text), id });
        }
        let end = self.expect(Kind::Link)?;
        let link = link::read(end, None);
        let text = if self.peek() == Kind::Pipe {
            self.advance();
            let text = self.text()?;
            self.expect(Kind::Pipe)?;
            self.spaces();
            Some(text)
        } else {
            None
        };
        Ok(LinkData { link, text, id })
    }
}

/// `setDirection`: arrows name directions, `TD` is `TB`.
fn header_direction(written: &str) -> Direction {
    match written.trim_matches(super::is_space) {
        "<" | "RL" => Direction::RL,
        "^" | "BT" => Direction::BT,
        ">" | "LR" => Direction::LR,
        _ => Direction::TB,
    }
}

/// Mermaid's lexer turns a line end and the spaces after it, inside a block's quotes, into `<br/>`.
fn with_line_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(line_end) = rest.find('\n') {
        out.push_str(&rest[..line_end]);
        out.push_str("<br/>");
        rest = rest[line_end + 1..].trim_start_matches(super::is_space);
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::super::{Limits, parse};
    use super::*;

    const LIMITS: Limits = Limits { bytes: usize::MAX, nodes: 100, edges: 100 };

    fn read(source: &str) -> Flowchart {
        parse(source, LIMITS).unwrap()
    }

    fn edges(flowchart: &Flowchart) -> Vec<(&str, &str, &str)> {
        flowchart.edges.iter().map(|edge| (edge.start.as_str(), edge.end.as_str(), edge.text.value.as_str())).collect()
    }

    #[test]
    fn a_header_names_the_direction_and_arrows_name_it_too() {
        let direction = |source: &str| read(source).direction;

        assert_eq!(
            [direction("flowchart\nA"), direction("graph RL;A"), direction("graph >\nA"), direction("graph TD\nA")],
            [Direction::TB, Direction::RL, Direction::LR, Direction::TB]
        );
    }

    #[test]
    fn a_chain_links_each_group_of_nodes_to_the_next() {
        let flowchart = read("graph TD\n  A & B -->|go| C -- stop --> D\n");

        assert_eq!(edges(&flowchart), [("A", "C", "go"), ("B", "C", "go"), ("C", "D", "stop")]);
    }

    #[test]
    fn a_block_before_an_ampersand_belongs_to_the_node_before_it() {
        let flowchart = read("graph TD\n  A@{ shape: hex } & B --> C@{ shape: cyl }\n");

        let shapes: Vec<Option<&str>> = flowchart.nodes.iter().map(|node| node.shape.as_deref()).collect();
        assert_eq!(shapes, [Some("hex"), None, Some("cyl")]);
    }

    #[test]
    fn what_mermaid_refuses_is_refused() {
        let refused = ["graph TD\n  end\n", "graph TD\n  A[ \"a\" ]\n", "graph TD A-->B\n"];

        for source in refused {
            assert!(matches!(parse(source, LIMITS), Err(Error::Invalid { .. })), "{source:?}");
        }
    }
}
