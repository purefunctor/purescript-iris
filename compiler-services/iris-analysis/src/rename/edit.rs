use std::collections::HashMap;

use building_types::QueryProxy;
use files::FileId;
use indexing::{
    ImplicitItems, ImportId, ImportItemId, IndexedTermItemKind, IndexedTypeItemKind, TermItemId,
    TypeItemId, TypeSelection,
};
use lowering::{BinderId, LetBindingNameGroupId, RecordPunId, TypeVariableBindingId};
use lsp_types::*;
use stabilizing::AstId;
use syntax::ast::AstNode;
use syntax::{SyntaxNode, SyntaxNodePtr, SyntaxToken, TextRange, TokenAtOffset, WalkEvent, cst};

use super::{NameKind, RenameTarget};
use crate::position::Utf8Range;
use crate::{AnalyzerContext, AnalyzerError, AnalyzerHost, common, position};

macro_rules! push_name_edits {
    ($self:expr, $file_id:expr, $new_name:expr, $range:expr; $($id:expr),+ $(,)?) => {
        $($self.push_name_edit($file_id, $id, $range, $new_name)?;)+
    };
}

pub(super) struct RenameEdits<'edits, 'language, Host>
where
    Host: AnalyzerHost,
{
    context: &'edits AnalyzerContext<'language, Host>,
    edits: Vec<(FileId, TextEdit)>,
}

impl<'edits, 'language, Host> RenameEdits<'edits, 'language, Host>
where
    Host: AnalyzerHost,
{
    pub(super) fn new(
        context: &'edits AnalyzerContext<'language, Host>,
    ) -> RenameEdits<'edits, 'language, Host> {
        RenameEdits { context, edits: vec![] }
    }
}

impl<'edits, 'language, Host> RenameEdits<'edits, 'language, Host>
where
    Host: AnalyzerHost,
{
    pub(super) fn collect_qualifier(
        &mut self,
        file_id: FileId,
        import_id: ImportId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let indexed = self.context.queries().indexed(file_id)?;
        let old_name = indexed
            .imports
            .get(&import_id)
            .and_then(|import| import.alias.as_deref())
            .ok_or(AnalyzerError::NonFatal)?;

        let content = self.context.queries().content(file_id)?;
        let (parsed, _) = self.context.queries().parsed(file_id)?;
        let root = parsed.syntax_node();

        let statements = root.preorder().filter_map(|event| {
            let WalkEvent::Enter(node) = event else {
                return None;
            };

            cst::ImportStatement::cast(node)
        });

        for statement in statements {
            let Some(module_name) = statement.import_alias().and_then(|alias| alias.module_name())
            else {
                continue;
            };
            if module_name.syntax().text(&content) != old_name {
                continue;
            }

            self.push_text_range_edit(file_id, module_name.syntax().text_range(), new_name)?;
        }

        let qualified_names = root.preorder().filter_map(|event| {
            let WalkEvent::Enter(node) = event else {
                return None;
            };

            cst::QualifiedName::cast(node)
        });

        for qualified in qualified_names {
            let Some(qualifier) = qualified.qualifier() else {
                continue;
            };
            let Some(token) = qualifier.text() else {
                continue;
            };
            if token.text(&content).trim_end_matches('.') != old_name {
                continue;
            }

            let new_name = format!("{new_name}.");
            self.push_text_range_edit(file_id, token.text_range(), &new_name)?;
        }

        let exports = root.preorder().filter_map(|event| {
            let WalkEvent::Enter(node) = event else {
                return None;
            };

            cst::ExportModule::cast(node)
        });

        for export in exports {
            let Some(module_name) = export.module_name() else {
                continue;
            };
            if module_name.syntax().text(&content) != old_name {
                continue;
            }

            self.push_text_range_edit(file_id, module_name.syntax().text_range(), new_name)?;
        }

        Ok(())
    }

    pub(super) fn collect_module(
        &mut self,
        target_file: FileId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let (parsed, _) = self.context.queries().parsed(target_file)?;
        let module_name = parsed
            .cst()
            .header()
            .and_then(|header| header.name())
            .ok_or(AnalyzerError::NonFatal)?;

        self.push_text_range_edit(target_file, module_name.syntax().text_range(), new_name)?;

        for file_id in self.context.active_files() {
            if !self.context.is_editable(file_id) {
                continue;
            }

            self.module_import_edits(file_id, target_file, new_name)?;
            self.module_export_edits(file_id, target_file, new_name)?;
        }

        Ok(())
    }

    fn module_import_edits(
        &mut self,
        file_id: FileId,
        target_file: FileId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let (parsed, _) = self.context.queries().parsed(file_id)?;
        let root = parsed.syntax_node();
        let indexed = self.context.queries().indexed(file_id)?;
        let stabilized = self.context.queries().stabilized(file_id)?;

        for (import_id, import) in &indexed.imports {
            let Some(name) = import.name.as_deref() else {
                continue;
            };
            if self.context.queries().module_file(name) != Some(target_file) {
                continue;
            }

            let ptr = stabilized.ast_ptr(*import_id).ok_or(AnalyzerError::NonFatal)?;
            let statement = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;
            let module_name = statement.module_name().ok_or(AnalyzerError::NonFatal)?;

            self.push_text_range_edit(file_id, module_name.syntax().text_range(), new_name)?;
        }

        Ok(())
    }

    fn module_export_edits(
        &mut self,
        file_id: FileId,
        target_file: FileId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let content = self.context.queries().content(file_id)?;
        let (parsed, _) = self.context.queries().parsed(file_id)?;
        let root = parsed.syntax_node();
        let indexed = self.context.queries().indexed(file_id)?;
        let stabilized = self.context.queries().stabilized(file_id)?;
        let current_module = parsed.module_name(&content);

        for export in &indexed.exports.modules {
            let exports_self =
                file_id == target_file && current_module.as_ref() == Some(&export.name);
            let exports_import = indexed.imports.values().any(|import| {
                import.alias.is_none()
                    && import.name.as_ref() == Some(&export.name)
                    && self.context.queries().module_file(&export.name) == Some(target_file)
            });

            if !exports_self && !exports_import {
                continue;
            }

            let ptr = stabilized.ast_ptr(export.id).ok_or(AnalyzerError::NonFatal)?;
            let item = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;
            let cst::ExportItem::ExportModule(export) = item else {
                return Err(AnalyzerError::NonFatal);
            };
            let module_name = export.module_name().ok_or(AnalyzerError::NonFatal)?;

            self.push_text_range_edit(file_id, module_name.syntax().text_range(), new_name)?;
        }

        Ok(())
    }
}

fn binder_name_range(
    positions: &position::PositionConverter<'_>,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let node = ptr.try_to_node(root)?;

    if let Some(binder) = cst::BinderVariable::cast(node.clone()) {
        let token = binder.name_token()?;
        return positions.text_range_to_utf8_range(token.text_range());
    }

    let binder = cst::BinderNamed::cast(node)?;
    let token = binder.name_token()?;
    positions.text_range_to_utf8_range(token.text_range())
}

fn let_binding_signature_name_range(
    positions: &position::PositionConverter<'_>,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let node = ptr.try_to_node(root)?;
    let signature = cst::LetBindingSignature::cast(node)?;
    let token = signature.name_token()?;
    positions.text_range_to_utf8_range(token.text_range())
}

fn let_binding_equation_name_range(
    positions: &position::PositionConverter<'_>,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let node = ptr.try_to_node(root)?;
    let equation = cst::LetBindingEquation::cast(node)?;
    let token = equation.name_token()?;
    positions.text_range_to_utf8_range(token.text_range())
}

fn record_field_has_name(content: &str, field: &cst::RecordField, name: &str) -> bool {
    field.name().and_then(|name| name.text()).is_some_and(|token| token.text(content) == name)
}

fn record_field_replacement_range(field: &cst::RecordField) -> Option<TextRange> {
    let start = field.name()?.text()?.text_range().start();
    let end = field.syntax().text_range().end();
    Some(TextRange::new(start, end))
}

fn record_field_can_collapse(content: &str, field: &cst::RecordField, value: &SyntaxToken) -> bool {
    let Some(label) = field.name().and_then(|name| name.text()) else {
        return false;
    };

    let separator_range = TextRange::new(label.text_range().end(), value.text_range().start());
    let trailing_range =
        TextRange::new(value.text_range().end(), field.syntax().text_range().end());
    let separator = &content[separator_range];
    let trailing = &content[trailing_range];

    separator.strip_prefix(':').is_some_and(|trivia| trivia.chars().all(char::is_whitespace))
        && trailing.chars().all(char::is_whitespace)
}

impl<'edits, 'language, Host> RenameEdits<'edits, 'language, Host>
where
    Host: AnalyzerHost,
{
    pub(super) fn collect_references(
        &mut self,
        locations: Vec<Location>,
        name_kind: NameKind,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let mut ranges_by_file: HashMap<FileId, Vec<Range>> = HashMap::default();
        for location in locations {
            let file_id =
                self.context.file_id(location.uri.as_str()).ok_or(AnalyzerError::NonFatal)?;
            if !self.context.is_editable(file_id) {
                continue;
            }
            ranges_by_file.entry(file_id).or_default().push(location.range);
        }

        for (file_id, ranges) in ranges_by_file {
            let content = self.context.queries().content(file_id)?;
            let positions =
                position::PositionConverter::new(&content, self.context.position_encoding());
            for range in ranges {
                let (range, new_text) =
                    self.reference_name_edit(file_id, &positions, range, name_kind, new_name)?;
                let new_text = self.new_name_text(&positions, range, &new_text)?;
                self.edits.push((file_id, TextEdit { range, new_text }));
            }
        }

        Ok(())
    }

    fn reference_name_edit(
        &self,
        file_id: FileId,
        positions: &position::PositionConverter<'_>,
        range: Range,
        name_kind: NameKind,
        new_name: &str,
    ) -> Result<(Range, String), AnalyzerError> {
        let position =
            positions.protocol_position_to_utf8(range.start).ok_or(AnalyzerError::NonFatal)?;
        let offset = positions.utf8_position_to_offset(position).ok_or(AnalyzerError::NonFatal)?;
        let content = positions.content();

        let (parsed, _) = self.context.queries().parsed(file_id)?;
        let root = parsed.syntax_node();

        let token = match root.token_at_offset(offset) {
            TokenAtOffset::None => return Err(AnalyzerError::NonFatal),
            TokenAtOffset::Single(token) => token,
            TokenAtOffset::Between(left, right) => {
                if Self::qualified_name_token(&right, name_kind).is_some()
                    || Self::type_variable_token(&right, name_kind).is_some()
                    || Self::record_pun(&right, name_kind).is_some()
                {
                    right
                } else {
                    left
                }
            }
        };

        if matches!(name_kind, NameKind::Lower)
            && let Some(pun) = Self::record_pun(&token, name_kind)
        {
            let name = pun.name().ok_or(AnalyzerError::NonFatal)?;
            let old_name = name.syntax().text(content);
            let range = positions
                .text_range_to_protocol(name.syntax().text_range())
                .ok_or(AnalyzerError::NonFatal)?;

            return Ok((range, format!("{old_name}: {new_name}")));
        }

        if let Some(field) = Self::collapsible_record_field(&token, content, name_kind, new_name) {
            let text_range =
                record_field_replacement_range(&field).ok_or(AnalyzerError::NonFatal)?;
            let range =
                positions.text_range_to_protocol(text_range).ok_or(AnalyzerError::NonFatal)?;

            return Ok((range, new_name.to_string()));
        }

        let token = Self::qualified_name_token(&token, name_kind)
            .or_else(|| Self::type_variable_token(&token, name_kind))
            .ok_or(AnalyzerError::NonFatal)?;
        let range =
            positions.text_range_to_protocol(token.text_range()).ok_or(AnalyzerError::NonFatal)?;

        Ok((range, new_name.to_string()))
    }

    fn record_pun(token: &SyntaxToken, name_kind: NameKind) -> Option<cst::RecordPun> {
        if !matches!(name_kind, NameKind::Lower) {
            return None;
        }

        token.parent_ancestors().find_map(cst::RecordPun::cast)
    }

    fn collapsible_record_field(
        token: &SyntaxToken,
        content: &str,
        name_kind: NameKind,
        new_name: &str,
    ) -> Option<cst::RecordField> {
        if !matches!(name_kind, NameKind::Lower) {
            return None;
        }

        let qualified = token.parent_ancestors().find_map(cst::QualifiedName::cast)?;
        if qualified.qualifier().is_some() {
            return None;
        }

        let field = token.parent_ancestors().find_map(cst::RecordField::cast)?;
        field.expression()?;
        let value = qualified.lower()?;

        (record_field_has_name(content, &field, new_name)
            && record_field_can_collapse(content, &field, &value))
        .then_some(field)
    }

    fn qualified_name_token(token: &SyntaxToken, name_kind: NameKind) -> Option<SyntaxToken> {
        let qualified = token.parent_ancestors().find_map(cst::QualifiedName::cast)?;

        match name_kind {
            NameKind::Lower => qualified.lower(),
            NameKind::Upper => qualified.upper(),
            NameKind::Operator => qualified.operator().or_else(|| qualified.operator_name()),
            NameKind::Module => None,
        }
    }

    fn type_variable_token(token: &SyntaxToken, name_kind: NameKind) -> Option<SyntaxToken> {
        if !matches!(name_kind, NameKind::Lower) {
            return None;
        }

        let variable = token.parent_ancestors().find_map(cst::TypeVariable::cast)?;
        variable.name_token()
    }
}

impl<'edits, 'language, Host> RenameEdits<'edits, 'language, Host>
where
    Host: AnalyzerHost,
{
    pub(super) fn collect_declaration(
        &mut self,
        target: RenameTarget,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        match target {
            RenameTarget::Term(file_id, term_id) => {
                self.term_declaration_edits(file_id, term_id, new_name)
            }
            RenameTarget::Type(file_id, type_id) => {
                self.type_declaration_edits(file_id, type_id, new_name)
            }
            RenameTarget::Instance(file_id, item_id) => {
                let indexed = self.context.queries().indexed(file_id)?;
                push_name_edits!(self, file_id, new_name, position::instance_declaration_name_range; Some(indexed.items[item_id].id));
                Ok(())
            }
            RenameTarget::Derive(file_id, item_id) => {
                let indexed = self.context.queries().indexed(file_id)?;
                push_name_edits!(self, file_id, new_name, position::declaration_name_range; Some(indexed.items[item_id].id));
                Ok(())
            }
            RenameTarget::Binder(file_id, binder_id) => {
                self.binder_declaration_edit(file_id, binder_id, new_name)
            }
            RenameTarget::TypeVariable(file_id, binding_id) => {
                self.type_variable_declaration_edit(file_id, binding_id, new_name)
            }
            RenameTarget::LetBinding(file_id, binding_id) => {
                self.let_binding_declaration_edits(file_id, binding_id, new_name)
            }
            RenameTarget::RecordPun(file_id, pun_id) => {
                self.record_pun_declaration_edit(file_id, pun_id, new_name)
            }
            RenameTarget::Qualifier(_, _) => Ok(()),
            RenameTarget::Module(_) => Ok(()),
        }
    }

    fn binder_declaration_edit(
        &mut self,
        file_id: FileId,
        binder_id: BinderId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let content = self.context.queries().content(file_id)?;
        let (parsed, _) = self.context.queries().parsed(file_id)?;
        let root = parsed.syntax_node();
        let stabilized = self.context.queries().stabilized(file_id)?;

        let ptr = stabilized.syntax_ptr(binder_id).ok_or(AnalyzerError::NonFatal)?;
        let node = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;

        if let Some(variable) = cst::BinderVariable::cast(node.clone())
            && let Some(field) = node.ancestors().find_map(cst::RecordField::cast)
            && let Some(binder) = field.binder()
            && let Some(value) = variable.name_token()
            && binder.syntax().text_range() == node.text_range()
            && record_field_has_name(&content, &field, new_name)
            && record_field_can_collapse(&content, &field, &value)
        {
            let range = record_field_replacement_range(&field).ok_or(AnalyzerError::NonFatal)?;
            return self.push_text_range_edit(file_id, range, new_name);
        }

        self.push_name_edit(file_id, Some(binder_id), binder_name_range, new_name)
    }

    fn type_variable_declaration_edit(
        &mut self,
        file_id: FileId,
        binding_id: TypeVariableBindingId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        self.push_name_edit(
            file_id,
            Some(binding_id),
            position::type_variable_binding_name_range,
            new_name,
        )
    }

    fn let_binding_declaration_edits(
        &mut self,
        file_id: FileId,
        binding_id: LetBindingNameGroupId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let lowered = self.context.queries().lowered(file_id)?;
        let binding = lowered.tree.get_let_binding_group(binding_id);

        push_name_edits!(
            self,
            file_id,
            new_name,
            let_binding_signature_name_range;
            binding.signature,
        );

        for &equation in binding.equations.iter() {
            push_name_edits!(
                self,
                file_id,
                new_name,
                let_binding_equation_name_range;
                Some(equation),
            );
        }

        Ok(())
    }

    fn record_pun_declaration_edit(
        &mut self,
        file_id: FileId,
        pun_id: RecordPunId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let content = self.context.queries().content(file_id)?;
        let (parsed, _) = self.context.queries().parsed(file_id)?;
        let root = parsed.syntax_node();
        let stabilized = self.context.queries().stabilized(file_id)?;

        let ptr = stabilized.syntax_ptr(pun_id).ok_or(AnalyzerError::NonFatal)?;
        let node = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;
        let pun = cst::RecordPun::cast(node).ok_or(AnalyzerError::NonFatal)?;
        let name = pun.name().ok_or(AnalyzerError::NonFatal)?;
        let old_name = name.syntax().text(&content);
        let new_text = format!("{old_name}: {new_name}");

        self.push_text_range_edit(file_id, name.syntax().text_range(), &new_text)
    }

    fn term_declaration_edits(
        &mut self,
        file_id: FileId,
        term_id: TermItemId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let indexed = self.context.queries().indexed(file_id)?;

        match &indexed.items[term_id].kind {
            IndexedTermItemKind::ClassMember { id, .. } => {
                push_name_edits!(self, file_id, new_name, position::class_member_name_range; Some(*id));
            }
            IndexedTermItemKind::Constructor { id, .. } => {
                push_name_edits!(self, file_id, new_name, position::data_constructor_name_range; Some(*id));
            }
            IndexedTermItemKind::Foreign { id } => {
                push_name_edits!(self, file_id, new_name, position::declaration_name_range; Some(*id));
            }
            IndexedTermItemKind::Operator { id } => {
                push_name_edits!(self, file_id, new_name, position::infix_operator_range; Some(*id));
            }
            IndexedTermItemKind::Value { signature, equations } => {
                push_name_edits!(self, file_id, new_name, position::declaration_name_range; *signature);

                for &equation in equations {
                    push_name_edits!(self, file_id, new_name, position::declaration_name_range; Some(equation));
                }
            }
        }

        Ok(())
    }

    fn type_declaration_edits(
        &mut self,
        file_id: FileId,
        type_id: TypeItemId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let indexed = self.context.queries().indexed(file_id)?;

        match indexed.items[type_id].kind {
            IndexedTypeItemKind::Data { signature, equation, role, .. } => {
                push_name_edits!(self, file_id, new_name, position::declaration_name_range; signature, equation, role);
            }
            IndexedTypeItemKind::Newtype { signature, equation, role, .. } => {
                push_name_edits!(self, file_id, new_name, position::declaration_name_range; signature, equation, role);
            }
            IndexedTypeItemKind::Synonym { signature, equation } => {
                push_name_edits!(self, file_id, new_name, position::declaration_name_range; signature, equation);
            }
            IndexedTypeItemKind::Class { signature, declaration, .. } => {
                push_name_edits!(self, file_id, new_name, position::declaration_name_range; signature, declaration);
            }
            IndexedTypeItemKind::Foreign { id, role } => {
                push_name_edits!(self, file_id, new_name, position::declaration_name_range; Some(id), role);
            }
            IndexedTypeItemKind::Operator { id } => {
                push_name_edits!(self, file_id, new_name, position::infix_operator_range; Some(id));
            }
        }

        Ok(())
    }
}

impl<'edits, 'language, Host> RenameEdits<'edits, 'language, Host>
where
    Host: AnalyzerHost,
{
    pub(super) fn collect_item_surfaces(
        &mut self,
        target: RenameTarget,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        for file_id in self.context.active_files() {
            if !self.context.is_editable(file_id) {
                continue;
            }

            self.import_edits(file_id, target, old_name, new_name)?;
            self.export_edits(file_id, target, old_name, new_name)?;
        }

        Ok(())
    }

    fn import_edits(
        &mut self,
        file_id: FileId,
        target: RenameTarget,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let indexed = self.context.queries().indexed(file_id)?;
        let resolved = self.context.queries().resolved(file_id)?;

        let unqualified = resolved.unqualified.values().flatten();
        let qualified = resolved.qualified.values().flatten();

        for import in unqualified.chain(qualified) {
            let Some(indexed_import) = indexed.imports.get(&import.id) else {
                continue;
            };

            match target {
                RenameTarget::Term(target_file, target_term) => {
                    let imported_terms = import.iter_terms().filter(|(_, file_id, term_id, _)| {
                        (*file_id, *term_id) == (target_file, target_term)
                    });

                    for (name, _, _, _) in imported_terms {
                        if let Some(import_item_id) = indexed_import.terms.get(name) {
                            self.push_import_item_edit(file_id, *import_item_id, new_name)?;
                        }
                    }

                    self.constructor_import_edits(
                        file_id,
                        import,
                        indexed_import,
                        (target_file, target_term),
                        (old_name, new_name),
                    )?;
                }
                RenameTarget::Type(target_file, target_type) => {
                    let imported_types = import.iter_types().chain(import.iter_classes()).filter(
                        |(_, file_id, type_id, _)| {
                            (*file_id, *type_id) == (target_file, target_type)
                        },
                    );

                    for (name, _, _, _) in imported_types {
                        if let Some((import_item_id, _)) = indexed_import.types.get(name) {
                            self.push_import_item_edit(file_id, *import_item_id, new_name)?;
                        }
                    }
                }
                RenameTarget::Binder(_, _)
                | RenameTarget::TypeVariable(_, _)
                | RenameTarget::Instance(_, _)
                | RenameTarget::Derive(_, _)
                | RenameTarget::LetBinding(_, _)
                | RenameTarget::RecordPun(_, _)
                | RenameTarget::Qualifier(_, _)
                | RenameTarget::Module(_) => {}
            }
        }

        Ok(())
    }

    fn constructor_import_edits(
        &mut self,
        file_id: FileId,
        import: &resolving::ResolvedImport,
        indexed_import: &indexing::IndexedImport,
        target: (FileId, TermItemId),
        names: (&str, &str),
    ) -> Result<(), AnalyzerError> {
        let (target_file, target_term) = target;
        let (old_name, new_name) = names;
        let target_indexed = self.context.queries().indexed(target_file)?;
        let Some(parent_type) = target_indexed.constructor_type(target_term) else {
            return Ok(());
        };

        let imported_types = import
            .iter_types()
            .filter(|(_, file_id, type_id, _)| (*file_id, *type_id) == (target_file, parent_type));

        for (name, _, _, _) in imported_types {
            let Some((import_item_id, Some(selection))) = indexed_import.types.get(name) else {
                continue;
            };
            let ImplicitItems::Enumerated(constructors) = selection else {
                continue;
            };
            if !constructors.iter().any(|constructor| constructor == old_name) {
                continue;
            }

            self.push_import_constructor_edit(file_id, *import_item_id, old_name, new_name)?;
        }

        Ok(())
    }

    fn export_edits(
        &mut self,
        file_id: FileId,
        target: RenameTarget,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let indexed = self.context.queries().indexed(file_id)?;
        let resolved = self.context.queries().resolved(file_id)?;

        match target {
            RenameTarget::Term(target_file, target_term) => {
                for export in &indexed.exports.terms {
                    if resolved.exports.lookup_term(&export.name)
                        == Some((target_file, target_term))
                    {
                        self.push_export_item_edit(file_id, export.id, new_name)?;
                    }
                }

                let target_indexed = self.context.queries().indexed(target_file)?;
                let Some(parent_type) = target_indexed.constructor_type(target_term) else {
                    return Ok(());
                };

                for export in &indexed.exports.types {
                    if resolved.exports.lookup_type(&export.name)
                        != Some((target_file, parent_type))
                    {
                        continue;
                    }
                    let Some(TypeSelection::Enumerated(constructors)) = &export.selection else {
                        continue;
                    };
                    if !constructors.iter().any(|constructor| constructor == old_name) {
                        continue;
                    }

                    self.push_export_constructor_edit(file_id, export.id, old_name, new_name)?;
                }
            }
            RenameTarget::Type(target_file, target_type) => {
                for export in &indexed.exports.types {
                    let exported_type = resolved
                        .exports
                        .lookup_type(&export.name)
                        .or_else(|| resolved.exports.lookup_class(&export.name));

                    if exported_type == Some((target_file, target_type)) {
                        self.push_export_item_edit(file_id, export.id, new_name)?;
                    }
                }
            }
            RenameTarget::Binder(_, _)
            | RenameTarget::TypeVariable(_, _)
            | RenameTarget::Instance(_, _)
            | RenameTarget::Derive(_, _)
            | RenameTarget::LetBinding(_, _)
            | RenameTarget::RecordPun(_, _)
            | RenameTarget::Qualifier(_, _)
            | RenameTarget::Module(_) => {}
        }

        Ok(())
    }

    fn push_import_item_edit(
        &mut self,
        file_id: FileId,
        import_item_id: ImportItemId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let content = self.context.queries().content(file_id)?;
        let positions =
            position::PositionConverter::new(&content, self.context.position_encoding());
        let (parsed, _) = self.context.queries().parsed(file_id)?;
        let root = parsed.syntax_node();
        let stabilized = self.context.queries().stabilized(file_id)?;

        let ptr = stabilized.ast_ptr(import_item_id).ok_or(AnalyzerError::NonFatal)?;
        let item = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;
        let range =
            position::import_item_name_range(&positions, item).ok_or(AnalyzerError::NonFatal)?;

        self.push_utf8_edit(file_id, range, new_name)
    }

    fn push_export_item_edit(
        &mut self,
        file_id: FileId,
        export_item_id: indexing::ExportItemId,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let content = self.context.queries().content(file_id)?;
        let positions =
            position::PositionConverter::new(&content, self.context.position_encoding());
        let (parsed, _) = self.context.queries().parsed(file_id)?;
        let root = parsed.syntax_node();
        let stabilized = self.context.queries().stabilized(file_id)?;

        let ptr = stabilized.ast_ptr(export_item_id).ok_or(AnalyzerError::NonFatal)?;
        let item = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;
        let range =
            position::export_item_name_range(&positions, item).ok_or(AnalyzerError::NonFatal)?;

        self.push_utf8_edit(file_id, range, new_name)
    }

    fn push_import_constructor_edit(
        &mut self,
        file_id: FileId,
        import_item_id: ImportItemId,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let (parsed, _) = self.context.queries().parsed(file_id)?;
        let root = parsed.syntax_node();
        let stabilized = self.context.queries().stabilized(file_id)?;

        let ptr = stabilized.ast_ptr(import_item_id).ok_or(AnalyzerError::NonFatal)?;
        let item = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;
        let cst::ImportItem::ImportType(item) = item else {
            return Err(AnalyzerError::NonFatal);
        };

        let type_items = item.type_items().ok_or(AnalyzerError::NonFatal)?;
        self.push_constructor_token_edit(file_id, type_items, old_name, new_name)
    }

    fn push_export_constructor_edit(
        &mut self,
        file_id: FileId,
        export_item_id: indexing::ExportItemId,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let (parsed, _) = self.context.queries().parsed(file_id)?;
        let root = parsed.syntax_node();
        let stabilized = self.context.queries().stabilized(file_id)?;

        let ptr = stabilized.ast_ptr(export_item_id).ok_or(AnalyzerError::NonFatal)?;
        let item = ptr.try_to_node(&root).ok_or(AnalyzerError::NonFatal)?;
        let cst::ExportItem::ExportType(item) = item else {
            return Err(AnalyzerError::NonFatal);
        };

        let type_items = item.type_items().ok_or(AnalyzerError::NonFatal)?;
        self.push_constructor_token_edit(file_id, type_items, old_name, new_name)
    }

    fn push_constructor_token_edit(
        &mut self,
        file_id: FileId,
        type_items: cst::TypeItems,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let content = self.context.queries().content(file_id)?;
        let positions =
            position::PositionConverter::new(&content, self.context.position_encoding());
        let cst::TypeItems::TypeItemsList(items) = type_items else {
            return Ok(());
        };

        for token in items.name_tokens() {
            if token.text(&content) == old_name {
                let range = positions
                    .text_range_to_utf8_range(token.text_range())
                    .ok_or(AnalyzerError::NonFatal)?;

                self.push_utf8_edit(file_id, range, new_name)?;
            }
        }

        Ok(())
    }
}

impl<'edits, 'language, Host> RenameEdits<'edits, 'language, Host>
where
    Host: AnalyzerHost,
{
    fn push_text_range_edit(
        &mut self,
        file_id: FileId,
        range: TextRange,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let content = self.context.queries().content(file_id)?;
        let positions =
            position::PositionConverter::new(&content, self.context.position_encoding());
        let range = positions.text_range_to_utf8_range(range).ok_or(AnalyzerError::NonFatal)?;

        self.push_utf8_edit(file_id, range, new_name)
    }

    fn push_utf8_edit(
        &mut self,
        file_id: FileId,
        range: Utf8Range,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let content = self.context.queries().content(file_id)?;
        let positions =
            position::PositionConverter::new(&content, self.context.position_encoding());
        let range = positions.utf8_range_to_protocol(range).ok_or(AnalyzerError::NonFatal)?;
        self.push_protocol_edit(file_id, range, new_name)
    }

    fn push_name_edit<T>(
        &mut self,
        file_id: FileId,
        id: Option<AstId<T>>,
        range: fn(
            &position::PositionConverter<'_>,
            &SyntaxNode,
            &SyntaxNodePtr,
        ) -> Option<Utf8Range>,
        new_name: &str,
    ) -> Result<(), AnalyzerError>
    where
        T: AstNode,
    {
        let Some(id) = id else {
            return Ok(());
        };

        let content = self.context.queries().content(file_id)?;
        let positions =
            position::PositionConverter::new(&content, self.context.position_encoding());
        let (parsed, _) = self.context.queries().parsed(file_id)?;
        let root = parsed.syntax_node();
        let stabilized = self.context.queries().stabilized(file_id)?;

        let ptr = stabilized.syntax_ptr(id).ok_or(AnalyzerError::NonFatal)?;
        let range = range(&positions, &root, &ptr).ok_or(AnalyzerError::NonFatal)?;
        let range = positions.utf8_range_to_protocol(range).ok_or(AnalyzerError::NonFatal)?;
        self.push_protocol_edit(file_id, range, new_name)
    }

    fn push_protocol_edit(
        &mut self,
        file_id: FileId,
        range: Range,
        new_name: &str,
    ) -> Result<(), AnalyzerError> {
        let content = self.context.queries().content(file_id)?;
        let positions =
            position::PositionConverter::new(&content, self.context.position_encoding());
        let new_name = self.new_name_text(&positions, range, new_name)?;
        self.edits.push((file_id, TextEdit { range, new_text: new_name }));
        Ok(())
    }

    fn new_name_text(
        &self,
        positions: &position::PositionConverter<'_>,
        range: Range,
        new_name: &str,
    ) -> Result<String, AnalyzerError> {
        let start = positions
            .protocol_position_to_utf8(range.start)
            .and_then(|position| positions.utf8_position_to_offset(position))
            .ok_or(AnalyzerError::NonFatal)?;
        let end = positions
            .protocol_position_to_utf8(range.end)
            .and_then(|position| positions.utf8_position_to_offset(position))
            .ok_or(AnalyzerError::NonFatal)?;

        let range = TextRange::new(start, end);
        let text = &positions.content()[range];

        if text.starts_with('(') && text.ends_with(')') {
            Ok(format!("({new_name})"))
        } else {
            Ok(new_name.to_string())
        }
    }

    pub(super) fn finish(
        mut self,
        conflicts: bool,
    ) -> Result<Option<WorkspaceEdit>, AnalyzerError> {
        self.edits.sort_by_key(|(file_id, edit)| {
            (
                *file_id,
                edit.range.start.line,
                edit.range.start.character,
                edit.range.end.line,
                edit.range.end.character,
            )
        });
        self.edits.dedup_by(|left, right| left.0 == right.0 && left.1.range == right.1.range);

        if self.edits.is_empty() {
            return Ok(None);
        }

        if conflicts {
            return self.finish_annotated();
        }

        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::default();
        for (file_id, edit) in self.edits {
            let uri = common::file_uri(self.context, file_id)?;
            changes.entry(uri).or_default().push(edit);
        }

        Ok(Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }))
    }

    fn finish_annotated(self) -> Result<Option<WorkspaceEdit>, AnalyzerError> {
        let annotation_id = "rename-conflict".to_string();
        let mut documents: HashMap<Url, Vec<OneOf<TextEdit, AnnotatedTextEdit>>> =
            HashMap::default();
        for (file_id, edit) in self.edits {
            let uri = common::file_uri(self.context, file_id)?;
            let edit = AnnotatedTextEdit { text_edit: edit, annotation_id: annotation_id.clone() };
            documents.entry(uri).or_default().push(OneOf::Right(edit));
        }

        let documents = documents.into_iter().map(|(uri, edits)| TextDocumentEdit {
            text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
            edits,
        });
        let documents = documents.collect();

        let annotation = ChangeAnnotation {
            label: "Rename may change name resolution".to_string(),
            needs_confirmation: Some(true),
            description: Some("The rename may change name resolution".to_string()),
        };
        let change_annotations = HashMap::from([(annotation_id, annotation)]);

        Ok(Some(WorkspaceEdit {
            document_changes: Some(DocumentChanges::Edits(documents)),
            change_annotations: Some(change_annotations),
            ..WorkspaceEdit::default()
        }))
    }
}
