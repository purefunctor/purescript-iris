pub mod imports;
pub mod let_signatures;

use files::FileId;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEdit {
    pub start_byte: usize,
    pub end_byte: usize,
    pub new_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRefactorPlan {
    pub file_id: FileId,
    pub edits: Vec<SourceEdit>,
    pub signatures_added: usize,
    pub imports_added: usize,
    pub imports_updated: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedEntry {
    pub file_id: FileId,
    pub offset: usize,
    pub name: String,
    pub reason: SkipReason,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    ModuleHasErrors,
    Unchecked,
    PolymorphicOrUnsolved,
    LeadingComment,
    NotAtLineStart,
    TabIndentation,
    UnimportableTypes,
}

impl SkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SkipReason::ModuleHasErrors => "module_has_errors",
            SkipReason::Unchecked => "unchecked",
            SkipReason::PolymorphicOrUnsolved => "polymorphic_or_unsolved",
            SkipReason::LeadingComment => "leading_comment",
            SkipReason::NotAtLineStart => "not_at_line_start",
            SkipReason::TabIndentation => "tab_indentation",
            SkipReason::UnimportableTypes => "unimportable_types",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefactorPlan {
    pub files: Vec<FileRefactorPlan>,
    pub skipped: Vec<SkippedEntry>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EditApplyError {
    #[error("edit range {start_byte}..{end_byte} is out of bounds for source length {length}")]
    OutOfBounds { start_byte: usize, end_byte: usize, length: usize },
    #[error("edit range {start_byte}..{end_byte} does not fall on UTF-8 character boundaries")]
    InvalidBoundary { start_byte: usize, end_byte: usize },
    #[error("edits overlap at byte offset {offset}")]
    Overlapping { offset: usize },
}

pub fn apply_edits(source: &str, edits: &[SourceEdit]) -> Result<String, EditApplyError> {
    let mut ordered = edits.to_vec();
    ordered.sort_by(|left, right| {
        left.start_byte.cmp(&right.start_byte).then_with(|| left.end_byte.cmp(&right.end_byte))
    });

    let mut previous_end = 0;
    for &SourceEdit { start_byte, end_byte, .. } in &ordered {
        if start_byte > end_byte || end_byte > source.len() {
            return Err(EditApplyError::OutOfBounds { start_byte, end_byte, length: source.len() });
        }
        if !source.is_char_boundary(start_byte) || !source.is_char_boundary(end_byte) {
            return Err(EditApplyError::InvalidBoundary { start_byte, end_byte });
        }
        if start_byte < previous_end {
            return Err(EditApplyError::Overlapping { offset: start_byte });
        }
        previous_end = end_byte;
    }

    let mut result = source.to_string();
    for SourceEdit { start_byte, end_byte, new_text } in ordered.into_iter().rev() {
        result.replace_range(start_byte..end_byte, &new_text);
    }
    Ok(result)
}
