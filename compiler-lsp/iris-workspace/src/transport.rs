use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, mpsc};
use std::task::{Context, Poll, Waker};
use std::thread;

use building::QueryCancellation;
use iris_build::analysis::CancellationToken;
use lsp_types::Url;
use parking_lot::Mutex;
use tokio::sync::oneshot;

use crate::controller::{Controller, Message};
use crate::events::EventSender;
use crate::{
    AnalysisStamp, Command, ConfigurationInput, EventReceiver, Generation, InputSequence, Options,
    Phase, RequestFailure, Status,
};

#[derive(Clone, Default)]
pub struct Cancellation {
    pub(crate) query: QueryCancellation,
    pub(crate) build: CancellationToken,
    terminal: Arc<Mutex<Terminal>>,
}

enum Terminal {
    Pending { waker: Option<Waker> },
    Cancelled,
    Settled,
}

impl Default for Terminal {
    fn default() -> Terminal {
        Terminal::Pending { waker: None }
    }
}

impl Cancellation {
    /// Cancel an unfinished request without waiting for its worker to retire. A settled reply
    /// is unchanged. Query interruption is cooperative; cancellation does not release snapshots.
    pub fn cancel(&self) {
        let mut terminal = self.terminal.lock();
        let Terminal::Pending { waker } = &mut *terminal else { return };
        let waker = waker.take();
        *terminal = Terminal::Cancelled;
        self.query.cancel();
        self.build.cancel();
        drop(terminal);
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.query.is_cancelled()
    }
}

// Cancellation does not free the worker slot; only its completion does.
pub(crate) enum WorkerState {
    Idle,
    Preparing { cancellation: Cancellation },
    Reconciling,
    Querying { cancellation: Cancellation },
    Stopping,
}

pub(crate) struct Admission {
    pub(crate) sequence: InputSequence,
    pub(crate) revision: InputSequence,
    pub(crate) generation: Generation,
    pub(crate) status: Status,
    pub(crate) requests: usize,
    pub(crate) worker: WorkerState,
    pub(crate) publications: BTreeMap<Url, u64>,
    pub(crate) status_revision: u64,
    configuration: Option<ConfigurationInput>,
}

impl Default for Admission {
    fn default() -> Admission {
        Admission {
            sequence: InputSequence::default(),
            revision: InputSequence::default(),
            generation: Generation::default(),
            status: Status::AwaitingConfiguration,
            requests: 0,
            worker: WorkerState::Idle,
            publications: BTreeMap::new(),
            status_revision: 0,
            configuration: None,
        }
    }
}

pub(crate) type Shared = Arc<Mutex<Admission>>;

#[derive(Clone)]
pub(crate) enum Fence {
    Analysis(AnalysisStamp),
    Publication { uri: Url, revision: u64, analysis: Option<AnalysisStamp> },
    Status(u64),
    Generation(Generation),
    Unconditional,
}

impl Fence {
    fn valid(&self, admission: &Admission) -> bool {
        match self {
            Fence::Analysis(stamp) => {
                matches!(admission.status, Status::Ready { stamp: current, .. } if current.incarnation == stamp.incarnation)
                    && admission.revision == stamp.revision
            }
            Fence::Publication { uri, revision, analysis } => {
                admission.publications.get(uri) == Some(revision)
                    && analysis.is_none_or(|stamp| Fence::Analysis(stamp).valid(admission))
            }
            Fence::Status(revision) => admission.status_revision == *revision,
            Fence::Generation(generation) => {
                admission.generation == *generation
                    && !matches!(admission.status, Status::Stopping | Status::Stopped)
            }
            Fence::Unconditional => true,
        }
    }
}

pub struct Delivery<T> {
    pub(crate) value: T,
    shared: Shared,
    fence: Fence,
}

impl<T> std::fmt::Debug for Delivery<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Delivery").finish_non_exhaustive()
    }
}

impl<T> Delivery<T> {
    pub(crate) fn new(value: T, shared: Shared, fence: Fence) -> Delivery<T> {
        Delivery { value, shared, fence }
    }

    /// Validate a publication in the service loop immediately before transport handoff.
    /// Keep the delivery guarded while forwarding it to that loop. Once released, later inputs
    /// do not recall the output; transport queueing and socket flush need no further check.
    pub fn release(self) -> Result<T, RequestFailure> {
        let admission = self.shared.lock();
        if !self.fence.valid(&admission) {
            return Err(RequestFailure::Stale);
        }
        Ok(self.value)
    }
}

struct ReplyAdmission {
    shared: Shared,
    stamp: AnalysisStamp,
}

impl Drop for ReplyAdmission {
    fn drop(&mut self) {
        self.shared.lock().requests -= 1;
    }
}

pub struct Reply<T> {
    sender: Option<oneshot::Sender<Result<T, RequestFailure>>>,
    pub(crate) cancellation: Cancellation,
    pub(crate) hooks: crate::testing::Hooks,
    admission: Option<ReplyAdmission>,
}

/// A reply settles once. Edits and cancellation do not revoke a settled result, even if unread.
pub struct Request<T> {
    receiver: Option<oneshot::Receiver<Result<T, RequestFailure>>>,
    cancellation: Cancellation,
    completed: bool,
}

impl<T> Reply<T> {
    pub fn channel() -> (Reply<T>, Request<T>) {
        let (sender, receiver) = oneshot::channel();
        let cancellation = Cancellation::default();
        (
            Reply {
                sender: Some(sender),
                cancellation: Cancellation::clone(&cancellation),
                admission: None,
                hooks: crate::testing::Hooks::default(),
            },
            Request { receiver: Some(receiver), cancellation, completed: false },
        )
    }

    pub(crate) fn admit(&mut self, shared: Shared, stamp: AnalysisStamp) {
        self.admission = Some(ReplyAdmission { shared, stamp });
    }

    pub(crate) fn stamp(&self) -> AnalysisStamp {
        self.admission.as_ref().expect("analysis reply must be admitted").stamp
    }

    pub(crate) fn reject(&mut self, failure: RequestFailure) {
        self.settle(Err(failure));
        self.admission = None;
    }

    pub(crate) fn finish(mut self, result: Result<T, RequestFailure>) {
        self.hooks.reach(crate::testing::Point::BeforeReplySettlement);
        let shared =
            Arc::clone(&self.admission.as_ref().expect("analysis reply must be admitted").shared);
        // Input admission takes this same lock, so an edit and reply settlement have one order.
        let admission = shared.lock();
        self.settle(result);
        drop(admission);
        self.hooks.reach(crate::testing::Point::AfterReplySettlement);
    }

    fn settle(&mut self, result: Result<T, RequestFailure>) {
        let mut terminal = self.cancellation.terminal.lock();
        let result = match *terminal {
            Terminal::Cancelled => Err(RequestFailure::Cancelled),
            Terminal::Pending { .. } => {
                *terminal = Terminal::Settled;
                result
            }
            Terminal::Settled => return,
        };
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(result);
        }
    }
}

impl<T> Request<T> {
    pub fn cancellation(&self) -> Cancellation {
        Cancellation::clone(&self.cancellation)
    }
}

impl<T> Future for Request<T> {
    type Output = Result<T, RequestFailure>;

    fn poll(mut self: Pin<&mut Request<T>>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let cancelled = {
            let mut terminal = self.cancellation.terminal.lock();
            match &mut *terminal {
                Terminal::Pending { waker } => {
                    *waker = Some(Waker::clone(context.waker()));
                    false
                }
                Terminal::Cancelled => true,
                Terminal::Settled => false,
            }
        };
        if cancelled {
            self.completed = true;
            return Poll::Ready(Err(RequestFailure::Cancelled));
        }
        let receiver = self.receiver.as_mut().expect("request polled after completion");
        match Pin::new(receiver).poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                self.completed = true;
                self.receiver = None;
                Poll::Ready(result.unwrap_or(Err(RequestFailure::Unavailable)))
            }
        }
    }
}

impl<T> Drop for Request<T> {
    fn drop(&mut self) {
        if !self.completed {
            self.cancellation.cancel();
        }
    }
}

#[derive(Clone)]
pub struct Workspace {
    sender: mpsc::Sender<Message>,
    shared: Shared,
    capacity: usize,
}

pub struct WorkspaceSession {
    pub workspace: Workspace,
    pub events: EventReceiver,
    pub join: WorkspaceJoin,
}

/// Owns controller-thread cleanup independently of cloneable command handles.
/// Dropping this owner requests shutdown but does not wait; call `join` to await cleanup.
pub struct WorkspaceJoin {
    workspace: Workspace,
    controller: Option<thread::JoinHandle<()>>,
}

impl Workspace {
    pub fn start(options: Options) -> std::io::Result<WorkspaceSession> {
        Workspace::start_inner(options, crate::testing::Hooks::default())
    }

    #[cfg(feature = "test-support")]
    pub fn start_with_hooks(
        options: Options,
        hooks: crate::testing::Hooks,
    ) -> std::io::Result<WorkspaceSession> {
        Workspace::start_inner(options, hooks)
    }

    fn start_inner(
        options: Options,
        hooks: crate::testing::Hooks,
    ) -> std::io::Result<WorkspaceSession> {
        let shared = Arc::new(Mutex::new(Admission::default()));
        let (sender, receiver) = mpsc::channel();
        let (events, event_receiver) = EventSender::channel();
        let controller =
            Controller::new(options, Arc::clone(&shared), sender.clone(), receiver, events, hooks)?;
        let controller =
            thread::Builder::new().name("iris-workspace".into()).spawn(move || controller.run())?;
        let workspace = Workspace { sender, shared, capacity: options.request_capacity };
        let join =
            WorkspaceJoin { workspace: Workspace::clone(&workspace), controller: Some(controller) };
        Ok(WorkspaceSession { workspace, events: event_receiver, join })
    }

    pub fn status(&self) -> Status {
        Status::clone(&self.shared.lock().status)
    }

    /// Admit input without waiting for analysis. Potential writes cancel executing reads and
    /// discard queued reads; requests are never replayed automatically. Document saves and
    /// inputs later rejected by document validation conservatively count as potential writes.
    ///
    /// Handle `Busy` as a request failure rather than pausing the protocol input loop: edits
    /// must remain admissible while the request capacity is exhausted.
    pub fn send(&self, mut command: Command) -> Result<InputSequence, RequestFailure> {
        if let Command::FilesChanged(uris) = &command {
            for uri in uris {
                if iris_build::analysis::file_path(uri).is_none() {
                    return Err(crate::InputFailure::UnsupportedDocument(Url::clone(uri)).into());
                }
            }
        }
        if let Command::LanguageServer(request) = &mut command {
            if let Err(failure) = request.validate() {
                request.reject(RequestFailure::InvalidInput(crate::InputFailure::clone(&failure)));
                return Err(failure.into());
            }
        }
        let mut admission = self.shared.lock();
        if matches!(admission.status, Status::Stopping | Status::Stopped) {
            return Err(RequestFailure::Unavailable);
        }
        if let Command::LanguageServer(request) = &mut command {
            let Status::Ready { mut stamp, .. } = admission.status else {
                drop(admission);
                request.reject(RequestFailure::Unavailable);
                return Err(RequestFailure::Unavailable);
            };
            if admission.requests >= self.capacity {
                drop(admission);
                request.reject(RequestFailure::Busy);
                return Err(RequestFailure::Busy);
            }
            stamp.revision = admission.revision;
            admission.requests += 1;
            request.admit(Arc::clone(&self.shared), stamp);
        } else {
            admission.sequence.advance();
            let rebuilds = match &command {
                Command::Configure(configuration) => {
                    let unchanged_sources =
                        admission.configuration.as_ref().is_some_and(|previous| {
                            previous.root == configuration.root
                                && previous.settings.sources == configuration.settings.sources
                        });
                    let rebuilds =
                        !unchanged_sources || !matches!(admission.status, Status::Ready { .. });
                    admission.configuration = Some(ConfigurationInput::clone(configuration));
                    rebuilds
                }
                _ => command.rebuilds(),
            };
            let invalidates = !matches!(&command, Command::Configure(_)) || rebuilds;
            if invalidates {
                admission.revision = admission.sequence;
                if let WorkerState::Querying { cancellation } = &admission.worker {
                    cancellation.cancel();
                }
            }
            match &command {
                _ if rebuilds => {
                    admission.generation.advance();
                    if let WorkerState::Preparing { cancellation } = &admission.worker {
                        cancellation.cancel();
                    }
                    admission.status = Status::Rebuilding {
                        generation: admission.generation,
                        phase: Phase::Retiring,
                    };
                    admission.status_revision += 1;
                }
                Command::Shutdown => {
                    if let WorkerState::Preparing { cancellation } = &admission.worker {
                        cancellation.cancel();
                    }
                    admission.status = Status::Stopping;
                    admission.status_revision += 1;
                }
                _ => {}
            }
        }
        let sequence = admission.sequence;
        let revision = admission.revision;
        let generation = admission.generation;
        let message = Message::Command { sequence, revision, generation, command };
        // Keep admission locked through enqueue, so readiness cannot overtake this input.
        let result = self.sender.send(message);
        drop(admission);
        result.map_err(|_| RequestFailure::Unavailable)?;
        Ok(sequence)
    }
}

impl WorkspaceJoin {
    /// Request shutdown and wait for the controller, worker, and process cleanup. Call on a
    /// blocking thread, not a protocol loop. `Status::Stopped` alone does not join the controller.
    pub fn join(mut self) -> thread::Result<()> {
        let _ = self.workspace.send(Command::Shutdown);
        self.controller.take().expect("workspace controller missing").join()
    }
}

impl Drop for WorkspaceJoin {
    fn drop(&mut self) {
        let _ = self.workspace.send(Command::Shutdown);
    }
}
