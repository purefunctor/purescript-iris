use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, mpsc};
use std::thread;

use iris_build::events::BuildEvent;
use lsp_types::Url;

use crate::documents::Documents;
use crate::events::EventSender;
use crate::transport::{Fence, Shared, WorkerState};
use crate::worker::{self, Completed, Work};
use crate::{
    AnalysisStamp, Cancellation, Command, ConfigurationInput, Delivery, Document, Event,
    Generation, Incarnation, InputSequence, LanguageServer, Options, Outcome, Phase,
    RequestFailure, Status,
};

const MESSAGE_BATCH_SIZE: usize = 64;

pub(crate) enum Message {
    Command { sequence: InputSequence, generation: Generation, command: Command },
    Completed(Completed),
    Progress { generation: Generation, event: BuildEvent },
}

// Desired lifecycle is independent of outstanding work: a superseded query may still be running.
#[derive(Clone, Copy)]
enum Lifecycle {
    AwaitingConfiguration,
    PreparationPending,
    Preparing,
    CatchingUp { stamp: AnalysisStamp },
    Active { stamp: AnalysisStamp },
    Failed,
    Stopping,
}

impl Lifecycle {
    fn has_attempt(self) -> bool {
        matches!(self, Lifecycle::Preparing | Lifecycle::CatchingUp { .. })
    }

    fn accepts_completion(self) -> bool {
        matches!(
            self,
            Lifecycle::Preparing | Lifecycle::CatchingUp { .. } | Lifecycle::Active { .. }
        )
    }
}

pub(crate) struct Controller {
    options: Options,
    shared: Shared,
    receiver: mpsc::Receiver<Message>,
    events: EventSender,
    worker: mpsc::Sender<Work>,
    worker_thread: Option<thread::JoinHandle<()>>,
    hooks: crate::testing::Hooks,
    documents: Documents,
    configuration: Option<ConfigurationInput>,
    generation: Generation,
    sequence: InputSequence,
    incarnation: Incarnation,
    lifecycle: Lifecycle,
    dirty: BTreeSet<Url>,
    requests: VecDeque<LanguageServer>,
    diagnostics: BTreeSet<Url>,
    sources: BTreeSet<Url>,
    request_diagnostics: bool,
}

impl Controller {
    pub(crate) fn new(
        options: Options,
        shared: Shared,
        sender: mpsc::Sender<Message>,
        receiver: mpsc::Receiver<Message>,
        events: EventSender,
        hooks: crate::testing::Hooks,
    ) -> std::io::Result<Controller> {
        let (worker, work) = mpsc::channel();
        let worker_hooks = crate::testing::Hooks::clone(&hooks);
        let worker_thread = thread::Builder::new()
            .name("iris-compilation".into())
            .spawn(move || worker::run(work, sender, options, worker_hooks))?;
        Ok(Controller {
            options,
            shared,
            receiver,
            events,
            worker,
            worker_thread: Some(worker_thread),
            hooks,
            documents: Documents::default(),
            configuration: None,
            generation: Generation::default(),
            sequence: InputSequence::default(),
            incarnation: Incarnation::default(),
            lifecycle: Lifecycle::AwaitingConfiguration,
            dirty: BTreeSet::new(),
            requests: VecDeque::new(),
            diagnostics: BTreeSet::new(),
            sources: BTreeSet::new(),
            request_diagnostics: false,
        })
    }

    pub(crate) fn run(mut self) {
        self.emit(Event::StatusChanged(Status::AwaitingConfiguration), Fence::Status(0));
        while let Ok(message) = self.receiver.recv() {
            if !self.handle(message) {
                break;
            }
            // Coalesce queued inputs without indefinitely postponing scheduling. The first
            // message already counts toward the batch; its size is only a throughput choice.
            for _ in 1..MESSAGE_BATCH_SIZE {
                if let Ok(message) = self.receiver.try_recv() {
                    if !self.handle(message) {
                        return;
                    }
                } else {
                    break;
                }
            }
            self.schedule();
        }
        if let Some(worker) = self.worker_thread.take() {
            let _ = worker.join();
        }
    }

    fn emit(&self, event: Event, fence: Fence) {
        self.events.send(Delivery::new(event, Arc::clone(&self.shared), fence));
    }

    fn status(&self, status: Status) {
        let mut admission = self.shared.lock();
        if admission.generation != self.generation && !matches!(status, Status::Stopped) {
            return;
        }
        if matches!(admission.status, Status::Stopping | Status::Stopped)
            && !matches!(status, Status::Stopping | Status::Stopped)
        {
            return;
        }
        admission.status = Status::clone(&status);
        admission.status_revision += 1;
        let revision = admission.status_revision;
        drop(admission);
        self.emit(Event::StatusChanged(status), Fence::Status(revision));
    }

    fn finish_attempt(&mut self, outcome: Outcome) {
        if self.lifecycle.has_attempt() {
            let generation = self.generation;
            self.emit(Event::Finished { generation, outcome }, Fence::Unconditional);
        }
    }

    fn clear_diagnostics(&mut self) {
        self.diagnostics.clear();
        let mut admission = self.shared.lock();
        let clears = admission.publications.iter_mut().map(|(uri, revision)| {
            *revision += 1;
            let event =
                Event::Diagnostics { uri: Url::clone(uri), version: None, diagnostics: vec![] };
            let fence =
                Fence::Publication { uri: Url::clone(uri), revision: *revision, analysis: None };
            Delivery::new(event, Arc::clone(&self.shared), fence)
        });

        let clears = clears.collect::<Vec<_>>();
        drop(admission);

        for clear in clears {
            self.events.send(clear);
        }
    }

    fn reject_requests(&mut self, failure: RequestFailure) {
        for mut request in self.requests.drain(..) {
            request.reject(RequestFailure::clone(&failure));
        }
    }

    fn handle(&mut self, message: Message) -> bool {
        match message {
            Message::Command { sequence, generation, command } => match command {
                Command::LanguageServer(mut request) => match self.lifecycle {
                    Lifecycle::Active { .. } => self.requests.push_back(request),
                    Lifecycle::Stopping => request.reject(RequestFailure::Cancelled),
                    _ => request.reject(RequestFailure::Unavailable),
                },
                Command::Document(document) => {
                    self.sequence = sequence;
                    let triggers = self
                        .configuration
                        .as_ref()
                        .map(|configuration| &configuration.settings.diagnostics);
                    let collect = match (&document, triggers) {
                        (Document::Open { .. }, Some(triggers)) => triggers.on_open,
                        (Document::Change { .. }, Some(triggers)) => triggers.on_change,
                        (Document::Save(_), Some(triggers)) => triggers.on_save,
                        (Document::Close(_), _) => true,
                        _ => false,
                    };
                    match self.documents.apply(document, self.options.position_encoding) {
                        Ok(uri) => {
                            self.dirty.insert(uri);
                            self.request_diagnostics |= collect;
                        }
                        Err(failure) => self
                            .emit(Event::InputRejected { sequence, failure }, Fence::Unconditional),
                    }
                }
                Command::Configure(configuration) => {
                    self.configuration = Some(configuration);
                    self.rebuild(sequence, generation);
                }
                Command::FilesChanged(uris) if generation == self.generation => {
                    self.sequence = sequence;
                    self.dirty.extend(uris);
                    self.request_diagnostics = true;
                }
                Command::Reload | Command::FilesChanged(_) => self.rebuild(sequence, generation),
                Command::Shutdown => {
                    self.sequence = sequence;
                    self.reject_requests(RequestFailure::Cancelled);
                    self.finish_attempt(Outcome::Cancelled);
                    self.lifecycle = Lifecycle::Stopping;
                    self.clear_diagnostics();
                    self.status(Status::Stopping);
                }
            },
            Message::Progress { generation, event } => {
                if generation == self.generation {
                    match event {
                        BuildEvent::PlanReady { .. } => {
                            self.status(Status::Rebuilding { generation, phase: Phase::Building })
                        }
                        BuildEvent::Finished { .. } => self
                            .status(Status::Rebuilding { generation, phase: Phase::Reconciling }),
                        _ => {}
                    }
                    self.emit(Event::Progress { generation, event }, Fence::Generation(generation));
                }
            }
            Message::Completed(completed) => {
                {
                    let mut admission = self.shared.lock();
                    admission.worker = WorkerState::Idle;
                }
                match completed {
                    Completed::Reconciled { generation, stamp, sources } => {
                        if generation == self.generation && self.lifecycle.accepts_completion() {
                            let rebuilding = self.lifecycle.has_attempt();
                            self.lifecycle = if rebuilding {
                                Lifecycle::CatchingUp { stamp }
                            } else {
                                Lifecycle::Active { stamp }
                            };
                            let sources = sources.into_iter().collect::<BTreeSet<_>>();
                            let removed =
                                self.sources.difference(&sources).cloned().collect::<Vec<_>>();
                            for uri in removed {
                                self.diagnostics.insert(uri);
                            }
                            self.sources = sources;
                            if self.request_diagnostics || rebuilding {
                                self.diagnostics.extend(self.sources.iter().cloned());
                                self.request_diagnostics = false;
                            }
                        }
                    }
                    Completed::Analyzed => {}
                    Completed::Diagnostics { uri, stamp, version, result } => {
                        let mut admission = self.shared.lock();
                        let current = admission.sequence == stamp.revision
                            && matches!(admission.status, Status::Ready { stamp: current, .. } if current == stamp);
                        if current {
                            if let Ok(diagnostics) = result {
                                let revision =
                                    admission.publications.entry(Url::clone(&uri)).or_default();
                                *revision += 1;
                                let revision = *revision;
                                drop(admission);
                                self.emit(
                                    Event::Diagnostics {
                                        uri: Url::clone(&uri),
                                        version,
                                        diagnostics,
                                    },
                                    Fence::Publication { uri, revision, analysis: Some(stamp) },
                                );
                            }
                        } else if self.lifecycle.accepts_completion() {
                            self.diagnostics.insert(uri);
                        }
                    }
                    Completed::Failed { generation, failure } => {
                        self.hooks.reach(crate::testing::Point::BeforeFailure);
                        if generation == self.generation && self.lifecycle.accepts_completion() {
                            self.clear_diagnostics();
                            self.reject_requests(RequestFailure::Unavailable);
                            self.finish_attempt(Outcome::Failed);
                            self.lifecycle = Lifecycle::Failed;
                            self.status(Status::Failed {
                                generation,
                                message: failure.to_string().into(),
                            });
                        }
                    }
                    Completed::Stopped => {
                        self.status(Status::Stopped);
                        if let Some(worker) = self.worker_thread.take() {
                            let _ = worker.join();
                        }
                        return false;
                    }
                }
            }
        }
        true
    }

    fn rebuild(&mut self, sequence: InputSequence, generation: Generation) {
        self.finish_attempt(Outcome::Superseded);
        self.generation = generation;
        self.sequence = sequence;
        self.lifecycle = Lifecycle::PreparationPending;
        self.reject_requests(RequestFailure::Unavailable);
        self.clear_diagnostics();
        self.status(Status::Rebuilding { generation, phase: Phase::Retiring });
    }

    fn schedule(&mut self) {
        let mut admission = self.shared.lock();
        if !matches!(admission.worker, WorkerState::Idle) {
            return;
        }
        if matches!(self.lifecycle, Lifecycle::Stopping) {
            admission.worker = WorkerState::Stopping;
            let _ = self.worker.send(Work::Stop);
            return;
        }
        if admission.sequence != self.sequence || admission.generation != self.generation {
            return;
        }
        if matches!(self.lifecycle, Lifecycle::PreparationPending) {
            let Some(configuration) = self.configuration.clone() else {
                self.lifecycle = Lifecycle::AwaitingConfiguration;
                drop(admission);
                self.status(Status::AwaitingConfiguration);
                return;
            };
            self.incarnation.advance();
            let stamp = AnalysisStamp { incarnation: self.incarnation, revision: self.sequence };
            let cancellation = Cancellation::default();
            admission.worker =
                WorkerState::Preparing { cancellation: Cancellation::clone(&cancellation) };
            self.lifecycle = Lifecycle::Preparing;
            self.dirty.clear();
            let work = Work::Prepare {
                configuration,
                documents: self.documents.open.clone(),
                generation: self.generation,
                stamp,
                cancellation,
            };
            let _ = self.worker.send(work);
            drop(admission);
            self.status(Status::Rebuilding {
                generation: self.generation,
                phase: Phase::Discovering,
            });
            return;
        }
        let (Lifecycle::CatchingUp { stamp } | Lifecycle::Active { stamp }) = self.lifecycle else {
            return;
        };
        if stamp.revision != self.sequence {
            let stamp = AnalysisStamp { revision: self.sequence, ..stamp };
            let work = Work::Reconcile {
                documents: self.documents.open.clone(),
                dirty: std::mem::take(&mut self.dirty),
                stamp,
            };
            admission.worker = WorkerState::Reconciling;
            let _ = self.worker.send(work);
            drop(admission);
            if self.lifecycle.has_attempt() {
                self.status(Status::Rebuilding {
                    generation: self.generation,
                    phase: Phase::Reconciling,
                });
            }
            return;
        }
        let status = Status::Ready { generation: self.generation, stamp };
        if admission.status != status {
            admission.status = Status::clone(&status);
            admission.status_revision += 1;
            let revision = admission.status_revision;
            self.emit(Event::StatusChanged(status), Fence::Status(revision));
        }
        drop(admission);
        self.finish_attempt(Outcome::Ready);
        self.lifecycle = Lifecycle::Active { stamp };
        while let Some(mut request) = self.requests.pop_front() {
            if request.cancellation().is_cancelled() {
                request.reject(RequestFailure::Cancelled);
                continue;
            }
            if request.stamp() != stamp {
                request.reject(RequestFailure::Stale);
                continue;
            }
            let mut admission = self.shared.lock();
            if admission.sequence != stamp.revision || admission.generation != self.generation {
                drop(admission);
                request.reject(RequestFailure::Stale);
                continue;
            }
            admission.worker = WorkerState::Querying { cancellation: request.cancellation() };
            let result = self.worker.send(Work::Analyze(request));
            drop(admission);
            drop(result);
            return;
        }
        if let Some(uri) = self.diagnostics.pop_first() {
            let mut admission = self.shared.lock();
            if admission.sequence != stamp.revision || admission.generation != self.generation {
                self.diagnostics.insert(uri);
                return;
            }
            let cancellation = Cancellation::default();
            admission.worker =
                WorkerState::Querying { cancellation: Cancellation::clone(&cancellation) };
            let _ = self.worker.send(Work::Diagnostics { uri, stamp, cancellation });
        }
    }
}
