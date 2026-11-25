//! A box's name: mermaid's `parseBoxData` takes a first word that is a CSS colour as the box's fill, the rest as its
//! name.

use super::db::Text;
use crate::source::{is_space, is_word};

/// CSS's named colours, lowercase, each followed by a space.
const NAMED: &str = concat!(
    "aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet brown burlywood ",
    "cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan darkgoldenrod darkgray ",
    "darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid darkred darksalmon darkseagreen ",
    "darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink deepskyblue dimgray dimgrey dodgerblue ",
    "firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite gold goldenrod gray green greenyellow grey honeydew ",
    "hotpink indianred indigo ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan ",
    "lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen lightskyblue lightslategray ",
    "lightslategrey lightsteelblue lightyellow lime limegreen linen magenta maroon mediumaquamarine mediumblue ",
    "mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen mediumturquoise mediumvioletred ",
    "midnightblue mintcream mistyrose moccasin navajowhite navy oldlace olive olivedrab orange orangered orchid ",
    "palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum powderblue purple ",
    "rebeccapurple red rosybrown royalblue saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue ",
    "slateblue slategray slategrey snow springgreen steelblue tan teal thistle tomato turquoise violet wheat white ",
    "whitesmoke yellow yellowgreen transparent currentcolor "
);

/// The name of a box whose line after `box` is `written`: without its first word when that word is a colour, without
/// a `wrap:` prefix; none when nothing is left.
pub(super) fn group_name(written: &str) -> Option<String> {
    let (colour, rest) = split_colour(written);
    let name = if is_colour(colour) { rest } else { written };
    Some(Text::parsed(name).value).filter(|name| !name.is_empty())
}

/// `^((?:rgba?|hsla?)\s*\(.*\)|\w*)(.*)$`: a colour function up to the last `)`, else the first word.
fn split_colour(text: &str) -> (&str, &str) {
    let function = ["rgba", "rgb", "hsla", "hsl"].iter().find_map(|name| {
        let after = text.strip_prefix(name)?.trim_start_matches(is_space).strip_prefix('(')?;
        let close = after.rfind(')')?;
        Some(text.len() - after.len() + close + 1)
    });
    let end = function.unwrap_or_else(|| text.find(|char: char| !is_word(char)).unwrap_or(text.len()));
    text.split_at(end)
}

/// What a browser takes as a colour: a named one in any case, or a colour function of three or four numbers; nothing
/// at all counts too, as mermaid then falls back to its own colour and keeps the whole line as the name.
fn is_colour(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let named = NAMED.split_whitespace().any(|name| name == lower);
    let function = text.find('(').is_some_and(|open| {
        let arguments: Vec<&str> = text[open + 1..text.len() - 1].split([',', '/', ' ']).filter(|argument| !argument.is_empty()).collect();
        (3..=4).contains(&arguments.len())
            && arguments.iter().all(|argument| is_number(argument.trim_end_matches(['%']).trim_end_matches("deg")))
    });
    named || function || lower.is_empty()
}

fn is_number(text: &str) -> bool {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    !digits.is_empty()
        && digits.chars().all(|char| char.is_ascii_digit() || char == '.')
        && digits.matches('.').count() <= 1
        && digits != "."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_word_that_is_a_colour_is_not_part_of_the_name() {
        assert_eq!(group_name("LightGrey Alice and Bob"), Some("Alice and Bob".to_owned()));
        assert_eq!(group_name("rgb(34, 56, 0) Group1"), Some("Group1".to_owned()));
        assert_eq!(group_name("Another Group"), Some("Another Group".to_owned()));
        assert_eq!(group_name("rgb(200,2) Group"), Some("rgb(200,2) Group".to_owned()));
    }

    #[test]
    fn a_box_with_only_a_colour_or_nothing_has_no_name() {
        assert_eq!(group_name("aqua"), None);
        assert_eq!(group_name(""), None);
    }
}
