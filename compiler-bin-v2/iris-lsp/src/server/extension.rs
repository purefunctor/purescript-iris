use lsp_types::*;
use serde::{Deserialize, Serialize};

pub enum CustomInitialize {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomInitializeParams {
    #[serde(flatten)]
    pub initialize_params: InitializeParams,
    pub work_done_token: Option<ProgressToken>,
}

impl request::Request for CustomInitialize {
    type Params = CustomInitializeParams;

    type Result = InitializeResult;

    const METHOD: &'static str = request::Initialize::METHOD;
}
