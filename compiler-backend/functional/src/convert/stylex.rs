//! Recognition and lowering for the virtual StyleX modules.

use building_types::QueryResult;
use files::FileId;
use indexing::TermItemId;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use crate::error::UnsupportedState;
use crate::optimize::{
    inline_simple_bindings, reachable_expressions, try_for_each_expression_child,
};
use crate::stylex::{
    StyleXCallTarget, StyleXCondition, StyleXConditionalCase, StyleXExpression, StyleXIntrinsic,
    StyleXRootCall, StyleXRootIntrinsic, StyleXTypeCall, StyleXWhenRelation,
};
use crate::tree::{
    Binding, Declaration, DeclarationKind, ExpressionId, ExpressionKind, Field, GlobalId, LocalId,
    Parameter, PatternKind, RecordField,
};

use super::{Context, ConversionResult, term_declaration};

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
    DefineConsts,
    DefineVars,
    CreateTheme,
    ViewTransitionClass,
    PositionTry,
}

#[derive(Default)]
struct StyleXStaticBindings {
    globals: FxHashMap<GlobalId, ExpressionId>,
    locals: FxHashMap<LocalId, ExpressionId>,
    imports: FxHashMap<GlobalId, bool>,
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
            (
                StyleXIntrinsic::Root(
                    StyleXRootIntrinsic::MarkerStyle | StyleXRootIntrinsic::DynamicStyle,
                ),
                [style],
            ) => Some(*style),
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
            (StyleXIntrinsic::Root(StyleXRootIntrinsic::ConditionalCase), [condition, value]) => {
                let condition = StyleXCondition::Expression(*condition);
                Some(self.stylex_conditional_case(condition, *value))
            }
            (StyleXIntrinsic::When { relation, marker: false }, [selector, value]) => {
                let condition =
                    StyleXCondition::When { relation, selector: *selector, marker: None };
                Some(self.stylex_conditional_case(condition, *value))
            }
            (StyleXIntrinsic::When { relation, marker: true }, [selector, marker, value]) => {
                let marker = Some(*marker);
                let condition = StyleXCondition::When { relation, selector: *selector, marker };
                Some(self.stylex_conditional_case(condition, *value))
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
        condition: StyleXCondition,
        value: ExpressionId,
    ) -> ExpressionId {
        let case = StyleXConditionalCase { condition, value };
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

        let mut labels = Vec::new();
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

    /// Returns the imported theme values that StyleX evaluates statically in this module.
    pub(super) fn validate_stylex_uses(
        &self,
        declarations: &[Declaration],
    ) -> ConversionResult<FxHashSet<GlobalId>> {
        // StyleX intrinsics and expressions only arise from references to the virtual modules.
        if !self.references_stylex_module && !self.module_is_virtual(self.file_id) {
            return Ok(FxHashSet::default());
        }
        let mut bindings = StyleXStaticBindings::default();
        for declaration in declarations {
            let DeclarationKind::Value(expression) = declaration.kind else { continue };
            if declaration.recursive_group.is_none() {
                bindings.globals.insert(declaration.global.id, expression);
            }
        }
        let roots = declarations.iter().filter_map(|declaration| match declaration.kind {
            DeclarationKind::Value(expression) => Some(expression),
            _ => None,
        });
        for expression in reachable_expressions(&self.storage, roots) {
            if let ExpressionKind::Let { recursive: false, bindings: locals, .. } =
                &self.storage[expression].kind
            {
                for binding in locals.iter() {
                    bindings.locals.insert(binding.parameter.id, binding.expression);
                }
            }
        }
        for declaration in declarations {
            let DeclarationKind::Value(expression) = declaration.kind else { continue };
            self.validate_stylex_expression(
                expression,
                declaration,
                StyleXStaticContext::None,
                &mut bindings,
            )?;
        }
        let theme_imports = bindings.imports.into_iter().filter(|&(_, theme)| theme);
        Ok(theme_imports.map(|(global, _)| global).collect())
    }

    fn validate_stylex_expression(
        &self,
        expression: ExpressionId,
        declaration: &Declaration,
        context: StyleXStaticContext,
        bindings: &mut StyleXStaticBindings,
    ) -> ConversionResult<()> {
        match &self.storage[expression].kind {
            ExpressionKind::Global { global }
                if let GlobalId::Term(file_id, term_id) = global.id
                    && let Some(intrinsic) =
                        self.stylex_intrinsic_identity(file_id, term_id)? =>
            {
                let state = UnsupportedState::InvalidStyleXUse {
                    function: intrinsic.qualified_name(),
                    declaration: declaration.global.id,
                };
                return Err(self.unsupported(state));
            }
            ExpressionKind::StyleX(stylex) => {
                let child_context = match stylex {
                    StyleXExpression::Call { target: StyleXCallTarget::Root(call), arguments } => {
                        let child_context = self.validate_stylex_root_call(
                            *call,
                            expression,
                            declaration,
                            context,
                            bindings,
                        )?;
                        if matches!(
                            call,
                            StyleXRootCall::Create
                                | StyleXRootCall::Keyframes
                                | StyleXRootCall::DefineVars
                                | StyleXRootCall::DefineConsts
                                | StyleXRootCall::CreateTheme
                                | StyleXRootCall::PositionTry
                                | StyleXRootCall::ViewTransitionClass
                        ) {
                            let record_literal = matches!(
                                arguments.as_ref(),
                                [argument]
                                    if matches!(
                                        self.storage[*argument].kind,
                                        ExpressionKind::Record { .. }
                                    )
                            );
                            if (*call == StyleXRootCall::Create
                                || *call == StyleXRootCall::Keyframes
                                    && context == StyleXStaticContext::None)
                                && !record_literal
                            {
                                return Err(self.invalid_stylex_context(
                                    StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(*call)),
                                    "requires a record literal after inlining",
                                    declaration.global.id,
                                ));
                            }
                            let mut visiting = FxHashSet::default();
                            if *call == StyleXRootCall::Create
                                && let [namespaces] = arguments.as_ref()
                                && let ExpressionKind::Record { fields } =
                                    &self.storage[*namespaces].kind
                            {
                                for field in fields.iter() {
                                    self.validate_stylex_namespace(
                                        field.expression,
                                        declaration,
                                        bindings,
                                        &mut visiting,
                                    )?;
                                }
                            } else {
                                for &argument in arguments.iter() {
                                    self.validate_stylex_static_expression(
                                        argument,
                                        *call,
                                        declaration,
                                        bindings,
                                        &mut visiting,
                                    )?;
                                }
                            }
                        }
                        child_context
                    }
                    StyleXExpression::Call { target: StyleXCallTarget::Types(call), .. } => {
                        if !matches!(
                            context,
                            StyleXStaticContext::DefineVars | StyleXStaticContext::CreateTheme
                        ) {
                            return Err(self.invalid_stylex_context(
                                StyleXIntrinsic::Types(*call),
                                "must be used inside defineVars or createTheme",
                                declaration.global.id,
                            ));
                        }
                        context
                    }
                    StyleXExpression::ConditionalCase(case) => {
                        return Err(self.invalid_stylex_context(
                            case.intrinsic(),
                            "must be used directly in a conditionalValue case array",
                            declaration.global.id,
                        ));
                    }
                    StyleXExpression::ConditionalValue { .. } => {
                        if context != StyleXStaticContext::Create {
                            return Err(self.invalid_stylex_context(
                                StyleXIntrinsic::Root(StyleXRootIntrinsic::ConditionalValue),
                                "must be used inside create",
                                declaration.global.id,
                            ));
                        }
                        context
                    }
                    StyleXExpression::Conditional { .. } => context,
                };
                return stylex.try_for_each_child(|child| {
                    self.validate_stylex_expression(child, declaration, child_context, bindings)
                });
            }
            _ => {}
        }
        try_for_each_expression_child(&self.storage[expression].kind, |child| {
            self.validate_stylex_expression(child, declaration, context, bindings)
        })
    }

    fn validate_stylex_static_expression(
        &self,
        expression: ExpressionId,
        call: StyleXRootCall,
        declaration: &Declaration,
        bindings: &mut StyleXStaticBindings,
        visiting: &mut FxHashSet<ExpressionId>,
    ) -> ConversionResult<()> {
        let invalid = || {
            let requirement = "requires statically evaluable arguments; runtime parameters, \
                functions, and ordinary function calls are not supported";
            let intrinsic = StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(call));
            self.invalid_stylex_context(intrinsic, requirement, declaration.global.id)
        };
        if !visiting.insert(expression) {
            return Err(invalid());
        }
        match &self.storage[expression].kind {
            ExpressionKind::Global { global } => {
                if let Some(&value) = bindings.globals.get(&global.id) {
                    // These definitions are transformed independently before their references
                    // are evaluated. Their arguments are checked at the defining call.
                    if !matches!(
                        self.storage[value].kind,
                        ExpressionKind::StyleX(StyleXExpression::Call {
                            target: StyleXCallTarget::Root(
                                StyleXRootCall::Create
                                    | StyleXRootCall::Keyframes
                                    | StyleXRootCall::DefineVars
                                    | StyleXRootCall::DefineConsts
                                    | StyleXRootCall::DefineMarker
                                    | StyleXRootCall::CreateTheme
                                    | StyleXRootCall::PositionTry
                                    | StyleXRootCall::ViewTransitionClass
                            ),
                            ..
                        })
                    ) {
                        self.validate_stylex_static_expression(
                            value,
                            call,
                            declaration,
                            bindings,
                            visiting,
                        )?;
                    }
                } else if let GlobalId::Term(file_id, term_id) = global.id
                    && file_id != self.file_id
                {
                    let static_import = if call == StyleXRootCall::DefineConsts {
                        false
                    } else if let Some(&static_import) = bindings.imports.get(&global.id) {
                        static_import
                    } else {
                        let static_import = self.stylex_import_is_static(file_id, term_id)?;
                        bindings.imports.insert(global.id, static_import);
                        static_import
                    };
                    if !static_import {
                        let module = self.source_module_name(file_id)?;
                        let name = &global.item_name;
                        let requirement = if call == StyleXRootCall::DefineConsts {
                            format!(
                                "cannot use imported value '{module}.{name}'; defineConsts arguments must use same-module static values"
                            )
                        } else {
                            format!(
                                "cannot statically evaluate imported value '{module}.{name}'; use a same-module static value or an exported defineVars, defineConsts, or defineMarker definition"
                            )
                        };
                        return Err(self.invalid_stylex_context(
                            StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(call)),
                            &requirement,
                            declaration.global.id,
                        ));
                    }
                } else {
                    return Err(invalid());
                }
            }
            ExpressionKind::Local { parameter } => {
                let value = bindings.locals.get(&parameter.id).ok_or_else(invalid)?;
                self.validate_stylex_static_expression(
                    *value,
                    call,
                    declaration,
                    bindings,
                    visiting,
                )?;
            }
            ExpressionKind::StyleX(StyleXExpression::Call {
                target: StyleXCallTarget::Root(StyleXRootCall::Props | StyleXRootCall::Attrs),
                ..
            }) => return Err(invalid()),
            ExpressionKind::Literal { .. }
            | ExpressionKind::Array { .. }
            | ExpressionKind::Record { .. }
            | ExpressionKind::Project { .. }
            | ExpressionKind::RecordUpdate { .. }
            | ExpressionKind::Unary { .. }
            | ExpressionKind::Binary { .. }
            | ExpressionKind::IfThenElse { .. }
            | ExpressionKind::StyleX(_) => {
                try_for_each_expression_child(&self.storage[expression].kind, |child| {
                    self.validate_stylex_static_expression(
                        child,
                        call,
                        declaration,
                        bindings,
                        visiting,
                    )
                })?;
            }
            _ => return Err(invalid()),
        }
        visiting.remove(&expression);
        Ok(())
    }

    /// A `create` namespace is a record of static declarations, or a function of one named
    /// parameter returning such a record, which StyleX compiles to CSS variables set at runtime.
    fn validate_stylex_namespace(
        &self,
        namespace: ExpressionId,
        declaration: &Declaration,
        bindings: &mut StyleXStaticBindings,
        visiting: &mut FxHashSet<ExpressionId>,
    ) -> ConversionResult<()> {
        let ExpressionKind::Abstraction { parameters, body } = &self.storage[namespace].kind else {
            let call = StyleXRootCall::Create;
            return self.validate_stylex_static_expression(
                namespace,
                call,
                declaration,
                bindings,
                visiting,
            );
        };
        let invalid = |requirement: &str| {
            let intrinsic =
                StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(StyleXRootCall::Create));
            self.invalid_stylex_context(intrinsic, requirement, declaration.global.id)
        };
        let [pattern] = parameters.as_ref() else {
            return Err(invalid(
                "requires dynamic namespaces to take exactly one parameter; group several values in a record",
            ));
        };
        let unused = "requires dynamic namespaces to use their parameter; write a namespace that ignores it as a record";
        let parameter = match &self.storage[*pattern].kind {
            PatternKind::Variable(parameter) => parameter,
            // Annotated wildcards lower to a generated name over a wildcard.
            PatternKind::Named { parameter, pattern }
                if matches!(self.storage[*pattern].kind, PatternKind::Wildcard) =>
            {
                parameter
            }
            PatternKind::Wildcard => return Err(invalid(unused)),
            _ => {
                return Err(invalid(
                    "requires dynamic namespaces to take a named parameter; read its fields in the body instead of destructuring it",
                ));
            }
        };
        if !matches!(self.storage[*body].kind, ExpressionKind::Record { .. }) {
            return Err(invalid("requires dynamic namespaces to return a record literal"));
        }
        let mut used = false;
        self.validate_stylex_dynamic_expression(
            *body,
            parameter.id,
            &mut used,
            declaration,
            bindings,
            visiting,
        )?;
        if !used {
            return Err(invalid(unused));
        }
        Ok(())
    }

    /// Values in a dynamic namespace may read the parameter, but StyleX only accepts an arrow
    /// function whose body is an object literal, so the body must render inline: no branches,
    /// record updates, or calls. Condition keys stay static.
    fn validate_stylex_dynamic_expression(
        &self,
        expression: ExpressionId,
        parameter: LocalId,
        used: &mut bool,
        declaration: &Declaration,
        bindings: &mut StyleXStaticBindings,
        visiting: &mut FxHashSet<ExpressionId>,
    ) -> ConversionResult<()> {
        let call = StyleXRootCall::Create;
        let kind = &self.storage[expression].kind;
        match kind {
            ExpressionKind::Local { parameter: local } if local.id == parameter => {
                *used = true;
                Ok(())
            }
            ExpressionKind::StyleX(StyleXExpression::ConditionalValue { default, cases }) => {
                self.validate_stylex_dynamic_expression(
                    *default,
                    parameter,
                    used,
                    declaration,
                    bindings,
                    visiting,
                )?;
                for case in cases.iter() {
                    let conditions = match case.condition {
                        StyleXCondition::Expression(condition) => [Some(condition), None],
                        StyleXCondition::When { selector, marker, .. } => [Some(selector), marker],
                    };
                    for condition in conditions.into_iter().flatten() {
                        self.validate_stylex_static_expression(
                            condition,
                            call,
                            declaration,
                            bindings,
                            visiting,
                        )?;
                    }
                    self.validate_stylex_dynamic_expression(
                        case.value,
                        parameter,
                        used,
                        declaration,
                        bindings,
                        visiting,
                    )?;
                }
                Ok(())
            }
            ExpressionKind::Application { .. }
            | ExpressionKind::UncurriedApplication { .. }
            | ExpressionKind::Abstraction { .. }
            | ExpressionKind::UncurriedAbstraction { .. }
            | ExpressionKind::IfThenElse { .. }
            | ExpressionKind::RecordUpdate { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. }
            | ExpressionKind::Effect { .. } => {
                let intrinsic = StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(call));
                let requirement = "requires dynamic namespace values to be built from the parameter, \
                    its fields, literals, records, arrays, and operators; compute other values in the \
                    caller and pass them through the parameter";
                Err(self.invalid_stylex_context(intrinsic, requirement, declaration.global.id))
            }
            ExpressionKind::Literal { .. }
            | ExpressionKind::Array { .. }
            | ExpressionKind::Record { .. }
            | ExpressionKind::Project { .. }
            | ExpressionKind::Unary { .. }
            | ExpressionKind::Binary { .. } => try_for_each_expression_child(kind, |child| {
                self.validate_stylex_dynamic_expression(
                    child,
                    parameter,
                    used,
                    declaration,
                    bindings,
                    visiting,
                )
            }),
            _ => self.validate_stylex_static_expression(
                expression,
                call,
                declaration,
                bindings,
                visiting,
            ),
        }
    }

    fn stylex_import_is_static(
        &self,
        file_id: FileId,
        term_id: TermItemId,
    ) -> ConversionResult<bool> {
        // commonJS resolution hashes the imported export name; it does not read its value.
        // Following an ordinary alias here would authorize a hash with no matching definition.
        // Lower only this declaration, without querying or validating its functional module.
        let mut context = Context::new(self.queries, file_id)?;
        let Some(declaration) = term_declaration(&mut context, term_id, true)? else {
            return Ok(false);
        };
        if declaration.recursive_group.is_some() {
            return Ok(false);
        }
        let DeclarationKind::Value(expression) = declaration.kind else {
            return Ok(false);
        };
        let recursive_globals =
            context.recursive_groups.keys().map(|&term_id| GlobalId::Term(file_id, term_id));
        let recursive_globals = recursive_globals.collect();
        inline_simple_bindings(&mut context.storage, expression, &recursive_globals);
        Ok(matches!(
            context.storage[expression].kind,
            ExpressionKind::StyleX(StyleXExpression::Call {
                target: StyleXCallTarget::Root(call),
                ..
            }) if call.defines_theme_value()
        ))
    }

    fn validate_stylex_root_call(
        &self,
        call: StyleXRootCall,
        expression: ExpressionId,
        declaration: &Declaration,
        context: StyleXStaticContext,
        bindings: &StyleXStaticBindings,
    ) -> ConversionResult<StyleXStaticContext> {
        let direct_initializer = bindings.globals.get(&declaration.global.id) == Some(&expression);
        let required_context = match call {
            StyleXRootCall::Create => StyleXStaticContext::Create,
            StyleXRootCall::Keyframes => StyleXStaticContext::Keyframes,
            StyleXRootCall::DefineConsts => StyleXStaticContext::DefineConsts,
            StyleXRootCall::DefineVars => StyleXStaticContext::DefineVars,
            StyleXRootCall::CreateTheme => StyleXStaticContext::CreateTheme,
            StyleXRootCall::ViewTransitionClass => StyleXStaticContext::ViewTransitionClass,
            StyleXRootCall::PositionTry => StyleXStaticContext::PositionTry,
            _ => context,
        };
        if call == StyleXRootCall::Keyframes
            && !direct_initializer
            && !bindings.locals.values().any(|&value| value == expression)
            && !matches!(
                context,
                StyleXStaticContext::Create
                    | StyleXStaticContext::DefineVars
                    | StyleXStaticContext::CreateTheme
                    | StyleXStaticContext::ViewTransitionClass
            )
        {
            return Err(self.invalid_stylex_context(
                StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(call)),
                "must directly initialize a non-recursive value or be used inside create, defineVars, createTheme, or viewTransitionClass",
                declaration.global.id,
            ));
        }
        if call == StyleXRootCall::Create && context != StyleXStaticContext::None {
            return Err(self.invalid_stylex_context(
                StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(call)),
                "cannot be used inside another static StyleX call",
                declaration.global.id,
            ));
        }
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
                StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(call)),
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
                StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(call)),
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
                StyleXIntrinsic::Root(StyleXRootIntrinsic::Call(call)),
                "must be used inside create, keyframes, positionTry, or viewTransitionClass",
                declaration.global.id,
            ));
        }
        Ok(required_context)
    }

    fn invalid_stylex_context(
        &self,
        intrinsic: StyleXIntrinsic,
        requirement: &str,
        declaration: GlobalId,
    ) -> super::ConversionError {
        self.unsupported(UnsupportedState::InvalidStyleXContext {
            function: intrinsic.qualified_name(),
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
        "markerStyle" => return Some(StyleXRootIntrinsic::MarkerStyle),
        "dynamicStyle" => return Some(StyleXRootIntrinsic::DynamicStyle),
        "conditional" => return Some(StyleXRootIntrinsic::Conditional),
        "conditionalValue" => return Some(StyleXRootIntrinsic::ConditionalValue),
        "conditionalCase" => return Some(StyleXRootIntrinsic::ConditionalCase),
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
