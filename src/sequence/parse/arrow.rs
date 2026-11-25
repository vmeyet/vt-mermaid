//! A message's arrow: how mermaid writes it and its name in mermaid's db.

/// A message's arrow, named after mermaid's line types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::sequence) enum Arrow {
    Solid,
    BidirectionalSolid,
    Dotted,
    BidirectionalDotted,
    SolidOpen,
    DottedOpen,
    SolidCross,
    DottedCross,
    SolidPoint,
    DottedPoint,
    SolidTopDotted,
    SolidBottomDotted,
    StickTopDotted,
    StickBottomDotted,
    SolidTopReverseDotted,
    SolidBottomReverseDotted,
    StickTopReverseDotted,
    StickBottomReverseDotted,
    SolidTop,
    SolidBottom,
    StickTop,
    StickBottom,
    SolidTopReverse,
    SolidBottomReverse,
    StickTopReverse,
    StickBottomReverse,
}

impl Arrow {
    /// Every arrow, in the order the lexer tries them: the first written as a prefix of another comes first only when
    /// the other cannot follow it.
    pub(in crate::sequence) const ALL: [Self; 26] = [
        Self::Solid,
        Self::BidirectionalSolid,
        Self::Dotted,
        Self::BidirectionalDotted,
        Self::SolidOpen,
        Self::DottedOpen,
        Self::SolidCross,
        Self::DottedCross,
        Self::SolidPoint,
        Self::DottedPoint,
        Self::SolidTopDotted,
        Self::SolidBottomDotted,
        Self::StickTopDotted,
        Self::StickBottomDotted,
        Self::SolidTopReverseDotted,
        Self::SolidBottomReverseDotted,
        Self::StickTopReverseDotted,
        Self::StickBottomReverseDotted,
        Self::SolidTop,
        Self::SolidBottom,
        Self::StickTop,
        Self::StickBottom,
        Self::SolidTopReverse,
        Self::SolidBottomReverse,
        Self::StickTopReverse,
        Self::StickBottomReverse,
    ];

    /// How the source writes it.
    pub(in crate::sequence) fn lexeme(self) -> &'static str {
        match self {
            Self::Solid => "->>",
            Self::BidirectionalSolid => "<<->>",
            Self::Dotted => "-->>",
            Self::BidirectionalDotted => "<<-->>",
            Self::SolidOpen => "->",
            Self::DottedOpen => "-->",
            Self::SolidCross => "-x",
            Self::DottedCross => "--x",
            Self::SolidPoint => "-)",
            Self::DottedPoint => "--)",
            Self::SolidTopDotted => "--|\\",
            Self::SolidBottomDotted => "--|/",
            Self::StickTopDotted => "--\\\\",
            Self::StickBottomDotted => "--//",
            Self::SolidTopReverseDotted => "/|--",
            Self::SolidBottomReverseDotted => "\\|--",
            Self::StickTopReverseDotted => "//--",
            Self::StickBottomReverseDotted => "\\\\--",
            Self::SolidTop => "-|\\",
            Self::SolidBottom => "-|/",
            Self::StickTop => "-\\\\",
            Self::StickBottom => "-//",
            Self::SolidTopReverse => "/|-",
            Self::SolidBottomReverse => "\\|-",
            Self::StickTopReverse => "//-",
            Self::StickBottomReverse => "\\\\-",
        }
    }

    /// Its name in mermaid's `LINETYPE`.
    #[cfg(test)]
    pub(in crate::sequence) fn name(self) -> &'static str {
        match self {
            Self::Solid => "SOLID",
            Self::BidirectionalSolid => "BIDIRECTIONAL_SOLID",
            Self::Dotted => "DOTTED",
            Self::BidirectionalDotted => "BIDIRECTIONAL_DOTTED",
            Self::SolidOpen => "SOLID_OPEN",
            Self::DottedOpen => "DOTTED_OPEN",
            Self::SolidCross => "SOLID_CROSS",
            Self::DottedCross => "DOTTED_CROSS",
            Self::SolidPoint => "SOLID_POINT",
            Self::DottedPoint => "DOTTED_POINT",
            Self::SolidTopDotted => "SOLID_TOP_DOTTED",
            Self::SolidBottomDotted => "SOLID_BOTTOM_DOTTED",
            Self::StickTopDotted => "STICK_TOP_DOTTED",
            Self::StickBottomDotted => "STICK_BOTTOM_DOTTED",
            Self::SolidTopReverseDotted => "SOLID_ARROW_TOP_REVERSE_DOTTED",
            Self::SolidBottomReverseDotted => "SOLID_ARROW_BOTTOM_REVERSE_DOTTED",
            Self::StickTopReverseDotted => "STICK_ARROW_TOP_REVERSE_DOTTED",
            Self::StickBottomReverseDotted => "STICK_ARROW_BOTTOM_REVERSE_DOTTED",
            Self::SolidTop => "SOLID_TOP",
            Self::SolidBottom => "SOLID_BOTTOM",
            Self::StickTop => "STICK_TOP",
            Self::StickBottom => "STICK_BOTTOM",
            Self::SolidTopReverse => "SOLID_ARROW_TOP_REVERSE",
            Self::SolidBottomReverse => "SOLID_ARROW_BOTTOM_REVERSE",
            Self::StickTopReverse => "STICK_ARROW_TOP_REVERSE",
            Self::StickBottomReverse => "STICK_ARROW_BOTTOM_REVERSE",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arrow_is_shadowed_by_one_tried_before_it() {
        for (index, arrow) in Arrow::ALL.iter().enumerate() {
            let shadowing = Arrow::ALL[..index].iter().find(|earlier| arrow.lexeme().starts_with(earlier.lexeme()));
            assert_eq!(shadowing, None, "{arrow:?}");
        }
    }
}
