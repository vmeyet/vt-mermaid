//! Mermaid's `ClassMember`: an attribute or a method read from the text of a member, and how it is shown.

use crate::source::is_space;

/// An attribute or a method of a class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::class) struct Member {
    /// `+`, `-`, `#`, `~`, or empty.
    pub(in crate::class) visibility: String,
    /// Its name, with the type written before or after it for an attribute.
    pub(in crate::class) name: String,
    /// `$` for static, `*` for abstract, or empty.
    pub(in crate::class) classifier: String,
    /// A method's parameters and return type; `None` for an attribute.
    pub(in crate::class) call: Option<Call>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::class) struct Call {
    pub(in crate::class) parameters: String,
    pub(in crate::class) returns: String,
}

const VISIBILITIES: [char; 4] = ['#', '+', '~', '-'];

impl Member {
    /// An attribute as `parseMember` reads it: a visibility first, a classifier last.
    pub(in crate::class) fn attribute(text: &str) -> Self {
        let visibility = text.chars().next().filter(|first| VISIBILITIES.contains(first));
        let classifier = text.chars().next_back().filter(|last| matches!(last, '$' | '*'));
        let start = visibility.map_or(0, char::len_utf8);
        let end = (text.len() - classifier.map_or(0, char::len_utf8)).max(start);
        Self {
            visibility: visibility.map(String::from).unwrap_or_default(),
            name: kept_leading_space(&text[start..end]),
            classifier: classifier.map(String::from).unwrap_or_default(),
            call: None,
        }
    }

    /// A method as `parseMember` reads it with `([#+~-])?(.+)\((.*)\)([\s$*])?(.*)([$*])?`: the name runs to the last
    /// `(` before the last `)`, a classifier stands right after the `)` or ends the return type. `None` where the
    /// pattern finds nothing, which makes mermaid fail.
    pub(in crate::class) fn method(text: &str) -> Option<Self> {
        let mut start = 0;
        loop {
            let line_end = start + text[start..].find(is_line_end).unwrap_or(text.len() - start);
            let line = &text[start..line_end];
            let visibility = line.chars().next().filter(|first| VISIBILITIES.contains(first));
            let with_visibility = visibility.and_then(|visibility| Some((Some(visibility), call_in(line, visibility.len_utf8())?)));
            if let Some((visibility, parts)) = with_visibility.or_else(|| Some((None, call_in(line, 0)?))) {
                return Some(Self::from_parts(visibility, &text[start..], parts));
            }
            if line_end == text.len() {
                return None;
            }
            start = line_end + text[line_end..].chars().next().map_or(1, char::len_utf8);
        }
    }

    fn from_parts(visibility: Option<char>, text: &str, parts: Parts) -> Self {
        let after = &text[parts.close + 1..];
        let after_line = &after[..after.find(is_line_end).unwrap_or(after.len())];
        let next = after.chars().next().filter(|&char| is_space(char) || matches!(char, '$' | '*'));
        let rest = next.map_or(after_line, |next| {
            let from = next.len_utf8();
            &after[from..from + after[from..].find(is_line_end).unwrap_or(after.len() - from)]
        });
        let mut classifier = next.map(String::from).unwrap_or_default().trim_matches(is_space).to_owned();
        let mut returns = rest.trim_matches(is_space).to_owned();
        if classifier.is_empty()
            && let Some(last) = returns.chars().next_back().filter(|last| matches!(last, '$' | '*'))
        {
            classifier = last.to_string();
            returns.pop();
        }
        Self {
            visibility: visibility.map(String::from).unwrap_or_default(),
            name: kept_leading_space(&text[parts.name_start..parts.open]),
            classifier,
            call: Some(Call { parameters: text[parts.open + 1..parts.close].trim_matches(is_space).to_owned(), returns }),
        }
    }

    /// What mermaid shows: the visibility, the name, a method's parameters and return type, generics as `<…>`.
    pub(in crate::class) fn shown(&self) -> String {
        let call = self.call.as_ref().map_or_else(String::new, |call| {
            let returns = if call.returns.is_empty() { String::new() } else { format!(" : {}", generic(&call.returns)) };
            format!("({}){returns}", generic(&call.parameters))
        });
        format!("{}{}{call}{}", self.visibility, generic(&self.name), self.classifier).trim_matches(is_space).to_owned()
    }
}

/// Where a method's name starts, and its `(` and `)`.
#[derive(Debug, Clone, Copy)]
struct Parts {
    name_start: usize,
    open: usize,
    close: usize,
}

/// The last `(` before the last `)` of `line`, with at least one character of name between `name_start` and it.
fn call_in(line: &str, name_start: usize) -> Option<Parts> {
    let close = line.rfind(')')?;
    let open = line[..close].rfind('(').filter(|&open| open > name_start)?;
    Some(Parts { name_start, open, close })
}

/// A character JavaScript's `.` does not match.
fn is_line_end(char: char) -> bool {
    matches!(char, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Trimmed, keeping one leading space if it had any, as `parseMember` does.
fn kept_leading_space(name: &str) -> String {
    let trimmed = name.trim_matches(is_space);
    if name.starts_with(' ') { format!(" {trimmed}") } else { trimmed.to_owned() }
}

/// Mermaid's `parseGenericTypes`: each pair of `~` around a type becomes `<` and `>`, the outermost first, within each
/// part between commas unless a comma splits one pair.
pub(in crate::class) fn generic(text: &str) -> String {
    let parts: Vec<&str> =
        text.split(',').enumerate().flat_map(|(index, part)| [(index > 0).then_some(","), Some(part)]).flatten().collect();
    let single_tilde = |part: &str| part.matches('~').count() == 1;
    let mut out: Vec<String> = Vec::with_capacity(parts.len());
    let mut index = 0;
    while index < parts.len() {
        let joined =
            parts[index] == "," && index > 0 && index + 1 < parts.len() && single_tilde(parts[index - 1]) && single_tilde(parts[index + 1]);
        if joined {
            out.pop();
            out.push(paired(&format!("{},{}", parts[index - 1], parts[index + 1])));
            index += 2;
        } else {
            out.push(paired(parts[index]));
            index += 1;
        }
    }
    out.concat()
}

/// `processSet`: the first and last `~` become `<` and `>`, then the next pair inwards, an odd leading `~` kept.
fn paired(part: &str) -> String {
    let tildes = part.matches('~').count();
    if tildes <= 1 {
        return part.to_owned();
    }
    let kept = !tildes.is_multiple_of(2) && part.starts_with('~');
    let mut chars: Vec<char> = part.chars().skip(usize::from(kept)).collect();
    let mut positions: Vec<usize> = chars.iter().enumerate().filter(|&(_, &char)| char == '~').map(|(at, _)| at).collect();
    while positions.len() >= 2 {
        let (first, last) = (positions.remove(0), positions.pop().unwrap_or_default());
        chars[first] = '<';
        chars[last] = '>';
    }
    format!("{}{}", if kept { "~" } else { "" }, chars.into_iter().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(text: &str) -> (String, String, String, String, String) {
        let member = Member::method(text).unwrap();
        let call = member.call.unwrap();
        (member.visibility, member.name, call.parameters, member.classifier, call.returns)
    }

    #[test]
    fn a_method_reads_its_visibility_parameters_classifier_and_return_type() {
        assert_eq!(method("+deposit(amount) bool"), ("+".into(), "deposit".into(), "amount".into(), String::new(), "bool".into()));
        assert_eq!(method("someAbstract()*"), (String::new(), "someAbstract".into(), String::new(), "*".into(), String::new()));
        assert_eq!(method("-total() int$"), ("-".into(), "total".into(), String::new(), "$".into(), "int".into()));
    }

    #[test]
    fn a_method_name_runs_to_the_last_parenthesis_before_the_last_closing_one() {
        assert_eq!(method("a(b)(c)").1, "a(b)");
        assert_eq!(method("+(x)").1, "+");
    }

    #[test]
    fn a_method_without_an_opening_parenthesis_after_its_name_is_not_read() {
        assert_eq!(Member::method("foo)"), None);
        assert_eq!(Member::method("(x)"), None);
    }

    #[test]
    fn an_attribute_reads_its_visibility_and_classifier() {
        let member = Member::attribute("-List~int~ items$");

        assert_eq!((member.visibility.as_str(), member.name.as_str(), member.classifier.as_str()), ("-", "List~int~ items", "$"));
        assert_eq!(member.shown(), "-List<int> items$");
    }

    #[test]
    fn generics_pair_their_tildes_from_the_outside_in() {
        assert_eq!(generic("List~List~int~~"), "List<List<int>>");
        assert_eq!(generic("Map~K, V~"), "Map<K, V>");
        assert_eq!(generic("~odd~one~"), "~odd<one>");
    }
}
