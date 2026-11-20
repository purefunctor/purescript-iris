use std::path::PathBuf;
use std::{env, io, iter};

use iris_watch_query::{
    DeclarationsAnswer, DependentsAnswer, DiagnosticsAnswer, InstancesAnswer, JavascriptAnswer,
    LocationsAnswer, RefactorAnswer, RefactorChanges, SearchAnswer, WaitAnswer,
};
use iris_watch_server::client;
use iris_watch_server::discovery::Discovery;
use iris_watch_server::protocol::{InstanceSearch, Namespace, Query, RefactorKind, Response};
use rmcp::handler::server::tool::{ToolRouter, schema_for_output};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const INSTRUCTIONS: &str = "\
Answers questions about a PureScript project that a running `iris watch` keeps compiled, \
dependencies included. Call `wait` after writing files and before other tools, so answers \
reflect the edits. Names are fully qualified: the module, a dot, then the item, such as \
Data.Maybe.fromMaybe. Use `search` when you do not know which module declares something. Paths \
inside the project are relative to its root; lines and columns count from 1, and columns count \
characters.";

pub fn serve(output: PathBuf, version: String) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let result = runtime.block_on(async {
        let tools = WatchTools::new(output, version);
        let service =
            tools.serve(rmcp::transport::stdio_pipes()).await.map_err(io::Error::other)?;
        service.waiting().await.map_err(io::Error::other)?;
        Ok(())
    });
    runtime.shutdown_background();
    result
}

#[derive(Deserialize, JsonSchema)]
struct ItemParameters {
    #[schemars(description = "Qualified name, such as Data.Maybe.fromMaybe or \
                              Data.Maybe.Maybe. Operators may be written with or without \
                              parentheses, such as Data.Function.(<<<).")]
    name: String,
    #[schemars(description = "Only the value, or only the type or class, with this name. \
                              Without it, the query is about both.")]
    namespace: Option<Namespace>,
}

#[derive(Deserialize, JsonSchema)]
struct ModuleParameters {
    #[schemars(description = "Module name, such as Data.Maybe.")]
    module: String,
}

#[derive(Deserialize, JsonSchema)]
struct DiagnosticsParameters {
    #[schemars(description = "Module name, such as Main. Without it, every module.")]
    module: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct InstancesParameters {
    #[schemars(description = "Qualified name of a class or type, such as Data.Show.Show.")]
    name: String,
    search: InstanceSearch,
}

#[derive(Deserialize, JsonSchema)]
struct SearchParameters {
    #[schemars(description = "Part of a name, such as foldl or fromMay; letters may be skipped.")]
    pattern: String,
}

#[derive(Deserialize, JsonSchema)]
struct RefactorParameters {
    #[schemars(description = "Module name, such as Main. Without it, every editable module.")]
    module: Option<String>,
    #[serde(default)]
    #[schemars(description = "Return the planned changes as a unified diff without writing \
                              files.")]
    preview: bool,
}

#[derive(Serialize, JsonSchema)]
struct RefactorOutcome {
    #[serde(flatten)]
    answer: RefactorAnswer,
    #[schemars(description = "Whether the files were written; false when previewing.")]
    written: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "The planned changes as a unified diff, when previewing.")]
    patch: Option<String>,
}

#[derive(Clone)]
struct WatchTools {
    output: PathBuf,
    version: String,
    tool_router: ToolRouter<WatchTools>,
}

#[tool_router]
impl WatchTools {
    fn new(output: PathBuf, version: String) -> WatchTools {
        WatchTools { output, version, tool_router: WatchTools::tool_router() }
    }

    #[tool(
        description = "Rescan sources, rebuild if anything changed, and report the build \
                       outcome. Call it after writing files and before other tools.",
        output_schema = schema_for_output::<WaitAnswer>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn wait(&self) -> CallToolResult {
        self.ask(Query::Wait).await
    }

    #[tool(
        description = "The signature of a value, or the kind of a type or class, with its \
                       documentation.",
        output_schema = schema_for_output::<DeclarationsAnswer>(),
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn signature(
        &self,
        Parameters(ItemParameters { name, namespace }): Parameters<ItemParameters>,
    ) -> CallToolResult {
        self.ask(Query::Signature { name, namespace }).await
    }

    #[tool(
        description = "Every export of a module with its signature and documentation.",
        output_schema = schema_for_output::<DeclarationsAnswer>(),
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn module(
        &self,
        Parameters(ModuleParameters { module }): Parameters<ModuleParameters>,
    ) -> CallToolResult {
        self.ask(Query::Module { name: module }).await
    }

    #[tool(
        description = "Where a value, type, or class is declared.",
        output_schema = schema_for_output::<LocationsAnswer>(),
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn definition(
        &self,
        Parameters(ItemParameters { name, namespace }): Parameters<ItemParameters>,
    ) -> CallToolResult {
        self.ask(Query::Definition { name, namespace }).await
    }

    #[tool(
        description = "Where a value, type, or class is used.",
        output_schema = schema_for_output::<LocationsAnswer>(),
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn references(
        &self,
        Parameters(ItemParameters { name, namespace }): Parameters<ItemParameters>,
    ) -> CallToolResult {
        self.ask(Query::References { name, namespace }).await
    }

    #[tool(
        description = "The modules that import a module, directly or through other modules. \
                       An indirect dependent names the module it imports on the way.",
        output_schema = schema_for_output::<DependentsAnswer>(),
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn dependents(
        &self,
        Parameters(ModuleParameters { module }): Parameters<ModuleParameters>,
    ) -> CallToolResult {
        self.ask(Query::Dependents { name: module }).await
    }

    #[tool(
        description = "The instances of a class, or the instances whose head mentions a type, \
                       whatever their class.",
        output_schema = schema_for_output::<InstancesAnswer>(),
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn instances(
        &self,
        Parameters(InstancesParameters { name, search }): Parameters<InstancesParameters>,
    ) -> CallToolResult {
        self.ask(Query::Instances { name, search }).await
    }

    #[tool(
        description = "Declarations whose names match a pattern, best matches first: whole \
                       names, then prefixes, then substrings, then letters in order, ignoring \
                       case. Lists the best 50 and counts every match.",
        output_schema = schema_for_output::<SearchAnswer>(),
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn search(
        &self,
        Parameters(SearchParameters { pattern }): Parameters<SearchParameters>,
    ) -> CallToolResult {
        self.ask(Query::Search { pattern }).await
    }

    #[tool(
        description = "The errors and warnings of one module, or of every module.",
        output_schema = schema_for_output::<DiagnosticsAnswer>(),
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn diagnostics(
        &self,
        Parameters(DiagnosticsParameters { module }): Parameters<DiagnosticsParameters>,
    ) -> CallToolResult {
        self.ask(Query::Diagnostics { name: module }).await
    }

    #[tool(
        description = "The JavaScript written to the output directory for a module. Fails \
                       while any module has errors, because nothing is written then.",
        output_schema = schema_for_output::<JavascriptAnswer>(),
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn javascript(
        &self,
        Parameters(ModuleParameters { module }): Parameters<ModuleParameters>,
    ) -> CallToolResult {
        self.ask(Query::Javascript { name: module }).await
    }

    #[tool(
        description = "Add explicit type signatures to strictly monomorphic let, where, and \
                       multi-line do and ado let bindings across the project's modules, or in \
                       one module, importing missing types as needed. Writes the files unless \
                       preview is set. Only editable project files change; modules with errors \
                       are skipped and reported. Call wait afterwards.",
        output_schema = schema_for_output::<RefactorOutcome>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn refactor_let_signatures(
        &self,
        Parameters(RefactorParameters { module, preview }): Parameters<RefactorParameters>,
    ) -> CallToolResult {
        let explicit_module = module.is_some();
        let query = Query::Refactor { refactor: RefactorKind::LetSignatures, module };
        let (discovery, value) = match self.request(&query).await {
            Ok(answer) => answer,
            Err(failure) => return failure,
        };
        let warning = discovery.version_warning(&self.version);
        let answer = match iris_watch_query::parse_refactor_answer(value) {
            Ok(answer) => answer,
            Err(error) => return failed(unreadable(&error), warning),
        };
        if explicit_module && answer.is_blocked_by_module_errors() {
            return failed(iris_watch_query::render_refactor_summary(&answer), warning);
        }
        let root = match discovery.root.map_or_else(env::current_dir, Ok) {
            Ok(root) => root,
            Err(error) => {
                let message = format!("could not determine working directory: {error}");
                return failed(message, warning);
            }
        };
        let changes = match RefactorChanges::prepare(&root, &answer) {
            Ok(changes) => changes,
            Err(error) => return failed(error.to_string(), warning),
        };
        let (text, patch) = if preview {
            let patch = changes.patch();
            let text = if patch.is_empty() {
                iris_watch_query::render_refactor_summary(&answer)
            } else if let Some(skipped) = iris_watch_query::render_refactor_skipped(&answer) {
                format!("{patch}{skipped}")
            } else {
                String::clone(&patch)
            };
            (text, Some(patch))
        } else {
            if let Err(error) = changes.write() {
                return failed(error.to_string(), warning);
            }
            (iris_watch_query::render_refactor_summary(&answer), None)
        };
        let outcome = RefactorOutcome { answer, written: !preview, patch };
        answered(text, iris_watch_query::to_value(&outcome), warning)
    }
}

impl WatchTools {
    async fn ask(&self, query: Query) -> CallToolResult {
        let (discovery, value) = match self.request(&query).await {
            Ok(answer) => answer,
            Err(failure) => return failure,
        };
        let warning = discovery.version_warning(&self.version);
        match iris_watch_query::render(&query, &value) {
            Ok(text) => answered(text, value, warning),
            Err(error) => failed(unreadable(&error), warning),
        }
    }

    async fn request(&self, query: &Query) -> Result<(Discovery, Value), CallToolResult> {
        let (discovery, response) = match client::request(&self.output, query).await {
            Ok(answer) => answer,
            Err(error) => return Err(failed(error.to_string(), None)),
        };
        match response {
            Response::Result { value } => Ok((discovery, value)),
            Response::Error { message } => {
                Err(failed(message, discovery.version_warning(&self.version)))
            }
            Response::Cancelled => {
                unreachable!("invariant violated: cancelled queries are sent again")
            }
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for WatchTools {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        ServerConfig::new(capabilities)
            .with_server_info(Implementation::new("iris", &self.version))
            .with_instructions(INSTRUCTIONS)
    }
}

fn answered(text: String, value: Value, warning: Option<String>) -> CallToolResult {
    let mut result = CallToolResult::success(contents(text, warning));
    result.structured_content = Some(value);
    result
}

fn failed(message: String, warning: Option<String>) -> CallToolResult {
    CallToolResult::error(contents(message, warning))
}

fn contents(text: String, warning: Option<String>) -> Vec<ContentBlock> {
    let warning = warning.map(|warning| ContentBlock::text(format!("warning: {warning}")));
    iter::once(ContentBlock::text(text)).chain(warning).collect()
}

fn unreadable(error: &serde_json::Error) -> String {
    format!("the watcher sent an answer this Iris cannot read: {error}")
}
