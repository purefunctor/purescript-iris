use checking::core::Role;
use checking::core::pretty::Pretty;
use checking::error::{
    CapabilityBundleErrorKind, CapabilityErrorKind, CheckingError, ConstraintErrorKind,
    DeriveErrorKind, ErrorCrumb, ErrorKind, HoleErrorKind, InstanceErrorKind, TermErrorKind,
    TypeErrorKind, UnificationErrorKind,
};
use checking::holes::HoleBinding;
use files::FileId;
use foreign_javascript::ForeignError;
use functional::tree::{GlobalId, InstanceIdentity};
use functional::{
    ModuleError as FunctionalModuleError, UnsupportedState as FunctionalUnsupportedState,
};
use indexing::{
    ExistingKind, IndexedTypeItemKind, IndexingError, InstanceSourceItemId, InvalidExportHint,
    ItemKind, OrderedTermItemId, TypeItemId, TypeRoleId,
};
use itertools::Itertools;
use javascript::{
    ModuleDiagnostic as JavaScriptModuleDiagnostic, ModuleError as JavaScriptModuleError,
    UnsupportedState as JavaScriptUnsupportedState,
};
use lowering::{Associativity, LoweringError, TypeKind};
use parsing::ParseError;
use resolving::{ExportSource, ResolvingError};
use smol_str::SmolStr;
use stabilizing::AstId;
use sugar::bracketing::OperatorInfo;
use sugar::{AssociativityError, AssociativityErrorKind, OperatorChainError};
use syntax::ast::AstNode;
use syntax::{SyntaxNodePtr, cst};

use building_types::QueryResult;

use crate::{Diagnostic, DiagnosticsContext, ExternalQueries, Severity, Span};

const INTERNAL_COMPILER_ERROR: &str = "This is an internal compiler error.";

pub trait ToDiagnostics {
    fn to_diagnostics<Q>(
        &self,
        context: &DiagnosticsContext<'_, Q>,
    ) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries;
}

fn global_span<Q>(context: &DiagnosticsContext<'_, Q>, global: GlobalId) -> Option<Span>
where
    Q: ExternalQueries,
{
    let pointer = match global {
        GlobalId::Term(_, id) => context.indexed.term_item_ptr(context.stabilized, id).next()?,
        GlobalId::Instance(InstanceIdentity::Declared(_, id)) => {
            context.stabilized.syntax_ptr(id)?
        }
        GlobalId::Instance(InstanceIdentity::Derived(_, id)) => {
            context.stabilized.syntax_ptr(id)?
        }
        GlobalId::Generated(_, _) => return None,
    };
    context.span_from_syntax_ptr(&pointer)
}

fn syntax_span<Q, N>(context: &DiagnosticsContext<'_, Q>, id: AstId<N>) -> Option<Span>
where
    Q: ExternalQueries,
    N: AstNode,
{
    let pointer = context.stabilized.syntax_ptr(id)?;
    context.span_from_syntax_ptr(&pointer)
}

fn span_text<'a, Q>(context: &DiagnosticsContext<'a, Q>, span: Option<Span>) -> &'a str
where
    Q: ExternalQueries,
{
    span.map_or("<ParseError>", |span| context.text_of(span).trim())
}

impl ToDiagnostics for FunctionalModuleError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        let (file_id, state) = match self {
            FunctionalModuleError::SourceErrors { .. } => return Ok(Vec::new()),
            FunctionalModuleError::Unsupported { file_id, state } => (file_id, state),
        };
        let local_global_span = |global| {
            let source_file = match global {
                GlobalId::Term(file, _) | GlobalId::Generated(file, _) => file,
                GlobalId::Instance(InstanceIdentity::Declared(file, _))
                | GlobalId::Instance(InstanceIdentity::Derived(file, _)) => file,
            };
            (source_file == *file_id).then(|| global_span(context, global)).flatten()
        };
        let span = match state {
            FunctionalUnsupportedState::BinderError(id) => {
                use checking::tree::BinderSource;
                let pointer = match context.checked.tree[*id].source {
                    BinderSource::Binder(id) => context.stabilized.syntax_ptr(id),
                    BinderSource::DoStatement(id) => context.stabilized.syntax_ptr(id),
                    BinderSource::Operator(id) => context.stabilized.syntax_ptr(id),
                    BinderSource::Section(id) => context.stabilized.syntax_ptr(id),
                    BinderSource::Generated { derive, .. } => context.stabilized.syntax_ptr(derive),
                };
                pointer.and_then(|pointer| context.span_from_syntax_ptr(&pointer))
            }
            FunctionalUnsupportedState::ExpressionError(id) => {
                use checking::tree::ExpressionSource;
                let pointer = match context.checked.tree[*id].source {
                    Some(ExpressionSource::Expression(id)) => context.stabilized.syntax_ptr(id),
                    Some(ExpressionSource::DoStatement(id)) => context.stabilized.syntax_ptr(id),
                    None => None,
                };
                pointer.and_then(|pointer| context.span_from_syntax_ptr(&pointer))
            }
            FunctionalUnsupportedState::PatternBindingError(id) => context
                .stabilized
                .syntax_ptr(*id)
                .and_then(|pointer| context.span_from_syntax_ptr(&pointer)),
            FunctionalUnsupportedState::MissingRuntimeExportOperatorResolution { term_id: id } => {
                global_span(context, GlobalId::Term(*file_id, *id))
            }
            FunctionalUnsupportedState::MissingLocalDeclaration(id) => {
                context.lowered.tree.get_let_binding(*id).and_then(|_| {
                    let crumb = ErrorCrumb::CheckingLetName(*id);
                    context.span_from_error_crumb(&crumb)
                })
            }
            FunctionalUnsupportedState::ConflictingRuntimeExport { duplicate, .. } => {
                local_global_span(*duplicate)
            }
            FunctionalUnsupportedState::InvalidStyleXUse { declaration, .. }
            | FunctionalUnsupportedState::InvalidStyleXContext { declaration, .. } => {
                local_global_span(*declaration)
            }
            _ => None,
        };
        let term_name = |id| {
            context
                .indexed
                .items
                .iter_terms()
                .find_map(|(candidate, item)| (candidate == id).then_some(item))
                .and_then(|item| item.name.as_ref())
        };
        let (message, context_note, hint) = match state {
            FunctionalUnsupportedState::MissingModuleName => {
                ("Module header is missing a module name".to_owned(), None, None)
            }
            FunctionalUnsupportedState::ExpressionError(_) => {
                ("Expression could not be lowered to JavaScript".to_owned(), None, None)
            }
            FunctionalUnsupportedState::BinderError(_) => {
                ("Pattern could not be lowered to JavaScript".to_owned(), None, None)
            }
            FunctionalUnsupportedState::RecordUpdateError => {
                ("Record update could not be lowered to JavaScript".to_owned(), None, None)
            }
            FunctionalUnsupportedState::PatternBindingError(_) => (
                "Pattern binding in 'let' could not be lowered to JavaScript".to_owned(),
                None,
                None,
            ),
            FunctionalUnsupportedState::UnsolvedEvidence(_) => (
                "Unresolved type class constraint remained during code generation".to_owned(),
                None,
                None,
            ),
            FunctionalUnsupportedState::EvidenceError(_) => (
                "Invalid type class dictionary remained during code generation".to_owned(),
                None,
                None,
            ),
            FunctionalUnsupportedState::CyclicEvidence(_) => (
                "Cycle encountered while building type class dictionaries".to_owned(),
                Some(INTERNAL_COMPILER_ERROR),
                None,
            ),
            FunctionalUnsupportedState::MissingTermDeclaration(_) => (
                "Checked representation of a top-level declaration is unavailable".to_owned(),
                Some(INTERNAL_COMPILER_ERROR),
                None,
            ),
            FunctionalUnsupportedState::MissingInstanceDeclaration => (
                "Checked representation of an instance declaration is unavailable".to_owned(),
                Some(INTERNAL_COMPILER_ERROR),
                None,
            ),
            FunctionalUnsupportedState::MissingLocalDeclaration(id) => {
                let declaration = context
                    .lowered
                    .tree
                    .get_let_binding(*id)
                    .and_then(|_| context.lowered.tree.get_let_binding_group(*id).name.as_ref())
                    .map(|name| format!("local declaration '{name}'"))
                    .unwrap_or_else(|| "a local declaration".to_owned());
                (
                    format!("Checked representation of {declaration} is unavailable"),
                    Some(INTERNAL_COMPILER_ERROR),
                    None,
                )
            }
            FunctionalUnsupportedState::MissingEquation => {
                ("Value declaration has no defining equations".to_owned(), None, None)
            }
            FunctionalUnsupportedState::ConflictingRuntimeExport { name, .. } => {
                (format!("Multiple declarations export the same runtime name '{name}'"), None, None)
            }
            FunctionalUnsupportedState::MissingRuntimeExportOperatorResolution { term_id } => {
                let operator = term_name(*term_id)
                    .map(|name| format!("exported operator '{name}'"))
                    .unwrap_or_else(|| "an exported operator".to_owned());
                (format!("Could not resolve the runtime implementation of {operator}"), None, None)
            }
            FunctionalUnsupportedState::InvalidInstancePrerequisite => (
                "Instance prerequisite has an unsupported dictionary representation".to_owned(),
                Some(INTERNAL_COMPILER_ERROR),
                None,
            ),
            FunctionalUnsupportedState::LocalIdentityOverflow => {
                ("Exceeded the compiler limit on local bindings".to_owned(), None, None)
            }
            FunctionalUnsupportedState::GeneratedGlobalIdentityOverflow => (
                "Exceeded the compiler limit on generated top-level declarations".to_owned(),
                None,
                None,
            ),
            FunctionalUnsupportedState::InvalidStyleXUse { function, .. } => (
                format!("'{function}' must be called directly with all of its arguments"),
                Some("It cannot be passed as a value or partially applied"),
                None,
            ),
            FunctionalUnsupportedState::InvalidStyleXContext {
                function,
                requirement,
                hint,
                ..
            } => (format!("'{function}' {requirement}"), None, hint.as_deref()),
            FunctionalUnsupportedState::ServiceEvidenceMismatch => (
                "Service dictionaries at this call do not match the declared service constraints"
                    .to_owned(),
                Some(INTERNAL_COMPILER_ERROR),
                None,
            ),
            FunctionalUnsupportedState::VirtualModuleRuntimeReference {
                module_name,
                item_name,
            } => (
                format!(
                    "'{module_name}.{item_name}' is a compile-time declaration and cannot be referenced at runtime"
                ),
                None,
                None,
            ),
        };
        let mut diagnostic = Diagnostic::error(
            "FunctionalCodegen",
            message,
            span.unwrap_or_else(|| context.module_span()),
            "functional",
        );
        if let FunctionalUnsupportedState::ConflictingRuntimeExport { existing, .. } = state
            && let Some(span) = local_global_span(*existing)
        {
            diagnostic = diagnostic.with_related(span, "Also exported here");
        }
        if let Some(context_note) = context_note {
            diagnostic = diagnostic.with_context(context_note);
        }
        if let Some(hint) = hint {
            diagnostic = diagnostic.with_hint(hint);
        }
        Ok(vec![diagnostic])
    }
}

impl ToDiagnostics for JavaScriptModuleError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        match self {
            JavaScriptModuleError::Functional(error) => error.to_diagnostics(context),
            JavaScriptModuleError::Unsupported { state, .. } => {
                let (message, is_internal) = match state {
                    JavaScriptUnsupportedState::InvalidNumber { value } => (
                        format!(
                            "Number literal '{value}' cannot be represented as a finite JavaScript number"
                        ),
                        false,
                    ),
                    JavaScriptUnsupportedState::InvalidInteger { value } => (
                        format!(
                            "Integer literal '{value}' is outside the signed 32-bit range (-2147483648 to 2147483647)"
                        ),
                        false,
                    ),
                    JavaScriptUnsupportedState::MissingGlobal { .. } => (
                        "Could not find a JavaScript declaration for a referenced value".to_owned(),
                        true,
                    ),
                    JavaScriptUnsupportedState::MissingLocal { .. } => (
                        "Could not find a JavaScript binding for a referenced local value"
                            .to_owned(),
                        true,
                    ),
                };
                let mut diagnostic = Diagnostic::error(
                    "JavaScriptCodegen",
                    message,
                    context.module_span(),
                    "javascript",
                );
                if is_internal {
                    diagnostic = diagnostic.with_context(INTERNAL_COMPILER_ERROR);
                }
                Ok(vec![diagnostic])
            }
        }
    }
}

impl ToDiagnostics for JavaScriptModuleDiagnostic {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        let JavaScriptModuleDiagnostic::InitializerCycle { declarations } = self;
        let mut spans = declarations.iter().filter_map(|&global| global_span(context, global));
        let span = spans.next().unwrap_or_else(|| context.module_span());
        let mut diagnostic = Diagnostic::new(
            self.severity(),
            "JavaScriptInitializerCycle",
            self.to_string(),
            span,
            "javascript",
        );
        for span in spans {
            diagnostic = diagnostic.with_related(span, "Also part of this cycle");
        }
        Ok(vec![diagnostic])
    }
}

impl ToDiagnostics for ForeignError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        let declaration = match self {
            ForeignError::MissingModule { declaration, .. }
            | ForeignError::AmbiguousModule { declaration }
            | ForeignError::MissingImplementation { declaration, .. }
            | ForeignError::Parse { declaration, .. } => declaration,
        };
        let span = context
            .indexed
            .term_item_ptr(context.stabilized, *declaration)
            .next()
            .and_then(|pointer| context.span_from_syntax_ptr(&pointer))
            .unwrap_or_else(|| context.fallback_span());

        let diagnostic = match self {
            ForeignError::MissingModule { name, .. } => Diagnostic::new(
                self.severity(),
                "MissingFFIModule",
                format!("No JavaScript FFI module found for foreign import '{name}'"),
                span,
                "foreign-javascript",
            ),
            ForeignError::AmbiguousModule { .. } => Diagnostic::new(
                self.severity(),
                "AmbiguousFFIModule",
                "Found both JavaScript and JSX FFI modules for this module",
                span,
                "foreign-javascript",
            )
            .with_hint("Remove one of the foreign files."),
            ForeignError::MissingImplementation { name, .. } => Diagnostic::new(
                self.severity(),
                "MissingFFIImplementation",
                format!("JavaScript FFI module does not export '{name}'"),
                span,
                "foreign-javascript",
            ),
            ForeignError::Parse { message, .. } => Diagnostic::new(
                self.severity(),
                "UnparseableFFIModule",
                "Could not parse the JavaScript FFI module",
                span,
                "foreign-javascript",
            )
            .with_context(format!("JavaScript parser error: {message}"))
            .with_context("Iris treated the module as opaque and skipped export validation")
            .with_hint("Fix the JavaScript syntax error so exports can be checked."),
        };

        Ok(vec![diagnostic])
    }
}

impl ToDiagnostics for ParseError {
    fn to_diagnostics<Q>(
        &self,
        _context: &DiagnosticsContext<'_, Q>,
    ) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        let offset =
            u32::try_from(self.offset).expect("invariant violated: source offset exceeds u32");
        let span = Span::new(offset, offset);
        Ok(vec![Diagnostic::new(
            self.severity(),
            "ParseError",
            self.message.as_ref(),
            span,
            "parsing",
        )])
    }
}

impl ToDiagnostics for LoweringError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        match self {
            LoweringError::NotInScope(not_in_scope) => {
                let ptr = match not_in_scope {
                    lowering::NotInScope::BinderConstructor { id } => {
                        context.stabilized.syntax_ptr(*id)
                    }
                    lowering::NotInScope::ExprConstructor { id } => {
                        context.stabilized.syntax_ptr(*id)
                    }
                    lowering::NotInScope::ExprVariable { id } => context.stabilized.syntax_ptr(*id),
                    lowering::NotInScope::ExprOperatorName { id } => {
                        context.stabilized.syntax_ptr(*id)
                    }
                    lowering::NotInScope::ExprRecordPun { id } => {
                        context.stabilized.syntax_ptr(*id)
                    }
                    lowering::NotInScope::TypeClass { id } => context.stabilized.syntax_ptr(*id),
                    lowering::NotInScope::DeriveClass { id } => context.stabilized.syntax_ptr(*id),
                    lowering::NotInScope::TypeConstructor { id } => {
                        context.stabilized.syntax_ptr(*id)
                    }
                    lowering::NotInScope::TypeVariable { id } => context.stabilized.syntax_ptr(*id),
                    lowering::NotInScope::TypeOperatorName { id } => {
                        context.stabilized.syntax_ptr(*id)
                    }
                    lowering::NotInScope::NegateFn { id } => context.stabilized.syntax_ptr(*id),
                    lowering::NotInScope::DoFn { id, .. } => context.stabilized.syntax_ptr(*id),
                    lowering::NotInScope::AdoFn { id, .. } => context.stabilized.syntax_ptr(*id),
                    lowering::NotInScope::TermOperator { id } => context.stabilized.syntax_ptr(*id),
                    lowering::NotInScope::TypeOperator { id } => context.stabilized.syntax_ptr(*id),
                    lowering::NotInScope::TermOperatorTarget { id } => {
                        context.stabilized.syntax_ptr(*id)
                    }
                    lowering::NotInScope::TypeOperatorTarget { id } => {
                        context.stabilized.syntax_ptr(*id)
                    }
                };

                let span = match not_in_scope {
                    lowering::NotInScope::BinderConstructor { id } => {
                        context.stabilized.ast_ptr(*id).and_then(|ptr| {
                            context.span_from_ast_ptr_child(&ptr, cst::BinderConstructor::name)
                        })
                    }
                    lowering::NotInScope::TypeClass { id } => {
                        context.stabilized.ast_ptr(*id).and_then(|ptr| {
                            context.span_from_ast_ptr_child(&ptr, cst::InstanceHead::qualified)
                        })
                    }
                    lowering::NotInScope::TermOperatorTarget { id }
                    | lowering::NotInScope::TypeOperatorTarget { id } => {
                        context.stabilized.ast_ptr(*id).and_then(|ptr| {
                            context.span_from_ast_ptr_child(&ptr, cst::InfixDeclaration::qualified)
                        })
                    }
                    _ => ptr.and_then(|ptr| context.span_from_syntax_ptr(&ptr)),
                };

                let message = match not_in_scope {
                    lowering::NotInScope::ExprVariable { .. }
                    | lowering::NotInScope::ExprRecordPun { .. } => {
                        format!("Unknown value '{}'", span_text(context, span))
                    }
                    lowering::NotInScope::ExprConstructor { .. }
                    | lowering::NotInScope::BinderConstructor { .. } => {
                        format!("Unknown data constructor '{}'", span_text(context, span))
                    }
                    lowering::NotInScope::ExprOperatorName { .. }
                    | lowering::NotInScope::TermOperator { .. } => {
                        format!("Unknown operator '{}'", span_text(context, span))
                    }
                    lowering::NotInScope::TypeConstructor { .. }
                    | lowering::NotInScope::TypeOperatorTarget { .. } => {
                        format!("Unknown type '{}'", span_text(context, span))
                    }
                    lowering::NotInScope::TypeVariable { .. } => {
                        format!("Unknown type variable '{}'", span_text(context, span))
                    }
                    lowering::NotInScope::TypeClass { .. }
                    | lowering::NotInScope::DeriveClass { .. } => {
                        format!("Unknown type class '{}'", span_text(context, span))
                    }
                    lowering::NotInScope::TypeOperatorName { .. }
                    | lowering::NotInScope::TypeOperator { .. } => {
                        format!("Unknown type operator '{}'", span_text(context, span))
                    }
                    lowering::NotInScope::TermOperatorTarget { .. } => {
                        format!("Unknown value or constructor '{}'", span_text(context, span))
                    }
                    lowering::NotInScope::NegateFn { .. } => {
                        "Unknown value 'negate', required by numeric negation".to_owned()
                    }
                    lowering::NotInScope::DoFn { kind, .. } => {
                        let name = match kind {
                            lowering::DoFn::Bind => "bind",
                            lowering::DoFn::Discard => "discard",
                        };
                        format!("Unknown value '{name}', required by this 'do' block")
                    }
                    lowering::NotInScope::AdoFn { kind, .. } => {
                        let name = match kind {
                            lowering::AdoFn::Map => "map",
                            lowering::AdoFn::Apply => "apply",
                            lowering::AdoFn::Pure => "pure",
                        };
                        format!("Unknown value '{name}', required by this 'ado' block")
                    }
                };
                let span = span.unwrap_or_else(|| context.fallback_span());

                Ok(vec![Diagnostic::new(self.severity(), "NotInScope", message, span, "lowering")])
            }

            LoweringError::InvalidStringEscape { source } => {
                let ptr = match source {
                    lowering::StringLiteralSource::Expression(id) => {
                        context.stabilized.syntax_ptr(*id)
                    }
                    lowering::StringLiteralSource::Binder(id) => context.stabilized.syntax_ptr(*id),
                    lowering::StringLiteralSource::Type(id) => context.stabilized.syntax_ptr(*id),
                };
                let span = ptr
                    .and_then(|ptr| context.span_from_syntax_ptr(&ptr))
                    .unwrap_or_else(|| context.fallback_span());

                Ok(vec![Diagnostic::new(
                    self.severity(),
                    "InvalidStringEscape",
                    "Invalid escape sequence in string literal",
                    span,
                    "lowering",
                )])
            }

            LoweringError::OverlappingBinderNames { name, duplicate, existing } => {
                let span = |source| {
                    let ptr = match source {
                        lowering::BinderNameSource::Binder(id) => context.stabilized.syntax_ptr(id),
                        lowering::BinderNameSource::RecordPun(id) => {
                            context.stabilized.syntax_ptr(id)
                        }
                    };
                    context.span_from_syntax_ptr(&ptr?)
                };
                let duplicate = span(*duplicate).unwrap_or_else(|| context.fallback_span());

                let message = format!("'{name}' is bound more than once in this pattern");
                let mut diagnostic = Diagnostic::new(
                    self.severity(),
                    "OverlappingBinderNames",
                    message,
                    duplicate,
                    "lowering",
                );
                if let Some(existing) = span(*existing) {
                    diagnostic = diagnostic.with_related(existing, "First bound here");
                }

                Ok(vec![diagnostic])
            }

            LoweringError::RecursiveSynonym(group) => Ok(convert_recursive_group(
                context,
                self.severity(),
                &group.group,
                "RecursiveSynonym",
                "Cycle in type synonym declarations",
            )),

            LoweringError::RecursiveKinds(group) => Ok(convert_recursive_group(
                context,
                self.severity(),
                &group.group,
                "RecursiveKinds",
                "Cycle in kind declarations",
            )),
        }
    }
}

fn convert_recursive_group<Q>(
    context: &DiagnosticsContext<'_, Q>,
    severity: Severity,
    group: &[indexing::TypeItemId],
    code: &'static str,
    message: &'static str,
) -> Vec<Diagnostic>
where
    Q: ExternalQueries,
{
    let spans = group.iter().filter_map(|&id| {
        let equation = match context.indexed.items[id].kind {
            IndexedTypeItemKind::Synonym { equation, .. } => {
                equation.and_then(|equation| context.stabilized.syntax_ptr(equation))
            }
            IndexedTypeItemKind::Data { equation, .. } => {
                equation.and_then(|equation| context.stabilized.syntax_ptr(equation))
            }
            IndexedTypeItemKind::Newtype { equation, .. } => {
                equation.and_then(|equation| context.stabilized.syntax_ptr(equation))
            }
            _ => None,
        };
        let ptr =
            equation.or_else(|| context.indexed.type_item_ptr(context.stabilized, id).next())?;
        context.span_from_syntax_ptr(&ptr)
    });

    let spans = spans.collect_vec();

    let primary = spans.first().copied().unwrap_or_else(|| context.fallback_span());

    let mut diagnostic = Diagnostic::new(severity, code, message, primary, "lowering");

    for &span in spans.iter().skip(1) {
        diagnostic = diagnostic.with_related(span, "Also part of this cycle");
    }

    vec![diagnostic]
}

impl ToDiagnostics for OperatorChainError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        let diagnostic = match self {
            OperatorChainError::Term(error) => {
                associativity_diagnostic(context, self.severity(), error)
            }
            OperatorChainError::Type(error) => {
                associativity_diagnostic(context, self.severity(), error)
            }
        };
        Ok(vec![diagnostic])
    }
}

fn associativity_diagnostic<Q, N>(
    context: &DiagnosticsContext<'_, Q>,
    severity: Severity,
    error: &AssociativityError<AstId<N>>,
) -> Diagnostic
where
    Q: ExternalQueries,
    N: AstNode,
{
    let AssociativityError { kind, previous, operator } = error;

    let previous_span = syntax_span(context, previous.id);
    let operator_span = syntax_span(context, operator.id);
    let previous_text = span_text(context, previous_span);
    let operator_text = span_text(context, operator_span);

    let (code, message) = match kind {
        AssociativityErrorKind::NonAssociative => {
            let message = if previous_text == operator_text {
                format!("Non-associative operator '{operator_text}' cannot be chained")
            } else {
                format!(
                    "Non-associative operators '{previous_text}' and '{operator_text}' cannot be chained"
                )
            };
            ("NonAssociative", message)
        }
        AssociativityErrorKind::MixedAssociativity => (
            "MixedAssociativity",
            format!(
                "Cannot mix '{previous_text}' ('{}') with '{operator_text}' ('{}')",
                fixity(previous),
                fixity(operator),
            ),
        ),
    };

    let span = operator_span.unwrap_or_else(|| context.fallback_span());
    let mut diagnostic = Diagnostic::new(severity, code, message, span, "sugar");
    if let Some(previous_span) = previous_span {
        diagnostic = diagnostic.with_related(previous_span, "Previous operator here");
    }
    diagnostic.with_hint("Add parentheses to clarify the grouping.")
}

fn fixity<T>(operator: &OperatorInfo<T>) -> String {
    let keyword = match operator.associativity {
        Associativity::None => "infix",
        Associativity::Left => "infixl",
        Associativity::Right => "infixr",
    };
    format!("{keyword} {}", operator.precedence)
}

impl ToDiagnostics for ResolvingError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        match self {
            ResolvingError::DuplicateModule { name } => {
                let span = context.fallback_span();
                let message = format!("Module '{name}' is defined more than once");
                Ok(vec![Diagnostic::new(
                    self.severity(),
                    "DuplicateModule",
                    message,
                    span,
                    "resolving",
                )])
            }
            ResolvingError::InstanceNameConflict { name, instance, existing } => {
                let pointer = match instance {
                    InstanceSourceItemId::Instance(id) => {
                        context.stabilized.syntax_ptr(context.indexed.items[*id].id)
                    }
                    InstanceSourceItemId::Derive(id) => {
                        context.stabilized.syntax_ptr(context.indexed.items[*id].id)
                    }
                };
                let span = pointer
                    .and_then(|pointer| context.span_from_syntax_ptr(&pointer))
                    .unwrap_or_else(|| context.fallback_span());
                let (code, message, related_label, pointer) = match existing {
                    OrderedTermItemId::Term(id) => (
                        "RedefinedIdent",
                        format!(
                            "Instance name '{name}' conflicts with a value declaration in this module"
                        ),
                        "Value declared here",
                        context.indexed.term_item_ptr(context.stabilized, *id).next(),
                    ),
                    OrderedTermItemId::Instance(id) => (
                        "DuplicateInstance",
                        format!("Instance name '{name}' is defined more than once"),
                        "First defined here",
                        context.stabilized.syntax_ptr(context.indexed.items[*id].id),
                    ),
                    OrderedTermItemId::Derive(id) => (
                        "DuplicateInstance",
                        format!("Instance name '{name}' is defined more than once"),
                        "First defined here",
                        context.stabilized.syntax_ptr(context.indexed.items[*id].id),
                    ),
                };
                let mut diagnostic =
                    Diagnostic::new(self.severity(), code, message, span, "resolving");
                if let Some(span) =
                    pointer.and_then(|pointer| context.span_from_syntax_ptr(&pointer))
                {
                    diagnostic = diagnostic.with_related(span, related_label);
                }
                Ok(vec![diagnostic])
            }
            ResolvingError::TermExportConflict { name, existing, duplicate } => {
                let pointer = |(_, id, source)| match source {
                    ExportSource::Local => {
                        context.indexed.term_item_ptr(context.stabilized, id).next()
                    }
                    ExportSource::Import(import) => context.stabilized.syntax_ptr(import),
                };
                Ok(export_conflict(
                    context,
                    self.severity(),
                    name,
                    pointer(*duplicate),
                    pointer(*existing),
                ))
            }
            ResolvingError::TypeExportConflict { name, existing, duplicate } => {
                let pointer = |(_, id, source)| match source {
                    ExportSource::Local => {
                        context.indexed.type_item_ptr(context.stabilized, id).next()
                    }
                    ExportSource::Import(import) => context.stabilized.syntax_ptr(import),
                };
                Ok(export_conflict(
                    context,
                    self.severity(),
                    name,
                    pointer(*duplicate),
                    pointer(*existing),
                ))
            }

            ResolvingError::InvalidImportStatement { id } => {
                let ptr = context.stabilized.ast_ptr(*id);

                let message = {
                    let cst = ptr.as_ref().map(|ptr| ptr.to_node(context.root));
                    let name = cst
                        .and_then(|cst| cst.module_name())
                        .map(|cst| {
                            let range = cst.syntax().text_range();
                            context.content[range].trim()
                        })
                        .filter(|name| !name.is_empty());
                    match name {
                        Some(name) => format!("Could not find module '{name}'"),
                        None => "Invalid import statement".to_owned(),
                    }
                };

                let span = ptr
                    .and_then(|ptr| context.span_from_ast_ptr(&ptr))
                    .unwrap_or_else(|| context.fallback_span());

                Ok(vec![Diagnostic::new(
                    self.severity(),
                    "InvalidImportStatement",
                    message,
                    span,
                    "resolving",
                )])
            }

            ResolvingError::InvalidImportItem { id } => {
                let span = syntax_span(context, *id);

                let text = span_text(context, span);
                let message = format!("Cannot import '{text}'");
                let span = span.unwrap_or_else(|| context.fallback_span());

                Ok(vec![Diagnostic::new(
                    self.severity(),
                    "InvalidImportItem",
                    message,
                    span,
                    "resolving",
                )])
            }
        }
    }
}

fn export_conflict<Q>(
    context: &DiagnosticsContext<'_, Q>,
    severity: Severity,
    name: &SmolStr,
    duplicate: Option<SyntaxNodePtr>,
    existing: Option<SyntaxNodePtr>,
) -> Vec<Diagnostic>
where
    Q: ExternalQueries,
{
    let span = |pointer: Option<SyntaxNodePtr>| {
        pointer.and_then(|pointer| context.span_from_syntax_ptr(&pointer))
    };
    let message = format!("Conflicting exports for '{name}'");
    let primary = span(duplicate).unwrap_or_else(|| context.fallback_span());
    let mut diagnostic = Diagnostic::new(severity, "ExportConflict", message, primary, "resolving");
    if let Some(existing) = span(existing) {
        diagnostic = diagnostic.with_related(existing, "Also exported here");
    }
    vec![diagnostic]
}

fn declaration_conflict<Q>(
    context: &DiagnosticsContext<'_, Q>,
    severity: Severity,
    code: &'static str,
    description: &str,
    related_label: &str,
    kind: ItemKind,
    existing: ExistingKind,
) -> Vec<Diagnostic>
where
    Q: ExternalQueries,
{
    let span = |pointer: Option<SyntaxNodePtr>| {
        pointer.and_then(|pointer| context.span_from_syntax_ptr(&pointer))
    };
    let (name, existing) = match existing {
        ExistingKind::Term(id) => (
            &context.indexed.items[id].name,
            context.indexed.term_item_ptr(context.stabilized, id).next(),
        ),
        ExistingKind::Type(id) => (
            &context.indexed.items[id].name,
            context.indexed.type_item_ptr(context.stabilized, id).next(),
        ),
    };
    let duplicate = match kind {
        ItemKind::DataSignature(id) => context.stabilized.syntax_ptr(id),
        ItemKind::DataEquation(id) => context.stabilized.syntax_ptr(id),
        ItemKind::NewtypeSignature(id) => context.stabilized.syntax_ptr(id),
        ItemKind::NewtypeEquation(id) => context.stabilized.syntax_ptr(id),
        ItemKind::SynonymSignature(id) => context.stabilized.syntax_ptr(id),
        ItemKind::SynonymEquation(id) => context.stabilized.syntax_ptr(id),
        ItemKind::ClassSignature(id) => context.stabilized.syntax_ptr(id),
        ItemKind::ClassDeclaration(id) => context.stabilized.syntax_ptr(id),
        ItemKind::ValueSignature(id) => context.stabilized.syntax_ptr(id),
        ItemKind::ValueEquation(id) => context.stabilized.syntax_ptr(id),
        ItemKind::Term(id) => context.indexed.term_item_ptr(context.stabilized, id).next(),
        ItemKind::Type(id) => context.indexed.type_item_ptr(context.stabilized, id).next(),
    };
    let name = name.as_deref().unwrap_or("<ParseError>");
    let message = format!("'{name}' {description}");
    let primary = span(duplicate).unwrap_or_else(|| context.fallback_span());
    let mut diagnostic = Diagnostic::new(severity, code, message, primary, "indexing");
    if let Some(existing) = span(existing) {
        diagnostic = diagnostic.with_related(existing, related_label);
    }
    vec![diagnostic]
}

fn role_declaration<'a, Q>(
    context: &DiagnosticsContext<'a, Q>,
    id: TypeRoleId,
) -> (Option<Span>, &'a str)
where
    Q: ExternalQueries,
{
    let ptr = context.stabilized.ast_ptr(id);
    let span = ptr.as_ref().and_then(|ptr| context.span_from_ast_ptr(ptr));
    let name = ptr.and_then(|ptr| ptr.to_node(context.root).name_token());
    let name = name.map_or("<ParseError>", |name| name.text(context.content));
    (span, name)
}

fn invalid_export_hint<Q>(
    context: &DiagnosticsContext<'_, Q>,
    text: &str,
    hint: InvalidExportHint,
) -> Option<(String, String, Span)>
where
    Q: ExternalQueries,
{
    match hint {
        InvalidExportHint::AliasedModule { import } => {
            let alias = context.indexed.imports.get(&import)?.alias.as_ref()?;
            let pointer = context.stabilized.syntax_ptr(import)?;
            let span = context.span_from_syntax_ptr(&pointer)?;
            let message = format!(
                "Cannot re-export '{text}' because it is imported under the alias '{alias}'"
            );
            let hint = format!("Re-export 'module {alias}' instead.");
            Some((message, hint, span))
        }
        InvalidExportHint::ImportedItem { import, item } => {
            let indexed = context.indexed.imports.get(&import)?;
            let module = indexed.alias.as_ref().or(indexed.name.as_ref())?;
            let pointer = context.stabilized.syntax_ptr(item)?;
            let span = context.span_from_syntax_ptr(&pointer)?;
            let message = format!("Cannot export imported item '{text}' directly");
            let hint = format!("Re-export 'module {module}' instead.");
            Some((message, hint, span))
        }
    }
}

impl ToDiagnostics for IndexingError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        match self {
            IndexingError::DuplicateImport { duplicate, existing } => {
                let span = syntax_span(context, *duplicate);

                let text = span_text(context, span);
                let message = format!("'{text}' is imported more than once");
                let span = span.unwrap_or_else(|| context.fallback_span());

                let mut diagnostic =
                    Diagnostic::new(self.severity(), "DuplicateImport", message, span, "indexing");

                if duplicate != existing
                    && let Some(existing_span) = syntax_span(context, *existing)
                {
                    diagnostic = diagnostic.with_related(existing_span, "First imported here");
                }

                Ok(vec![diagnostic])
            }
            IndexingError::OrphanRole { id } => {
                let (span, name) = role_declaration(context, *id);
                let span = span.unwrap_or_else(|| context.fallback_span());
                let message = format!(
                    "Role declaration for '{name}' must immediately follow its type definition"
                );
                Ok(vec![Diagnostic::new(
                    self.severity(),
                    "OrphanRoleDeclaration",
                    message,
                    span,
                    "indexing",
                )])
            }
            IndexingError::DuplicateRole { id, existing } => {
                let (span, name) = role_declaration(context, *id);
                let span = span.unwrap_or_else(|| context.fallback_span());
                let message = format!("Duplicate role declaration for '{name}'");
                let mut diagnostic = Diagnostic::new(
                    self.severity(),
                    "DuplicateRoleDeclaration",
                    message,
                    span,
                    "indexing",
                );
                if let (Some(existing_span), _) = role_declaration(context, *existing) {
                    diagnostic = diagnostic.with_related(existing_span, "First declared here");
                }
                Ok(vec![diagnostic])
            }
            IndexingError::UnsupportedRole { id } => {
                let (span, _) = role_declaration(context, *id);
                let span = span.unwrap_or_else(|| context.fallback_span());
                let message = "Role declarations are only supported for 'data', 'newtype', and foreign data types".to_string();
                Ok(vec![Diagnostic::new(
                    self.severity(),
                    "UnsupportedRoleDeclaration",
                    message,
                    span,
                    "indexing",
                )])
            }
            IndexingError::DuplicateItem { kind, existing } => Ok(declaration_conflict(
                context,
                self.severity(),
                "DuplicateDeclaration",
                "is declared more than once",
                "First declared here",
                *kind,
                *existing,
            )),
            IndexingError::MismatchedItem { kind, existing } => Ok(declaration_conflict(
                context,
                self.severity(),
                "MismatchedDeclaration",
                "conflicts with an earlier declaration of a different kind",
                "Previously declared here",
                *kind,
                *existing,
            )),
            IndexingError::DuplicateExport { id, existing } => {
                let span = syntax_span(context, *id);

                let text = span_text(context, span);
                let message = format!("'{text}' is exported more than once");
                let span = span.unwrap_or_else(|| context.fallback_span());

                let mut diagnostic =
                    Diagnostic::new(self.severity(), "DuplicateExport", message, span, "indexing");

                if id != existing
                    && let Some(existing_span) = syntax_span(context, *existing)
                {
                    diagnostic = diagnostic.with_related(existing_span, "First exported here");
                }

                Ok(vec![diagnostic])
            }
            IndexingError::InvalidExport { id, hint } => {
                let span = syntax_span(context, *id);

                let text = span_text(context, span);
                let hint = hint.and_then(|hint| invalid_export_hint(context, text, hint));
                let (message, hint_text, imported) = match hint {
                    Some((message, hint_text, imported)) => {
                        (message, Some(hint_text), Some(imported))
                    }
                    None => (format!("Cannot export '{text}'"), None, None),
                };
                let span = span.unwrap_or_else(|| context.fallback_span());

                let mut diagnostic = Diagnostic::new(
                    self.severity(),
                    "InvalidExportItem",
                    message,
                    span,
                    "indexing",
                );

                if let Some(imported) = imported {
                    diagnostic = diagnostic.with_related(imported, "Imported here");
                }

                if let Some(hint_text) = hint_text {
                    diagnostic = diagnostic.with_hint(hint_text);
                }

                Ok(vec![diagnostic])
            }
        }
    }
}

fn type_item_name<Q>(
    context: &DiagnosticsContext<'_, Q>,
    file_id: FileId,
    item_id: TypeItemId,
) -> QueryResult<Option<SmolStr>>
where
    Q: ExternalQueries,
{
    let indexed = context.queries.indexed(file_id)?;
    Ok(indexed.items[item_id].name.clone())
}

fn partial_synonym_name<Q>(
    context: &DiagnosticsContext<'_, Q>,
    mut id: lowering::TypeId,
) -> QueryResult<Option<SmolStr>>
where
    Q: ExternalQueries,
{
    loop {
        match context.lowered.tree.get_type_kind(id) {
            Some(TypeKind::ApplicationChain { function: Some(function), .. }) => id = *function,
            Some(TypeKind::Parenthesized { parenthesized: Some(inner) }) => id = *inner,
            Some(TypeKind::Kinded { type_: Some(inner), .. }) => id = *inner,
            Some(
                TypeKind::Constructor { resolution: Some((file_id, type_id)) }
                | TypeKind::Operator { resolution: Some((file_id, type_id)) },
            ) => return type_item_name(context, *file_id, *type_id),
            _ => {
                let name = context
                    .stabilized
                    .syntax_ptr(id)
                    .and_then(|pointer| context.span_from_syntax_ptr(&pointer))
                    .map(|span| SmolStr::new(context.text_of(span).trim()))
                    .filter(|text| !text.is_empty());
                return Ok(name);
            }
        }
    }
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Nominal => "nominal",
        Role::Representational => "representational",
        Role::Phantom => "phantom",
    }
}

impl ToDiagnostics for CheckingError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        let span = context.primary_span_from_crumbs(&self.crumbs);
        let diagnostic = match &self.kind {
            ErrorKind::Constraint(kind) => constraint_diagnostic(context, self, kind, span)?,
            ErrorKind::Capability(kind) => capability_diagnostic(context, self, kind, span)?,
            ErrorKind::CapabilityBundle(kind) => {
                capability_bundle_diagnostic(context, self, kind, span)?
            }
            ErrorKind::Instance(kind) => instance_diagnostic(context, self, kind, span)?,
            ErrorKind::Derive(kind) => derive_diagnostic(context, self, kind, span)?,
            ErrorKind::Unification(kind) => unification_diagnostic(context, self, kind, span)?,
            ErrorKind::Type(kind) => type_diagnostic(context, self, kind, span)?,
            ErrorKind::Term(kind) => term_diagnostic(context, self, kind, span)?,
            ErrorKind::Hole(kind) => hole_diagnostic(context, self, kind, span)?,
        };
        Ok(vec![diagnostic])
    }
}

fn checking_diagnostic<Q>(
    context: &DiagnosticsContext<'_, Q>,
    error: &CheckingError,
    span: Span,
    code: &'static str,
    message: impl Into<String>,
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    let mut diagnostic = Diagnostic::new(error.severity(), code, message, span, "checking");
    for crumb in error.crumbs.iter() {
        if let ErrorCrumb::CapabilityBundle(origin) = *crumb {
            let origin = context.render_type(origin)?;
            diagnostic =
                diagnostic.with_context(format!("Required via capability bundle '{origin}'"));
        }
    }
    Ok(diagnostic)
}

fn constraint_diagnostic<Q>(
    context: &DiagnosticsContext<'_, Q>,
    error: &CheckingError,
    kind: &ConstraintErrorKind,
    span: Span,
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    match kind {
        ConstraintErrorKind::AmbiguousConstraint { constraint } => {
            let constraint = context.render_type(*constraint)?;
            let message = format!("Ambiguous type class constraint '{constraint}'");
            checking_diagnostic(context, error, span, "AmbiguousConstraint", message)
        }
        ConstraintErrorKind::NoInstanceFound { given, constraint } => {
            let constraint = context.render_type(*constraint)?;
            let message = format!("No type class instance found for '{constraint}'");
            let diagnostic = checking_diagnostic(context, error, span, "NoInstanceFound", message)?;
            let mut items = Vec::with_capacity(given.len());
            for &given in given.iter() {
                items.push(context.render_type(given)?.to_string());
            }
            Ok(diagnostic.with_list("Constraints in scope:", items))
        }
        ConstraintErrorKind::OverlappingInstances { constraint, instances } => {
            let constraint = context.render_type(*constraint)?;
            let message = format!("Overlapping type class instances found for '{constraint}'");
            let diagnostic =
                checking_diagnostic(context, error, span, "OverlappingInstances", message)?;
            let mut items = Vec::with_capacity(instances.len());
            for &instance in instances.iter() {
                items.push(context.render_type(instance)?.to_string());
            }
            Ok(diagnostic.with_list("Matching instances:", items))
        }
        ConstraintErrorKind::CoercibleConstructorNotInScope { file_id, item_id } => {
            let message = match type_item_name(context, *file_id, *item_id)? {
                Some(name) => {
                    format!("Constructor for '{name}' must be in scope to derive 'Coercible'")
                }
                None => "Constructor must be in scope to derive 'Coercible'".to_owned(),
            };
            checking_diagnostic(context, error, span, "CoercibleConstructorNotInScope", message)
        }
        ConstraintErrorKind::CustomWarning { message_id } => {
            let message = context.queries.lookup_checking_smol_str(*message_id);
            checking_diagnostic(context, error, span, "CustomWarning", message.as_str())
        }
        ConstraintErrorKind::CustomFailure { message_id } => {
            let message = context.queries.lookup_checking_smol_str(*message_id);
            checking_diagnostic(context, error, span, "CustomFailure", message.as_str())
        }
        ConstraintErrorKind::ReflectedIntegerOutOfRange { integer } => {
            let integer = context.queries.lookup_integer(*integer);
            let message = format!(
                "Type-level integer '{integer}' cannot be reflected as an 'Int' because it is \
                 outside the range -2147483648 to 2147483647"
            );
            checking_diagnostic(context, error, span, "ReflectedIntegerOutOfRange", message)
        }
    }
}

fn capability_diagnostic<Q>(
    context: &DiagnosticsContext<'_, Q>,
    error: &CheckingError,
    kind: &CapabilityErrorKind,
    span: Span,
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    let (services, mut diagnostic) = match kind {
        CapabilityErrorKind::AmbiguousCapability { service, services, roles } => {
            let service = context.render_type(*service)?;
            let roles = roles.iter().map(|role| format!("'{role}'")).join(", ");
            let message =
                format!("Capability '{service}' is declared under multiple roles: {roles}");
            let diagnostic =
                checking_diagnostic(context, error, span, "AmbiguousCapability", message)?;
            (services, diagnostic.with_hint("Use a distinct service type for each role."))
        }
        CapabilityErrorKind::CapabilityNotDeclared { service, services } => {
            let service = context.render_type(*service)?;
            let rendered_services = context.render_type(*services)?;
            let message =
                format!("Capability '{service}' is not declared in '{rendered_services}'");
            let diagnostic =
                checking_diagnostic(context, error, span, "CapabilityNotDeclared", message)?;
            (services, diagnostic)
        }
        CapabilityErrorKind::UnresolvedCapability { service, services } => {
            let service = context.render_type(*service)?;
            let rendered_services = context.render_type(*services)?;
            let message = format!(
                "Cannot resolve capability '{service}' from open capability set '{rendered_services}'"
            );
            let diagnostic =
                checking_diagnostic(context, error, span, "UnresolvedCapability", message)?;
            let hint =
                format!("Close the capability set or add a 'Has' constraint for '{service}'.");
            (services, diagnostic.with_hint(hint))
        }
    };
    if let Some(declared) = context.written_type_span(&error.crumbs, *services)? {
        diagnostic = diagnostic.with_related(span, "Required by this expression");
        diagnostic.span = declared;
    }
    Ok(diagnostic)
}

fn capability_bundle_diagnostic<Q>(
    context: &DiagnosticsContext<'_, Q>,
    error: &CheckingError,
    kind: &CapabilityBundleErrorKind,
    span: Span,
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    match kind {
        CapabilityBundleErrorKind::InvalidCapabilityBundleMember { member } => {
            let member = Pretty::new(context.queries, context.checked).render(*member)?;
            let message = format!("Capability bundle member '{member}' cannot be a type variable");
            let diagnostic = checking_diagnostic(
                context,
                error,
                span,
                "InvalidCapabilityBundleMember",
                message,
            )?;
            Ok(diagnostic.with_hint(
                "Name each service explicitly instead of using a type synonym parameter.",
            ))
        }
        CapabilityBundleErrorKind::InvalidCapabilityBundlePosition { bundle } => {
            let bundle = context.render_type(*bundle)?;
            let message = format!("Capability bundle '{bundle}' cannot be used as a service type");
            let diagnostic = checking_diagnostic(
                context,
                error,
                span,
                "InvalidCapabilityBundlePosition",
                message,
            )?;
            Ok(diagnostic.with_hint(
                "Use a capability bundle only as the first argument to 'Has' or inside another bundle synonym.",
            ))
        }
    }
}

fn instance_diagnostic<Q>(
    context: &DiagnosticsContext<'_, Q>,
    error: &CheckingError,
    kind: &InstanceErrorKind,
    span: Span,
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    match kind {
        InstanceErrorKind::InstanceHeadMismatch { class_file, class_item, expected, actual } => {
            let noun = if *expected == 1 { "type argument" } else { "type arguments" };
            let message = match type_item_name(context, *class_file, *class_item)? {
                Some(name) => format!(
                    "Class '{name}' expects {expected} {noun}, but this instance provides {actual}"
                ),
                None => {
                    format!("Class expects {expected} {noun}, but this instance provides {actual}")
                }
            };
            checking_diagnostic(context, error, span, "InstanceHeadMismatch", message)
        }
        InstanceErrorKind::InstanceHeadLabeledRow { class_file, class_item, position, type_id } => {
            let position = position + 1;
            let rendered = context.render_type(*type_id)?;
            let message = match type_item_name(context, *class_file, *class_item)? {
                Some(name) => format!(
                    "Instance argument {position} of '{name}' cannot contain labelled row '{rendered}'"
                ),
                None => {
                    format!("Instance argument {position} cannot contain labelled row '{rendered}'")
                }
            };
            let diagnostic =
                checking_diagnostic(context, error, span, "InstanceHeadLabeledRow", message)?;
            Ok(diagnostic.with_context(
                "This parameter is not determined by a functional dependency, so only a row variable like '( | r )' is allowed",
            ))
        }
        InstanceErrorKind::InstanceMemberTypeMismatch { expected, actual } => {
            let expected = context.render_type(*expected)?;
            let actual = context.render_type(*actual)?;
            let message =
                format!("Instance member has type '{actual}', but the class requires '{expected}'");
            checking_diagnostic(context, error, span, "InstanceMemberTypeMismatch", message)
        }
        InstanceErrorKind::MissingInstanceMembers { members } => {
            let (message, heading) = if members.len() == 1 {
                ("Instance is missing a class member", "Missing member:")
            } else {
                ("Instance is missing class members", "Missing members:")
            };
            let diagnostic =
                checking_diagnostic(context, error, span, "MissingInstanceMembers", message)?;
            let items = members.iter().map(SmolStr::to_string).collect();
            Ok(diagnostic.with_list(heading, items))
        }
        InstanceErrorKind::OrphanInstance { constraint } => {
            let constraint = context.render_type(*constraint)?;
            let message = format!("Orphan instance for '{constraint}'");
            let diagnostic = checking_diagnostic(context, error, span, "OrphanInstance", message)?;
            Ok(diagnostic.with_context(
                "Instances must be defined in the same module as either the class or one of its head types",
            ))
        }
    }
}

fn derive_diagnostic<Q>(
    context: &DiagnosticsContext<'_, Q>,
    error: &CheckingError,
    kind: &DeriveErrorKind,
    span: Span,
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    match kind {
        DeriveErrorKind::CannotDeriveClass { class_file, class_id } => {
            let message = match type_item_name(context, *class_file, *class_id)? {
                Some(name) => format!("Cannot derive instances for class '{name}'"),
                None => "Cannot derive instances for this class".to_owned(),
            };
            checking_diagnostic(context, error, span, "CannotDeriveClass", message)
        }
        DeriveErrorKind::CannotDeriveForType { type_id } => {
            let rendered = context.render_type(*type_id)?;
            let message = format!("Cannot derive an instance for type '{rendered}'");
            checking_diagnostic(context, error, span, "CannotDeriveForType", message)
        }
        DeriveErrorKind::ContravariantOccurrence { type_id } => {
            let rendered = context.render_type(*type_id)?;
            let message = format!("Type variable '{rendered}' appears in a contravariant position");
            checking_diagnostic(context, error, span, "ContravariantOccurrence", message)
        }
        DeriveErrorKind::CovariantOccurrence { type_id } => {
            let rendered = context.render_type(*type_id)?;
            let message = format!("Type variable '{rendered}' appears in a covariant position");
            checking_diagnostic(context, error, span, "CovariantOccurrence", message)
        }
        DeriveErrorKind::DeriveInvalidArity { class_file, class_id, expected, actual } => {
            let noun = if *expected == 1 { "type argument" } else { "type arguments" };
            let message = match type_item_name(context, *class_file, *class_id)? {
                Some(name) => {
                    format!("Deriving '{name}' requires {expected} {noun}, but found {actual}")
                }
                None => {
                    format!("Deriving this class requires {expected} {noun}, but found {actual}")
                }
            };
            checking_diagnostic(context, error, span, "DeriveInvalidArity", message)
        }
        DeriveErrorKind::DeriveMissingFunctor => {
            let message = "Deriving 'Functor' requires 'Data.Functor' to be imported";
            checking_diagnostic(context, error, span, "DeriveMissingFunctor", message)
        }
        DeriveErrorKind::ExpectedNewtype { type_id } => {
            let rendered = context.render_type(*type_id)?;
            let message = format!("Expected a newtype, but found '{rendered}'");
            checking_diagnostic(context, error, span, "ExpectedNewtype", message)
        }
        DeriveErrorKind::InvalidNewtypeDeriveSkolemArguments => {
            let message = "Cannot derive a newtype instance for this type";
            let diagnostic = checking_diagnostic(
                context,
                error,
                span,
                "InvalidNewtypeDeriveSkolemArguments",
                message,
            )?;
            Ok(diagnostic.with_context(
                "Type arguments left out of the instance head must appear last in the wrapped type and nowhere else",
            ))
        }
        DeriveErrorKind::NonLocalNewtype { type_id } => {
            let rendered = context.render_type(*type_id)?;
            let message = format!(
                "Cannot derive a newtype instance for '{rendered}' because it is defined in another module"
            );
            checking_diagnostic(context, error, span, "NonLocalNewtype", message)
        }
    }
}

fn unification_diagnostic<Q>(
    context: &DiagnosticsContext<'_, Q>,
    error: &CheckingError,
    kind: &UnificationErrorKind,
    span: Span,
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    match kind {
        &UnificationErrorKind::CannotUnify { t1, t2, outer: (outer_t1, outer_t2) } => {
            let rendered_t1 = context.render_type(t1)?;
            let rendered_t2 = context.render_type(t2)?;
            let message = format!("Could not match '{rendered_t1}' with '{rendered_t2}'");
            let mut diagnostic = checking_diagnostic(context, error, span, "CannotUnify", message)?;
            // Types are zonked by now, so equal types share an identifier.
            if (t1, t2) != (outer_t1, outer_t2) {
                let outer_t1 = context.render_type(outer_t1)?;
                let outer_t2 = context.render_type(outer_t2)?;
                diagnostic = diagnostic
                    .with_context(format!("While matching '{outer_t1}' with '{outer_t2}'"));
            }
            Ok(diagnostic)
        }
        UnificationErrorKind::EscapedSkolem { skolem, type_id } => {
            let skolem = context.render_type(*skolem)?;
            let message = format!("Rigid type variable '{skolem}' escaped its scope");
            let diagnostic = checking_diagnostic(context, error, span, "EscapedSkolem", message)?;
            let rendered = context.render_type(*type_id)?;
            Ok(diagnostic.with_context(format!("Appears in type '{rendered}'")))
        }
        UnificationErrorKind::PropertyIsMissing { labels } => {
            let noun = if labels.len() == 1 { "property" } else { "properties" };
            let labels = labels.iter().map(|label| format!("'{label}'")).join(", ");
            let message = format!("Missing required {noun} {labels}");
            checking_diagnostic(context, error, span, "PropertyIsMissing", message)
        }
        UnificationErrorKind::AdditionalProperty { labels } => {
            let noun = if labels.len() == 1 { "property" } else { "properties" };
            let labels = labels.iter().map(|label| format!("'{label}'")).join(", ");
            let message = format!("Unexpected {noun} {labels}");
            checking_diagnostic(context, error, span, "AdditionalProperty", message)
        }
        UnificationErrorKind::CannotGeneraliseRecursiveFunction { type_id } => {
            let message = "Could not generalise the type of this recursive function";
            let diagnostic = checking_diagnostic(
                context,
                error,
                span,
                "CannotGeneraliseRecursiveFunction",
                message,
            )?;
            let inferred = context.render_type(*type_id)?;
            Ok(diagnostic
                .with_context(format!("Inferred type: '{inferred}'"))
                .with_hint("Add an explicit type signature."))
        }
        UnificationErrorKind::UnreportedUnknownType { type_id } => {
            let type_id = context.render_type(*type_id)?;
            let message =
                format!("Unknown type '{type_id}' remained unresolved without a reported error");
            let diagnostic =
                checking_diagnostic(context, error, span, "UnreportedUnknownType", message)?;
            Ok(diagnostic.with_context(INTERNAL_COMPILER_ERROR))
        }
    }
}

fn type_diagnostic<Q>(
    context: &DiagnosticsContext<'_, Q>,
    error: &CheckingError,
    kind: &TypeErrorKind,
    span: Span,
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    match kind {
        TypeErrorKind::InvalidTypeApplication { function_type, function_kind, argument_type } => {
            let function_type = context.render_type(*function_type)?;
            let argument_type = context.render_type(*argument_type)?;
            let message = format!("Cannot apply type '{function_type}' to '{argument_type}'");
            let diagnostic =
                checking_diagnostic(context, error, span, "InvalidTypeApplication", message)?;
            let function_kind = context.render_type(*function_kind)?;
            Ok(diagnostic.with_context(format!(
                "'{function_type}' has kind '{function_kind}' and does not take type arguments"
            )))
        }
        TypeErrorKind::NoVisibleTypeVariable { function_type } => {
            let rendered = context.render_type(*function_type)?;
            let message = format!("Cannot apply a visible type argument to '{rendered}'");
            let diagnostic =
                checking_diagnostic(context, error, span, "NoVisibleTypeVariable", message)?;
            Ok(diagnostic.with_context("This type has no remaining visible 'forall' binder"))
        }
        TypeErrorKind::PartialSynonymApplication { id } => {
            let message = match partial_synonym_name(context, *id)? {
                Some(name) => format!("Type synonym '{name}' is partially applied"),
                None => "Type synonym is partially applied".to_owned(),
            };
            checking_diagnostic(context, error, span, "PartialSynonymApplication", message)
        }
        TypeErrorKind::RecursiveSynonymExpansion { file_id, type_id } => {
            let message = match type_item_name(context, *file_id, *type_id)? {
                Some(name) => format!("Type synonym '{name}' expands recursively"),
                None => "Type synonym expands recursively".to_owned(),
            };
            checking_diagnostic(context, error, span, "RecursiveSynonymExpansion", message)
        }
        TypeErrorKind::TypeSignatureVariableMismatch { expected, actual, .. } => {
            let noun = if *expected == 1 { "type parameter" } else { "type parameters" };
            let message = format!(
                "Expected {expected} {noun} to match the kind signature, but found {actual}"
            );
            checking_diagnostic(context, error, span, "TypeSignatureVariableMismatch", message)
        }
        TypeErrorKind::InvalidRoleDeclaration { index, declared, inferred } => {
            let parameter = index + 1;
            let message = format!(
                "Role for parameter {parameter} cannot be '{}' because it was inferred as '{}'",
                role_name(*declared),
                role_name(*inferred),
            );
            checking_diagnostic(context, error, span, "InvalidRoleDeclaration", message)
        }
    }
}

fn term_diagnostic<Q>(
    context: &DiagnosticsContext<'_, Q>,
    error: &CheckingError,
    kind: &TermErrorKind,
    span: Span,
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    match kind {
        TermErrorKind::EmptyAdoBlock => {
            let message = "An 'ado' block cannot be empty";
            checking_diagnostic(context, error, span, "EmptyAdoBlock", message)
        }
        TermErrorKind::EmptyDoBlock => {
            let message = "A 'do' block cannot be empty";
            checking_diagnostic(context, error, span, "EmptyDoBlock", message)
        }
        TermErrorKind::InvalidFinalBind => {
            let message = "The final statement in a 'do' block should not be a bind";
            checking_diagnostic(context, error, span, "InvalidFinalBind", message)
        }
        TermErrorKind::InvalidFinalLet => {
            let message = "A 'do' block cannot end with a 'let' statement";
            checking_diagnostic(context, error, span, "InvalidFinalLet", message)
        }
        TermErrorKind::TooManyBinders { expected, actual, .. } => {
            let noun = if *expected == 1 { "pattern" } else { "patterns" };
            let message = format!("Expected at most {expected} {noun}, but found {actual}");
            checking_diagnostic(context, error, span, "TooManyBinders", message)
        }
        TermErrorKind::RedundantPatterns { patterns } => {
            let (message, heading) = if patterns.len() == 1 {
                ("Pattern match contains an unreachable pattern", "Redundant pattern:")
            } else {
                ("Pattern match contains unreachable patterns", "Redundant patterns:")
            };
            let diagnostic =
                checking_diagnostic(context, error, span, "RedundantPattern", message)?;
            let items = patterns.iter().map(SmolStr::to_string).collect();
            Ok(diagnostic.with_list(heading, items))
        }
        TermErrorKind::MissingPatterns { patterns } => {
            let message = "Pattern match is not exhaustive";
            let diagnostic = checking_diagnostic(context, error, span, "MissingPatterns", message)?;
            let heading =
                if patterns.len() == 1 { "Missing pattern:" } else { "Missing patterns:" };
            let items = patterns.iter().map(SmolStr::to_string).collect();
            Ok(diagnostic.with_list(heading, items))
        }
    }
}

fn hole_diagnostic<Q>(
    context: &DiagnosticsContext<'_, Q>,
    error: &CheckingError,
    kind: &HoleErrorKind,
    span: Span,
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    let name = context.text_of(span).trim();
    match kind {
        HoleErrorKind::TermHole { source_term } => {
            let Some(hole) = context.checked.lookup_term_hole(*source_term) else {
                let message = format!("Hole '{name}' has an unknown type");
                return checking_diagnostic(context, error, span, "TermHole", message);
            };
            let type_id = context.render_type(hole.type_id)?;
            let message = format!("Hole '{name}' has inferred type '{type_id}'");
            let diagnostic = checking_diagnostic(context, error, span, "TermHole", message)?;
            attach_hole_binding_section(diagnostic, context, &hole.bindings)
        }
        HoleErrorKind::TypeHole { source_type } => {
            let Some(hole) = context.checked.lookup_type_hole(*source_type) else {
                let message = format!("Type hole '{name}' has an unknown kind");
                return checking_diagnostic(context, error, span, "TypeHole", message);
            };
            let type_id = context.render_type(hole.type_id)?;
            let kind = context.render_type(hole.kind_id)?;
            let message = format!("Type hole '{name}' has inferred type '{type_id} :: {kind}'");
            let diagnostic = checking_diagnostic(context, error, span, "TypeHole", message)?;
            attach_hole_binding_section(diagnostic, context, &hole.bindings)
        }
    }
}

const MAX_HOLE_BINDINGS: usize = 5;

fn attach_hole_binding_section<Q>(
    diagnostic: Diagnostic,
    context: &DiagnosticsContext<'_, Q>,
    bindings: &[HoleBinding],
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    let mut items = Vec::with_capacity(bindings.len().min(MAX_HOLE_BINDINGS));

    for binding in bindings.iter().take(MAX_HOLE_BINDINGS) {
        let type_id = context.render_type(binding.type_id)?;
        items.push(format!("{} :: {type_id}", binding.name));
    }

    Ok(diagnostic.with_list("In scope:", items))
}
