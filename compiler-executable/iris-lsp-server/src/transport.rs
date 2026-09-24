//! Connects `lsp-server`'s blocking channels to Tokio channels.
//!
//! `lsp-server` reads and writes stdio on its own threads and exposes crossbeam channels whose
//! stdio capacity is zero, so sending blocks until the writer thread takes the message. Two bridge
//! threads move messages between those channels and unbounded Tokio channels, so the protocol
//! actor never blocks on the editor.

use std::{io, thread};

use lsp_server::{Connection, IoThreads, Message};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, timeout_at};

pub struct Transport {
    incoming: mpsc::UnboundedReceiver<Message>,
    outgoing: mpsc::UnboundedSender<Message>,
    threads: TransportThreads,
}

struct TransportThreads {
    incoming: thread::JoinHandle<()>,
    outgoing: thread::JoinHandle<()>,
    io: Option<IoThreads>,
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("the stdio threads did not finish before the cleanup deadline")]
    Timeout,
    #[error("a stdio thread panicked")]
    Panicked,
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl Transport {
    /// Reads from standard input and writes to standard output.
    pub fn stdio() -> Transport {
        let (connection, io_threads) = Connection::stdio();
        Transport::connect(connection, Some(io_threads))
    }

    /// Uses one end of [`Connection::memory`]; the other end plays the editor.
    pub fn memory(connection: Connection) -> Transport {
        Transport::connect(connection, None)
    }

    fn connect(connection: Connection, io: Option<IoThreads>) -> Transport {
        let Connection { sender, receiver } = connection;

        let (incoming_sender, incoming) = mpsc::unbounded_channel();
        let incoming_thread = thread::Builder::new()
            .name("iris-lsp-incoming".to_string())
            .spawn(move || {
                for message in receiver {
                    // `lsp-server`'s stdio reader also stops after `exit`; stopping here keeps an
                    // in-memory connection from holding this thread after the session ends.
                    let exit =
                        matches!(&message, Message::Notification(notification) if notification.method == "exit");
                    if incoming_sender.send(message).is_err() || exit {
                        break;
                    }
                }
            })
            .expect("invariant violated: failed to spawn the incoming message thread");

        let (outgoing, mut outgoing_receiver) = mpsc::unbounded_channel::<Message>();
        let outgoing_thread = thread::Builder::new()
            .name("iris-lsp-outgoing".to_string())
            .spawn(move || {
                while let Some(message) = outgoing_receiver.blocking_recv() {
                    if sender.send(message).is_err() {
                        break;
                    }
                }
            })
            .expect("invariant violated: failed to spawn the outgoing message thread");

        let threads = TransportThreads { incoming: incoming_thread, outgoing: outgoing_thread, io };
        Transport { incoming, outgoing, threads }
    }

    /// Returns the next message from the editor, or `None` once the editor's input ended.
    pub(crate) async fn receive(&mut self) -> Option<Message> {
        self.incoming.recv().await
    }

    pub(crate) fn send(&self, message: impl Into<Message>) {
        if self.outgoing.send(message.into()).is_err() {
            tracing::warn!("Dropped a message to the editor after the transport closed");
        }
    }

    /// Flushes queued messages and joins the transport threads.
    ///
    /// Reads and writes inside `lsp-server`'s stdio threads cannot be interrupted, so the joins
    /// run on a separate thread and are abandoned at `deadline`. Unless the editor's input ended
    /// (with `exit` or end of file), the reading threads may be blocked on input that never
    /// arrives; then only the writing side is joined, and process exit ends the rest.
    pub(crate) async fn close(
        self,
        deadline: Instant,
        input_ended: bool,
    ) -> Result<(), TransportError> {
        let Transport { incoming, outgoing, threads } = self;
        drop(outgoing);
        drop(incoming);

        let (joined, result) = oneshot::channel();
        thread::Builder::new()
            .name("iris-lsp-transport-join".to_string())
            .spawn(move || {
                let result = if input_ended {
                    threads.join()
                } else {
                    threads.outgoing.join().map_err(|_| TransportError::Panicked)
                };
                let _ = joined.send(result);
            })
            .map_err(TransportError::Io)?;

        match timeout_at(deadline, result).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(TransportError::Panicked),
            Err(_) => Err(TransportError::Timeout),
        }
    }
}

impl TransportThreads {
    fn join(self) -> Result<(), TransportError> {
        let TransportThreads { incoming, outgoing, io } = self;
        incoming.join().map_err(|_| TransportError::Panicked)?;
        outgoing.join().map_err(|_| TransportError::Panicked)?;
        if let Some(io) = io {
            io.join()?;
        }
        Ok(())
    }
}
