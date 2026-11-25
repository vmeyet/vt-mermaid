//! Mermaid's sequence db (`sequenceDb.ts`): what each statement adds to the diagram, merges into it or refuses.

use super::arrow::Arrow;
use super::lexer::Block;
use super::{Invalid, Limits, colour};
use crate::error::Error;
use crate::source::{is_space, yaml};

/// A sequence diagram as mermaid's db holds it once the source is read.
#[derive(Debug, Clone, Default, PartialEq)]
pub(in crate::sequence) struct Sequence {
    /// In the order the source first names them.
    pub(in crate::sequence) participants: Vec<Participant>,
    /// Everything that happens, in order: mermaid's `messages`.
    pub(in crate::sequence) items: Vec<Item>,
    /// Mermaid's boxes.
    pub(in crate::sequence) groups: Vec<Group>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::sequence) struct Participant {
    pub(in crate::sequence) id: String,
    /// What is drawn for it: its alias, or its id.
    pub(in crate::sequence) description: String,
    /// Mermaid's type: `participant`, `actor`, `database`, `queue`…
    pub(in crate::sequence) figure: String,
    /// The index of the box holding it.
    pub(in crate::sequence) group: Option<usize>,
    /// The index of the item that creates it, and of the one that destroys it.
    pub(in crate::sequence) created: Option<usize>,
    pub(in crate::sequence) destroyed: Option<usize>,
}

/// A box around participants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::sequence) struct Group {
    pub(in crate::sequence) name: Option<String>,
    /// The ids of the participants it holds, as mermaid lists them: one may come twice.
    pub(in crate::sequence) participants: Vec<String>,
}

/// A text as written after `:` or a keyword, and whether it asks to wrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::sequence) struct Text {
    pub(in crate::sequence) value: String,
    pub(in crate::sequence) wrap: Option<bool>,
}

impl Text {
    /// Mermaid's `parseMessage`: trimmed, a `wrap:` or `nowrap:` prefix taken off.
    pub(in crate::sequence) fn parsed(written: &str) -> Self {
        let trimmed = written.trim_matches(is_space);
        let unprefixed = trimmed.strip_prefix(':').unwrap_or(trimmed);
        let (wrap, value) = match (unprefixed.strip_prefix("wrap:"), unprefixed.strip_prefix("nowrap:")) {
            (Some(rest), _) => (Some(true), rest),
            (_, Some(rest)) => (Some(false), rest),
            _ => (None, trimmed),
        };
        Self { value: value.trim_matches(is_space).to_owned(), wrap }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::sequence) enum Placement {
    LeftOf,
    RightOf,
    Over,
}

/// One entry of mermaid's `messages`.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::sequence) enum Item {
    Message {
        from: String,
        to: String,
        text: Text,
        arrow: Arrow,
        activate: bool,
    },
    Note {
        from: String,
        to: String,
        placement: Placement,
        text: Text,
    },
    /// `activate`, `+`, or with `start` false `deactivate`, `-`.
    Activation {
        id: String,
        start: bool,
    },
    /// `()` on the receiving side, or `reverse` on the sending side.
    Central {
        id: String,
        reverse: bool,
    },
    Autonumber {
        start: Option<f64>,
        step: Option<f64>,
        visible: bool,
    },
    /// `loop`, `alt`, `rect`… and its text, a colour for `rect`.
    Open {
        block: Block,
        text: Text,
    },
    /// `else`, `and` or `option`, in the block it divides.
    Section {
        block: Block,
        text: Text,
    },
    Close {
        block: Block,
    },
}

/// What the grammar hands the db, as jison's actions build it.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Statement {
    /// `participant`, `actor` or `create`, or a name a statement mentions, with no figure then.
    Participant {
        id: String,
        figure: Option<&'static str>,
        description: Option<Text>,
        config: Option<String>,
        created: bool,
    },
    Destroy {
        id: String,
    },
    GroupStart {
        text: String,
    },
    GroupEnd,
    Message {
        from: String,
        to: String,
        text: Text,
        arrow: Arrow,
        activate: bool,
    },
    Note {
        from: String,
        to: String,
        placement: Placement,
        text: Text,
    },
    Activation {
        id: String,
        start: bool,
    },
    Central {
        id: String,
        reverse: bool,
    },
    Autonumber {
        start: Option<f64>,
        step: Option<f64>,
        visible: bool,
    },
    Open {
        block: Block,
        text: Text,
    },
    Section {
        block: Block,
        text: Text,
    },
    Close {
        block: Block,
    },
}

impl Statement {
    /// A participant a message, a note or a link names.
    pub(super) fn named(id: String) -> Self {
        Self::Participant { id, figure: None, description: None, config: None, created: false }
    }

    /// The statement after `create`: mermaid makes any participant statement a creation, even `destroy`.
    pub(super) fn created(self) -> Self {
        match self {
            Self::Participant { id, figure, description, config, .. } => {
                Self::Participant { id, figure, description, config, created: true }
            }
            Self::Destroy { id } => Self::Participant { id, figure: None, description: None, config: None, created: true },
            other => other,
        }
    }
}

/// Applies `statements` in order, as `apply` does once the whole source parsed.
pub(super) fn apply(statements: Vec<Statement>, limits: Limits) -> Result<Sequence, Error> {
    let mut db =
        Db { limits, sequence: Sequence::default(), current_group: None, last_created: None, last_destroyed: None, destroyed: Vec::new() };
    for statement in statements {
        db.apply(statement)?;
    }
    Ok(db.finished())
}

#[derive(Debug)]
struct Db {
    limits: Limits,
    sequence: Sequence,
    current_group: Option<usize>,
    last_created: Option<String>,
    last_destroyed: Option<String>,
    /// Each `destroy` and the item it comes before, kept by id as mermaid does, so it may name a participant added later.
    destroyed: Vec<(String, usize)>,
}

impl Db {
    fn apply(&mut self, statement: Statement) -> Result<(), Error> {
        match statement {
            Statement::Participant { id, figure, description, config, created } => {
                if created {
                    if self.find(&id).is_some() {
                        return Err(Invalid::Recreated { id }.into());
                    }
                    self.last_created = Some(id.clone());
                }
                self.add_participant(&id, figure, description, config.as_deref())?;
                if created && let Some(index) = self.find(&id) {
                    self.sequence.participants[index].created = Some(self.sequence.items.len());
                }
                Ok(())
            }
            Statement::Destroy { id } => {
                self.destroyed.push((id.clone(), self.sequence.items.len()));
                self.last_destroyed = Some(id);
                Ok(())
            }
            Statement::GroupStart { text } => {
                self.sequence.groups.push(Group { name: colour::group_name(&text), participants: Vec::new() });
                self.current_group = Some(self.sequence.groups.len() - 1);
                Ok(())
            }
            Statement::GroupEnd => {
                self.current_group = None;
                Ok(())
            }
            Statement::Message { from, to, text, arrow, activate } => {
                self.check_lifecycle(&from, &to)?;
                self.push(Item::Message { from, to, text, arrow, activate })
            }
            Statement::Note { from, to, placement, text } => self.push(Item::Note { from, to, placement, text }),
            Statement::Activation { id, start } => {
                if !start && self.activations(&id) < 1 {
                    return Err(Invalid::Inactive { id }.into());
                }
                self.push(Item::Activation { id, start })
            }
            Statement::Central { id, reverse } => self.push(Item::Central { id, reverse }),
            Statement::Autonumber { start, step, visible } => self.push(Item::Autonumber { start, step, visible }),
            Statement::Open { block, text } => self.push(Item::Open { block, text }),
            Statement::Section { block, text } => self.push(Item::Section { block, text }),
            Statement::Close { block } => self.push(Item::Close { block }),
        }
    }

    fn finished(mut self) -> Sequence {
        for participant in &mut self.sequence.participants {
            participant.destroyed = self.destroyed.iter().rfind(|(id, _)| *id == participant.id).map(|&(_, item)| item);
        }
        self.sequence
    }

    fn find(&self, id: &str) -> Option<usize> {
        self.sequence.participants.iter().position(|participant| participant.id == id)
    }

    fn push(&mut self, item: Item) -> Result<(), Error> {
        if self.sequence.items.len() >= self.limits.items {
            return Err(Error::TooManyMessages { messages: self.sequence.items.len() + 1 });
        }
        self.sequence.items.push(item);
        Ok(())
    }

    /// Mermaid's `addActor`: the first statement naming a participant adds it, a later one with a figure or an alias
    /// sets them again, in its first place; a box keeps its participants.
    fn add_participant(&mut self, id: &str, figure: Option<&str>, description: Option<Text>, config: Option<&str>) -> Result<(), Error> {
        let block = config.map(|config| yaml::read(config).map_err(|_malformed| Invalid::Config { id: id.to_owned() })).transpose()?;
        let figure = block.as_ref().and_then(|block| block.value("type").and_then(yaml::Value::text)).or(figure.map(str::to_owned));
        let alias = block.as_ref().and_then(|block| block.given("alias").and_then(yaml::Value::text));
        let description = match alias {
            Some(alias) if description.as_ref().is_none_or(|text| text.value == id) => Some(alias),
            _ => description.map(|text| text.value),
        };
        let mut group = self.current_group;
        if let Some(old) = self.find(id) {
            let old_group = self.sequence.participants[old].group;
            if let (Some(current), Some(earlier)) = (self.current_group, old_group)
                && current != earlier
            {
                return Err(Invalid::TwoBoxes { id: id.to_owned() }.into());
            }
            group = old_group.or(self.current_group);
            self.sequence.participants[old].group = group;
            if description.is_none() {
                return Ok(());
            }
        }
        let description = if figure.is_none() { id.to_owned() } else { description.unwrap_or_else(|| id.to_owned()) };
        let participant = Participant {
            id: id.to_owned(),
            description,
            figure: figure.unwrap_or_else(|| "participant".to_owned()),
            group,
            created: None,
            destroyed: None,
        };
        match self.find(id) {
            Some(index) => {
                let created = self.sequence.participants[index].created;
                self.sequence.participants[index] = Participant { created, ..participant };
            }
            None if self.sequence.participants.len() >= self.limits.participants => {
                return Err(Error::TooManyParticipants { participants: self.sequence.participants.len() + 1 });
            }
            None => self.sequence.participants.push(participant),
        }
        if let Some(current) = self.current_group {
            self.sequence.groups[current].participants.push(id.to_owned());
        }
        Ok(())
    }

    /// A participant just created must be the receiver of the next message, one just destroyed its sender or
    /// receiver.
    fn check_lifecycle(&mut self, from: &str, to: &str) -> Result<(), Invalid> {
        if let Some(created) = self.last_created.take() {
            return if created == to { Ok(()) } else { Err(Invalid::NotCreated { id: created }) };
        }
        if let Some(destroyed) = self.last_destroyed.take() {
            return if destroyed == to || destroyed == from { Ok(()) } else { Err(Invalid::NotDestroyed { id: destroyed }) };
        }
        Ok(())
    }

    /// Mermaid's `activationCount`: activations of `id` minus its deactivations so far.
    fn activations(&self, id: &str) -> isize {
        self.sequence
            .items
            .iter()
            .map(|item| match item {
                Item::Activation { id: active, start } if active == id => {
                    if *start {
                        1
                    } else {
                        -1
                    }
                }
                _ => 0,
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Limits, parse};
    use super::*;

    fn read(body: &str) -> Result<Sequence, Error> {
        parse(&format!("sequenceDiagram\n{body}"), Limits::DRAWN)
    }

    fn reason(body: &str) -> String {
        match read(body) {
            Err(Error::Invalid { reason }) => reason,
            other => format!("{other:?}"),
        }
    }

    #[test]
    fn a_later_mention_keeps_the_alias_and_the_place() {
        let sequence = read("participant B\nparticipant A as Alice\nA->>B: hi\nparticipant A\n").unwrap();

        let described: Vec<(&str, &str)> =
            sequence.participants.iter().map(|participant| (participant.id.as_str(), participant.description.as_str())).collect();
        assert_eq!(described, [("B", "B"), ("A", "Alice")]);
    }

    #[test]
    fn a_block_sets_the_type_and_the_alias_unless_the_source_gives_one() {
        let sequence =
            read("participant D@{ \"type\": \"database\", \"alias\": \"Store\" }\nparticipant Q@{ type: queue, alias: x } as Jobs\n")
                .unwrap();

        let read: Vec<(&str, &str)> =
            sequence.participants.iter().map(|participant| (participant.figure.as_str(), participant.description.as_str())).collect();
        assert_eq!(read, [("database", "Store"), ("queue", "Jobs")]);
        assert_eq!(reason("participant D@{ \"type: \"x\" }\n"), "the @{ } block of D is not valid YAML");
    }

    #[test]
    fn deactivating_a_participant_not_active_is_refused() {
        assert!(read("A->>+B: go\nB-->>-A: done\n").is_ok());
        assert_eq!(reason("A->>B: go\nB-->>-A: done\n"), "B is deactivated but not active");
    }

    #[test]
    fn the_message_after_create_must_go_to_the_participant_created() {
        assert!(read("create participant B\nA->>B: hi\n").is_ok());
        assert_eq!(reason("create participant B\nA->>C: hi\n"), "no message is sent to B right after it is created");
        assert_eq!(reason("participant B\ncreate participant B\nA->>B: hi\n"), "B is created but already there");
    }

    #[test]
    fn the_message_after_destroy_must_involve_the_participant_destroyed() {
        let sequence = read("A->>B: hi\ndestroy B\nB->>A: bye\n").unwrap();

        assert_eq!(sequence.participants[1].destroyed, Some(1));
        assert_eq!(reason("A->>B: hi\ndestroy B\nA->>C: bye\n"), "no message comes from or goes to B right after it is destroyed");
    }

    #[test]
    fn a_participant_stays_in_its_first_box() {
        let sequence = read("box Purple Team\nparticipant A\nend\nA->>B: hi\n").unwrap();

        assert_eq!(sequence.groups, [Group { name: Some("Team".to_owned()), participants: vec!["A".to_owned()] }]);
        assert_eq!(sequence.participants[0].group, Some(0));
        assert_eq!(reason("box One\nparticipant A\nend\nbox Two\nparticipant A\nend\n"), "A is in two boxes");
    }

    #[test]
    fn a_wrap_prefix_is_taken_off_the_text() {
        assert_eq!(Text::parsed(" :wrap: long text "), Text { value: "long text".to_owned(), wrap: Some(true) });
        assert_eq!(Text::parsed("nowrap:short"), Text { value: "short".to_owned(), wrap: Some(false) });
    }
}
