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
