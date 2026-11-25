//! Entities in labels: mermaid's own (`#quot;`, `#35;`, marked by [`prepare`](super::prepare)) and HTML's (`&amp;`),
//! both shown as the character they name, as a browser shows mermaid's labels.

use super::prepare::{ENTITY_END, ENTITY_START};

/// The entities a label most likely holds; any other is shown as written.
const NAMED: [(&str, char); 36] = [
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("nbsp", '\u{a0}'),
    ("equals", '='),
    ("copy", '©'),
    ("reg", '®'),
    ("trade", '™'),
    ("hellip", '…'),
    ("mdash", '—'),
    ("ndash", '–'),
    ("lsquo", '‘'),
    ("rsquo", '’'),
    ("ldquo", '“'),
    ("rdquo", '”'),
    ("laquo", '«'),
    ("raquo", '»'),
    ("bull", '•'),
    ("middot", '·'),
    ("deg", '°'),
    ("plusmn", '±'),
    ("times", '×'),
    ("divide", '÷'),
    ("ge", '≥'),
    ("le", '≤'),
    ("ne", '≠'),
    ("infin", '∞'),
    ("larr", '←'),
    ("rarr", '→'),
    ("uarr", '↑'),
    ("darr", '↓'),
    ("harr", '↔'),
    ("lArr", '⇐'),
    ("rArr", '⇒'),
];

/// `text` with mermaid's marks turned back into entities, then every entity it knows decoded.
pub(super) fn decoded(text: &str) -> String {
    let entities = text.replace(&format!("{ENTITY_START}°"), "&#").replace(ENTITY_START, "&").replace(ENTITY_END, ";");
    let mut out = String::with_capacity(entities.len());
    let mut rest = entities.as_str();
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some((char, end)) = after.find(';').and_then(|end| character(&after[..end]).map(|char| (char, end))) {
            out.push(char);
            rest = &after[end + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// The character `&name;` names: a known name, `#` and a decimal code, `#x` and a hexadecimal one.
fn character(name: &str) -> Option<char> {
    let code = match name.strip_prefix('#') {
        Some(hex) if hex.starts_with(['x', 'X']) => u32::from_str_radix(&hex[1..], 16).ok(),
        Some(decimal) => decimal.parse().ok(),
        None => return NAMED.iter().find(|(known, _)| *known == name).map(|&(_, char)| char),
    };
    code.and_then(char::from_u32).filter(|&char| char != '\0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mermaid_entities_and_html_ones_become_their_characters() {
        assert_eq!(decoded("ﬂ°quot¶ßhiﬂ°quot¶ß ﬂ°°35¶ß &amp; &#x2192; &ge;"), "\"hi\" # & → ≥");
    }

    #[test]
    fn an_unknown_or_unclosed_entity_is_shown_as_written() {
        assert_eq!(decoded("ﬂ°nope¶ß &copy &#xZZ; a & b"), "&nope; &copy &#xZZ; a & b");
    }
}
