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
    AnalysisStamp, Command, EventReceiver, Generation, InputSequence, Options, Phase,
    RequestFailure, Status,
};

#[derive(Clone, Default)]
pub struct Cancellation {
    pub(crate) query: QueryCancellation,
    pub(crate) build: CancellationToken,
    wake: Arc<Mutex<Option<Waker>>>,
}

impl Cancellation {
    pub fn cancel(&self) {
        self.query.cancel();
        self.build.cancel();
        if let Some(waker) = self.wake.lock().take() {
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
    pub(crate) generation: Generation,
    pub(crate) status: Status,
    pub(crate) requests: usize,
    pub(crate) worker: WorkerState,
    pub(crate) publications: BTreeMap<Url, u64>,
    pub(crate) status_revision: u64,
}

impl Default for Admission {
    fn default() -> Admission {
        Admission {
            sequence: InputSequence::default(),
            generation: Generation::default(),
            status: Status::AwaitingConfiguration,
            requests: 0,
            worker: WorkerState::Idle,
            publications: BTreeMap::new(),
            status_revision: 0,
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
                    && admission.sequence == stamp.revision
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
    cancellation: Option<Cancellation>,
}

impl<T> std::fmt::Debug for Delivery<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Delivery").finish_non_exhaustive()
    }
}

impl<T> Delivery<T> {
    pub(crate) fn new(value: T, shared: Shared, fence: Fence) -> Delivery<T> {
        Delivery { value, shared, fence, cancellation: None }
    }

    /// Validate at ordered commitment to a reserved final-writer slot, not socket flush.
    /// The adapter must serialize release and commitment with input admission. Do not release
    /// before router serialization, a forwarding queue, or another await.
    ///
    /// Stock async-lsp 0.2.4 does not expose deferred output with writer reservation. An adapter
    /// needs that support before integrating this boundary; `ClientSocket::emit` is too early.
    /// The runnable example demonstrates workspace behavior, not transport integration.
    pub fn release(self) -> Result<T, RequestFailure> {
        let admission = self.shared.lock();
        if self.cancellation.as_ref().is_some_and(Cancellation::is_cancelled) {
            return Err(RequestFailure::Cancelled);
        }
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
    sender: Option<oneshot::Sender<Result<Delivery<Result<T, RequestFailure>>, RequestFailure>>>,
    pub(crate) cancellation: Cancellation,
    admission: Option<ReplyAdmission>,
}

/// Admission and cancellation failures are outer errors. Computed successes and failures both
/// remain guarded until the caller releases the delivery at the publication boundary.
pub struct Request<T> {
    receiver:
        Option<oneshot::Receiver<Result<Delivery<Result<T, RequestFailure>>, RequestFailure>>>,
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
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Err(failure));
        }
        self.admission = None;
    }

    pub(crate) fn finish(mut self, result: Result<T, RequestFailure>) {
        let result = if self.cancellation.is_cancelled() {
            Err(RequestFailure::Cancelled)
        } else {
            let admission = self.admission.as_ref().expect("analysis reply must be admitted");
            let mut delivery = Delivery::new(
                result,
                Arc::clone(&admission.shared),
                Fence::Analysis(admission.stamp),
            );
            delivery.cancellation = Some(Cancellation::clone(&self.cancellation));
            Ok(delivery)
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
    type Output = Result<Delivery<Result<T, RequestFailure>>, RequestFailure>;

    fn poll(mut self: Pin<&mut Request<T>>, context: &mut Context<'_>) -> Poll<Self::Output> {
        *self.cancellation.wake.lock() = Some(Waker::clone(context.waker()));
        if self.cancellation.is_cancelled() {
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

    pub fn send(&self, mut command: Command) -> Result<InputSequence, RequestFailure> {
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
            stamp.revision = admission.sequence;
            admission.requests += 1;
            request.admit(Arc::clone(&self.shared), stamp);
        } else {
            admission.sequence.advance();
            if let WorkerState::Querying { cancellation } = &admission.worker {
                cancellation.cancel();
            }
            match &command {
                command if command.rebuilds() => {
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
        let generation = admission.generation;
        let message = Message::Command { sequence, generation, command };
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
