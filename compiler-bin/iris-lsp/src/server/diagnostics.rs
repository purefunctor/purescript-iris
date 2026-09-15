use std::collections::hash_map::Entry;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

use analyzer::diagnostics::CollectedDiagnostics;
use async_lsp::{ClientSocket, LanguageClient};
use files::FileId;
use lsp_types::PublishDiagnosticsParams;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use tokio::task;

use crate::server::State;
use crate::server::analysis::AnalysisSnapshot;
use crate::server::error::LspError;

#[derive(Default)]
struct DiagnosticScheduler {
    jobs: FxHashMap<FileId, DiagnosticJob>,
}

#[derive(Default)]
struct DiagnosticValidity {
    generations: FxHashMap<FileId, u64>,
}

struct DiagnosticJob {
    running: DiagnosticTicket,
    queued: Option<DiagnosticTicket>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DiagnosticTicket {
    pub(super) file_id: FileId,
    generation: u64,
    pub(super) version: Option<i32>,
}

impl DiagnosticScheduler {
    fn invalidate(&mut self, file_ids: impl IntoIterator<Item = FileId>) {
        for file_id in file_ids {
            if let Some(job) = self.jobs.get_mut(&file_id) {
                job.queued = None;
            }
        }
    }

    fn schedule(&mut self, ticket: DiagnosticTicket) -> Option<DiagnosticTicket> {
        match self.jobs.entry(ticket.file_id) {
            Entry::Vacant(entry) => {
                entry.insert(DiagnosticJob { running: ticket, queued: None });
                Some(ticket)
            }
            Entry::Occupied(mut entry) => {
                let job = entry.get_mut();
                if job.running != ticket && job.queued != Some(ticket) {
                    job.queued = Some(ticket);
                }
                None
            }
        }
    }

    fn complete(&mut self, ticket: DiagnosticTicket) -> Option<DiagnosticTicket> {
        let job = self.jobs.get_mut(&ticket.file_id)?;
        if job.running != ticket {
            return None;
        }
        let next = job.queued.take();
        if let Some(next) = next {
            job.running = next;
        } else {
            self.jobs.remove(&ticket.file_id);
        }
        next
    }
}

impl DiagnosticValidity {
    fn ticket(&self, file_id: FileId, version: Option<i32>) -> DiagnosticTicket {
        let generation = self.generations.get(&file_id).copied().unwrap_or_default();
        DiagnosticTicket { file_id, generation, version }
    }

    fn invalidate(&mut self, file_ids: &[FileId]) {
        for file_id in file_ids {
            let generation = self.generations.entry(*file_id).or_default();
            *generation = generation
                .checked_add(1)
                .expect("invariant violated: diagnostic generation overflowed");
        }
    }

    fn is_current(&self, ticket: DiagnosticTicket) -> bool {
        self.generations.get(&ticket.file_id).copied().unwrap_or_default() == ticket.generation
    }
}

enum Command {
    Schedule(DiagnosticTicket),
    Invalidate(Vec<FileId>),
    Completed { ticket: DiagnosticTicket, collected: Option<CollectedDiagnostics> },
    Shutdown,
}

#[derive(Clone)]
pub(super) struct DiagnosticsSender {
    sender: mpsc::Sender<Command>,
    active: Arc<AtomicBool>,
}

pub(super) struct Diagnostics {
    sender: DiagnosticsSender,
    validity: Arc<RwLock<DiagnosticValidity>>,
    actor: Option<thread::JoinHandle<()>>,
}

impl Diagnostics {
    pub(super) fn start(client: ClientSocket) -> Diagnostics {
        let (sender, receiver) = mpsc::channel();
        let active = Arc::new(AtomicBool::new(true));
        let actor = thread::Builder::new()
            .name("iris-diagnostics".into())
            .spawn(move || run(receiver, client))
            .expect("failed to start diagnostics actor");
        Diagnostics {
            sender: DiagnosticsSender { sender, active },
            validity: Arc::new(RwLock::new(DiagnosticValidity::default())),
            actor: Some(actor),
        }
    }

    pub(super) fn sender(&self) -> DiagnosticsSender {
        self.sender.clone()
    }

    pub(super) fn schedule(&self, file_id: FileId, version: Option<i32>) {
        let ticket = self.validity.read().ticket(file_id, version);
        self.sender.send(Command::Schedule(ticket));
    }

    pub(super) fn invalidate(&self, file_ids: Vec<FileId>) {
        self.validity.write().invalidate(&file_ids);
        self.sender.send(Command::Invalidate(file_ids));
    }

    pub(super) fn is_current(&self, ticket: DiagnosticTicket) -> bool {
        self.validity.read().is_current(ticket)
    }

    pub(super) fn shutdown(&mut self) {
        if !self.sender.active.swap(false, Ordering::AcqRel) {
            return;
        }
        let _ = self.sender.sender.send(Command::Shutdown);
        if let Some(actor) = self.actor.take() {
            let _ = actor.join();
        }
    }

    #[cfg(test)]
    pub(super) fn is_active(&self) -> bool {
        self.sender.active.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(super) fn ticket(&self, file_id: FileId, version: Option<i32>) -> DiagnosticTicket {
        self.validity.read().ticket(file_id, version)
    }
}

impl DiagnosticsSender {
    fn send(&self, command: Command) {
        if self.active.load(Ordering::Acquire) && self.sender.send(command).is_err() {
            tracing::warn!("Diagnostics actor stopped before accepting work");
        }
    }

    fn completed(&self, ticket: DiagnosticTicket, collected: Option<CollectedDiagnostics>) {
        self.send(Command::Completed { ticket, collected });
    }
}

impl Drop for Diagnostics {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(receiver: mpsc::Receiver<Command>, client: ClientSocket) {
    let mut scheduler = DiagnosticScheduler::default();
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Schedule(ticket) => {
                if let Some(ticket) = scheduler.schedule(ticket) {
                    let _ = client.emit(StartDiagnostics(ticket));
                }
            }
            Command::Invalidate(file_ids) => scheduler.invalidate(file_ids),
            Command::Completed { ticket, collected } => {
                let next = scheduler.complete(ticket);
                let _ = client.emit(DiagnosticsFinished { ticket, collected });
                if let Some(ticket) = next {
                    let _ = client.emit(StartDiagnostics(ticket));
                }
            }
            Command::Shutdown => break,
        }
    }
}

pub struct CollectDiagnostics(pub(super) FileId);

pub fn collect_diagnostics(
    state: &mut State,
    CollectDiagnostics(file_id): CollectDiagnostics,
) -> Result<(), LspError> {
    if let Some(version) = state.workspace.diagnostic_version(file_id)? {
        state.diagnostics.schedule(file_id, version);
    }
    Ok(())
}

pub struct StartDiagnostics(pub(super) DiagnosticTicket);

pub fn start_diagnostics(
    state: &mut State,
    StartDiagnostics(ticket): StartDiagnostics,
) -> Result<(), LspError> {
    if state.protocol.shutting_down
        || !state.diagnostics.is_current(ticket)
        || !state.workspace.diagnostic_current(ticket)?
    {
        state.diagnostics.sender().completed(ticket, None);
        return Ok(());
    }
    let worker = state.spawn(move |snapshot| {
        let _span = tracing::info_span!("collect_diagnostics").entered();
        collect_diagnostics_core(snapshot, ticket)
    })?;
    let diagnostics = state.diagnostics.sender();
    task::spawn(async move {
        let collected = await_diagnostics(worker).await;
        diagnostics.completed(ticket, collected);
    });
    Ok(())
}

fn collect_diagnostics_core(
    snapshot: AnalysisSnapshot,
    ticket: DiagnosticTicket,
) -> Option<CollectedDiagnostics> {
    let result = snapshot.diagnostics(ticket.file_id);
    match result {
        Ok(collected) => Some(collected),
        Err(error) => {
            LspError::from(error).emit_trace();
            None
        }
    }
}

async fn await_diagnostics(
    worker: task::JoinHandle<Option<CollectedDiagnostics>>,
) -> Option<CollectedDiagnostics> {
    match worker.await {
        Ok(collected) => collected,
        Err(error) => {
            LspError::JoinError(error).emit_trace();
            None
        }
    }
}

pub struct DiagnosticsFinished {
    ticket: DiagnosticTicket,
    collected: Option<CollectedDiagnostics>,
}

pub fn finish_diagnostics(
    state: &mut State,
    DiagnosticsFinished { ticket, collected }: DiagnosticsFinished,
) -> Result<(), LspError> {
    let current = !state.protocol.shutting_down
        && state.diagnostics.is_current(ticket)
        && state.workspace.diagnostic_current(ticket)?;
    if current && let Some(collected) = collected {
        let mut client = ClientSocket::clone(&state.client);
        client.publish_diagnostics(PublishDiagnosticsParams {
            uri: collected.uri,
            diagnostics: collected.diagnostics,
            version: ticket.version,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use files::Files;

    use super::{DiagnosticScheduler, DiagnosticValidity};

    fn file_id() -> files::FileId {
        let mut files = Files::default();
        files.insert("file:///src/Main.purs", "module Main where\n")
    }

    #[test]
    fn coalesces_requests_to_the_latest_generation() {
        let file_id = file_id();
        let mut validity = DiagnosticValidity::default();
        let mut scheduler = DiagnosticScheduler::default();
        let first = scheduler.schedule(validity.ticket(file_id, Some(1))).unwrap();

        validity.invalidate(&[file_id]);
        scheduler.invalidate([file_id]);
        assert_eq!(scheduler.schedule(validity.ticket(file_id, Some(2))), None);
        let queued = scheduler.jobs[&file_id].queued.unwrap();
        assert_eq!(queued.version, Some(2));

        assert_eq!(scheduler.complete(first), Some(queued));
    }

    #[test]
    fn invalidation_discards_a_queued_stale_request() {
        let file_id = file_id();
        let mut validity = DiagnosticValidity::default();
        let mut scheduler = DiagnosticScheduler::default();
        let first = scheduler.schedule(validity.ticket(file_id, Some(1))).unwrap();
        validity.invalidate(&[file_id]);
        scheduler.invalidate([file_id]);
        scheduler.schedule(validity.ticket(file_id, Some(2)));

        validity.invalidate(&[file_id]);
        scheduler.invalidate([file_id]);
        assert_eq!(scheduler.jobs[&file_id].queued, None);
        assert_eq!(scheduler.complete(first), None);
        assert!(!validity.is_current(first));
    }

    #[test]
    fn invalidating_one_file_does_not_invalidate_another() {
        let first_id = file_id();
        let second_id = files::FileId::new(first_id.into_raw() + 1);
        let mut validity = DiagnosticValidity::default();
        let first = validity.ticket(first_id, None);
        let second = validity.ticket(second_id, None);

        validity.invalidate(&[second_id]);

        assert!(validity.is_current(first));
        assert!(!validity.is_current(second));
    }
}
