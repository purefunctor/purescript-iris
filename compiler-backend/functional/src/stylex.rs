//! Functional identities and semantic expressions for StyleX compiler intrinsics.

use std::sync::Arc;

use crate::tree::ExpressionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleXExpression {
    Call { target: StyleXCallTarget, arguments: Arc<[ExpressionId]> },
    Conditional { condition: ExpressionId, style: ExpressionId },
    ConditionalCase(StyleXConditionalCase),
    ConditionalValue { default: ExpressionId, cases: Arc<[StyleXConditionalCase]> },
}

impl StyleXExpression {
    pub fn try_for_each_child<Error>(
        &self,
        mut visit: impl FnMut(ExpressionId) -> Result<(), Error>,
    ) -> Result<(), Error> {
        match self {
            StyleXExpression::Call { arguments, .. } => {
                for &argument in arguments.iter() {
                    visit(argument)?;
                }
            }
            StyleXExpression::Conditional { condition, style } => {
                visit(*condition)?;
                visit(*style)?;
            }
            StyleXExpression::ConditionalCase(case) => case.try_for_each_child(&mut visit)?,
            StyleXExpression::ConditionalValue { default, cases } => {
                visit(*default)?;
                for case in cases.iter() {
                    case.try_for_each_child(&mut visit)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleXConditionalCase {
    pub condition: StyleXCondition,
    pub value: ExpressionId,
}

/// The key of a conditional case: a `stylex.when.*` relation, or a condition string such as a
/// media query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleXCondition {
    Expression(ExpressionId),
    When { relation: StyleXWhenRelation, selector: ExpressionId, marker: Option<ExpressionId> },
}

impl StyleXConditionalCase {
    pub(crate) fn intrinsic(&self) -> StyleXIntrinsic {
        match self.condition {
            StyleXCondition::Expression(_) => {
                StyleXIntrinsic::Root(StyleXRootIntrinsic::ConditionalCase)
            }
            StyleXCondition::When { relation, marker, .. } => {
                StyleXIntrinsic::When { relation, marker: marker.is_some() }
            }
        }
    }

    fn try_for_each_child<Error>(
        &self,
        visit: &mut impl FnMut(ExpressionId) -> Result<(), Error>,
    ) -> Result<(), Error> {
        match self.condition {
            StyleXCondition::Expression(condition) => visit(condition)?,
            StyleXCondition::When { selector, marker, .. } => {
                visit(selector)?;
                if let Some(marker) = marker {
                    visit(marker)?;
                }
            }
        }
        visit(self.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleXCallTarget {
    Root(StyleXRootCall),
    Types(StyleXTypeCall),
}

impl StyleXCallTarget {
    pub fn name(self) -> &'static str {
        match self {
            StyleXCallTarget::Root(call) => call.name(),
            StyleXCallTarget::Types(call) => call.name(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleXRootCall {
    Create,
    Props,
    Attrs,
    Keyframes,
    DefineConsts,
    DefineVars,
    CreateTheme,
    DefineMarker,
    DefaultMarker,
    ViewTransitionClass,
    PositionTry,
    FirstThatWorks,
}

impl StyleXRootCall {
    pub fn name(self) -> &'static str {
        match self {
            StyleXRootCall::Create => "create",
            StyleXRootCall::Props => "props",
            StyleXRootCall::Attrs => "attrs",
            StyleXRootCall::Keyframes => "keyframes",
            StyleXRootCall::DefineConsts => "defineConsts",
            StyleXRootCall::DefineVars => "defineVars",
            StyleXRootCall::CreateTheme => "createTheme",
            StyleXRootCall::DefineMarker => "defineMarker",
            StyleXRootCall::DefaultMarker => "defaultMarker",
            StyleXRootCall::ViewTransitionClass => "viewTransitionClass",
            StyleXRootCall::PositionTry => "positionTry",
            StyleXRootCall::FirstThatWorks => "firstThatWorks",
        }
    }

    /// StyleX hashes these definitions by file path, so it accepts them only in theme files and
    /// resolves references to them only through imports of theme files.
    pub fn defines_theme_value(self) -> bool {
        matches!(
            self,
            StyleXRootCall::DefineVars
                | StyleXRootCall::DefineConsts
                | StyleXRootCall::DefineMarker
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleXWhenRelation {
    Ancestor,
    Descendant,
    SiblingBefore,
    SiblingAfter,
    AnySibling,
}

impl StyleXWhenRelation {
    pub fn name(self) -> &'static str {
        match self {
            StyleXWhenRelation::Ancestor => "ancestor",
            StyleXWhenRelation::Descendant => "descendant",
            StyleXWhenRelation::SiblingBefore => "siblingBefore",
            StyleXWhenRelation::SiblingAfter => "siblingAfter",
            StyleXWhenRelation::AnySibling => "anySibling",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleXTypeCall {
    Angle,
    Color,
    Url,
    Image,
    Integer,
    LengthPercentage,
    Length,
    Percentage,
    Number,
    Resolution,
    Time,
    TransformFunction,
    TransformList,
}

impl StyleXTypeCall {
    pub fn name(self) -> &'static str {
        match self {
            StyleXTypeCall::Angle => "angle",
            StyleXTypeCall::Color => "color",
            StyleXTypeCall::Url => "url",
            StyleXTypeCall::Image => "image",
            StyleXTypeCall::Integer => "integer",
            StyleXTypeCall::LengthPercentage => "lengthPercentage",
            StyleXTypeCall::Length => "length",
            StyleXTypeCall::Percentage => "percentage",
            StyleXTypeCall::Number => "number",
            StyleXTypeCall::Resolution => "resolution",
            StyleXTypeCall::Time => "time",
            StyleXTypeCall::TransformFunction => "transformFunction",
            StyleXTypeCall::TransformList => "transformList",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StyleXIntrinsic {
    Root(StyleXRootIntrinsic),
    When { relation: StyleXWhenRelation, marker: bool },
    Types(StyleXTypeCall),
}

impl StyleXIntrinsic {
    pub(crate) fn qualified_name(self) -> String {
        let module_name = match self {
            StyleXIntrinsic::Root(_) => "Iris.StyleX",
            StyleXIntrinsic::When { .. } => "Iris.StyleX.When",
            StyleXIntrinsic::Types(_) => "Iris.StyleX.Types",
        };
        let name = match self {
            StyleXIntrinsic::Root(intrinsic) => intrinsic.name(),
            StyleXIntrinsic::When { relation, marker: false } => relation.name(),
            StyleXIntrinsic::When { relation: StyleXWhenRelation::Ancestor, marker: true } => {
                "ancestorMarker"
            }
            StyleXIntrinsic::When { relation: StyleXWhenRelation::Descendant, marker: true } => {
                "descendantMarker"
            }
            StyleXIntrinsic::When { relation: StyleXWhenRelation::SiblingBefore, marker: true } => {
                "siblingBeforeMarker"
            }
            StyleXIntrinsic::When { relation: StyleXWhenRelation::SiblingAfter, marker: true } => {
                "siblingAfterMarker"
            }
            StyleXIntrinsic::When { relation: StyleXWhenRelation::AnySibling, marker: true } => {
                "anySiblingMarker"
            }
            StyleXIntrinsic::Types(call) => call.name(),
        };
        format!("{module_name}.{name}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StyleXRootIntrinsic {
    Call(StyleXRootCall),
    RecordProps,
    RecordAttrs,
    MarkerStyle,
    DynamicStyle,
    Conditional,
    ConditionalValue,
    ConditionalCase,
}

impl StyleXRootIntrinsic {
    fn name(self) -> &'static str {
        match self {
            StyleXRootIntrinsic::Call(call) => call.name(),
            StyleXRootIntrinsic::RecordProps => "recordProps",
            StyleXRootIntrinsic::RecordAttrs => "recordAttrs",
            StyleXRootIntrinsic::MarkerStyle => "markerStyle",
            StyleXRootIntrinsic::DynamicStyle => "dynamicStyle",
            StyleXRootIntrinsic::Conditional => "conditional",
            StyleXRootIntrinsic::ConditionalValue => "conditionalValue",
            StyleXRootIntrinsic::ConditionalCase => "conditionalCase",
        }
    }
}
