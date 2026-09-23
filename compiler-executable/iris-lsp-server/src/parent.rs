//! Monitoring of the editor process named by `processId` in the `initialize` parameters.
//!
//! The LSP specification asks the server to exit when that process is no longer alive, for
//! example when the editor crashed and left the transport open.

use std::{io, thread};

use serde_json::Value;
use tokio::sync::oneshot;

/// `ESRCH`, reported when opening a process that already exited. It has the same value on every
/// Unix platform Iris supports.
#[cfg(unix)]
const NO_SUCH_PROCESS: i32 = 3;

/// Returns the `processId` in the `initialize` parameters if it names a process that can be
/// monitored.
///
/// The field may hold any integer, but process IDs that do not fit in an `i32` cannot be opened
/// by the monitor, so they are accepted and ignored.
pub(crate) fn process_id(initialize: &Value) -> Option<i32> {
    let process_id = initialize.get("processId")?.as_i64()?;
    match i32::try_from(process_id) {
        Ok(process_id) => Some(process_id),
        Err(_) => {
            tracing::warn!("Not monitoring editor process {process_id}: the ID is out of range");
            None
        }
    }
}

/// Starts monitoring `process_id`. The returned channel completes when the process exits, and is
/// `None` if the process cannot be monitored.
pub(crate) fn monitor(process_id: i32) -> Option<oneshot::Receiver<()>> {
    let (exited, receiver) = oneshot::channel();
    match waitpid_any::WaitHandle::open(process_id) {
        Ok(mut handle) => {
            let spawned = thread::Builder::new().name("iris-lsp-editor-monitor".to_string()).spawn(
                move || match handle.wait() {
                    Ok(()) => {
                        let _ = exited.send(());
                    }
                    Err(error) => {
                        tracing::error!("Failed to monitor editor process {process_id}: {error}");
                    }
                },
            );
            if let Err(error) = spawned {
                tracing::error!("Failed to start monitoring editor process {process_id}: {error}");
                return None;
            }
            Some(receiver)
        }
        Err(error) if already_exited(&error) => {
            let _ = exited.send(());
            Some(receiver)
        }
        Err(error) => {
            tracing::error!("Failed to monitor editor process {process_id}: {error}");
            None
        }
    }
}

#[cfg(unix)]
fn already_exited(error: &io::Error) -> bool {
    error.raw_os_error() == Some(NO_SUCH_PROCESS)
}

#[cfg(not(unix))]
fn already_exited(_: &io::Error) -> bool {
    false
}
