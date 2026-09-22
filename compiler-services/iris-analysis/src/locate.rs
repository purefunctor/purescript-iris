//! Abstractions for identifying syntax at a given location.

use std::iter;

use files::FileId;
use indexing::{
    DeriveItemId, ImportItemId, IndexedModule, IndexedTermItemKind, IndexedTypeItemKind,
    InstanceItemId, TermItemId, TypeItemId,
};
use lowering::{
    BinderId, ExpressionId, LetBindingNameGroupId, LoweredModule, RecordAccessLabelId, RecordPunId,
    TermItemKind, TermOperatorId, TypeId, TypeItemKind, TypeOperatorId, TypeVariableBindingId,
};
use stabilizing::{AstId, StabilizedModule};
use syntax::ast::{AstNode, AstPtr};
use syntax::{SyntaxNode, SyntaxNodePtr, SyntaxToken, TokenAtOffset, cst};

use crate::extract::AnnotationSyntaxRange;
use crate::position::{PositionConverter, Utf8Position, Utf8Range};
use crate::{AnalyzerError, AnalyzerQueries, position};

pub fn syntax_range(
    positions: &PositionConverter<'_>,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let range = AnnotationSyntaxRange::from_ptr(root, ptr);
    range.syntax.and_then(|range| positions.text_range_to_utf8_range(range))
}

pub fn id_range<T>(
    positions: &PositionConverter<'_>,
    parsed: &parsing::ParsedModule,
    stabilized: &StabilizedModule,
    item_id: AstId<T>,
) -> Option<Utf8Range>
where
    T: AstNode,
{
    let root = parsed.syntax_node();
    let ptr = stabilized.syntax_ptr(item_id)?;
    syntax_range(positions, &root, &ptr)
}

pub fn instance_head_ranges(
    positions: &PositionConverter<'_>,
    parsed: &parsing::ParsedModule,
    stabilized: &StabilizedModule,
    indexed: &IndexedModule,
    lowered: &LoweredModule,
    target: (FileId, TypeItemId),
) -> Vec<Utf8Range> {
    let root = parsed.syntax_node();
    let ranges = indexed.items.instance_sources().iter().filter_map(|item_id| match item_id {
        indexing::InstanceSourceItemId::Instance(id) => {
            let item = &indexed.items[*id];
            let lowered = lowered.tree.get_instance_item(*id)?;
            if lowered.resolution != Some(target) {
                return None;
            }
            instance_head_range(positions, &root, stabilized, item.id)
        }
        indexing::InstanceSourceItemId::Derive(id) => {
            let item = &indexed.items[*id];
            let lowered = lowered.tree.get_derive_item(*id)?;
            if lowered.resolution != Some(target) {
                return None;
            }
            instance_head_range(positions, &root, stabilized, item.id)
        }
    });
    ranges.collect()
}

pub fn term_infix_reference_ranges(
    positions: &PositionConverter<'_>,
    parsed: &parsing::ParsedModule,
    stabilized: &StabilizedModule,
    indexed: &IndexedModule,
    lowered: &LoweredModule,
    target: (FileId, TermItemId),
) -> Vec<Utf8Range> {
    let root = parsed.syntax_node();
    let ranges = indexed.items.iter_terms().filter_map(|(term_id, item)| {
        let IndexedTermItemKind::Operator { id } = &item.kind else { return None };
        let TermItemKind::Operator { resolution, .. } = lowered.tree.get_term_item_kind(term_id)?
        else {
            return None;
        };
        if resolution.as_ref() != Some(&target) {
            return None;
        }
        infix_reference_range(positions, &root, stabilized, *id)
    });
    ranges.collect()
}

pub fn type_infix_reference_ranges(
    positions: &PositionConverter<'_>,
    parsed: &parsing::ParsedModule,
    stabilized: &StabilizedModule,
    indexed: &IndexedModule,
    lowered: &LoweredModule,
    target: (FileId, TypeItemId),
) -> Vec<Utf8Range> {
    let root = parsed.syntax_node();
    let ranges = indexed.items.iter_types().filter_map(|(type_id, item)| {
        let IndexedTypeItemKind::Operator { id } = &item.kind else { return None };
        let TypeItemKind::Operator { resolution, .. } = lowered.tree.get_type_item_kind(type_id)?
        else {
            return None;
        };
        if resolution.as_ref() != Some(&target) {
            return None;
        }
        infix_reference_range(positions, &root, stabilized, *id)
    });
    ranges.collect()
}

fn infix_reference_range(
    positions: &PositionConverter<'_>,
    root: &SyntaxNode,
    stabilized: &StabilizedModule,
    id: AstId<cst::InfixDeclaration>,
) -> Option<Utf8Range> {
    let ptr = stabilized.syntax_ptr(id)?;
    let declaration = ptr.try_to_node(root).and_then(cst::InfixDeclaration::cast)?;
    let qualified = declaration.qualified()?;
    let range = position::qualified_name_text_range(&qualified)?;
    positions.text_range_to_utf8_range(range)
}

fn instance_head_range<T>(
    positions: &PositionConverter<'_>,
    root: &SyntaxNode,
    stabilized: &StabilizedModule,
    id: AstId<T>,
) -> Option<Utf8Range>
where
    T: AstNode,
{
    let ptr = stabilized.syntax_ptr(id)?;
    let node = ptr.try_to_node(root)?;
    let head = cst::InstanceDeclaration::cast(node.clone())
        .and_then(|instance| instance.instance_head())
        .or_else(|| cst::DeriveDeclaration::cast(node)?.instance_head())?;
    let token = head.qualified()?.upper()?;
    positions.text_range_to_utf8_range(token.text_range())
}

pub fn value_equation_ranges(
    positions: &PositionConverter<'_>,
    root: &SyntaxNode,
    stabilized: &StabilizedModule,
    indexed: &IndexedModule,
    term_id: TermItemId,
) -> Option<Vec<Utf8Range>> {
    let indexing::IndexedTermItemKind::Value { signature, equations } =
        &indexed.items[term_id].kind
    else {
        return None;
    };

    let mut ranges = vec![];

    if let Some(sig_id) = signature
        && let Some(ptr) = stabilized.ast_ptr(*sig_id)
        && let Some(node) = ptr.try_to_node(root)
        && let Some(tok) = node.name_token()
        && let Some(range) = positions.text_range_to_utf8_range(tok.text_range())
    {
        ranges.push(range);
    }

    for eq_id in equations {
        if let Some(ptr) = stabilized.ast_ptr(*eq_id)
            && let Some(node) = ptr.try_to_node(root)
            && let Some(tok) = node.name_token()
            && let Some(range) = positions.text_range_to_utf8_range(tok.text_range())
        {
            ranges.push(range);
        }
    }

    Some(ranges)
}

type ModuleNamePtr = AstPtr<cst::ModuleName>;

#[derive(Debug, PartialEq, Eq)]
pub enum Located {
    ModuleName(ModuleNamePtr),
    ImportItem(ImportItemId),
    Binder(BinderId),
    Expression(ExpressionId),
    RecordAccessLabel(RecordAccessLabelId),
    Type(TypeId),
    TypeVariableBinding(TypeVariableBindingId),
    BinderPun(RecordPunId),
    ExpressionPun(RecordPunId),
    TermOperator(TermOperatorId),
    TypeOperator(TypeOperatorId),
    TermReference(FileId, TermItemId),
    TypeReference(FileId, TypeItemId),
    InstanceHead(FileId, TypeItemId),
    InstanceMember(FileId, TermItemId),
    TermItem(TermItemId),
    TypeItem(TypeItemId),
    InstanceItem(InstanceItemId),
    DeriveItem(DeriveItemId),
    LetBinding(LetBindingNameGroupId),
    Nothing,
}

pub fn locate(
    engine: &impl AnalyzerQueries,
    id: FileId,
    positions: &PositionConverter<'_>,
    position: Utf8Position,
) -> Result<Located, AnalyzerError> {
    let (located, _) = locate_with_token(engine, id, positions, position)?;
    Ok(located)
}

pub(crate) fn locate_with_token(
    engine: &impl AnalyzerQueries,
    id: FileId,
    positions: &PositionConverter<'_>,
    position: Utf8Position,
) -> Result<(Located, Option<SyntaxToken>), AnalyzerError> {
    let (parsed, _) = engine.parsed(id)?;
    let stabilized = engine.stabilized(id)?;
    let indexed = engine.indexed(id)?;
    let lowered = engine.lowered(id)?;

    let Some(offset) = positions.utf8_position_to_offset(position) else {
        return Ok((Located::Nothing, None));
    };

    let node = parsed.syntax_node();
    let token = node.token_at_offset(offset);

    Ok(match token {
        TokenAtOffset::None => (Located::Nothing, None),
        TokenAtOffset::Single(token) => {
            let located = locate_single(&stabilized, &indexed, &lowered, &token);
            (located, Some(token))
        }
        TokenAtOffset::Between(left, right) => {
            locate_between(&stabilized, &indexed, &lowered, left, right)
        }
    })
}

fn locate_single(
    stabilized: &StabilizedModule,
    indexed: &IndexedModule,
    lowered: &LoweredModule,
    token: &SyntaxToken,
) -> Located {
    token
        .parent_ancestors()
        .find_map(|node| locate_node(stabilized, indexed, lowered, node))
        .unwrap_or(Located::Nothing)
}

fn locate_node(
    stabilized: &StabilizedModule,
    indexed: &IndexedModule,
    lowered: &LoweredModule,
    node: SyntaxNode,
) -> Option<Located> {
    let kind = node.kind();
    let ptr = SyntaxNodePtr::new(&node);
    if cst::Annotation::can_cast(kind) {
        Some(Located::Nothing)
    } else if cst::ModuleName::can_cast(kind) {
        let ptr = ptr.cast()?;
        Some(Located::ModuleName(ptr))
    } else if cst::QualifiedName::can_cast(kind) {
        locate_infix_reference(stabilized, indexed, lowered, node)
    } else if cst::ImportItem::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        Some(Located::ImportItem(id))
    } else if cst::Binder::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        Some(Located::Binder(id))
    } else if cst::RecordAccessLabel::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        Some(Located::RecordAccessLabel(id))
    } else if cst::Expression::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        Some(Located::Expression(id))
    } else if cst::TypeVariableBinding::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        Some(Located::TypeVariableBinding(id))
    } else if cst::Type::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        Some(Located::Type(id))
    } else if cst::RecordPun::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;

        let mut parents = iter::successors(Some(node), |node| node.parent());
        parents.find_map(|node| {
            let kind = node.kind();
            if cst::Binder::can_cast(kind) {
                Some(Located::BinderPun(id))
            } else if cst::Expression::can_cast(kind) {
                Some(Located::ExpressionPun(id))
            } else {
                None
            }
        })
    } else if cst::TermOperator::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        Some(Located::TermOperator(id))
    } else if cst::TypeOperator::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        Some(Located::TypeOperator(id))
    } else if cst::InstanceHead::can_cast(kind) {
        let resolution =
            if let Some(instance) = node.ancestors().find_map(cst::InstanceDeclaration::cast) {
                let ptr = AstPtr::new(&instance);
                let instance_id = stabilized.lookup_ptr(&ptr)?;
                let item_id = indexed.pairs.instance_to_item(instance_id)?;
                lowered.tree.get_instance_item(item_id)?.resolution
            } else {
                let derive = node.ancestors().find_map(cst::DeriveDeclaration::cast)?;
                let ptr = AstPtr::new(&derive);
                let derive_id = stabilized.lookup_ptr(&ptr)?;
                let item_id = indexed.pairs.derive_to_item(derive_id)?;
                lowered.tree.get_derive_item(item_id)?.resolution
            };
        let (file_id, type_id) = resolution?;
        Some(Located::InstanceHead(file_id, type_id))
    } else if cst::InstanceDeclaration::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        indexed.pairs.instance_to_item(id).map(Located::InstanceItem)
    } else if cst::Declaration::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        None.or_else(|| indexed.pairs.declaration_to_term(id).map(Located::TermItem))
            .or_else(|| indexed.pairs.declaration_to_type(id).map(Located::TypeItem))
            .or_else(|| indexed.pairs.declaration_to_instance(id).map(Located::InstanceItem))
            .or_else(|| indexed.pairs.declaration_to_derive(id).map(Located::DeriveItem))
    } else if cst::LetBinding::can_cast(kind) {
        let node = cst::LetBinding::cast(node)?;
        match node {
            cst::LetBinding::LetBindingPattern(_) => None,
            cst::LetBinding::LetBindingSignature(signature) => {
                let ptr = AstPtr::new(&signature);
                let id = stabilized.lookup_ptr(&ptr)?;
                lowered.tree.find_let_binding_group_by_signature(id).map(Located::LetBinding)
            }
            cst::LetBinding::LetBindingEquation(equation) => {
                let ptr = AstPtr::new(&equation);
                let id = stabilized.lookup_ptr(&ptr)?;
                lowered.tree.find_let_binding_group_by_equation(id).map(Located::LetBinding)
            }
        }
    } else if cst::DataConstructor::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        let id = indexed.pairs.constructor_to_term(id)?;
        Some(Located::TermItem(id))
    } else if cst::ClassMemberStatement::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        let id = indexed.pairs.class_member_to_term(id)?;
        Some(Located::TermItem(id))
    } else if cst::InstanceMemberStatement::can_cast(kind) {
        let ptr = ptr.cast()?;
        let id = stabilized.lookup_ptr(&ptr)?;
        let (file_id, term_id) = lowered.tree.find_instance_member_resolution(id)?;
        Some(Located::InstanceMember(file_id, term_id))
    } else {
        None
    }
}

fn locate_infix_reference(
    stabilized: &StabilizedModule,
    indexed: &IndexedModule,
    lowered: &LoweredModule,
    node: SyntaxNode,
) -> Option<Located> {
    let parent = node.parent()?;
    if !cst::InfixDeclaration::can_cast(parent.kind()) {
        return None;
    }
    let located = resolve_infix_reference(stabilized, indexed, lowered, parent);
    Some(located.unwrap_or(Located::Nothing))
}

fn resolve_infix_reference(
    stabilized: &StabilizedModule,
    indexed: &IndexedModule,
    lowered: &LoweredModule,
    declaration: SyntaxNode,
) -> Option<Located> {
    let declaration = cst::Declaration::cast(declaration)?;
    let ptr = AstPtr::new(&declaration);
    let declaration_id = stabilized.lookup_ptr(&ptr)?;

    if let Some(term_id) = indexed.pairs.declaration_to_term(declaration_id) {
        let TermItemKind::Operator { resolution, .. } = lowered.tree.get_term_item_kind(term_id)?
        else {
            return None;
        };
        let (file_id, term_id) = resolution.as_ref()?;
        Some(Located::TermReference(*file_id, *term_id))
    } else if let Some(type_id) = indexed.pairs.declaration_to_type(declaration_id) {
        let TypeItemKind::Operator { resolution, .. } = lowered.tree.get_type_item_kind(type_id)?
        else {
            return None;
        };
        let (file_id, type_id) = resolution.as_ref()?;
        Some(Located::TypeReference(*file_id, *type_id))
    } else {
        None
    }
}

fn locate_between(
    stabilized: &StabilizedModule,
    indexed: &IndexedModule,
    lowered: &LoweredModule,
    left: SyntaxToken,
    right: SyntaxToken,
) -> (Located, Option<SyntaxToken>) {
    let left_located = locate_single(stabilized, indexed, lowered, &left);
    let right_located = locate_single(stabilized, indexed, lowered, &right);
    match (&left_located, &right_located) {
        // If left/right share an ancestor;
        (_, _) if left_located == right_located => (right_located, Some(right)),
        (_, Located::Nothing) => (left_located, Some(left)),
        (Located::Nothing, _) => (right_located, Some(right)),
        // otherwise, lean towards the right.
        (_, _) => (right_located, Some(right)),
    }
}

#[cfg(test)]
mod tests;
