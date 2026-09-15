use std::sync::Arc;

use analyzer::AnalyzerCapabilities;
use analyzer::diagnostics::CollectedDiagnostics;
use analyzer::position::PositionEncoding;
use async_lsp::ClientSocket;
use building::QueryCancellation;
use files::FileId;
use rustc_hash::FxHashMap;
use tokio::sync::mpsc;
use tokio::task::{JoinHandle, JoinSet};

use super::analysis::Analysis;
use super::event::{DiagnosticTicket, DiagnosticsFinished};

pub(super) enum DiagnosticEvent {
    Schedule { ticket: DiagnosticTicket },
    Invalidate { file_id: FileId, generation: u64 },
    Completed { ticket: DiagnosticTicket, collected: Option<CollectedDiagnostics> },
    Shutdown,
}

struct RunningDiagnostic {
    ticket: DiagnosticTicket,
    cancellation: QueryCancellation,
    queued: Option<DiagnosticTicket>,
}

#[derive(Default)]
struct Scheduler {
    generations: FxHashMap<FileId, u64>,
    running: FxHashMap<FileId, RunningDiagnostic>,
    stopped: bool,
}

impl Scheduler {
    fn schedule(&mut self, ticket: DiagnosticTicket) -> Option<QueryCancellation> {
        if self.stopped
            || ticket.generation
                < self.generations.get(&ticket.file_id).copied().unwrap_or_default()
        {
            return None;
        }
        if let Some(running) = self.running.get_mut(&ticket.file_id) {
            if ticket.sequence > running.ticket.sequence
                && running.queued.is_none_or(|queued| ticket.sequence > queued.sequence)
            {
                running.queued = Some(ticket);
                running.cancellation.cancel();
            }
            return None;
        }
        let cancellation = QueryCancellation::default();
        let running = RunningDiagnostic {
            ticket,
            cancellation: QueryCancellation::clone(&cancellation),
            queued: None,
        };
        self.running.insert(ticket.file_id, running);
        Some(cancellation)
    }

    fn invalidate(&mut self, file_id: FileId, generation: u64) {
        let current = self.generations.entry(file_id).or_default();
        *current = (*current).max(generation);
        if let Some(running) = self.running.get_mut(&file_id) {
            if running.ticket.generation < *current {
                running.cancellation.cancel();
            }
            if running.queued.is_some_and(|ticket| ticket.generation < *current) {
                running.queued = None;
            }
        }
    }

    fn complete(&mut self, ticket: DiagnosticTicket) -> (bool, Option<DiagnosticTicket>) {
        if self.running.get(&ticket.file_id).is_none_or(|running| running.ticket != ticket) {
            return (false, None);
        }
        let running = self.running.remove(&ticket.file_id).unwrap();
        let current = !self.stopped && running.cancellation.check().is_ok();
        (current, running.queued.filter(|_| !self.stopped))
    }

    fn shutdown(&mut self) {
        self.stopped = true;
        for running in self.running.values_mut() {
            running.cancellation.cancel();
            running.queued = None;
        }
    }
}

pub(super) struct DiagnosticWorker {
    pub(super) sender: mpsc::UnboundedSender<DiagnosticEvent>,
    task: Option<JoinHandle<()>>,
}

impl DiagnosticWorker {
    pub(super) fn start(
        analysis: Arc<Analysis>,
        encoding: PositionEncoding,
        capabilities: AnalyzerCapabilities,
        client: ClientSocket,
    ) -> DiagnosticWorker {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            let mut scheduler = Scheduler::default();
            let mut workers = JoinSet::new();
            let mut tickets = FxHashMap::default();
            loop {
                let event = tokio::select! {
                    event = receiver.recv() => event.unwrap_or(DiagnosticEvent::Shutdown),
                    completed = workers.join_next_with_id(), if !workers.is_empty() => {
                        let (identifier, collected) = match completed.unwrap() {
                            Ok((identifier, collected)) => (identifier, collected),
                            Err(error) => {
                                tracing::error!("Diagnostics worker failed: {error}");
                                (error.id(), None)
                            }
                        };
                        let ticket = tickets.remove(&identifier).expect("diagnostic worker has no ticket");
                        DiagnosticEvent::Completed { ticket, collected }
                    }
                };
                let next = match event {
                    DiagnosticEvent::Schedule { ticket } => Some(ticket),
                    DiagnosticEvent::Invalidate { file_id, generation } => {
                        scheduler.invalidate(file_id, generation);
                        None
                    }
                    DiagnosticEvent::Completed { ticket, collected } => {
                        let (current, next) = scheduler.complete(ticket);
                        if current {
                            let _ = client.emit(DiagnosticsFinished { ticket, collected });
                        }
                        next
                    }
                    DiagnosticEvent::Shutdown => {
                        scheduler.shutdown();
                        receiver.close();
                        while workers.join_next().await.is_some() {}
                        break;
                    }
                };
                if let Some(ticket) = next
                    && let Some(cancellation) = scheduler.schedule(ticket)
                {
                    let analysis = Arc::clone(&analysis);
                    let worker = workers.spawn_blocking(move || {
                        let snapshot = analysis
                            .snapshot(
                                encoding,
                                capabilities,
                                QueryCancellation::clone(&cancellation),
                            )
                            .ok()?;
                        let collected = snapshot
                            .with_analyzer_context(|context| {
                                analyzer::diagnostics::implementation(context, ticket.file_id)
                            })
                            .ok()?;
                        drop(snapshot);
                        cancellation.check().ok()?;
                        Some(collected)
                    });
                    tickets.insert(worker.id(), ticket);
                }
            }
        });
        DiagnosticWorker { sender, task: Some(task) }
    }

    pub(super) async fn shutdown(mut self) {
        let _ = self.sender.send(DiagnosticEvent::Shutdown);
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for DiagnosticWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(DiagnosticEvent::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use files::Files;

    use super::*;

    fn ticket(file_id: FileId, sequence: u64) -> DiagnosticTicket {
        DiagnosticTicket { file_id, generation: sequence, version: Some(3), sequence }
    }

    #[test]
    fn coalesces_latest_and_isolates_files() {
        let mut files = Files::default();
        let first = files.insert("First", "");
        let second = files.insert("Second", "");
        let mut scheduler = Scheduler::default();
        let cancellation = scheduler.schedule(ticket(first, 1)).unwrap();
        let independent = scheduler.schedule(ticket(second, 2)).unwrap();
        assert!(scheduler.schedule(ticket(first, 3)).is_none());
        assert!(scheduler.schedule(ticket(first, 4)).is_none());
        assert!(cancellation.check().is_err());
        assert!(independent.check().is_ok());
        assert_eq!(scheduler.complete(ticket(first, 3)), (false, None));
        assert_eq!(scheduler.complete(ticket(first, 1)), (false, Some(ticket(first, 4))));
        scheduler.schedule(ticket(first, 4)).unwrap();
        assert_eq!(scheduler.complete(ticket(first, 1)), (false, None));
        assert_eq!(scheduler.complete(ticket(first, 4)), (true, None));
        assert_eq!(scheduler.complete(ticket(second, 2)), (true, None));
        assert!(scheduler.running.is_empty());
    }

    #[test]
    fn invalidation_rejects_delayed_starts_and_shutdown_discards_work() {
        let file_id = Files::default().insert("Main", "");
        let mut scheduler = Scheduler::default();
        let cancellation = scheduler.schedule(ticket(file_id, 1)).unwrap();
        scheduler.schedule(ticket(file_id, 2));
        scheduler.invalidate(file_id, 3);
        assert!(cancellation.check().is_err());
        assert_eq!(scheduler.complete(ticket(file_id, 1)), (false, None));
        assert!(scheduler.schedule(ticket(file_id, 2)).is_none());
        let cancellation = scheduler.schedule(ticket(file_id, 3)).unwrap();
        scheduler.schedule(ticket(file_id, 4));
        scheduler.shutdown();
        assert!(cancellation.check().is_err());
        assert_eq!(scheduler.complete(ticket(file_id, 3)), (false, None));
        assert!(scheduler.schedule(ticket(file_id, 5)).is_none());
    }
}
