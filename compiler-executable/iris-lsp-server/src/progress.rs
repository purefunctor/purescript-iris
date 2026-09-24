//! Preparation progress: tokens, `window/workDoneProgress/create`, late acceptance, and
//! cancellation.
//!
//! Each preparation attempt gets the token `iris/startup/<generation>`. Its creation request is
//! sent without waiting for the editor's answer. Until the editor accepts it, only the latest
//! report and the end message are kept; on acceptance they are sent after the begin message, so
//! begin and end stay balanced even when creation is accepted after preparation ended.

use std::collections::HashMap;

use lsp_server::ResponseError;
use lsp_types::{
    Notification, ProgressNotification, ProgressParams, ProgressToken, Request,
    WorkDoneProgressBegin, WorkDoneProgressCreateParams, WorkDoneProgressCreateRequest,
    WorkDoneProgressEnd, WorkDoneProgressReport,
};
use serde::Serialize;

use crate::outgoing::{EditorConnection, OutgoingPurpose};
use crate::settings::to_value;

#[derive(Default)]
pub(crate) struct Progress {
    supported: bool,
    /// The latest attempt; only its token can be cancelled.
    current: Option<u64>,
    attempts: HashMap<u64, Attempt>,
}

struct Attempt {
    token: ProgressToken,
    state: AttemptState,
    cancel_requested: bool,
}

enum AttemptState {
    Creating {
        begin: WorkDoneProgressBegin,
        latest: Option<WorkDoneProgressReport>,
        end: Option<String>,
    },
    Active,
}

impl Progress {
    pub(crate) fn new(supported: bool) -> Progress {
        Progress { supported, current: None, attempts: HashMap::new() }
    }

    pub(crate) fn started(
        &mut self,
        editor: &mut EditorConnection,
        generation: u64,
        title: String,
        message: String,
    ) {
        if !self.supported {
            return;
        }
        let token = ProgressToken::String(format!("iris/startup/{generation}"));
        let begin = WorkDoneProgressBegin {
            title,
            cancellable: Some(true),
            message: Some(message),
            percentage: Some(0),
        };
        let state = AttemptState::Creating { begin, latest: None, end: None };
        let parameters = WorkDoneProgressCreateParams { token: ProgressToken::clone(&token) };
        self.current = Some(generation);
        self.attempts.insert(generation, Attempt { token, state, cancel_requested: false });
        let purpose = OutgoingPurpose::ProgressCreation { generation };
        let method = WorkDoneProgressCreateRequest::METHOD.as_str();
        editor.request(method, to_value(parameters), purpose, None);
    }

    pub(crate) fn report(
        &mut self,
        editor: &EditorConnection,
        generation: u64,
        message: String,
        percentage: Option<u32>,
    ) {
        let Some(attempt) = self.attempts.get_mut(&generation) else { return };
        let report =
            WorkDoneProgressReport { cancellable: Some(true), message: Some(message), percentage };
        match &mut attempt.state {
            AttemptState::Creating { latest, end: None, .. } => *latest = Some(report),
            AttemptState::Creating { end: Some(_), .. } => {}
            AttemptState::Active => {
                notify(editor, &attempt.token, WorkDoneProgress::Report(report))
            }
        }
    }

    pub(crate) fn ended(&mut self, editor: &EditorConnection, generation: u64, message: String) {
        let Some(attempt) = self.attempts.get_mut(&generation) else { return };
        match &mut attempt.state {
            AttemptState::Creating { end: end @ None, .. } => *end = Some(message),
            AttemptState::Creating { end: Some(_), .. } => {}
            AttemptState::Active => {
                let end = WorkDoneProgressEnd { message: Some(message) };
                notify(editor, &attempt.token, WorkDoneProgress::End(end));
                self.attempts.remove(&generation);
            }
        }
    }

    /// Handles the editor's answer to the creation request for `generation`.
    pub(crate) fn created(
        &mut self,
        editor: &EditorConnection,
        generation: u64,
        result: Result<(), String>,
    ) {
        let Some(attempt) = self.attempts.get_mut(&generation) else { return };
        let AttemptState::Creating { begin, latest, end } = &mut attempt.state else { return };
        if let Err(error) = result {
            tracing::warn!("Failed to create workspace preparation progress: {error}");
            self.attempts.remove(&generation);
            return;
        }
        let token = ProgressToken::clone(&attempt.token);
        notify(editor, &token, WorkDoneProgress::Begin(begin.clone()));
        if let Some(report) = latest.take() {
            notify(editor, &token, WorkDoneProgress::Report(report));
        }
        if let Some(message) = end.take() {
            let end = WorkDoneProgressEnd { message: Some(message) };
            notify(editor, &token, WorkDoneProgress::End(end));
            self.attempts.remove(&generation);
            return;
        }
        attempt.state = AttemptState::Active;
    }

    /// Returns the attempt to cancel for a `window/workDoneProgress/cancel` of `token`: only the
    /// current attempt's token, only while its progress has not ended, and only once.
    pub(crate) fn cancel(&mut self, token: &ProgressToken) -> Option<u64> {
        let generation = self.current?;
        let attempt = self.attempts.get_mut(&generation)?;
        let ended = matches!(attempt.state, AttemptState::Creating { end: Some(_), .. });
        if attempt.token != *token || ended || attempt.cancel_requested {
            return None;
        }
        attempt.cancel_requested = true;
        Some(generation)
    }
}

pub(crate) fn creation_result(
    result: Result<serde_json::Value, ResponseError>,
) -> Result<(), String> {
    result.map(|_| ()).map_err(|error| format!("{} (jsonrpc error {})", error.message, error.code))
}

/// The `$/progress` payloads for work done progress. Each structure serializes its own `kind`.
#[derive(Serialize)]
#[serde(untagged)]
enum WorkDoneProgress {
    Begin(WorkDoneProgressBegin),
    Report(WorkDoneProgressReport),
    End(WorkDoneProgressEnd),
}

fn notify(editor: &EditorConnection, token: &ProgressToken, value: WorkDoneProgress) {
    let parameters = ProgressParams { token: ProgressToken::clone(token), value: to_value(value) };
    editor.notify(ProgressNotification::METHOD.as_str(), to_value(parameters));
}
