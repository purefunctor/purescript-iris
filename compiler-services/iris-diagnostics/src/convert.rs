use checking::error::{CheckingError, ErrorCrumb, ErrorKind};
use checking::holes::HoleBinding;
use foreign_javascript::ForeignError;
use functional::tree::{GlobalId, InstanceIdentity};
use functional::{
    ModuleError as FunctionalModuleError, UnsupportedState as FunctionalUnsupportedState,
};
use indexing::{IndexedTypeItemKind, IndexingError, InstanceSourceItemId, OrderedTermItemId};
use itertools::Itertools;
use javascript::{
    ModuleDiagnostic as JavaScriptModuleDiagnostic, ModuleError as JavaScriptModuleError,
    UnsupportedState as JavaScriptUnsupportedState,
};
use lowering::LoweringError;
use parsing::ParseError;
use resolving::{ExportSource, ResolvingError};
use smol_str::SmolStr;
use syntax::ast::AstNode;
use syntax::{SyntaxNodePtr, cst};

use building_types::QueryResult;

use crate::{Diagnostic, DiagnosticsContext, ExternalQueries, Span};

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

impl ToDiagnostics for FunctionalModuleError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        let FunctionalModuleError::Unsupported { file_id, state } = self;
        let local_global_span = |global| {
            let source_file = match global {
                GlobalId::Term(file, _) | GlobalId::Generated(file, _) => file,
                GlobalId::Instance(InstanceIdentity::Declared(file, _))
                | GlobalId::Instance(InstanceIdentity::Derived(file, _)) => file,
            };
            (source_file == *file_id).then(|| global_span(context, global)).flatten()
        };
        let module = codegen_module_name(context);
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
        let reason = match state {
            FunctionalUnsupportedState::MissingModuleName =>
                "A module header with a module name is required.".to_owned(),
            FunctionalUnsupportedState::BinderError(_) =>
                "A pattern could not be translated from its checked representation.".to_owned(),
            FunctionalUnsupportedState::RecordUpdateError =>
                "A record update contains an error.".to_owned(),
            FunctionalUnsupportedState::PatternBindingError(_) =>
                "A pattern binding in a let expression contains an error.".to_owned(),
            FunctionalUnsupportedState::UnsolvedEvidence(_) =>
                "An unresolved type class constraint remains in the checked code.".to_owned(),
            FunctionalUnsupportedState::CyclicEvidence(_) =>
                "The compiler encountered a cycle while constructing type class dictionaries.\n\nThis is an unsupported internal compiler state.".to_owned(),
            FunctionalUnsupportedState::MissingTermDeclaration(_) =>
                "The checked representation of a top-level declaration is unavailable.".to_owned(),
            FunctionalUnsupportedState::MissingInstanceDeclaration =>
                "The checked representation of an instance declaration is unavailable.".to_owned(),
            FunctionalUnsupportedState::MissingLocalDeclaration(id) => {
                let declaration = context.lowered.tree.get_let_binding(*id)
                    .and_then(|_| context.lowered.tree.get_let_binding_group(*id).name.as_ref())
                    .map(|name| format!("local declaration '{name}'"))
                    .unwrap_or_else(|| "a local declaration".to_owned());
                format!("The checked representation of {declaration} is unavailable.")
            }
            FunctionalUnsupportedState::MissingEquation =>
                "A checked value declaration has no defining equations.".to_owned(),
            FunctionalUnsupportedState::ConflictingRuntimeExport { name, .. } =>
                format!("More than one declaration exports the runtime name '{name}'."),
            FunctionalUnsupportedState::MissingRuntimeExportOperatorResolution { term_id } => {
                let operator = term_name(*term_id)
                    .map(|name| format!("exported operator '{name}'"))
                    .unwrap_or_else(|| "an exported operator".to_owned());
                format!("The runtime implementation of {operator} could not be resolved.")
            }
            FunctionalUnsupportedState::InvalidInstancePrerequisite =>
                "An instance prerequisite has an unsupported dictionary representation.\n\nThis is an unsupported internal compiler state.".to_owned(),
            FunctionalUnsupportedState::LocalIdentityOverflow =>
                "The compiler's limit on local bindings was exceeded.".to_owned(),
            FunctionalUnsupportedState::GeneratedGlobalIdentityOverflow =>
                "The compiler's limit on generated top-level declarations was exceeded.".to_owned(),
            FunctionalUnsupportedState::InvalidStyleXUse { function, .. } =>
                format!("'{function}' must be called directly with all of its arguments.\n\nIt cannot be passed around as a function or partially applied."),
            FunctionalUnsupportedState::InvalidStyleXContext { function, requirement, .. } =>
                format!("'{function}' {requirement}."),
            FunctionalUnsupportedState::VirtualModuleRuntimeReference { module_name, item_name } =>
                format!("'{module_name}.{item_name}' is a compile-time declaration and cannot be used at runtime."),
        };
        let mut diagnostic = Diagnostic::error(
            "FunctionalCodegen",
            format!("Cannot generate JavaScript for {module}.\n\n{reason}"),
            span.unwrap_or_else(|| context.module_span()),
            "functional",
        );
        if let FunctionalUnsupportedState::ConflictingRuntimeExport { existing, .. } = state
            && let Some(span) = local_global_span(*existing)
        {
            diagnostic = diagnostic
                .with_related(span, "The other declaration exports the same runtime name");
        }
        Ok(vec![diagnostic])
    }
}

fn codegen_module_name<Q>(context: &DiagnosticsContext<'_, Q>) -> String
where
    Q: ExternalQueries,
{
    cst::Module::cast(context.root.clone())
        .and_then(|module| module.header())
        .and_then(|header| header.name())
        .filter(|name| !name.syntax().text(context.content).is_empty())
        .map(|name| format!("module '{}'", name.syntax().text(context.content)))
        .unwrap_or_else(|| "this module".to_owned())
}

impl ToDiagnostics for JavaScriptModuleError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        match self {
            JavaScriptModuleError::Functional(error) => error.to_diagnostics(context),
            JavaScriptModuleError::Unsupported { state, .. } => {
                let reason = match state {
                    JavaScriptUnsupportedState::InvalidNumber { value } => format!(
                        "The number literal '{value}' cannot be represented as a finite JavaScript number."
                    ),
                    JavaScriptUnsupportedState::InvalidInteger { value } => format!(
                        "The integer literal '{value}' is outside the signed 32-bit range (-2147483648 to 2147483647)."
                    ),
                    JavaScriptUnsupportedState::MissingGlobal { .. } =>
                        "The compiler could not find a JavaScript declaration for a referenced value.\n\nThis is an unsupported internal compiler state.".to_owned(),
                    JavaScriptUnsupportedState::MissingLocal { .. } =>
                        "The compiler could not find a JavaScript binding for a referenced local value.\n\nThis is an unsupported internal compiler state.".to_owned(),
                };
                let module = codegen_module_name(context);
                Ok(vec![Diagnostic::error(
                    "JavaScriptCodegen",
                    format!("Cannot generate JavaScript for {module}.\n\n{reason}"),
                    context.module_span(),
                    "javascript",
                )])
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
        let mut diagnostic =
            Diagnostic::error("JavaScriptInitializerCycle", self.to_string(), span, "javascript");
        for span in spans {
            diagnostic =
                diagnostic.with_related(span, "This declaration is a member of the same cycle");
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
        let Some(span) = context
            .indexed
            .term_item_ptr(context.stabilized, *declaration)
            .next()
            .and_then(|pointer| context.span_from_syntax_ptr(&pointer))
        else {
            return Ok(Vec::new());
        };

        let diagnostic = match self {
            ForeignError::MissingModule { name, .. } => Diagnostic::error(
                "MissingFFIModule",
                format!("No JavaScript FFI module was found for foreign import '{name}'"),
                span,
                "foreign-javascript",
            ),
            ForeignError::AmbiguousModule { .. } => Diagnostic::error(
                "AmbiguousFFIModule",
                "Both JavaScript and JSX FFI modules were found. Remove one of the foreign files",
                span,
                "foreign-javascript",
            ),
            ForeignError::MissingImplementation { name, .. } => Diagnostic::error(
                "MissingFFIImplementation",
                format!("JavaScript FFI module does not export '{name}'"),
                span,
                "foreign-javascript",
            ),
            ForeignError::Parse { message, .. } => Diagnostic::warning(
                "UnparseableFFIModule",
                format!(
                    "Oxc could not parse the JavaScript FFI module. Fix the invalid or unsupported JavaScript syntax; Iris treated the module as opaque and skipped export-name validation: {message}"
                ),
                span,
                "foreign-javascript",
            ),
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
        Ok(vec![Diagnostic::error("ParseError", self.message.as_ref(), span, "parsing")])
    }
}

impl ToDiagnostics for LoweringError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        match self {
            LoweringError::NotInScope(not_in_scope) => {
                let (ptr, name) = match not_in_scope {
                    lowering::NotInScope::BinderConstructor { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::ExprConstructor { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::ExprVariable { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::ExprOperatorName { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::ExprRecordPun { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::TypeClass { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::TypeConstructor { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::TypeVariable { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::TypeOperatorName { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::NegateFn { id } => {
                        (context.stabilized.syntax_ptr(*id), Some("negate"))
                    }
                    lowering::NotInScope::DoFn { kind, id } => (
                        context.stabilized.syntax_ptr(*id),
                        match kind {
                            lowering::DoFn::Bind => Some("bind"),
                            lowering::DoFn::Discard => Some("discard"),
                        },
                    ),
                    lowering::NotInScope::AdoFn { kind, id } => (
                        context.stabilized.syntax_ptr(*id),
                        match kind {
                            lowering::AdoFn::Map => Some("map"),
                            lowering::AdoFn::Apply => Some("apply"),
                            lowering::AdoFn::Pure => Some("pure"),
                        },
                    ),
                    lowering::NotInScope::TermOperator { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::TypeOperator { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::TermOperatorTarget { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                    lowering::NotInScope::TypeOperatorTarget { id } => {
                        (context.stabilized.syntax_ptr(*id), None)
                    }
                };

                let Some(ptr) = ptr else { return Ok(Vec::new()) };
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
                    _ => context.span_from_syntax_ptr(&ptr),
                };
                let Some(span) = span else { return Ok(Vec::new()) };

                let message = if let Some(name) = name {
                    format!("'{name}' is not in scope")
                } else {
                    let text = context.text_of(span).trim();
                    format!("'{text}' is not in scope")
                };

                Ok(vec![Diagnostic::error("NotInScope", message, span, "lowering")])
            }

            LoweringError::InvalidStringEscape { source } => {
                let ptr = match source {
                    lowering::StringLiteralSource::Expression(id) => {
                        context.stabilized.syntax_ptr(*id)
                    }
                    lowering::StringLiteralSource::Binder(id) => context.stabilized.syntax_ptr(*id),
                    lowering::StringLiteralSource::Type(id) => context.stabilized.syntax_ptr(*id),
                };
                let Some(ptr) = ptr else { return Ok(Vec::new()) };
                let Some(span) = context.span_from_syntax_ptr(&ptr) else { return Ok(Vec::new()) };

                Ok(vec![Diagnostic::error(
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
                let Some(duplicate) = span(*duplicate) else { return Ok(Vec::new()) };

                let message = format!("Name '{name}' is bound more than once");
                let mut diagnostic =
                    Diagnostic::error("OverlappingBinderNames", message, duplicate, "lowering");
                if let Some(existing) = span(*existing) {
                    diagnostic = diagnostic.with_related(existing, "First bound here");
                }

                Ok(vec![diagnostic])
            }

            LoweringError::RecursiveSynonym(group) => Ok(convert_recursive_group(
                context,
                &group.group,
                "RecursiveSynonym",
                "Invalid type synonym cycle",
            )),

            LoweringError::RecursiveKinds(group) => Ok(convert_recursive_group(
                context,
                &group.group,
                "RecursiveKinds",
                "Invalid kind cycle",
            )),
        }
    }
}

fn convert_recursive_group<Q>(
    context: &DiagnosticsContext<'_, Q>,
    group: &[indexing::TypeItemId],
    code: &'static str,
    message: &'static str,
) -> Vec<Diagnostic>
where
    Q: ExternalQueries,
{
    let spans = group.iter().filter_map(|id| {
        let ptr = match context.indexed.items[*id].kind {
            IndexedTypeItemKind::Synonym { equation, .. } => {
                context.stabilized.syntax_ptr(equation?)?
            }
            IndexedTypeItemKind::Data { equation, .. } => {
                context.stabilized.syntax_ptr(equation?)?
            }
            IndexedTypeItemKind::Newtype { equation, .. } => {
                context.stabilized.syntax_ptr(equation?)?
            }
            IndexedTypeItemKind::Foreign { id, .. } => context.stabilized.syntax_ptr(id)?,
            _ => return None,
        };
        context.span_from_syntax_ptr(&ptr)
    });

    let spans = spans.collect_vec();

    let Some(&primary) = spans.first() else { return Vec::new() };

    let mut diagnostic = Diagnostic::error(code, message, primary, "lowering");

    for &span in &spans[1..] {
        diagnostic = diagnostic.with_related(span, "Includes this type");
    }

    vec![diagnostic]
}

impl ToDiagnostics for ResolvingError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        match self {
            ResolvingError::DuplicateModule { name } => {
                let span = context.module_name_span().unwrap_or_else(|| context.module_span());
                let message = format!("Module '{name}' has been defined multiple times");
                Ok(vec![Diagnostic::error("DuplicateModule", message, span, "resolving")])
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
                let Some(span) = pointer.and_then(|pointer| context.span_from_syntax_ptr(&pointer))
                else {
                    return Ok(Vec::new());
                };
                let (code, message, pointer) = match existing {
                    OrderedTermItemId::Term(id) => (
                        "RedefinedIdent",
                        format!("Instance name '{name}' conflicts with a local value declaration"),
                        context.indexed.term_item_ptr(context.stabilized, *id).next(),
                    ),
                    OrderedTermItemId::Instance(id) => (
                        "DuplicateInstance",
                        format!("Instance name '{name}' has been defined multiple times"),
                        context.stabilized.syntax_ptr(context.indexed.items[*id].id),
                    ),
                    OrderedTermItemId::Derive(id) => (
                        "DuplicateInstance",
                        format!("Instance name '{name}' has been defined multiple times"),
                        context.stabilized.syntax_ptr(context.indexed.items[*id].id),
                    ),
                };
                let mut diagnostic = Diagnostic::error(code, message, span, "resolving");
                if let Some(span) =
                    pointer.and_then(|pointer| context.span_from_syntax_ptr(&pointer))
                {
                    diagnostic = diagnostic.with_related(span, "Conflicting declaration");
                }
                Ok(vec![diagnostic])
            }
            ResolvingError::TermExportConflict { name, existing, duplicate } => {
                let (_, _, duplicate_source) = *duplicate;
                let (_, _, existing_source) = *existing;
                let pointer = |(_, id, source)| match source {
                    ExportSource::Local => {
                        context.indexed.term_item_ptr(context.stabilized, id).next()
                    }
                    ExportSource::Import(import) => context.stabilized.syntax_ptr(import),
                };
                let duplicate_declaration = matches!(
                    (existing_source, duplicate_source),
                    (ExportSource::Local, ExportSource::Local)
                );
                Ok(export_conflict(
                    context,
                    name,
                    pointer(*duplicate),
                    pointer(*existing),
                    duplicate_declaration,
                ))
            }
            ResolvingError::TypeExportConflict { name, existing, duplicate } => {
                let (_, _, duplicate_source) = *duplicate;
                let (_, _, existing_source) = *existing;
                let pointer = |(_, id, source)| match source {
                    ExportSource::Local => {
                        context.indexed.type_item_ptr(context.stabilized, id).next()
                    }
                    ExportSource::Import(import) => context.stabilized.syntax_ptr(import),
                };
                let duplicate_declaration = matches!(
                    (existing_source, duplicate_source),
                    (ExportSource::Local, ExportSource::Local)
                );
                Ok(export_conflict(
                    context,
                    name,
                    pointer(*duplicate),
                    pointer(*existing),
                    duplicate_declaration,
                ))
            }
            ResolvingError::ExistingTerm { .. } | ResolvingError::ExistingType { .. } => {
                Ok(Vec::new())
            }

            ResolvingError::InvalidImportStatement { id } => {
                let Some(ptr) = context.stabilized.ast_ptr(*id) else { return Ok(Vec::new()) };

                let message = {
                    let cst = ptr.to_node(context.root);
                    let name = cst.module_name().map(|cst| {
                        let range = cst.syntax().text_range();
                        context.content[range].trim()
                    });
                    let name = name.unwrap_or("<ParseError>");
                    format!("Cannot import module '{name}'")
                };

                let Some(span) = context.span_from_ast_ptr(&ptr) else { return Ok(Vec::new()) };

                Ok(vec![Diagnostic::error("InvalidImportStatement", message, span, "resolving")])
            }

            ResolvingError::InvalidImportItem { id } => {
                let Some(ptr) = context.stabilized.syntax_ptr(*id) else { return Ok(Vec::new()) };
                let Some(span) = context.span_from_syntax_ptr(&ptr) else { return Ok(Vec::new()) };

                let text = context.text_of(span).trim();
                let message = format!("Cannot import item '{text}'");

                Ok(vec![Diagnostic::error("InvalidImportItem", message, span, "resolving")])
            }
        }
    }
}

fn export_conflict<Q>(
    context: &DiagnosticsContext<'_, Q>,
    name: &SmolStr,
    duplicate: Option<SyntaxNodePtr>,
    existing: Option<SyntaxNodePtr>,
    duplicate_declaration: bool,
) -> Vec<Diagnostic>
where
    Q: ExternalQueries,
{
    let span = |pointer: Option<SyntaxNodePtr>| {
        pointer.and_then(|pointer| context.span_from_syntax_ptr(&pointer))
    };
    let (code, related, message) = if duplicate_declaration {
        (
            "DuplicateDeclaration",
            "Previous declaration",
            format!("'{name}' is declared more than once"),
        )
    } else {
        ("ExportConflict", "Conflicting export", format!("Conflicting exports for '{name}'"))
    };
    let primary = span(duplicate)
        .or_else(|| context.module_name_span())
        .unwrap_or_else(|| context.module_span());
    let mut diagnostic = Diagnostic::error(code, message, primary, "resolving");
    if let Some(existing) = span(existing) {
        diagnostic = diagnostic.with_related(existing, related);
    }
    vec![diagnostic]
}

impl ToDiagnostics for IndexingError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        match self {
            IndexingError::DuplicateImport { duplicate, existing } => {
                let Some(ptr) = context.stabilized.syntax_ptr(*duplicate) else {
                    return Ok(Vec::new());
                };
                let Some(span) = context.span_from_syntax_ptr(&ptr) else { return Ok(Vec::new()) };

                let text = context.text_of(span).trim();
                let message = format!("Import list contains multiple references to '{text}'");

                let mut diagnostic =
                    Diagnostic::warning("DuplicateImport", message, span, "indexing");

                if let Some(existing_ptr) = context.stabilized.syntax_ptr(*existing)
                    && let Some(existing_span) = context.span_from_syntax_ptr(&existing_ptr)
                {
                    diagnostic = diagnostic.with_related(existing_span, "First imported here");
                }

                Ok(vec![diagnostic])
            }
            IndexingError::DuplicateItem { .. }
            | IndexingError::MismatchedItem { .. }
            | IndexingError::InvalidRole { .. }
            | IndexingError::InvalidExport { .. }
            | IndexingError::DuplicateExport { .. } => Ok(Vec::new()),
        }
    }
}

impl ToDiagnostics for CheckingError {
    fn to_diagnostics<Q>(&self, context: &DiagnosticsContext<'_, Q>) -> QueryResult<Vec<Diagnostic>>
    where
        Q: ExternalQueries,
    {
        let span = context.primary_span_from_crumbs(&self.crumbs);
        let lookup_message = |id| context.queries.lookup_checking_smol_str(id);
        let render_type = |id| context.render_type(id);

        let (code, message) = match &self.kind {
            ErrorKind::AmbiguousConstraint { constraint } => {
                let message = render_type(*constraint)?;
                ("AmbiguousConstraint", format!("Ambiguous constraint: {message}"))
            }
            ErrorKind::CannotDeriveClass { .. } => {
                ("CannotDeriveClass", "Cannot derive this class".to_string())
            }
            ErrorKind::CannotDeriveForType { type_id } => {
                let message = render_type(*type_id)?;
                ("CannotDeriveForType", format!("Cannot derive for type: {message}"))
            }
            ErrorKind::CannotGeneraliseRecursiveFunction { .. } => (
                "CannotGeneraliseRecursiveFunction",
                "Unable to generalise the type of this recursive function.".to_string(),
            ),
            ErrorKind::ContravariantOccurrence { type_id } => {
                let message = render_type(*type_id)?;
                (
                    "ContravariantOccurrence",
                    format!("Type variable occurs in contravariant position: {message}"),
                )
            }
            ErrorKind::CovariantOccurrence { type_id } => {
                let message = render_type(*type_id)?;
                (
                    "CovariantOccurrence",
                    format!("Type variable occurs in covariant position: {message}"),
                )
            }
            ErrorKind::CannotUnify { t1, t2, .. } => {
                let t1 = render_type(*t1)?;
                let t2 = render_type(*t2)?;
                ("CannotUnify", format!("Cannot unify '{t1}' with '{t2}'"))
            }
            ErrorKind::DeriveInvalidArity { expected, actual, .. } => (
                "DeriveInvalidArity",
                format!("Invalid arity for derive: expected {expected}, got {actual}"),
            ),
            ErrorKind::DeriveNotSupportedYet { .. } => {
                ("DeriveNotSupportedYet", "Deriving this class is not supported yet".to_string())
            }
            ErrorKind::DeriveMissingFunctor => (
                "DeriveMissingFunctor",
                "Deriving Functor requires Data.Functor to be in scope".to_string(),
            ),
            ErrorKind::EmptyAdoBlock => ("EmptyAdoBlock", "Empty ado block".to_string()),
            ErrorKind::EmptyDoBlock => ("EmptyDoBlock", "Empty do block".to_string()),
            ErrorKind::EscapedSkolem { skolem, type_id } => {
                let skolem = render_type(*skolem)?;
                let type_id = render_type(*type_id)?;
                (
                    "EscapedSkolem",
                    format!(
                        "Type variable '{skolem}' has escaped its scope, appearing in type '{type_id}'"
                    ),
                )
            }
            ErrorKind::TermHole { source_term } => {
                let name = context.text_of(span).trim();
                if let Some(hole) = context.checked.lookup_term_hole(*source_term) {
                    let type_id = render_type(hole.type_id)?;
                    ("TermHole", format!("Hole '{name}' has inferred type: {type_id}"))
                } else {
                    ("TermHole", format!("Hole '{name}' has unknown type"))
                }
            }
            ErrorKind::TypeHole { source_type } => {
                let name = context.text_of(span).trim();
                if let Some(hole) = context.checked.lookup_type_hole(*source_type) {
                    let type_id = render_type(hole.type_id)?;
                    let kind = render_type(hole.kind_id)?;
                    (
                        "TypeHole",
                        format!("Type hole '{name}' has inferred type: {type_id} :: {kind}"),
                    )
                } else {
                    ("TypeHole", format!("Type hole '{name}' has unknown kind"))
                }
            }
            ErrorKind::InvalidFinalBind => {
                ("InvalidFinalBind", "Invalid final bind statement in do expression".to_string())
            }
            ErrorKind::InvalidFinalLet => {
                ("InvalidFinalLet", "Invalid final let statement in do expression".to_string())
            }
            ErrorKind::InstanceHeadMismatch { expected, actual, .. } => (
                "InstanceHeadMismatch",
                format!("Instance head mismatch: expected {expected} arguments, got {actual}"),
            ),
            ErrorKind::InstanceHeadLabeledRow { position, type_id, .. } => {
                let type_msg = render_type(*type_id)?;
                (
                    "InstanceHeadLabeledRow",
                    format!(
                        "Instance argument at position {position} contains a labeled row, \
                         but this position is not determined by any functional dependency. \
                         Only the `( | r )` form is allowed. Got '{type_msg}' instead."
                    ),
                )
            }
            ErrorKind::InstanceMemberTypeMismatch { expected, actual } => {
                let expected = render_type(*expected)?;
                let actual = render_type(*actual)?;
                (
                    "InstanceMemberTypeMismatch",
                    format!("Instance member type mismatch: expected '{expected}', got '{actual}'"),
                )
            }
            ErrorKind::MissingInstanceMembers { .. } => {
                ("MissingInstanceMembers", "Instance is missing class members".to_string())
            }
            ErrorKind::InvalidTypeApplication { function_type, function_kind, argument_type } => {
                let function_type = render_type(*function_type)?;
                let function_kind = render_type(*function_kind)?;
                let argument_type = render_type(*argument_type)?;
                (
                    "InvalidTypeApplication",
                    format!(
                        "Cannot apply type '{function_type}' to '{argument_type}'. \
                         '{function_type}' has kind '{function_kind}', which is not a function kind."
                    ),
                )
            }
            ErrorKind::ExpectedNewtype { type_id } => {
                let message = render_type(*type_id)?;
                ("ExpectedNewtype", format!("Expected a newtype, got: {message}"))
            }
            ErrorKind::InvalidNewtypeDeriveSkolemArguments => (
                "InvalidNewtypeDeriveSkolemArguments",
                "Cannot derive newtype instance where skolemised arguments do not appear trailing in the inner type."
                    .to_string(),
            ),
            ErrorKind::NonLocalNewtype { type_id } => {
                let message = render_type(*type_id)?;
                ("NonLocalNewtype", format!("Expected a local newtype, got: {message}"))
            }
            ErrorKind::NoInstanceFound { constraint, .. } => {
                let constraint = render_type(*constraint)?;
                let message = format!("No instance found for: {constraint}");
                ("NoInstanceFound", message)
            }
            ErrorKind::OrphanInstance { constraint } => {
                let constraint = render_type(*constraint)?;
                let message = format!("Orphan instance found for: {constraint}");
                ("OrphanInstance", message)
            }
            ErrorKind::OverlappingInstances { constraint, .. } => {
                let constraint = render_type(*constraint)?;
                let message = format!("Overlapping type class instances found for: {constraint}");
                ("OverlappingInstances", message)
            }
            ErrorKind::NoVisibleTypeVariable { function_type } => {
                let message = render_type(*function_type)?;
                (
                    "NoVisibleTypeVariable",
                    format!("No visible type variable for type application in: {message}"),
                )
            }
            ErrorKind::PartialSynonymApplication { .. } => {
                ("PartialSynonymApplication", "Partial type synonym application".to_string())
            }
            ErrorKind::RecursiveSynonymExpansion { .. } => {
                ("RecursiveSynonymExpansion", "Recursive type synonym expansion".to_string())
            }
            ErrorKind::TooManyBinders { expected, actual, .. } => {
                ("TooManyBinders", format!("Too many binders: expected {expected}, got {actual}"))
            }
            ErrorKind::TypeSignatureVariableMismatch { expected, actual, .. } => (
                "TypeSignatureVariableMismatch",
                format!(
                    "Type signature variable mismatch: expected {expected} variables, got {actual}"
                ),
            ),
            ErrorKind::InvalidRoleDeclaration { declared, inferred, .. } => (
                "InvalidRoleDeclaration",
                format!("Invalid role declaration: declared {declared:?}, inferred {inferred:?}"),
            ),
            ErrorKind::CoercibleConstructorNotInScope { .. } => (
                "CoercibleConstructorNotInScope",
                "Constructor not in scope for Coercible".to_string(),
            ),
            ErrorKind::RedundantPatterns { patterns } => {
                let patterns = patterns.join(", ");
                ("RedundantPattern", format!("Pattern match has redundant patterns: {patterns}"))
            }
            ErrorKind::MissingPatterns { patterns } => {
                let patterns = patterns.join(", ");
                ("MissingPatterns", format!("Pattern match is not exhaustive. Missing: {patterns}"))
            }
            ErrorKind::CustomWarning { message_id } => {
                let message = lookup_message(*message_id);
                ("CustomWarning", message.to_string())
            }
            ErrorKind::CustomFailure { message_id } => {
                let message = lookup_message(*message_id);
                ("CustomFailure", message.to_string())
            }
            ErrorKind::PropertyIsMissing { labels } => {
                let labels_str = labels.join(", ");
                ("PropertyIsMissing", format!("Missing required properties: {labels_str}"))
            }
            ErrorKind::UnreportedUnknownType { type_id } => {
                let type_id = render_type(*type_id)?;
                (
                    "UnreportedUnknownType",
                    format!(
                        "The unknown type '{type_id}' was accepted without an error that explains it.\n\nThis is an unsupported internal compiler state."
                    ),
                )
            }
            ErrorKind::AdditionalProperty { labels } => {
                let labels_str = labels.join(", ");
                ("AdditionalProperty", format!("Additional properties not allowed: {labels_str}"))
            }
        };

        let mut diagnostic = if self.kind.is_warning() {
            Diagnostic::warning(code, message, span, "checking")
        } else {
            Diagnostic::error(code, message, span, "checking")
        };

        if let ErrorKind::NoInstanceFound { given, .. } = &self.kind {
            for &given in given.iter() {
                let given = render_type(given)?;
                let trivia = format!("{given} is in scope");
                diagnostic = diagnostic.with_trivia(trivia)
            }
        }

        match &self.kind {
            // Types are zonked by now, so equal types share an identifier.
            &ErrorKind::CannotUnify { t1, t2, outer: (outer_t1, outer_t2) }
                if (t1, t2) != (outer_t1, outer_t2) =>
            {
                let outer_t1 = render_type(outer_t1)?;
                let outer_t2 = render_type(outer_t2)?;
                let trivia = format!("matching '{outer_t1}' with '{outer_t2}'");
                diagnostic = diagnostic.with_trivia(trivia);
            }
            ErrorKind::OverlappingInstances { instances, .. } => {
                for &instance in instances.iter() {
                    let instance = render_type(instance)?;
                    let trivia = format!("{instance} is matching");
                    diagnostic = diagnostic.with_trivia(trivia)
                }
            }
            ErrorKind::TermHole { source_term } => {
                if let Some(hole) = context.checked.lookup_term_hole(*source_term) {
                    diagnostic = attach_hole_binding_trivia(diagnostic, context, &hole.bindings)?;
                }
            }
            ErrorKind::TypeHole { source_type } => {
                if let Some(hole) = context.checked.lookup_type_hole(*source_type) {
                    diagnostic = attach_hole_binding_trivia(diagnostic, context, &hole.bindings)?;
                }
            }
            ErrorKind::CannotGeneraliseRecursiveFunction { type_id } => {
                let inferred = render_type(*type_id)?;
                let trivia = format!("The inferred type was: {inferred}");
                diagnostic = diagnostic.with_trivia(trivia);
                diagnostic = diagnostic.with_trivia("Try adding a type signature.");
            }
            ErrorKind::MissingInstanceMembers { members } => {
                for member in members.iter() {
                    let trivia = format!("{member} is not implemented");
                    diagnostic = diagnostic.with_trivia(trivia);
                }
            }
            _ => {}
        }

        Ok(vec![diagnostic])
    }
}

const MAX_HOLE_BINDINGS: usize = 5;

fn attach_hole_binding_trivia<Q>(
    mut diagnostic: Diagnostic,
    context: &DiagnosticsContext<'_, Q>,
    bindings: &[HoleBinding],
) -> QueryResult<Diagnostic>
where
    Q: ExternalQueries,
{
    diagnostic.trivia.reserve(bindings.len().min(MAX_HOLE_BINDINGS));

    for binding in bindings.iter().take(MAX_HOLE_BINDINGS) {
        let type_id = context.render_type(binding.type_id)?;
        let trivia = format!("{} :: {type_id} is in scope", binding.name);
        diagnostic = diagnostic.with_trivia(trivia);
    }

    Ok(diagnostic)
}
