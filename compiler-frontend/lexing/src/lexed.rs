use std::ops::Range;
use std::sync::Arc;

use syntax::SyntaxKind;

use crate::Position;

/// Information attached to a [`SyntaxKind`].
///
/// [`SyntaxKind`] by itself does not store metadata, as it's represented by
/// a [`u16`]. In particular, this data type encapsulates offset information
/// used in post-lexing and parsing.
///
/// ## Invariants
///
/// The following invariants must hold for a [`SyntaxKindInfo`]:
///
/// * [`annotation`] is less than or equal to [`qualifier`].
/// * [`qualifier`] is less than or equal to [`token`].
/// * [`annotation`] is less than or equal to [`token`].
///
/// Refer to the documentation of each field for more invariants.
///
/// [`annotation`]: SyntaxKindInfo::annotation
/// [`qualifier`]: SyntaxKindInfo::qualifier
/// [`token`]: SyntaxKindInfo::token
#[derive(Debug, Clone, Copy)]
pub struct SyntaxKindInfo {
    /// The ending offset of a token's annotation.
    ///
    /// Annotations are either whitespace or comments attached as a prefix to a
    /// specific token. Rather than storing them as separate tokens, we simply
    /// attach them as metadata to a token to greatly simplify parsing.
    ///
    /// To obtain the textual content of an annotation, we use the [`token`]
    /// field of the previous token as the start offset and this field as the
    /// end offset.
    ///
    /// [`token`]: SyntaxKindInfo::token
    pub annotation: u32,
    /// The ending offset of a token's qualifier.
    ///
    /// Qualifiers are module names that identify the namespace of a token.
    /// They are usually present for name tokens such as [`SyntaxKind::LOWER`],
    /// and in special features such as qualified [`SyntaxKind::DO`].
    ///
    /// To obtain the textual content of a qualifier, we use the [`annotation`]
    /// as the start offset and this field as the end offset. Consequently, if
    /// the [`annotation`] is equal to this field, then there is no qualifier.
    ///
    /// [`annotation`]: SyntaxKindInfo::annotation
    pub qualifier: u32,
    /// The ending offset of a token's annotation.
    ///
    /// To obtain the textual content of a token, we use the [`qualifier`]
    /// as the start offset and this field as the end offset. Consequently, if
    /// the [`qualifier`] is equal to this field, then the token is zero-width.
    ///
    /// [`qualifier`]: SyntaxKindInfo::qualifier
    pub token: u32,
    /// The token's position starting from the qualifier.
    pub position: Position,
}

#[derive(Debug)]
struct LexerError {
    message: Arc<str>,
    index: u32,
}

pub(super) struct LexedBuilder<'s> {
    source: &'s str,
    kinds: Vec<SyntaxKind>,
    infos: Vec<SyntaxKindInfo>,
    errors: Vec<LexerError>,
}

impl<'s> LexedBuilder<'s> {
    pub(super) fn new(source: &'s str) -> LexedBuilder<'s> {
        let capacity = source.len() / 4;
        let kinds = Vec::with_capacity(capacity);
        let infos = Vec::with_capacity(capacity);
        let errors = Vec::new();
        LexedBuilder { source, kinds, infos, errors }
    }

    pub(super) fn push(&mut self, kind: SyntaxKind, info: SyntaxKindInfo, error: Option<&str>) {
        self.kinds.push(kind);
        self.infos.push(info);

        if let Some(error) = error {
            let message = error.into();
            let index = self.kinds.len() as u32 - 1;
            self.errors.push(LexerError { message, index });
        }
    }

    pub(super) fn build(self) -> Lexed<'s> {
        Lexed { source: self.source, kinds: self.kinds, infos: self.infos, errors: self.errors }
    }
}

pub struct Lexed<'s> {
    pub source: &'s str,
    kinds: Vec<SyntaxKind>,
    infos: Vec<SyntaxKindInfo>,
    errors: Vec<LexerError>,
}

impl Lexed<'_> {
    pub fn kind(&self, index: usize) -> SyntaxKind {
        assert!(index < self.kinds.len());
        self.kinds[index]
    }

    pub fn info(&self, index: usize) -> SyntaxKindInfo {
        assert!(index < self.infos.len());
        self.infos[index]
    }

    pub fn position(&self, index: usize) -> Position {
        assert!(index < self.infos.len());
        self.infos[index].position
    }

    pub fn annotation(&self, index: usize) -> Option<&str> {
        assert!(index < self.infos.len());

        let low = if index > 0 { self.infos[index - 1].token as usize } else { 0 };
        let high = self.infos[index].annotation as usize;

        if low < high { Some(&self.source[low..high]) } else { None }
    }

    pub fn qualifier(&self, index: usize) -> Option<&str> {
        assert!(index < self.infos.len());

        let low = self.infos[index].annotation as usize;
        let high = self.infos[index].qualifier as usize;

        if low < high { Some(&self.source[low..high]) } else { None }
    }

    pub fn error(&self, index: usize) -> Option<Arc<str>> {
        assert!(index < self.kinds.len());
        let token_index = index as u32;
        let error_index = self.errors.binary_search_by_key(&token_index, |v| v.index).ok()?;
        Some(Arc::clone(&self.errors[error_index].message))
    }

    pub fn text(&self, index: usize) -> &str {
        assert!(index < self.infos.len());

        let low = self.infos[index].qualifier as usize;
        let high = self.infos[index].token as usize;

        &self.source[low..high]
    }

    pub fn text_in_range(&self, range: Range<usize>) -> &str {
        assert!(range.start < range.end && range.end < self.infos.len());

        let low = if range.start > 0 { self.infos[range.start].token as usize } else { 0 };
        let high = self.infos[range.end].token as usize;

        &self.source[low..high]
    }

    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}
