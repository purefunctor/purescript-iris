use building_types::QueryProxy;
use checking::holes::HoleBinding;
use lsp_types::*;

use crate::code_action::{CodeActionRequest, expression_range, type_range, workspace_edit};
use crate::{AnalyzerError, locate};

pub fn collect(
    request: &CodeActionRequest<impl crate::AnalyzerHost>,
    actions: &mut Vec<CodeActionResponse>,
) -> Result<(), AnalyzerError> {
    if !request.kinds.includes(&CodeActionKind::QuickFix) {
        return Ok(());
    }

    let queries = request.language.queries();
    let located = locate::locate(queries, request.file, request.positions, request.position)?;
    match located {
        locate::Located::Expression(expression_id) => {
            let checked = queries.checked(request.file)?;
            let Some(hole) = checked.lookup_term_hole(expression_id) else { return Ok(()) };

            let range = expression_range(request, expression_id)?;
            collect_binding_actions(request, range, &hole.bindings, actions);
        }
        locate::Located::Type(type_id) => {
            let checked = queries.checked(request.file)?;
            let Some(hole) = checked.lookup_type_hole(type_id) else { return Ok(()) };

            let range = type_range(request, type_id)?;
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
    actions: &mut Vec<CodeActionResponse>,
) {
    for binding in bindings {
        let name = binding.name.to_string();
        let title = format!("Replace hole with '{name}'");

        actions.push(CodeActionResponse::CodeAction(CodeAction {
            title,
            kind: Some(CodeActionKind::QuickFix),
            edit: Some(workspace_edit(request.uri, vec![TextEdit { range, new_text: name }])),
            ..CodeAction::default()
        }));
    }
}
