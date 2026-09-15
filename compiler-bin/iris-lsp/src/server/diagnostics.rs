use std::collections::hash_map::Entry;
use std::sync::Arc;

use analyzer::AnalyzerCapabilities;
use analyzer::diagnostics::CollectedDiagnostics;
use analyzer::position::PositionEncoding;
use async_lsp::ClientSocket;
use building::Cancellation;
use files::FileId;
use rustc_hash::FxHashMap;
use tokio::sync::mpsc;
use tokio::task::{self, JoinSet};

use super::analysis::Analysis;
use super::error::LspError;
use super::event::{DiagnosticTicket, DiagnosticsFinished};

pub(super) enum DiagnosticWorkerEvent {
    Schedule {
        ticket: DiagnosticTicket,
        analysis: Arc<Analysis>,
        position_encoding: PositionEncoding,
        analyzer_capabilities: AnalyzerCapabilities,
    },
    Invalidate {
        file_id: FileId,
    },
    Completed {
        ticket: DiagnosticTicket,
        collected: Option<CollectedDiagnostics>,
    },
    Shutdown,
}

pub(super) struct DiagnosticActor {
    sender: mpsc::UnboundedSender<DiagnosticWorkerEvent>,
    monitor: task::JoinHandle<()>,
}

impl DiagnosticActor {
    pub(super) fn spawn(client: ClientSocket) -> DiagnosticActor {
        let (sender, receiver) = mpsc::unbounded_channel();
        let actor_sender = mpsc::UnboundedSender::clone(&sender);
        let monitor = task::spawn(run_actor(client, actor_sender, receiver));
        DiagnosticActor { sender, monitor }
    }

    pub(super) fn send(&self, event: DiagnosticWorkerEvent) {
        let _ = self.sender.send(event);
    }

    pub(super) async fn shutdown(self) {
        let _ = self.sender.send(DiagnosticWorkerEvent::Shutdown);
        let _ = self.monitor.await;
    }
}

struct PendingDiagnostic {
    ticket: DiagnosticTicket,
    analysis: Arc<Analysis>,
    position_encoding: PositionEncoding,
    analyzer_capabilities: AnalyzerCapabilities,
}

#[derive(Default)]
struct Scheduler {
    running: FxHashMap<FileId, RunningDiagnostic>,
    queued: FxHashMap<FileId, PendingDiagnostic>,
    shutdown: bool,
}

struct RunningDiagnostic {
    ticket: DiagnosticTicket,
    cancellation: Cancellation,
}

enum Transition {
    None,
    Start { pending: PendingDiagnostic, cancellation: Cancellation },
    Finish { ticket: DiagnosticTicket, collected: Option<CollectedDiagnostics> },
}

impl Scheduler {
    fn apply(&mut self, event: DiagnosticWorkerEvent) -> Transition {
        match event {
            DiagnosticWorkerEvent::Schedule {
                ticket,
                analysis,
                position_encoding,
                analyzer_capabilities,
            } => {
                if self.shutdown {
                    return Transition::None;
                }
                let pending = PendingDiagnostic {
                    ticket,
                    analysis,
                    position_encoding,
                    analyzer_capabilities,
                };
                if let Entry::Vacant(entry) = self.running.entry(ticket.file_id) {
                    let cancellation = Cancellation::new();
                    let running = RunningDiagnostic {
                        ticket,
                        cancellation: Cancellation::clone(&cancellation),
                    };
                    entry.insert(running);
                    Transition::Start { pending, cancellation }
                } else {
                    self.queued.insert(ticket.file_id, pending);
                    Transition::None
                }
            }
            DiagnosticWorkerEvent::Invalidate { file_id } => {
                self.queued.remove(&file_id);
                if let Some(running) = self.running.get(&file_id) {
                    running.cancellation.cancel();
                }
                Transition::None
            }
            DiagnosticWorkerEvent::Completed { ticket, collected } => {
                if !self
                    .running
                    .get(&ticket.file_id)
                    .is_some_and(|running| running.ticket == ticket)
                {
                    return Transition::None;
                }
                self.running.remove(&ticket.file_id);
                if self.shutdown {
                    return Transition::None;
                }
                Transition::Finish { ticket, collected }
            }
            DiagnosticWorkerEvent::Shutdown => {
                self.shutdown = true;
                self.queued.clear();
                for running in self.running.values() {
                    running.cancellation.cancel();
                }
                Transition::None
            }
        }
    }

    fn start_next(&mut self, file_id: FileId) -> Transition {
        if self.shutdown || self.running.contains_key(&file_id) {
            return Transition::None;
        }
        if let Some(pending) = self.queued.remove(&file_id) {
            let cancellation = Cancellation::new();
            let running = RunningDiagnostic {
                ticket: pending.ticket,
                cancellation: Cancellation::clone(&cancellation),
            };
            self.running.insert(file_id, running);
            return Transition::Start { pending, cancellation };
        }
        Transition::None
    }
}

async fn run_actor(
    client: ClientSocket,
    sender: mpsc::UnboundedSender<DiagnosticWorkerEvent>,
    mut receiver: mpsc::UnboundedReceiver<DiagnosticWorkerEvent>,
) {
    let mut scheduler = Scheduler::default();
    let mut monitors = JoinSet::new();
    while let Some(event) = receiver.recv().await {
        let shutdown = matches!(event, DiagnosticWorkerEvent::Shutdown);
        let completed = match &event {
            DiagnosticWorkerEvent::Completed { ticket, .. } => Some(ticket.file_id),
            _ => None,
        };
        execute(scheduler.apply(event), &client, &sender, &mut monitors);
        if let Some(file_id) = completed {
            execute(scheduler.start_next(file_id), &client, &sender, &mut monitors);
        }
        while monitors.try_join_next().is_some() {}
        if shutdown {
            break;
        }
    }
    while monitors.join_next().await.is_some() {}
}

fn execute(
    transition: Transition,
    client: &ClientSocket,
    sender: &mpsc::UnboundedSender<DiagnosticWorkerEvent>,
    monitors: &mut JoinSet<()>,
) {
    match transition {
        Transition::None => {}
        Transition::Start { pending, cancellation } => {
            let ticket = pending.ticket;
            if cancellation.check().is_err() {
                let _ = sender.send(DiagnosticWorkerEvent::Completed { ticket, collected: None });
                return;
            }
            // Queued scheduler tickets own no snapshot. Admission happens only
            // once the actor grants this file's running slot, before pool work.
            let snapshot = pending.analysis.snapshot(
                pending.position_encoding,
                pending.analyzer_capabilities,
                Cancellation::clone(&cancellation),
            );
            let worker = task::spawn_blocking(move || {
                let result = snapshot.with_analyzer_context(|context| {
                    analyzer::diagnostics::implementation(context, ticket.file_id)
                });
                drop(snapshot);
                if cancellation.check().is_err() {
                    return None;
                }
                match result {
                    Ok(collected) => Some(collected),
                    Err(error) => {
                        LspError::from(error).emit_trace();
                        None
                    }
                }
            });
            let sender = mpsc::UnboundedSender::clone(sender);
            monitors.spawn(async move {
                let collected = match worker.await {
                    Ok(collected) => collected,
                    Err(error) => {
                        LspError::JoinError(error).emit_trace();
                        None
                    }
                };
                let _ = sender.send(DiagnosticWorkerEvent::Completed { ticket, collected });
            });
        }
        Transition::Finish { ticket, collected } => {
            let event = DiagnosticsFinished { ticket, collected };
            if let Err(error) = client.emit(event) {
                LspError::from(error).emit_trace();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use analyzer::AnalyzerCapabilities;
    use analyzer::position::PositionEncoding;
    use building::QueryEngine;
    use building::lifecycle::FileLifecycle;
    use files::{FileId, Files};

    use super::*;
    use crate::server::SourceMetadata;

    fn file_id(uri: &str) -> FileId {
        let mut files = Files::default();
        files.insert(uri, "module Main where\n")
    }

    fn pending(ticket: DiagnosticTicket) -> DiagnosticWorkerEvent {
        DiagnosticWorkerEvent::Schedule {
            ticket,
            analysis: Arc::new(Analysis::new(
                QueryEngine::default(),
                FileLifecycle::<i32, SourceMetadata>::default(),
            )),
            position_encoding: PositionEncoding::Utf16,
            analyzer_capabilities: AnalyzerCapabilities::default(),
        }
    }

    fn ticket(file_id: FileId, job_id: u64) -> DiagnosticTicket {
        DiagnosticTicket { file_id, generation: 0, version: None, job_id }
    }

    fn started(transition: Transition) -> DiagnosticTicket {
        let Transition::Start { pending, .. } = transition else { panic!("expected a start") };
        pending.ticket
    }

    #[test]
    fn coalesces_to_latest_ticket_without_disturbing_other_files() {
        let mut files = Files::default();
        let first_file = files.insert("file:///Main.purs", "module Main where");
        let second_file = files.insert("file:///Library.purs", "module Library where");
        let mut scheduler = Scheduler::default();
        let running = ticket(first_file, 1);
        scheduler.apply(pending(running));
        scheduler.apply(pending(ticket(first_file, 2)));
        let latest = ticket(first_file, 3);
        scheduler.apply(pending(latest));
        let other = ticket(second_file, 4);
        assert_eq!(started(scheduler.apply(pending(other))), other);
        assert_eq!(scheduler.running.len(), 2);
        scheduler.apply(DiagnosticWorkerEvent::Completed { ticket: running, collected: None });
        assert_eq!(started(scheduler.start_next(first_file)), latest);
        scheduler.apply(DiagnosticWorkerEvent::Completed { ticket: latest, collected: None });
        assert_eq!(scheduler.running[&second_file].ticket, other);
    }

    #[test]
    fn stale_and_duplicate_completions_do_not_release_the_slot() {
        let file_id = file_id("file:///Main.purs");
        let mut scheduler = Scheduler::default();
        let first = ticket(file_id, 1);
        scheduler.apply(pending(first));
        assert!(matches!(
            scheduler.apply(DiagnosticWorkerEvent::Completed {
                ticket: ticket(file_id, 0),
                collected: None
            }),
            Transition::None
        ));
        assert!(scheduler.running.contains_key(&file_id));
        scheduler.apply(DiagnosticWorkerEvent::Completed { ticket: first, collected: None });
        let repeated = ticket(file_id, 2);
        scheduler.apply(pending(repeated));
        assert!(matches!(
            scheduler.apply(DiagnosticWorkerEvent::Completed { ticket: first, collected: None }),
            Transition::None
        ));
        assert_eq!(scheduler.running[&file_id].ticket, repeated);
    }

    #[test]
    fn failure_or_cancellation_completion_releases_the_slot() {
        let file_id = file_id("file:///Main.purs");
        let mut scheduler = Scheduler::default();
        let first = ticket(file_id, 1);
        scheduler.apply(pending(first));
        scheduler.apply(DiagnosticWorkerEvent::Invalidate { file_id });
        assert!(scheduler.running[&file_id].cancellation.check().is_err());
        assert!(matches!(
            scheduler.apply(DiagnosticWorkerEvent::Completed { ticket: first, collected: None }),
            Transition::Finish { collected: None, .. }
        ));
        assert!(scheduler.running.is_empty());
    }

    #[test]
    fn shutdown_cancels_running_and_admits_no_more_work() {
        let file_id = file_id("file:///Main.purs");
        let mut scheduler = Scheduler::default();
        scheduler.apply(pending(ticket(file_id, 1)));
        scheduler.apply(DiagnosticWorkerEvent::Shutdown);
        assert!(scheduler.running[&file_id].cancellation.check().is_err());
        assert!(matches!(scheduler.apply(pending(ticket(file_id, 2))), Transition::None));
        scheduler.apply(DiagnosticWorkerEvent::Completed {
            ticket: ticket(file_id, 1),
            collected: None,
        });
        assert!(matches!(scheduler.start_next(file_id), Transition::None));
    }

    #[test]
    fn diagnostic_admission_cannot_deadlock_the_blocking_pool_with_an_admitted_request() {
        let timeout = Duration::from_secs(5);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let analysis = Arc::new(Analysis::new(
            QueryEngine::default(),
            FileLifecycle::<i32, SourceMetadata>::default(),
        ));
        let (blocker_release_sender, blocker_release_receiver) = mpsc::channel();
        let blocker_release_receiver = Arc::new(std::sync::Mutex::new(blocker_release_receiver));
        let (blocker_started_sender, blocker_started_receiver) = mpsc::channel();
        for _ in 0..2 {
            let blocker_release_receiver = Arc::clone(&blocker_release_receiver);
            let blocker_started_sender = mpsc::Sender::clone(&blocker_started_sender);
            runtime.spawn_blocking(move || {
                blocker_started_sender.send(()).unwrap();
                blocker_release_receiver.lock().unwrap().recv().unwrap();
            });
        }
        blocker_started_receiver.recv_timeout(timeout).unwrap();
        blocker_started_receiver.recv_timeout(timeout).unwrap();

        let diagnostic = PendingDiagnostic {
            ticket: ticket(file_id("file:///Diagnostic.purs"), 1),
            analysis: Arc::clone(&analysis),
            position_encoding: PositionEncoding::Utf16,
            analyzer_capabilities: AnalyzerCapabilities::default(),
        };
        let (client_sender, client_receiver) = mpsc::channel();
        let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
            client_sender.send(client).unwrap();
            async_lsp::router::Router::<(), async_lsp::ResponseError>::new(())
        });
        let client = client_receiver.recv().unwrap();
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut monitors = JoinSet::new();
        {
            let _runtime = runtime.enter();
            execute(
                Transition::Start { pending: diagnostic, cancellation: Cancellation::new() },
                &client,
                &sender,
                &mut monitors,
            );
        }

        let request_snapshot = analysis.snapshot(
            PositionEncoding::Utf16,
            AnalyzerCapabilities::default(),
            Cancellation::new(),
        );
        let (request_started_sender, request_started_receiver) = mpsc::channel();
        let (request_release_sender, request_release_receiver) = mpsc::channel();
        runtime.spawn_blocking(move || {
            request_started_sender.send(()).unwrap();
            request_release_receiver.recv().unwrap();
            drop(request_snapshot);
        });

        let (write_admitted_sender, write_admitted_receiver) = mpsc::channel();
        analysis.notify_when_write_admitted(write_admitted_sender);
        let writer_analysis = Arc::clone(&analysis);
        let (writer_finished_sender, writer_finished_receiver) = mpsc::channel();
        let writer = std::thread::spawn(move || {
            writer_analysis.apply([]);
            writer_finished_sender.send(()).unwrap();
        });
        write_admitted_receiver.recv_timeout(timeout).unwrap();

        // One pool slot remains occupied. The released slot must retire the
        // diagnostic snapshot before running the already-admitted request.
        blocker_release_sender.send(()).unwrap();
        let request_started = request_started_receiver.recv_timeout(timeout);

        // Release every gate before asserting so a failed ordering check cannot
        // strand the reduced blocking pool or its writer.
        let _ = request_release_sender.send(());
        let _ = blocker_release_sender.send(());
        let writer_finished = writer_finished_receiver.recv_timeout(timeout);
        writer.join().unwrap();
        runtime.block_on(async { while monitors.join_next().await.is_some() {} });
        runtime.shutdown_timeout(timeout);

        assert!(request_started.is_ok());
        assert!(writer_finished.is_ok());
    }
}
