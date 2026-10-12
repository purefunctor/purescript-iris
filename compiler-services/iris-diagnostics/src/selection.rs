//! Selects the diagnostics to report, with at most one error for each top-level declaration.
//!
//! The compiler keeps checking after an error so that later stages and the editor still
//! have results to work with. Later errors in the same declaration are often caused by
//! the first one, such as a type error after an unresolved name, so only the first error
//! of each declaration is reported and the rest are kept as hidden diagnostics.

use indexing::{DeriveItemId, IndexedTermItemKind, InstanceItemId, TermItemId, TypeItemId};
use itertools::Itertools;
use rustc_hash::FxHashSet;

use crate::{Diagnostic, DiagnosticsContext, ExternalQueries, Severity, Span};

/// Diagnostics sorted by position and split into the ones to report and the hidden ones.
pub(crate) struct SelectedDiagnostics {
    pub(crate) reported: Vec<Diagnostic>,
    pub(crate) hidden: Vec<Diagnostic>,
}

/// Selects the diagnostics to report.
///
/// Within a top-level declaration, only the first error is reported, and warnings are
/// hidden once the declaration has an error. Diagnostics outside of declarations, such
/// as import errors, are always reported. Backend diagnostics are hidden when the source
/// has errors, since code generation then fails on the checked data that is missing.
pub(crate) fn select_diagnostics<Q>(
    context: &DiagnosticsContext<'_, Q>,
    source: Vec<Diagnostic>,
    foreign: Vec<Diagnostic>,
    backend: Vec<Diagnostic>,
) -> SelectedDiagnostics
where
    Q: ExternalQueries,
{
    let declarations = DeclarationRanges::new(context);

    let mut hidden = Vec::new();
    let backend = if source.iter().any(is_error) {
        hidden.extend(backend);
        Vec::new()
    } else {
        backend
    };

    let candidates = source.into_iter().chain(foreign).chain(backend);
    let candidates = candidates
        .map(|diagnostic| (declarations.owner(diagnostic.span), diagnostic))
        .collect_vec();

    let failed: FxHashSet<_> = candidates
        .iter()
        .filter(|(_, diagnostic)| is_error(diagnostic))
        .filter_map(|(owner, _)| *owner)
        .collect();

    let mut reported_errors = FxHashSet::default();
    let mut reported = Vec::new();
    for (owner, diagnostic) in candidates {
        let is_reported = match (owner, diagnostic.severity) {
            (None, _) => true,
            (Some(owner), Severity::Error) => reported_errors.insert(owner),
            (Some(owner), Severity::Warning) => !failed.contains(&owner),
        };
        if is_reported {
            reported.push(diagnostic);
        } else {
            hidden.push(diagnostic);
        }
    }

    reported.sort_by_key(|diagnostic| diagnostic.span.start);
    hidden.sort_by_key(|diagnostic| diagnostic.span.start);

    SelectedDiagnostics { reported, hidden }
}

fn is_error(diagnostic: &Diagnostic) -> bool {
    diagnostic.severity == Severity::Error
}

/// A top-level declaration, which includes its signature, equations, and role declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Declaration {
    Term(TermItemId),
    Type(TypeItemId),
    Instance(InstanceItemId),
    Derive(DeriveItemId),
}

/// The source ranges of top-level declarations, sorted by their start.
struct DeclarationRanges {
    ranges: Vec<(Span, Declaration)>,
}

impl DeclarationRanges {
    fn new<Q>(context: &DiagnosticsContext<'_, Q>) -> DeclarationRanges
    where
        Q: ExternalQueries,
    {
        let indexed = context.indexed;
        let stabilized = context.stabilized;

        // Constructors and class members are nested in the declarations of their parents.
        let terms = indexed.items.iter_terms().filter(|(_, item)| {
            !matches!(
                item.kind,
                IndexedTermItemKind::Constructor { .. } | IndexedTermItemKind::ClassMember { .. }
            )
        });
        let terms = terms.flat_map(|(id, _)| {
            let pointers = indexed.term_item_ptr(stabilized, id);
            pointers.map(move |pointer| (pointer, Declaration::Term(id)))
        });

        let types = indexed.items.iter_types().flat_map(|(id, _)| {
            let pointers = indexed.type_item_ptr(stabilized, id);
            pointers.map(move |pointer| (pointer, Declaration::Type(id)))
        });

        let instances = indexed.items.iter_instances().filter_map(|(id, item)| {
            let pointer = stabilized.syntax_ptr(item.id)?;
            Some((pointer, Declaration::Instance(id)))
        });

        let derives = indexed.items.iter_derives().filter_map(|(id, item)| {
            let pointer = stabilized.syntax_ptr(item.id)?;
            Some((pointer, Declaration::Derive(id)))
        });

        let ranges = terms.chain(types).chain(instances).chain(derives);
        let ranges = ranges.filter_map(|(pointer, declaration)| {
            let span = context.span_from_syntax_ptr(&pointer)?;
            Some((span, declaration))
        });

        let ranges = ranges.sorted_by_key(|(span, _)| span.start).collect();
        DeclarationRanges { ranges }
    }

    /// Finds the top-level declaration that contains a span.
    ///
    /// The parser reports a missing token at the start of the next token, which may begin
    /// the next declaration. An empty span therefore belongs to the last declaration that
    /// starts before it, even when it is past the end of that declaration.
    fn owner(&self, span: Span) -> Option<Declaration> {
        let preceding = if span.start == span.end {
            self.ranges.partition_point(|(range, _)| range.start < span.start)
        } else {
            self.ranges.partition_point(|(range, _)| range.start <= span.start)
        };

        let (range, declaration) = self.ranges[..preceding].last()?;
        if span.start == span.end || span.end <= range.end { Some(*declaration) } else { None }
    }
}
