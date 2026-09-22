use line_index::{LineCol, LineIndex, WideEncoding, WideLineCol};
use lsp_types;
use syntax::ast::AstNode;
use syntax::{SyntaxNode, SyntaxNodePtr, TextRange, TextSize, cst};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionEncoding {
    Utf8,
    Utf16,
    Utf32,
}

impl PositionEncoding {
    fn wide(self) -> Option<WideEncoding> {
        WideEncoding::try_from(self).ok()
    }
}

impl TryFrom<PositionEncoding> for WideEncoding {
    type Error = ();

    fn try_from(encoding: PositionEncoding) -> Result<WideEncoding, ()> {
        match encoding {
            PositionEncoding::Utf8 => Err(()),
            PositionEncoding::Utf16 => Ok(WideEncoding::Utf16),
            PositionEncoding::Utf32 => Ok(WideEncoding::Utf32),
        }
    }
}

impl From<PositionEncoding> for lsp_types::PositionEncodingKind {
    fn from(encoding: PositionEncoding) -> lsp_types::PositionEncodingKind {
        match encoding {
            PositionEncoding::Utf8 => lsp_types::PositionEncodingKind::UTF8,
            PositionEncoding::Utf16 => lsp_types::PositionEncodingKind::UTF16,
            PositionEncoding::Utf32 => lsp_types::PositionEncodingKind::UTF32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utf8Position {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utf8Range {
    pub start: Utf8Position,
    pub end: Utf8Position,
}

pub struct PositionConverter<'content> {
    content: &'content str,
    line_index: LineIndex,
    encoding: PositionEncoding,
}

impl<'content> PositionConverter<'content> {
    pub fn new(content: &'content str, encoding: PositionEncoding) -> PositionConverter<'content> {
        let line_index = LineIndex::new(content);
        PositionConverter { content, line_index, encoding }
    }

    pub fn content(&self) -> &'content str {
        self.content
    }

    pub fn protocol_position_to_utf8(&self, position: lsp_types::Position) -> Option<Utf8Position> {
        let line_col = match self.encoding.wide() {
            None => LineCol { line: position.line, col: position.character },
            Some(encoding) => self
                .line_index
                .to_utf8(encoding, WideLineCol { line: position.line, col: position.character })?,
        };

        let position = Utf8Position { line: line_col.line, column: line_col.col };
        let offset = self.utf8_position_to_offset(position)?;
        self.offset_to_utf8_position(offset)
    }

    pub fn utf8_position_to_protocol(&self, position: Utf8Position) -> Option<lsp_types::Position> {
        let line_col = LineCol { line: position.line, col: position.column };

        let offset = self.line_index.offset(line_col)?;
        self.line_index.try_line_col(offset)?;

        let position = match self.encoding.wide() {
            None => lsp_types::Position { line: line_col.line, character: line_col.col },
            Some(encoding) => {
                let line_col = self.line_index.to_wide(encoding, line_col)?;
                lsp_types::Position { line: line_col.line, character: line_col.col }
            }
        };

        Some(position)
    }

    pub fn utf8_range_to_protocol(&self, range: Utf8Range) -> Option<lsp_types::Range> {
        let start = self.utf8_position_to_protocol(range.start)?;
        let end = self.utf8_position_to_protocol(range.end)?;
        Some(lsp_types::Range { start, end })
    }

    pub fn utf8_position_to_offset(&self, position: Utf8Position) -> Option<TextSize> {
        let line_range = self.line_index.line(position.line)?;
        let line_content = self.content[line_range].trim_end_matches(['\n', '\r']);

        let column = if line_content.is_empty() {
            0
        } else if position.column > line_content.len() as u32 {
            line_content.len() as u32
        } else {
            line_content.get(position.column as usize..)?;
            position.column
        };

        let line_col = LineCol { line: position.line, col: column };
        let offset = self.line_index.offset(line_col)?;
        self.line_index.try_line_col(offset)?;
        Some(offset)
    }

    pub fn offset_to_utf8_position(&self, offset: TextSize) -> Option<Utf8Position> {
        let LineCol { line, col } = self.line_index.try_line_col(offset)?;
        Some(Utf8Position { line, column: col })
    }

    pub fn text_range_to_utf8_range(&self, range: TextRange) -> Option<Utf8Range> {
        let start = self.offset_to_utf8_position(range.start())?;
        let end = self.offset_to_utf8_position(range.end())?;
        Some(Utf8Range { start, end })
    }

    pub fn text_range_to_protocol(&self, range: TextRange) -> Option<lsp_types::Range> {
        let convert = |offset| {
            let line_col = self.line_index.try_line_col(offset)?;
            let line_col = match self.encoding.wide() {
                None => line_col,
                Some(encoding) => {
                    let wide = self.line_index.to_wide(encoding, line_col)?;
                    LineCol { line: wide.line, col: wide.col }
                }
            };
            Some(lsp_types::Position { line: line_col.line, character: line_col.col })
        };

        let start = convert(range.start())?;
        let end = convert(range.end())?;
        Some(lsp_types::Range { start, end })
    }
}

pub fn import_item_name_range(
    positions: &PositionConverter<'_>,
    import_item: cst::ImportItem,
) -> Option<Utf8Range> {
    let range = match import_item {
        cst::ImportItem::ImportValue(cst) => cst.name_token()?.text_range(),
        cst::ImportItem::ImportClass(cst) => cst.name_token()?.text_range(),
        cst::ImportItem::ImportType(cst) => cst.name_token()?.text_range(),
        cst::ImportItem::ImportOperator(cst) => cst.name_token()?.text_range(),
        cst::ImportItem::ImportTypeOperator(cst) => cst.name_token()?.text_range(),
    };

    positions.text_range_to_utf8_range(range)
}

pub fn export_item_name_range(
    positions: &PositionConverter<'_>,
    export_item: cst::ExportItem,
) -> Option<Utf8Range> {
    let token = match export_item {
        cst::ExportItem::ExportValue(cst) => cst.name_token()?,
        cst::ExportItem::ExportClass(cst) => cst.name_token()?,
        cst::ExportItem::ExportType(cst) => cst.name_token()?,
        cst::ExportItem::ExportOperator(cst) => cst.name_token()?,
        cst::ExportItem::ExportTypeOperator(cst) => cst.name_token()?,
        cst::ExportItem::ExportModule(_) => return None,
    };

    positions.text_range_to_utf8_range(token.text_range())
}

pub fn declaration_name_range(
    positions: &PositionConverter<'_>,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let node = ptr.try_to_node(root)?;
    let declaration = cst::Declaration::cast(node.clone())?;

    macro_rules! declaration_name_range {
        ($declaration:expr, $($variant:ident),+ $(,)?) => {
            match $declaration {
                $(cst::Declaration::$variant(declaration) => declaration.name_token()?.text_range(),)+
                _ => return None,
            }
        };
    }

    let range = match declaration {
        cst::Declaration::ClassDeclaration(declaration) => {
            declaration.class_head()?.name_token()?.text_range()
        }
        cst::Declaration::DeriveDeclaration(declaration) => {
            declaration.instance_name()?.name_token()?.text_range()
        }
        cst::Declaration::InstanceChain(_) => return None,
        declaration => declaration_name_range!(
            declaration,
            ValueSignature,
            ValueEquation,
            DataSignature,
            DataEquation,
            NewtypeSignature,
            NewtypeEquation,
            TypeSynonymSignature,
            TypeSynonymEquation,
            ClassSignature,
            TypeRoleDeclaration,
            ForeignImportDataDeclaration,
            ForeignImportValueDeclaration,
        ),
    };

    positions.text_range_to_utf8_range(range)
}

pub fn data_constructor_name_range(
    positions: &PositionConverter<'_>,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let node = ptr.try_to_node(root)?;
    let constructor = cst::DataConstructor::cast(node)?;
    let token = constructor.name_token()?;
    positions.text_range_to_utf8_range(token.text_range())
}

pub fn class_member_name_range(
    positions: &PositionConverter<'_>,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let node = ptr.try_to_node(root)?;
    let member = cst::ClassMemberStatement::cast(node)?;
    let token = member.name_token()?;
    positions.text_range_to_utf8_range(token.text_range())
}

pub fn instance_declaration_name_range(
    positions: &PositionConverter<'_>,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let node = ptr.try_to_node(root)?;
    let instance = cst::InstanceDeclaration::cast(node)?;
    let token = instance.instance_name()?.name_token()?;
    positions.text_range_to_utf8_range(token.text_range())
}

pub fn record_pun_name_range(
    positions: &PositionConverter<'_>,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let node = ptr.try_to_node(root)?;
    let pun = cst::RecordPun::cast(node)?;
    let token = pun.name()?.text()?;
    positions.text_range_to_utf8_range(token.text_range())
}

pub fn type_variable_binding_name_range(
    positions: &PositionConverter<'_>,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let node = ptr.try_to_node(root)?;
    let binding = cst::TypeVariableBinding::cast(node)?;
    let token = binding.name()?;
    positions.text_range_to_utf8_range(token.text_range())
}

pub fn qualified_name_text_range(qualified: &cst::QualifiedName) -> Option<TextRange> {
    let qualifier_range = qualified
        .qualifier()
        .and_then(|qualifier| qualifier.text())
        .map(|token| token.text_range());
    let name_range = qualified
        .lower()
        .or_else(|| qualified.upper())
        .or_else(|| qualified.operator())
        .or_else(|| qualified.operator_name())
        .map(|token| token.text_range());

    match (qualifier_range, name_range) {
        (Some(qualifier), Some(name)) => Some(qualifier.cover(name)),
        (Some(range), None) | (None, Some(range)) => Some(range),
        (None, None) => None,
    }
}

pub fn infix_operator_range(
    positions: &PositionConverter<'_>,
    root: &SyntaxNode,
    ptr: &SyntaxNodePtr,
) -> Option<Utf8Range> {
    let node = ptr.try_to_node(root)?;
    let declaration = cst::InfixDeclaration::cast(node)?;
    let token = declaration.operator_token()?;
    positions.text_range_to_utf8_range(token.text_range())
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, PositionEncodingKind};
    use syntax::{TextRange, TextSize};

    use super::{PositionConverter, PositionEncoding, Utf8Position, Utf8Range};

    #[test]
    fn utf16_protocol_position_maps_to_utf8_column() {
        let content = "a😀b";
        let positions = PositionConverter::new(content, PositionEncoding::Utf16);
        let position = Position::new(0, 3);

        let position = positions.protocol_position_to_utf8(position).unwrap();
        let offset = positions.utf8_position_to_offset(position);
        insta::assert_debug_snapshot!((position, offset), @"
        (
            Utf8Position {
                line: 0,
                column: 5,
            },
            Some(
                5,
            ),
        )
        ");
    }

    #[test]
    fn utf32_protocol_position_maps_to_utf8_column() {
        let content = "a😀b";
        let positions = PositionConverter::new(content, PositionEncoding::Utf32);
        let position = Position::new(0, 2);

        let position = positions.protocol_position_to_utf8(position).unwrap();
        insta::assert_debug_snapshot!(position, @"
        Utf8Position {
            line: 0,
            column: 5,
        }
        ");
    }

    #[test]
    fn utf8_position_maps_to_utf16_protocol_position() {
        let content = "a😀b";
        let positions = PositionConverter::new(content, PositionEncoding::Utf16);
        let position = positions.offset_to_utf8_position(TextSize::new(5)).unwrap();

        let position = positions.utf8_position_to_protocol(position).unwrap();
        insta::assert_debug_snapshot!(position, @"
        Position {
            line: 0,
            character: 3,
        }
        ");
    }

    #[test]
    fn utf8_protocol_positions_use_utf8_columns() {
        let content = "a😀b";
        let positions = PositionConverter::new(content, PositionEncoding::Utf8);
        let position = Position::new(0, 5);

        let position = positions.protocol_position_to_utf8(position).unwrap();
        let protocol_position = positions.utf8_position_to_protocol(position).unwrap();
        insta::assert_debug_snapshot!((position, protocol_position), @"
        (
            Utf8Position {
                line: 0,
                column: 5,
            },
            Position {
                line: 0,
                character: 5,
            },
        )
        ");
    }

    #[test]
    fn text_ranges_use_negotiated_position_encoding() {
        let content = "a😀b";
        let range = TextRange::new(TextSize::new(1), TextSize::new(5));

        let utf8 = PositionConverter::new(content, PositionEncoding::Utf8)
            .text_range_to_protocol(range)
            .unwrap();
        let utf16 = PositionConverter::new(content, PositionEncoding::Utf16)
            .text_range_to_protocol(range)
            .unwrap();
        let utf32 = PositionConverter::new(content, PositionEncoding::Utf32)
            .text_range_to_protocol(range)
            .unwrap();
        insta::assert_debug_snapshot!((utf8, utf16, utf32), @"
        (
            Range {
                start: Position {
                    line: 0,
                    character: 1,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            },
            Range {
                start: Position {
                    line: 0,
                    character: 1,
                },
                end: Position {
                    line: 0,
                    character: 3,
                },
            },
            Range {
                start: Position {
                    line: 0,
                    character: 1,
                },
                end: Position {
                    line: 0,
                    character: 2,
                },
            },
        )
        ");
    }

    #[test]
    fn position_encoding_converts_to_lsp_encoding_kind() {
        let encodings = [PositionEncoding::Utf8, PositionEncoding::Utf16, PositionEncoding::Utf32]
            .map(PositionEncodingKind::from);
        insta::assert_debug_snapshot!(encodings, @r#"
        [
            PositionEncodingKind(
                "utf-8",
            ),
            PositionEncodingKind(
                "utf-16",
            ),
            PositionEncodingKind(
                "utf-32",
            ),
        ]
        "#);
    }

    #[test]
    fn protocol_position_past_line_end_clamps_to_same_line() {
        let content = "abc\ndef";
        let positions = PositionConverter::new(content, PositionEncoding::Utf16);
        let position = Position::new(0, 99);

        let position = positions.protocol_position_to_utf8(position).unwrap();
        insta::assert_debug_snapshot!(position, @"
        Utf8Position {
            line: 0,
            column: 3,
        }
        ");
    }

    #[test]
    fn range_conversions_use_shared_line_index() {
        let content = "é😀x\r\nplain\r\n";
        let ranges = [
            Utf8Range {
                start: Utf8Position { line: 0, column: 2 },
                end: Utf8Position { line: 0, column: 6 },
            },
            Utf8Range {
                start: Utf8Position { line: 1, column: 0 },
                end: Utf8Position { line: 1, column: 5 },
            },
        ];

        let converted = (
            PositionConverter::new(content, PositionEncoding::Utf8)
                .utf8_range_to_protocol(ranges[0]),
            PositionConverter::new(content, PositionEncoding::Utf16)
                .utf8_range_to_protocol(ranges[0]),
            PositionConverter::new(content, PositionEncoding::Utf16)
                .utf8_range_to_protocol(ranges[1]),
        );
        insta::assert_debug_snapshot!(converted, @"
        (
            Some(
                Range {
                    start: Position {
                        line: 0,
                        character: 2,
                    },
                    end: Position {
                        line: 0,
                        character: 6,
                    },
                },
            ),
            Some(
                Range {
                    start: Position {
                        line: 0,
                        character: 1,
                    },
                    end: Position {
                        line: 0,
                        character: 3,
                    },
                },
            ),
            Some(
                Range {
                    start: Position {
                        line: 1,
                        character: 0,
                    },
                    end: Position {
                        line: 1,
                        character: 5,
                    },
                },
            ),
        )
        ");
    }

    #[test]
    fn invalid_protocol_positions_return_none() {
        let content = "é😀x\r\nplain\r\n";
        let utf8 = PositionConverter::new(content, PositionEncoding::Utf8);
        let utf16 = PositionConverter::new(content, PositionEncoding::Utf16);

        let converted = (
            utf8.protocol_position_to_utf8(Position::new(9, 0)),
            utf8.protocol_position_to_utf8(Position::new(0, 1)),
            utf16.protocol_position_to_utf8(Position::new(0, 2)),
        );
        insta::assert_debug_snapshot!(converted, @"
        (
            None,
            None,
            None,
        )
        ");
    }
}
