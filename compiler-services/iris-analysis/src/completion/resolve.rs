use std::mem;

use checking::core::pretty::{Pretty, PrettyConfig};
use files::FileId;
use indexing::{TermItemId, TypeItemId};
use lowering::{
    BinderId, GraphNodeId, ImplicitBindingId, LetBindingNameGroupId, RecordPunId,
    TypeVariableBindingId,
};
use lsp_types::*;
use serde::{Deserialize, Serialize};

use crate::extract::{AnnotationSyntaxRange, extract_annotation, extract_syntax};
use crate::{AnalyzerError, AnalyzerQueries};

const PRETTY_CONFIG: PrettyConfig = PrettyConfig::new().width(80);

#[allow(clippy::result_large_err)]
pub fn implementation(
    engine: &impl AnalyzerQueries,
    mut item: CompletionItem,
) -> Result<CompletionItem, (AnalyzerError, CompletionItem)> {
    let Some(value) = mem::take(&mut item.data) else {
        return Ok(item);
    };

    let Ok(resolve) = serde_json::from_value::<CompletionResolveData>(value) else {
        return Ok(item);
    };

    match resolve {
        CompletionResolveData::Import(file_id) => {
            let content = match engine.content(file_id) {
                Ok(content) => content,
                Err(error) => return Err((error.into(), item)),
            };
            match AnnotationSyntaxRange::of_file(engine, file_id) {
                Ok(range) => Ok(resolve_documentation(&content, range, item)),
                Err(error) => Err((error, item)),
            }
        }
        CompletionResolveData::TermItem(file_id, term_id) => {
            resolve_term_item(engine, file_id, term_id, item).map_err(|error| *error)
        }
        CompletionResolveData::TypeItem(file_id, type_id) => {
            resolve_type_item(engine, file_id, type_id, item).map_err(|error| *error)
        }
        CompletionResolveData::Binder(file_id, binder_id) => {
            Ok(resolve_binder(engine, file_id, binder_id, item))
        }
        CompletionResolveData::Let(file_id, let_id) => {
            Ok(resolve_let(engine, file_id, let_id, item))
        }
        CompletionResolveData::RecordPun(file_id, pun_id) => {
            Ok(resolve_record_pun(engine, file_id, pun_id, item))
        }
        CompletionResolveData::ForallTypeVariable(file_id, binding_id) => {
            Ok(resolve_forall_type_variable(engine, file_id, binding_id, item))
        }
        CompletionResolveData::ImplicitTypeVariable(file_id, node_id, binding_id) => {
            Ok(resolve_implicit_type_variable(engine, file_id, node_id, binding_id, item))
        }
    }
}

fn resolve_documentation(
    source: &str,
    range: AnnotationSyntaxRange,
    mut item: CompletionItem,
) -> CompletionItem {
    let annotation = range.annotation.map(|range| extract_annotation(source, range));
    let syntax = range.syntax.map(|range| extract_syntax(source, range));

    item.detail = syntax;
    item.documentation = annotation.map(|annotation| {
        Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: annotation,
        })
    });

    item
}

fn resolve_term_item(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    term_id: TermItemId,
    mut item: CompletionItem,
) -> Result<CompletionItem, Box<(AnalyzerError, CompletionItem)>> {
    let content = match engine.content(file_id) {
        Ok(content) => content,
        Err(error) => return Err(Box::new((error.into(), item))),
    };
    if let Ok(range) = AnnotationSyntaxRange::of_file_term(engine, file_id, term_id) {
        let annotation = range.annotation.map(|range| extract_annotation(&content, range));
        item.documentation = annotation.map(|annotation| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: annotation,
            })
        });
    }

    if let Some(signature) = render_term_signature(engine, file_id, term_id) {
        item.detail = Some(signature);
    }

    Ok(item)
}

fn render_term_signature(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    term_id: TermItemId,
) -> Option<String> {
    let indexed = engine.indexed(file_id).ok()?;
    let checked = engine.checked(file_id).ok()?;

    let name = &indexed.items[term_id].name;
    let name = name.as_deref()?;
    let signature = checked.lookup_term_item_type(term_id)?;

    let pretty = Pretty::with_config(engine, &checked, PRETTY_CONFIG);
    Some(pretty.render_signature(name, signature).to_string())
}

fn resolve_type_item(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    type_id: TypeItemId,
    mut item: CompletionItem,
) -> Result<CompletionItem, Box<(AnalyzerError, CompletionItem)>> {
    let content = match engine.content(file_id) {
        Ok(content) => content,
        Err(error) => return Err(Box::new((error.into(), item))),
    };
    if let Ok(range) = AnnotationSyntaxRange::of_file_type(engine, file_id, type_id) {
        let annotation = range.annotation.map(|range| extract_annotation(&content, range));
        item.documentation = annotation.map(|annotation| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: annotation,
            })
        });
    }

    if let Some(signature) = render_type_signature(engine, file_id, type_id) {
        item.detail = Some(signature);
    }

    Ok(item)
}

fn render_type_signature(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    type_id: TypeItemId,
) -> Option<String> {
    let indexed = engine.indexed(file_id).ok()?;
    let checked = engine.checked(file_id).ok()?;

    let name = &indexed.items[type_id].name;
    let name = name.as_deref()?;
    let signature = checked.lookup_type_item_kind(type_id)?;

    let pretty = Pretty::with_config(engine, &checked, PRETTY_CONFIG);
    Some(pretty.render_signature(name, signature).to_string())
}

fn resolve_binder(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    binder_id: BinderId,
    mut item: CompletionItem,
) -> CompletionItem {
    if let Some(signature) = render_local_signature(engine, file_id, &item.label, |checked| {
        checked.node_types.lookup_binder(binder_id)
    }) {
        item.detail = Some(signature);
    }

    item
}

fn resolve_let(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    let_id: LetBindingNameGroupId,
    mut item: CompletionItem,
) -> CompletionItem {
    if let Some(signature) = render_local_signature(engine, file_id, &item.label, |checked| {
        checked.node_types.lookup_let(let_id)
    }) {
        item.detail = Some(signature);
    }

    item
}

fn resolve_record_pun(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    pun_id: RecordPunId,
    mut item: CompletionItem,
) -> CompletionItem {
    if let Some(signature) = render_local_signature(engine, file_id, &item.label, |checked| {
        checked.node_types.lookup_pun(pun_id)
    }) {
        item.detail = Some(signature);
    }

    item
}

fn resolve_forall_type_variable(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    binding_id: TypeVariableBindingId,
    mut item: CompletionItem,
) -> CompletionItem {
    if let Some(signature) = render_local_signature(engine, file_id, &item.label, |checked| {
        checked.node_types.lookup_forall_binding(binding_id)
    }) {
        item.detail = Some(signature);
    }

    item
}

fn resolve_implicit_type_variable(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    node_id: GraphNodeId,
    binding_id: ImplicitBindingId,
    mut item: CompletionItem,
) -> CompletionItem {
    if let Some(signature) = render_local_signature(engine, file_id, &item.label, |checked| {
        checked.node_types.lookup_implicit_binding(node_id, binding_id)
    }) {
        item.detail = Some(signature);
    }

    item
}

fn render_local_signature(
    engine: &impl AnalyzerQueries,
    file_id: FileId,
    name: &str,
    lookup: impl FnOnce(&checking::CheckedModule) -> Option<checking::TypeId>,
) -> Option<String> {
    let checked = engine.checked(file_id).ok()?;
    let signature = lookup(&checked)?;

    let pretty = Pretty::with_config(engine, &checked, PRETTY_CONFIG);
    Some(pretty.render_signature(name, signature).to_string())
}

#[derive(Serialize, Deserialize)]
pub(crate) enum CompletionResolveData {
    Import(#[serde(with = "file_id")] FileId),
    TermItem(#[serde(with = "file_id")] FileId, #[serde(with = "id")] TermItemId),
    TypeItem(#[serde(with = "file_id")] FileId, #[serde(with = "id")] TypeItemId),
    Binder(#[serde(with = "file_id")] FileId, #[serde(with = "ast_id")] BinderId),
    Let(#[serde(with = "file_id")] FileId, #[serde(with = "id")] LetBindingNameGroupId),
    RecordPun(#[serde(with = "file_id")] FileId, #[serde(with = "ast_id")] RecordPunId),
    ForallTypeVariable(
        #[serde(with = "file_id")] FileId,
        #[serde(with = "ast_id")] TypeVariableBindingId,
    ),
    ImplicitTypeVariable(
        #[serde(with = "file_id")] FileId,
        #[serde(with = "id")] GraphNodeId,
        #[serde(with = "id")] ImplicitBindingId,
    ),
}

mod file_id {
    use files::FileId;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S>(file_id: &FileId, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        file_id.into_raw().serialize(serializer)
    }

    pub(super) fn deserialize<'d, D>(deserializer: D) -> Result<FileId, D::Error>
    where
        D: Deserializer<'d>,
    {
        let value = u32::deserialize(deserializer)?;
        Ok(FileId::new(value))
    }
}

mod id {
    use la_arena::{Idx, RawIdx};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<T, S>(index: &Idx<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        index.into_raw().into_u32().serialize(serializer)
    }

    pub(super) fn deserialize<'d, T, D>(deserializer: D) -> Result<Idx<T>, D::Error>
    where
        D: Deserializer<'d>,
    {
        let value = u32::deserialize(deserializer)?;
        Ok(Idx::from_raw(RawIdx::from_u32(value)))
    }
}

mod ast_id {
    use std::num::NonZeroU32;

    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use stabilizing::AstId;
    use syntax::ast::AstNode;

    pub(super) fn serialize<N, S>(index: &AstId<N>, serializer: S) -> Result<S::Ok, S::Error>
    where
        N: AstNode,
        S: Serializer,
    {
        index.into_raw().get().serialize(serializer)
    }

    pub(super) fn deserialize<'d, N, D>(deserializer: D) -> Result<AstId<N>, D::Error>
    where
        N: AstNode,
        D: Deserializer<'d>,
    {
        let value = u32::deserialize(deserializer)?;
        let value = NonZeroU32::new(value).ok_or_else(|| D::Error::custom("invalid AstId"))?;
        Ok(AstId::new(value))
    }
}
