//! Recognition and lowering for the virtual StyleX modules.

use building_types::QueryResult;
use files::FileId;
use indexing::TermItemId;
use smol_str::SmolStr;

use crate::error::UnsupportedState;
use crate::optimize::try_for_each_expression_child;
use crate::stylex::{
    StyleXCallTarget, StyleXConditionalCase, StyleXExpression, StyleXIntrinsic, StyleXRootCall,
    StyleXRootIntrinsic, StyleXTypeCall, StyleXWhenRelation,
};
use crate::tree::{
    Binding, Declaration, DeclarationKind, ExpressionId, ExpressionKind, Field, GlobalId,
    Parameter, RecordField,
};

use super::{Context, ConversionResult};

/// Files of the virtual StyleX modules, which have no runtime representation.
#[derive(Debug, Clone, Copy)]
pub(super) struct StyleXModules {
    root: Option<FileId>,
    when: Option<FileId>,
    types: Option<FileId>,
}

#[derive(Debug, Clone, Copy)]
enum StyleXModule {
    Root,
    When,
    Types,
}

impl StyleXModules {
    fn module(&self, file_id: FileId) -> Option<StyleXModule> {
        let file_id = Some(file_id);
        if file_id == self.root {
            Some(StyleXModule::Root)
        } else if file_id == self.when {
            Some(StyleXModule::When)
        } else if file_id == self.types {
            Some(StyleXModule::Types)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StyleXStaticContext {
    None,
    Create,
    Keyframes,
    DefineVars,
    CreateTheme,
    ViewTransitionClass,
    PositionTry,
}

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn stylex_intrinsic(
        &mut self,
        expression: ExpressionId,
        arguments: &[ExpressionId],
        result_type: Option<checking::TypeId>,
    ) -> ConversionResult<Option<ExpressionId>> {
        let ExpressionKind::Global { global } = &self.storage[expression].kind else {
            return Ok(None);
        };
        let GlobalId::Term(file_id, term_id) = global.id else { return Ok(None) };
        let Some(intrinsic) = self.stylex_intrinsic_identity(file_id, term_id)? else {
            return Ok(None);
        };
        let expression = match (intrinsic, arguments) {
            (
                StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(
                    call @ (StyleXRootCall::Create
                    | StyleXRootCall::Props
                    | StyleXRootCall::Attrs
                    | StyleXRootCall::DefineVars),
                )),
                [_, argument],
            ) => Some(self.stylex_call(StyleXCallTarget::Root(call), [*argument])),
            (
                StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(
                    call @ (StyleXRootCall::Keyframes
                    | StyleXRootCall::DefineConsts
                    | StyleXRootCall::ViewTransitionClass
                    | StyleXRootCall::PositionTry),
                )),
                [argument],
            ) => Some(self.stylex_call(StyleXCallTarget::Root(call), [*argument])),
            (
                StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(StyleXRootCall::CreateTheme)),
                [_, variables, overrides],
            ) => Some(self.stylex_call(
                StyleXCallTarget::Root(StyleXRootCall::CreateTheme),
                [*variables, *overrides],
            )),
            (
                StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(StyleXRootCall::FirstThatWorks)),
                [argument],
            ) => {
                let target = StyleXCallTarget::Root(StyleXRootCall::FirstThatWorks);
                let ExpressionKind::Array { elements } = &self.storage[*argument].kind else {
                    return Ok(None);
                };
                if elements.is_empty() {
                    return Ok(None);
                }
                let elements = elements.to_vec();
                Some(self.stylex_call(target, elements))
            }
            (StyleXIntrinsic::Root(StyleXRootIntrinsic::RecordProps), [_, argument]) => {
                let Some(result_type) = result_type else { return Ok(None) };
                self.stylex_record_map(*argument, result_type, StyleXRootCall::Props)?
            }
            (StyleXIntrinsic::Root(StyleXRootIntrinsic::RecordAttrs), [_, argument]) => {
                let Some(result_type) = result_type else { return Ok(None) };
                self.stylex_record_map(*argument, result_type, StyleXRootCall::Attrs)?
            }
            (StyleXIntrinsic::Root(StyleXRootIntrinsic::Conditional), [condition, style]) => {
                Some(self.expression(ExpressionKind::StyleX(StyleXExpression::Conditional {
                    condition: *condition,
                    style: *style,
                })))
            }
            (StyleXIntrinsic::Root(StyleXRootIntrinsic::ConditionalValue), [default, cases]) => {
                let ExpressionKind::Array { elements } = &self.storage[*cases].kind else {
                    return Ok(None);
                };
                let mut converted = Vec::with_capacity(elements.len());
                for element in elements.iter() {
                    let ExpressionKind::StyleX(StyleXExpression::ConditionalCase(case)) =
                        &self.storage[*element].kind
                    else {
                        return Ok(None);
                    };
                    converted.push(StyleXConditionalCase::clone(case));
                }
                Some(self.expression(ExpressionKind::StyleX(StyleXExpression::ConditionalValue {
                    default: *default,
                    cases: converted.into(),
                })))
            }
            (StyleXIntrinsic::When { relation, marker: false }, [selector, value]) => {
                Some(self.stylex_conditional_case(relation, *selector, None, *value))
            }
            (StyleXIntrinsic::When { relation, marker: true }, [selector, marker, value]) => {
                Some(self.stylex_conditional_case(relation, *selector, Some(*marker), *value))
            }
            (StyleXIntrinsic::Types(call), [_, argument]) => {
                Some(self.stylex_call(StyleXCallTarget::Types(call), [*argument]))
            }
            _ => None,
        };
        Ok(expression)
    }

    pub(super) fn stylex_value_intrinsic(
        &mut self,
        file_id: FileId,
        term_id: TermItemId,
    ) -> ConversionResult<Option<ExpressionId>> {
        let Some(intrinsic) = self.stylex_intrinsic_identity(file_id, term_id)? else {
            return Ok(None);
        };
        self.references_stylex_module = true;
        let StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(call)) = intrinsic else {
            return Ok(None);
        };
        if let StyleXRootCall::DefineMarker | StyleXRootCall::DefaultMarker = call {
            Ok(Some(self.stylex_call(StyleXCallTarget::Root(call), [])))
        } else {
            Ok(None)
        }
    }

    fn stylex_call(
        &mut self,
        target: StyleXCallTarget,
        arguments: impl IntoIterator<Item = ExpressionId>,
    ) -> ExpressionId {
        let arguments = arguments.into_iter().collect();
        self.expression(ExpressionKind::StyleX(StyleXExpression::Call { target, arguments }))
    }

    fn stylex_conditional_case(
        &mut self,
        relation: StyleXWhenRelation,
        selector: ExpressionId,
        marker: Option<ExpressionId>,
        value: ExpressionId,
    ) -> ExpressionId {
        let case = StyleXConditionalCase { relation, selector, marker, value };
        self.expression(ExpressionKind::StyleX(StyleXExpression::ConditionalCase(case)))
    }

    fn stylex_record_map(
        &mut self,
        argument: ExpressionId,
        result_type: checking::TypeId,
        call: StyleXRootCall,
    ) -> ConversionResult<Option<ExpressionId>> {
        let checking::Type::Application(_, mut row_type) = *self.queries.lookup_type(result_type)
        else {
            return Ok(None);
        };

        let mut labels = vec![];
        loop {
            let checking::Type::Row(row_id) = *self.queries.lookup_type(row_type) else {
                return Ok(None);
            };
            let row = self.queries.lookup_row_type(row_id);
            labels.extend(row.fields.iter().map(|field| SmolStr::clone(&field.label)));
            let Some(tail) = row.tail else { break };
            row_type = tail;
        }

        let stable = self.expression_is_stable(argument);
        let (record, parameter) = if stable {
            (argument, None)
        } else {
            let parameter = self.fresh_parameter("stylexStyles".into())?;
            let record =
                self.expression(ExpressionKind::Local { parameter: Parameter::clone(&parameter) });
            (record, Some(parameter))
        };

        let fields = labels.into_iter().map(|label| {
            let field = self.label_field(label);
            let style =
                self.expression(ExpressionKind::Project { record, field: Field::clone(&field) });
            let expression = self.stylex_call(StyleXCallTarget::Root(call), [style]);
            RecordField { field, expression }
        });

        let fields = fields.collect();
        let body = self.expression(ExpressionKind::Record { fields });

        let Some(parameter) = parameter else { return Ok(Some(body)) };
        let binding = Binding { parameter, expression: argument, source_order: 0 };

        Ok(Some(self.expression(ExpressionKind::Let {
            recursive: false,
            bindings: [binding].into(),
            body,
        })))
    }

    fn stylex_intrinsic_identity(
        &self,
        file_id: FileId,
        term_id: TermItemId,
    ) -> QueryResult<Option<StyleXIntrinsic>> {
        let Some(module) = self.stylex_modules().module(file_id) else {
            return Ok(None);
        };
        let indexed = self.indexed_module(file_id)?;
        let Some(name) = indexed.items[term_id].name.as_deref() else {
            return Ok(None);
        };
        let intrinsic = match module {
            StyleXModule::Root => stylex_root_intrinsic(name).map(StyleXIntrinsic::Root),
            StyleXModule::When => stylex_when_intrinsic(name),
            StyleXModule::Types => stylex_type_intrinsic(name).map(StyleXIntrinsic::Types),
        };
        Ok(intrinsic)
    }

    pub(super) fn validate_stylex_uses(
        &self,
        declarations: &[Declaration],
    ) -> ConversionResult<()> {
        // StyleX intrinsics and expressions only arise from references to the virtual modules.
        if !self.references_stylex_module && !self.module_is_virtual(self.file_id) {
            return Ok(());
        }
        for declaration in declarations {
            let DeclarationKind::Value(expression) = declaration.kind else { continue };
            self.validate_stylex_expression(
                expression,
                expression,
                declaration,
                StyleXStaticContext::None,
            )?;
        }
        Ok(())
    }

    fn validate_stylex_expression(
        &self,
        expression: ExpressionId,
        root: ExpressionId,
        declaration: &Declaration,
        context: StyleXStaticContext,
    ) -> ConversionResult<()> {
        match &self.storage[expression].kind {
            ExpressionKind::Global { global }
                if let GlobalId::Term(file_id, term_id) = global.id
                    && let Some(intrinsic) =
                        self.stylex_intrinsic_identity(file_id, term_id)? =>
            {
                let state = UnsupportedState::InvalidStyleXUse {
                    function: intrinsic.name().to_owned(),
                    declaration: declaration.global.id,
                };
                return Err(self.unsupported(state));
            }
            ExpressionKind::StyleX(stylex) => {
                let child_context = match stylex {
                    StyleXExpression::Call { target: StyleXCallTarget::Root(call), .. } => self
                        .validate_stylex_root_call(*call, expression, root, declaration, context)?,
                    StyleXExpression::Call { target: StyleXCallTarget::Types(call), .. } => {
                        if !matches!(
                            context,
                            StyleXStaticContext::DefineVars | StyleXStaticContext::CreateTheme
                        ) {
                            return Err(self.invalid_stylex_context(
                                call.name(),
                                "must be used inside defineVars or createTheme",
                                declaration.global.id,
                            ));
                        }
                        context
                    }
                    StyleXExpression::ConditionalCase(case) => {
                        return Err(self.invalid_stylex_context(
                            case.relation.name(),
                            "must be used directly in a conditionalValue case array",
                            declaration.global.id,
                        ));
                    }
                    StyleXExpression::ConditionalValue { .. } => {
                        if context != StyleXStaticContext::Create {
                            return Err(self.invalid_stylex_context(
                                "conditionalValue",
                                "must be used inside create",
                                declaration.global.id,
                            ));
                        }
                        context
                    }
                    StyleXExpression::Conditional { .. } => context,
                };
                return stylex.try_for_each_child(|child| {
                    self.validate_stylex_expression(child, root, declaration, child_context)
                });
            }
            _ => {}
        }
        try_for_each_expression_child(&self.storage[expression].kind, |child| {
            self.validate_stylex_expression(child, root, declaration, context)
        })
    }

    fn validate_stylex_root_call(
        &self,
        call: StyleXRootCall,
        expression: ExpressionId,
        root: ExpressionId,
        declaration: &Declaration,
        context: StyleXStaticContext,
    ) -> ConversionResult<StyleXStaticContext> {
        let direct_initializer = expression == root && declaration.recursive_group.is_none();
        let required_context = match call {
            StyleXRootCall::Create => StyleXStaticContext::Create,
            StyleXRootCall::Keyframes => StyleXStaticContext::Keyframes,
            StyleXRootCall::DefineVars => StyleXStaticContext::DefineVars,
            StyleXRootCall::CreateTheme => StyleXStaticContext::CreateTheme,
            StyleXRootCall::ViewTransitionClass => StyleXStaticContext::ViewTransitionClass,
            StyleXRootCall::PositionTry => StyleXStaticContext::PositionTry,
            _ => context,
        };
        let requires_direct_initializer = matches!(
            call,
            StyleXRootCall::DefineConsts
                | StyleXRootCall::DefineVars
                | StyleXRootCall::CreateTheme
                | StyleXRootCall::DefineMarker
                | StyleXRootCall::ViewTransitionClass
                | StyleXRootCall::PositionTry
        );
        if requires_direct_initializer && !direct_initializer {
            return Err(self.invalid_stylex_context(
                call.name(),
                "must directly initialize a non-recursive top-level value",
                declaration.global.id,
            ));
        }
        if matches!(
            call,
            StyleXRootCall::DefineConsts
                | StyleXRootCall::DefineVars
                | StyleXRootCall::DefineMarker
        ) && !declaration.exported
        {
            return Err(self.invalid_stylex_context(
                call.name(),
                "must initialize an exported top-level value",
                declaration.global.id,
            ));
        }
        if call == StyleXRootCall::FirstThatWorks
            && !matches!(
                context,
                StyleXStaticContext::Create
                    | StyleXStaticContext::Keyframes
                    | StyleXStaticContext::PositionTry
                    | StyleXStaticContext::ViewTransitionClass
            )
        {
            return Err(self.invalid_stylex_context(
                call.name(),
                "must be used inside create, keyframes, positionTry, or viewTransitionClass",
                declaration.global.id,
            ));
        }
        Ok(required_context)
    }

    fn invalid_stylex_context(
        &self,
        function: &str,
        requirement: &str,
        declaration: GlobalId,
    ) -> super::ConversionError {
        self.unsupported(UnsupportedState::InvalidStyleXContext {
            function: function.to_owned(),
            requirement: requirement.to_owned(),
            declaration,
        })
    }

    fn stylex_modules(&self) -> StyleXModules {
        *self.stylex_modules.get_or_init(|| StyleXModules {
            root: self.queries.module_file("Iris.StyleX"),
            when: self.queries.module_file("Iris.StyleX.When"),
            types: self.queries.module_file("Iris.StyleX.Types"),
        })
    }

    pub(super) fn module_is_virtual(&self, file_id: FileId) -> bool {
        self.stylex_modules().module(file_id).is_some()
    }

    pub(super) fn validate_runtime_reference(
        &self,
        file_id: FileId,
        term_id: TermItemId,
    ) -> ConversionResult<()> {
        if !self.module_is_virtual(file_id) {
            return Ok(());
        }
        let module_name = self.source_module_name(file_id)?.to_string();
        let indexed = self.indexed_module(file_id)?;
        let item_name = match &indexed.items[term_id].name {
            Some(name) => SmolStr::clone(name),
            None => self.term_fallback(term_id),
        };
        let item_name = item_name.to_string();
        let state = UnsupportedState::VirtualModuleRuntimeReference { module_name, item_name };
        Err(self.unsupported(state))
    }
}

fn stylex_root_intrinsic(name: &str) -> Option<StyleXRootIntrinsic> {
    let call = match name {
        "create" => StyleXRootCall::Create,
        "props" => StyleXRootCall::Props,
        "attrs" => StyleXRootCall::Attrs,
        "keyframes" => StyleXRootCall::Keyframes,
        "defineConsts" => StyleXRootCall::DefineConsts,
        "defineVars" => StyleXRootCall::DefineVars,
        "createTheme" => StyleXRootCall::CreateTheme,
        "defineMarker" => StyleXRootCall::DefineMarker,
        "defaultMarker" => StyleXRootCall::DefaultMarker,
        "viewTransitionClass" => StyleXRootCall::ViewTransitionClass,
        "positionTry" => StyleXRootCall::PositionTry,
        "firstThatWorks" => StyleXRootCall::FirstThatWorks,
        "recordProps" => return Some(StyleXRootIntrinsic::RecordProps),
        "recordAttrs" => return Some(StyleXRootIntrinsic::RecordAttrs),
        "conditional" => return Some(StyleXRootIntrinsic::Conditional),
        "conditionalValue" => return Some(StyleXRootIntrinsic::ConditionalValue),
        _ => return None,
    };
    Some(StyleXRootIntrinsic::Call(call))
}

fn stylex_when_intrinsic(name: &str) -> Option<StyleXIntrinsic> {
    let (relation, marker) = match name {
        "ancestor" => (StyleXWhenRelation::Ancestor, false),
        "ancestorMarker" => (StyleXWhenRelation::Ancestor, true),
        "descendant" => (StyleXWhenRelation::Descendant, false),
        "descendantMarker" => (StyleXWhenRelation::Descendant, true),
        "siblingBefore" => (StyleXWhenRelation::SiblingBefore, false),
        "siblingBeforeMarker" => (StyleXWhenRelation::SiblingBefore, true),
        "siblingAfter" => (StyleXWhenRelation::SiblingAfter, false),
        "siblingAfterMarker" => (StyleXWhenRelation::SiblingAfter, true),
        "anySibling" => (StyleXWhenRelation::AnySibling, false),
        "anySiblingMarker" => (StyleXWhenRelation::AnySibling, true),
        _ => return None,
    };
    Some(StyleXIntrinsic::When { relation, marker })
}

fn stylex_type_intrinsic(name: &str) -> Option<StyleXTypeCall> {
    match name {
        "angle" => Some(StyleXTypeCall::Angle),
        "color" => Some(StyleXTypeCall::Color),
        "url" => Some(StyleXTypeCall::Url),
        "image" => Some(StyleXTypeCall::Image),
        "integer" => Some(StyleXTypeCall::Integer),
        "lengthPercentage" => Some(StyleXTypeCall::LengthPercentage),
        "length" => Some(StyleXTypeCall::Length),
        "percentage" => Some(StyleXTypeCall::Percentage),
        "number" => Some(StyleXTypeCall::Number),
        "resolution" => Some(StyleXTypeCall::Resolution),
        "time" => Some(StyleXTypeCall::Time),
        "transformFunction" => Some(StyleXTypeCall::TransformFunction),
        "transformList" => Some(StyleXTypeCall::TransformList),
        _ => None,
    }
}
