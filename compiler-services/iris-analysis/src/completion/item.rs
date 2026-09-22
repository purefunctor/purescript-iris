use lsp_types::*;

use super::resolve::CompletionResolveData;

pub struct CompletionItemSpec {
    name: String,
    range: Option<Range>,
    kind: CompletionItemKind,
    data: CompletionResolveData,

    label_detail: Option<String>,
    label_description: Option<String>,
    edit_text: Option<String>,
    sort_text: Option<String>,
    filter_text: Option<String>,
    additional_text_edits: Option<Vec<TextEdit>>,
}

impl CompletionItemSpec {
    pub fn new(
        name: String,
        range: Option<Range>,
        kind: CompletionItemKind,
        data: CompletionResolveData,
    ) -> CompletionItemSpec {
        CompletionItemSpec {
            name,
            range,
            kind,
            data,
            label_detail: None,
            label_description: None,
            edit_text: None,
            sort_text: None,
            filter_text: None,
            additional_text_edits: None,
        }
    }

    pub fn build(self) -> CompletionItem {
        let label = self.name;
        let kind = self.kind;
        let additional_text_edits = self.additional_text_edits;

        let label_details = CompletionItemLabelDetails {
            detail: self.label_detail,
            description: self.label_description,
        };

        let edit_text = self.edit_text.unwrap_or_else(|| label.clone());
        let sort_text = self.sort_text.unwrap_or_else(|| edit_text.clone());
        let filter_text = self.filter_text.unwrap_or_else(|| edit_text.clone());

        let text_edit = self.range.map(|range| {
            let new_text = edit_text;
            CompletionTextEdit::Edit(TextEdit { range, new_text })
        });

        let data = serde_json::to_value(self.data)
            .expect("invariant violated: invalid CompletionResolveData");

        CompletionItem {
            label,
            text_edit,
            additional_text_edits,
            label_details: Some(label_details),
            kind: Some(kind),
            sort_text: Some(sort_text),
            filter_text: Some(filter_text),
            data: Some(data),
            ..Default::default()
        }
    }
}

macro_rules! create_setters {
    ($($f:ident -> $t:ty $(,)?)+) => {
        impl CompletionItemSpec {
            $(
                pub fn $f(&mut self, value: $t) -> &mut CompletionItemSpec {
                    self.$f = Some(value);
                    self
                }
            )*
        }
    };
}

create_setters!(
    label_detail -> String,
    label_description -> String,
    edit_text -> String,
    sort_text -> String,
    additional_text_edits -> Vec<TextEdit>,
);
