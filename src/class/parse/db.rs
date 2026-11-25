//! Mermaid's class db (`classDb.ts`): what each statement adds to the diagram, merges into it or refuses.

use std::collections::HashMap;

use super::lexer::{Direction, End};
use super::member::Member;
use super::{Invalid, Limits};
use crate::error::Error;
use crate::source::is_space;

/// A class diagram as mermaid's db holds it once the source is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::class) struct ClassDiagram {
    pub(in crate::class) direction: Direction,
    /// In the order the source first names them.
    pub(in crate::class) classes: Vec<Class>,
    pub(in crate::class) namespaces: Vec<Namespace>,
    pub(in crate::class) relations: Vec<Relation>,
    pub(in crate::class) notes: Vec<Note>,
    /// The lollipops a relation draws instead of a class.
    pub(in crate::class) interfaces: Vec<Interface>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::class) struct Class {
    pub(in crate::class) id: String,
    /// The type between `~` after its name, empty for none.
    pub(in crate::class) generic: String,
    pub(in crate::class) label: String,
    pub(in crate::class) annotations: Vec<String>,
    pub(in crate::class) attributes: Vec<Member>,
    pub(in crate::class) methods: Vec<Member>,
    /// The namespace that last listed it.
    pub(in crate::class) namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::class) struct Namespace {
    /// Its name with its parents', joined by dots.
    pub(in crate::class) id: String,
    pub(in crate::class) label: String,
    pub(in crate::class) parent: Option<String>,
    pub(in crate::class) classes: Vec<String>,
    pub(in crate::class) notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::class) struct Relation {
    pub(in crate::class) from: String,
    pub(in crate::class) to: String,
    pub(in crate::class) from_end: Option<End>,
    pub(in crate::class) to_end: Option<End>,
    pub(in crate::class) dotted: bool,
    /// The text written in quotes next to each end, as mermaid keeps it: `none` for no text.
    pub(in crate::class) from_text: String,
    pub(in crate::class) to_text: String,
    pub(in crate::class) label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::class) struct Note {
    pub(in crate::class) id: String,
    pub(in crate::class) class: Option<String>,
    pub(in crate::class) text: String,
    pub(in crate::class) namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::class) struct Interface {
    pub(in crate::class) id: String,
    pub(in crate::class) label: String,
}

/// A relation as the grammar reads it, before the db names its ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Written {
    pub(super) from: String,
    pub(super) to: String,
    pub(super) from_end: Option<End>,
    pub(super) to_end: Option<End>,
    pub(super) dotted: bool,
    pub(super) from_text: Option<String>,
    pub(super) to_text: Option<String>,
    pub(super) label: Option<String>,
}

/// The db while the grammar reads the source.
#[derive(Debug, Clone)]
pub(super) struct Db {
    diagram: ClassDiagram,
    class_index: HashMap<String, usize>,
    namespace_index: HashMap<String, usize>,
    namespace_stack: Vec<String>,
    limits: Limits,
}

impl Db {
    pub(super) fn new(limits: Limits) -> Self {
        let diagram = ClassDiagram {
            direction: Direction::TB,
            classes: Vec::new(),
            namespaces: Vec::new(),
            relations: Vec::new(),
            notes: Vec::new(),
            interfaces: Vec::new(),
        };
        Self { diagram, class_index: HashMap::new(), namespace_index: HashMap::new(), namespace_stack: Vec::new(), limits }
    }

    pub(super) fn into_diagram(self) -> ClassDiagram {
        self.diagram
    }

    pub(super) fn set_direction(&mut self, direction: Direction) {
        self.diagram.direction = direction;
    }

    /// `addClass`: a class is added the first time it is named, its generic type read from its name.
    pub(super) fn add_class(&mut self, written: &str) -> Result<(), Error> {
        let (id, generic) = split_generic(written);
        if self.class_index.contains_key(id) {
            return Ok(());
        }
        self.class_index.insert(id.to_owned(), self.diagram.classes.len());
        self.diagram.classes.push(Class {
            id: id.to_owned(),
            generic: generic.to_owned(),
            label: id.to_owned(),
            annotations: Vec::new(),
            attributes: Vec::new(),
            methods: Vec::new(),
            namespace: None,
        });
        self.check_nodes()
    }

    pub(super) fn set_class_label(&mut self, written: &str, label: &str) {
        if let Some(class) = self.class_mut(written) {
            label.clone_into(&mut class.label);
        }
    }

    /// `addAnnotation`: refused, as mermaid fails, when the class is not there yet.
    pub(super) fn add_annotation(&mut self, written: &str, annotation: &str) -> Result<(), Error> {
        let class = self.class_mut(written).ok_or_else(|| Invalid::NoClass { id: split_generic(written).0.to_owned() })?;
        class.annotations.push(annotation.to_owned());
        Ok(())
    }

    /// `addMember`: `<<…>>` is an annotation, text with a `)` after its first character a method, any other text an
    /// attribute.
    pub(super) fn add_member(&mut self, written: &str, member: &str) -> Result<(), Error> {
        self.add_class(written)?;
        let text = member.trim_matches(is_space);
        let Some(class) = self.class_mut(written) else { return Ok(()) };
        if let Some(annotation) = text.strip_prefix("<<").and_then(|inner| inner.strip_suffix(">>")) {
            class.annotations.push(annotation.to_owned());
        } else if text.find(')').is_some_and(|at| at > 0) {
            class.methods.push(Member::method(text).ok_or_else(|| Invalid::Method { text: text.to_owned() })?);
        } else if !text.is_empty() {
            class.attributes.push(Member::attribute(text));
        }
        Ok(())
    }

    pub(super) fn add_members(&mut self, written: &str, members: &[&str]) -> Result<(), Error> {
        members.iter().try_for_each(|member| self.add_member(written, member))
    }

    /// `addNote`: its id is its place among the notes.
    pub(super) fn add_note(&mut self, text: &str, class: Option<&str>) -> Result<String, Error> {
        let id = format!("note{}", self.diagram.notes.len());
        self.diagram.notes.push(Note { id: id.clone(), class: class.map(str::to_owned), text: text.to_owned(), namespace: None });
        self.check_nodes()?;
        self.check_edges()?;
        Ok(id)
    }

    /// `addRelation`: an end written `()` with nothing at the other end is a lollipop named after the class it stands
    /// for, else both ends are classes.
    pub(super) fn add_relation(&mut self, written: Written) -> Result<(), Error> {
        let not_drawn = |end: Option<End>| end.is_none();
        let (from, to) = if written.from_end == Some(End::Lollipop) && not_drawn(written.to_end) {
            self.add_class(&written.to)?;
            (self.add_interface(&written.from)?, written.to)
        } else if written.to_end == Some(End::Lollipop) && not_drawn(written.from_end) {
            self.add_class(&written.from)?;
            let interface = self.add_interface(&written.to)?;
            (written.from, interface)
        } else {
            self.add_class(&written.from)?;
            self.add_class(&written.to)?;
            (written.from, written.to)
        };
        let text = |text: Option<String>| text.map_or_else(|| "none".to_owned(), |text| text.trim_matches(is_space).to_owned());
        self.diagram.relations.push(Relation {
            from: split_generic(&from).0.to_owned(),
            to: split_generic(&to).0.to_owned(),
            from_end: written.from_end,
            to_end: written.to_end,
            dotted: written.dotted,
            from_text: text(written.from_text),
            to_text: text(written.to_text),
            label: written.label,
        });
        self.check_edges()
    }

    fn add_interface(&mut self, label: &str) -> Result<String, Error> {
        let id = format!("interface{}", self.diagram.interfaces.len());
        self.diagram.interfaces.push(Interface { id: id.clone(), label: label.to_owned() });
        self.check_nodes()?;
        Ok(id)
    }

    /// `addNamespace`: the name is read inside the namespace being read, every parent its dots name is made, and
    /// naming one again keeps it, with the new label if any.
    pub(super) fn add_namespace(&mut self, name: &str, label: Option<&str>) -> Result<String, Error> {
        let id = self.namespace_stack.last().map_or_else(|| name.to_owned(), |outer| format!("{outer}.{name}"));
        self.namespace_stack.push(id.clone());
        if let Some(&index) = self.namespace_index.get(&id) {
            if let Some(label) = label {
                label.clone_into(&mut self.diagram.namespaces[index].label);
            }
            return Ok(id);
        }
        let parts: Vec<&str> = id.split('.').collect();
        let mut parent: Option<String> = None;
        for (depth, part) in parts.iter().enumerate() {
            let current = parts[..=depth].join(".");
            let leaf = depth + 1 == parts.len();
            if let Some(&index) = self.namespace_index.get(&current) {
                let existing = &mut self.diagram.namespaces[index];
                if existing.parent.is_none() {
                    existing.parent.clone_from(&parent);
                }
            } else {
                self.namespace_index.insert(current.clone(), self.diagram.namespaces.len());
                let label = if leaf { label.unwrap_or(part) } else { part };
                self.diagram.namespaces.push(Namespace {
                    id: current.clone(),
                    label: label.to_owned(),
                    parent: parent.clone(),
                    classes: Vec::new(),
                    notes: Vec::new(),
                });
                self.check_nodes()?;
            }
            parent = Some(current);
        }
        Ok(id)
    }

    pub(super) fn pop_namespace(&mut self) {
        self.namespace_stack.pop();
    }

    /// `addClassesToNamespace`: each class and note listed moves into the namespace.
    pub(super) fn add_to_namespace(&mut self, id: &str, classes: &[String], notes: &[String]) -> Result<(), Error> {
        let Some(&index) = self.namespace_index.get(id) else { return Ok(()) };
        for written in classes {
            let class = self.class_mut(written).ok_or_else(|| Invalid::NoClass { id: split_generic(written).0.to_owned() })?;
            class.namespace = Some(id.to_owned());
            let class_id = class.id.clone();
            push_new(&mut self.diagram.namespaces[index].classes, class_id);
        }
        for note_id in notes {
            if let Some(note) = self.diagram.notes.iter_mut().find(|note| note.id == *note_id) {
                note.namespace = Some(id.to_owned());
            }
            push_new(&mut self.diagram.namespaces[index].notes, note_id.clone());
        }
        Ok(())
    }

    fn class_mut(&mut self, written: &str) -> Option<&mut Class> {
        let index = *self.class_index.get(split_generic(written).0)?;
        self.diagram.classes.get_mut(index)
    }

    /// Classes, lollipops, notes and namespaces are each a block of the drawing.
    fn check_nodes(&self) -> Result<(), Error> {
        let diagram = &self.diagram;
        let nodes = diagram.classes.len() + diagram.interfaces.len() + diagram.notes.len() + diagram.namespaces.len();
        if nodes > self.limits.nodes { Err(Error::TooManyNodes { nodes }) } else { Ok(()) }
    }

    /// Relations and the links of notes to their classes are each an edge of the drawing.
    fn check_edges(&self) -> Result<(), Error> {
        let diagram = &self.diagram;
        let edges = diagram.relations.len() + diagram.notes.iter().filter(|note| note.class.is_some()).count();
        if edges > self.limits.edges { Err(Error::TooManyEdges { edges }) } else { Ok(()) }
    }
}

fn push_new(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

/// `splitClassNameAndType`: a `~` after the first character starts the generic type, up to the next `~`.
pub(super) fn split_generic(written: &str) -> (&str, &str) {
    match written.find('~').filter(|&at| at > 0) {
        Some(at) => {
            let rest = &written[at + 1..];
            (&written[..at], &rest[..rest.find('~').unwrap_or(rest.len())])
        }
        None => (written, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_with_a_generic_type_splits_at_its_first_tilde() {
        assert_eq!(split_generic("List~int~"), ("List", "int"));
        assert_eq!(split_generic("~odd"), ("~odd", ""));
    }

    #[test]
    fn an_annotation_for_a_class_not_there_yet_is_refused() {
        let mut db = Db::new(Limits::DRAWN);

        assert!(db.add_annotation("Shape", "interface").is_err());
        assert!(db.add_class("Shape").is_ok() && db.add_annotation("Shape", "interface").is_ok());
    }

    #[test]
    fn a_dotted_namespace_makes_its_parents() {
        let mut db = Db::new(Limits::DRAWN);

        let id = db.add_namespace("A.B", Some("Inner")).unwrap();

        let namespaces: Vec<(String, String, Option<String>)> =
            db.into_diagram().namespaces.into_iter().map(|namespace| (namespace.id, namespace.label, namespace.parent)).collect();
        assert_eq!(id, "A.B");
        assert_eq!(namespaces, [("A".into(), "A".into(), None), ("A.B".into(), "Inner".into(), Some("A".into()))]);
    }

    #[test]
    fn members_are_attributes_methods_or_annotations() {
        let mut db = Db::new(Limits::DRAWN);

        db.add_members("Duck", &["  +int age", "+swim() bool", "<<animal>>", "   "]).unwrap();

        let duck = &db.into_diagram().classes[0];
        assert_eq!((duck.attributes.len(), duck.methods.len(), duck.annotations.as_slice()), (1, 1, ["animal".to_owned()].as_slice()));
    }

    #[test]
    fn a_method_without_its_parenthesis_is_refused() {
        assert!(Db::new(Limits::DRAWN).add_member("Duck", "swim)").is_err());
    }

    #[test]
    fn a_label_and_a_direction_are_kept() {
        let mut db = Db::new(Limits::DRAWN);

        db.add_class("A~T~").unwrap();
        db.set_class_label("A~T~", "Alpha");
        db.set_direction(Direction::LR);

        let diagram = db.into_diagram();
        assert_eq!(
            (diagram.classes[0].label.as_str(), diagram.classes[0].generic.as_str(), diagram.direction),
            ("Alpha", "T", Direction::LR)
        );
    }

    #[test]
    fn a_namespace_takes_in_the_classes_and_notes_listed_once_it_is_read() {
        let mut db = Db::new(Limits::DRAWN);

        let outer = db.add_namespace("Outer", None).unwrap();
        let inner = db.add_namespace("Inner", None).unwrap();
        db.pop_namespace();
        db.add_class("A").unwrap();
        let note = db.add_note("a note", Some("A")).unwrap();
        db.add_to_namespace(&outer, &["A".to_owned()], &[note]).unwrap();
        db.pop_namespace();

        let diagram = db.into_diagram();
        assert_eq!((outer.as_str(), inner.as_str()), ("Outer", "Outer.Inner"));
        assert_eq!(
            (diagram.namespaces[0].classes.as_slice(), diagram.notes[0].namespace.as_deref()),
            (["A".to_owned()].as_slice(), Some("Outer"))
        );
    }

    #[test]
    fn a_lollipop_end_is_an_interface_named_after_the_text_written() {
        let mut db = Db::new(Limits::DRAWN);
        let written = Written {
            from: "Clickable".into(),
            to: "Button".into(),
            from_end: Some(End::Lollipop),
            to_end: None,
            dotted: false,
            from_text: None,
            to_text: None,
            label: None,
        };

        db.add_relation(written).unwrap();

        let diagram = db.into_diagram();
        assert_eq!((diagram.relations[0].from.as_str(), diagram.interfaces[0].label.as_str()), ("interface0", "Clickable"));
        assert_eq!(diagram.classes.iter().map(|class| class.id.as_str()).collect::<Vec<_>>(), ["Button"]);
    }
}
