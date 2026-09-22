use building_types::QueryProxy;
use checking::holes::HoleBinding;
use lsp_types::*;

use crate::code_action::{CodeActionRequest, expression_range, type_range, workspace_edit};
use crate::{AnalyzerError, locate};

pub fn collect(
    request: &CodeActionRequest<impl crate::AnalyzerHost>,
    actions: &mut Vec<CodeActionOrCommand>,
) -> Result<(), AnalyzerError> {
    if !request.kinds.includes(&CodeActionKind::QUICKFIX) {
        return Ok(());
    }

    let checked = request.language.queries().checked(request.file)?;
    match &request.located {
        locate::Located::Expression(expression_id) => {
            let Some(hole) = checked.lookup_term_hole(*expression_id) else { return Ok(()) };

            let range = expression_range(request, *expression_id)?;
            collect_binding_actions(request, range, &hole.bindings, actions);
        }
        locate::Located::Type(type_id) => {
            let Some(hole) = checked.lookup_type_hole(*type_id) else { return Ok(()) };

            let range = type_range(request, *type_id)?;
            collect_binding_actions(request, range, &hole.bindings, actions);
        }
        _ => (),
    }

    Ok(())
}

fn collect_binding_actions(
    request: &CodeActionRequest<impl crate::AnalyzerHost>,
    range: Range,
    bindings: &[HoleBinding],
    actions: &mut Vec<CodeActionOrCommand>,
) {
    for binding in bindings {
        let name = binding.name.to_string();
        let title = format!("Replace hole with '{name}'");

        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title,
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(workspace_edit(request.uri, vec![TextEdit { range, new_text: name }])),
            ..CodeAction::default()
        }));
    }
}
