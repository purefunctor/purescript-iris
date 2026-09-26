//! The build actor: the build session, filesystem events, rebuilds, and requests.
//!
//! The actor handles one thing at a time. Reading files and rebuilding run on a blocking thread
//! that owns the session while it runs, so a signal can still stop the watcher meanwhile. A
//! request that arrives while filesystem events are waiting applies them first, so it is never
//! answered from a state the watcher already knows is out of date.
//!
//! Queries run on blocking threads against snapshots. Applying a change to the engine's inputs
//! cancels them at their next engine query and waits until they drop their snapshots.

use std::panic;
use std::path::PathBuf;
use std::time::Duration;

use iris_build::{
    BuildSession, BuildSessionConfig, InputChange, PreparedProject, RebuildOutcome,
    initialize_project,
};
use iris_progress::WatchOutcome;
use iris_watch_query::{BuildState, QueryContext, QueryFailure, WaitAnswer};
use iris_watch_server::QueryRequest;
use iris_watch_server::protocol::{Query, ResponseBody};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::task;
use tokio::time::{Instant, sleep_until};

use crate::filesystem::{Batch, is_access, persistent_watch_root};
use crate::{WatchConfig, WatchFailure, report};

const DEBOUNCE_DURATION: Duration = Duration::from_millis(100);

pub(crate) struct BuildActor {
    config: WatchConfig,
    root: PathBuf,
    /// `None` only while a blocking task owns the session.
    session: Option<BuildSession>,
    generation: u64,
    build: BuildState,
    /// Filesystem events that have not been applied yet.
    batch: Option<Batch>,
    /// Reading files failed, so the next batch walks every source glob again.
    needs_rescan: bool,
    /// The latest rebuild failed, so the next batch rebuilds even if nothing changed.
    needs_rebuild: bool,
    /// The inputs the next watch summary reports as changed.
    changed_inputs: Vec<InputChange>,
    filesystem: mpsc::UnboundedReceiver<notify::Result<Event>>,
    _watcher: RecommendedWatcher,
}

impl BuildActor {
    /// Starts watching the project's sources, then builds it.
    pub(crate) async fn start(
        project: PreparedProject,
        config: WatchConfig,
    ) -> Result<BuildActor, WatchFailure> {
        let root = project.root_directory().to_path_buf();
        let (sender, filesystem) = mpsc::unbounded_channel();
        let handler = move |event| {
            let _ = sender.send(event);
        };
        let mut watcher = RecommendedWatcher::new(handler, notify::Config::default())?;
        for source_root in project.source_roots()? {
            watcher.watch(&persistent_watch_root(&source_root), RecursiveMode::Recursive)?;
        }

        let started = Instant::now();
        let session_config =
            BuildSessionConfig { color: config.color, diagnostics: config.diagnostics };
        let initialized = task::spawn_blocking(move || {
            let project = initialize_project(project)?;
            let mut session = BuildSession::new(project, session_config)?;
            let initial_inputs = session.take_initial_inputs();
            let changes = session.rescan()?;
            Ok::<_, WatchFailure>((session, initial_inputs, changes))
        });
        let (session, initial_inputs, changes) = join(initialized.await)?;
        report::warnings(&changes);
        let changed_inputs =
            if changes.inputs.is_empty() { initial_inputs } else { changes.inputs };

        let mut actor = BuildActor {
            config,
            root,
            session: Some(session),
            generation: 1,
            build: BuildState::NoInputs,
            batch: None,
            needs_rescan: false,
            needs_rebuild: false,
            changed_inputs,
            filesystem,
            _watcher: watcher,
        };
        actor.rebuild(true, started).await;
        Ok(actor)
    }

    /// Handles filesystem events and requests until the server stops sending requests.
    pub(crate) async fn run(
        mut self,
        mut requests: mpsc::UnboundedReceiver<QueryRequest>,
    ) -> Result<(), WatchFailure> {
        loop {
            let deadline = self.batch.as_ref().map(|batch| batch.deadline);
            tokio::select! {
                biased;
                Some(event) = self.filesystem.recv() => self.add_event(event)?,
                () = sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => {
                    self.synchronize(false).await?;
                }
                request = requests.recv() => {
                    let Some(request) = request else { return Ok(()) };
                    if request.query == Query::Wait {
                        self.synchronize(true).await?;
                        let answer = WaitAnswer { build: BuildState::clone(&self.build) };
                        let value = iris_watch_query::to_value(&answer);
                        let body = ResponseBody::Result { generation: self.generation, value };
                        let _ = request.reply.send(body);
                    } else {
                        self.synchronize(false).await?;
                        self.dispatch(request);
                    }
                }
            }
        }
    }

    fn add_event(&mut self, event: notify::Result<Event>) -> Result<(), WatchFailure> {
        let event = event?;
        if is_access(&event) {
            return Ok(());
        }
        let batch =
            self.batch.get_or_insert_with(|| Batch::new(Instant::now() + DEBOUNCE_DURATION));
        batch.add(event);
        Ok(())
    }

    /// Applies filesystem events that have arrived, and rebuilds if the inputs changed. With
    /// `everything`, every source glob is walked again, which also finds changes whose events
    /// have not arrived yet.
    async fn synchronize(&mut self, everything: bool) -> Result<(), WatchFailure> {
        while let Ok(event) = self.filesystem.try_recv() {
            self.add_event(event)?;
        }
        let batch = self.batch.take();
        let batch_rescan = batch.as_ref().is_some_and(|batch| batch.rescan);
        let paths = batch.map(|batch| batch.paths.into_iter().collect::<Vec<_>>());
        let paths = paths.unwrap_or_default();
        if !everything && !batch_rescan && paths.is_empty() {
            return Ok(());
        }

        let started = Instant::now();
        let rescan = everything || batch_rescan || self.needs_rescan;
        let changes =
            self.blocking(move |session| {
                if rescan { session.rescan() } else { session.synchronize_paths(&paths) }
            })
            .await;
        match changes {
            Ok(changes) => {
                self.needs_rescan = false;
                report::warnings(&changes);
                if !changes.is_empty() {
                    self.generation += 1;
                    self.changed_inputs = changes.inputs;
                    self.needs_rebuild = true;
                }
            }
            Err(error) => {
                self.needs_rescan = true;
                self.needs_rebuild = true;
                self.build = BuildState::Failed { message: error.to_string() };
                report::operational_failure(
                    &self.config,
                    &self.root,
                    &self.changed_inputs,
                    false,
                    started.elapsed(),
                    &error,
                );
                return Ok(());
            }
        }
        if self.needs_rebuild {
            self.rebuild(false, started).await;
        }
        Ok(())
    }

    async fn rebuild(&mut self, initial: bool, started: Instant) {
        let outcome = self.blocking(BuildSession::rebuild).await;
        let (build, summary) = match outcome {
            Ok(RebuildOutcome::Succeeded) => (BuildState::Succeeded, WatchOutcome::Succeeded),
            Ok(RebuildOutcome::Diagnostics) => (BuildState::Diagnostics, WatchOutcome::Diagnostics),
            Ok(RebuildOutcome::NoInputs) => (BuildState::NoInputs, WatchOutcome::Waiting),
            Err(error) => {
                report::operational_failure(
                    &self.config,
                    &self.root,
                    &self.changed_inputs,
                    initial,
                    started.elapsed(),
                    &error,
                );
                self.needs_rebuild = true;
                self.build = BuildState::Failed { message: error.to_string() };
                return;
            }
        };
        self.needs_rebuild = false;
        let duration = started.elapsed();
        report::summary(&self.config, &self.root, &self.changed_inputs, initial, duration, summary);
        self.build = build;
    }

    fn dispatch(&self, request: QueryRequest) {
        let QueryRequest { query, reply } = request;
        let session = self
            .session
            .as_ref()
            .expect("invariant violated: the session is owned by a blocking task");
        let snapshot = session.snapshot();
        let context = QueryContext {
            engine: snapshot.engine,
            files: snapshot.files,
            build: BuildState::clone(&self.build),
            root: PathBuf::clone(&self.root),
        };
        let generation = self.generation;
        task::spawn_blocking(move || {
            let answer = iris_watch_query::answer(&query, &context);
            // The snapshot must be dropped before answering, so that a change the client makes
            // next is not kept waiting for it.
            drop(context);
            let body = match answer {
                Ok(value) => ResponseBody::Result { generation, value },
                Err(QueryFailure::Cancelled) => ResponseBody::Cancelled,
                Err(QueryFailure::Failed(message)) => ResponseBody::Error { message },
            };
            let _ = reply.send(body);
        });
    }

    /// Runs `action` on a blocking thread that owns the session until it returns.
    async fn blocking<T: Send + 'static>(
        &mut self,
        action: impl FnOnce(&mut BuildSession) -> T + Send + 'static,
    ) -> T {
        let mut session = self
            .session
            .take()
            .expect("invariant violated: the session is owned by a blocking task");
        let result = task::spawn_blocking(move || {
            let result = action(&mut session);
            (session, result)
        });
        let (session, result) = join(result.await);
        self.session = Some(session);
        result
    }
}

fn join<T>(result: Result<T, task::JoinError>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic::resume_unwind(error.into_panic()),
    }
}
