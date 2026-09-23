//! The workspace actor loop and dispatch by method.

use iris_analysis::AnalyzerCapabilities;
use iris_analysis::position::PositionEncoding;
use iris_build::compilation::{CompilationParts, CompilationState, MaterializedPrim};
use iris_lsp_server::{
    Answer, OrderedMessage, Rejection, WorkspaceEventSender, WorkspaceFailure, WorkspaceReceivers,
};
use lsp_types::{InitializeParams, WorkspaceSymbolParams};
use serde_json::Value;
use tokio::task;

use crate::WorkspaceConfig;
use crate::analysis::Analysis;
use crate::capabilities::{
    initialize_result, negotiate_analyzer_capabilities, negotiate_position_encoding,
};
use crate::state::SourceMetadata;

pub(crate) struct Actor {
    config: WorkspaceConfig,
    _events: WorkspaceEventSender,
    position_encoding: PositionEncoding,
    analyzer_capabilities: AnalyzerCapabilities,
    analysis: Analysis,
    _prim: MaterializedPrim,
}

impl Actor {
    pub(crate) fn new(config: WorkspaceConfig, events: WorkspaceEventSender) -> Actor {
        let prim = MaterializedPrim::new()
            .expect("invariant violated: failed to materialize the Prim modules");
        let compilation = CompilationState::new(prim, SourceMetadata::Builtin);
        let CompilationParts { engine, files, prim } = compilation.into_parts();
        Actor {
            config,
            _events: events,
            position_encoding: PositionEncoding::Utf16,
            analyzer_capabilities: AnalyzerCapabilities::default(),
            analysis: Analysis::new(engine, files),
            _prim: prim,
        }
    }

    pub(crate) async fn run(
        mut self,
        mut receivers: WorkspaceReceivers,
    ) -> Result<(), WorkspaceFailure> {
        while let Some(message) = receivers.ordered.recv().await {
            match message {
                OrderedMessage::Initialize { params, reply } => {
                    let _ = reply.send(self.initialize(params));
                }
                OrderedMessage::Request { method, params, reply } => {
                    if method != "workspace/symbol" {
                        let _ = reply.send(Err(Rejection::MethodNotFound));
                        continue;
                    }
                    let parameters = match serde_json::from_value::<WorkspaceSymbolParams>(params) {
                        Ok(parameters) => parameters,
                        Err(error) => {
                            let message = format!("Failed to deserialize parameters: {error}");
                            let _ = reply.send(Err(Rejection::InvalidParams(message)));
                            continue;
                        }
                    };
                    let snapshot =
                        self.analysis.snapshot(self.position_encoding, self.analyzer_capabilities);
                    task::spawn(async move {
                        let answer = task::spawn_blocking(move || {
                            let mut cache = snapshot.workspace_symbols_cache.write();
                            let result = snapshot.with_analyzer_context(|context| {
                                iris_analysis::symbols::workspace(
                                    context,
                                    &mut cache,
                                    &parameters.query,
                                )
                            });
                            match result {
                                Ok(result) => Ok(serde_json::to_value(result)
                                    .expect("invariant violated: result must serialize")),
                                Err(_) => Err(Rejection::RequestFailed("Request failed".into())),
                            }
                        })
                        .await
                        .unwrap_or_else(|error| Err(Rejection::Internal(error.to_string())));
                        let _ = reply.send(answer);
                    });
                }
                OrderedMessage::Initialized
                | OrderedMessage::Settings(_)
                | OrderedMessage::Notification { .. } => {}
            }
        }
        Ok(())
    }

    fn initialize(&mut self, params: Value) -> Answer {
        let parameters = serde_json::from_value::<InitializeParams>(params).map_err(|error| {
            Rejection::InvalidParams(format!("Failed to deserialize parameters: {error}"))
        })?;
        self.position_encoding = negotiate_position_encoding(&parameters);
        self.analyzer_capabilities = negotiate_analyzer_capabilities(&parameters);
        let result =
            initialize_result(&self.config.name, &self.config.version, self.position_encoding);
        Ok(serde_json::to_value(result)
            .expect("invariant violated: InitializeResult must serialize"))
    }
}
