//! Termination signals, so that the watcher removes its socket and socket file before exiting.

use std::io;

/// Waits for a termination signal and returns the exit status a process killed by it would have.
#[cfg(unix)]
pub(crate) async fn terminated() -> io::Result<i32> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    let status = tokio::select! {
        _ = interrupt.recv() => 130,
        _ = terminate.recv() => 143,
        _ = hangup.recv() => 129,
    };
    Ok(status)
}

/// Waits for Ctrl-C, Ctrl-Break, or the console closing.
#[cfg(windows)]
pub(crate) async fn terminated() -> io::Result<i32> {
    use tokio::signal::windows::{ctrl_break, ctrl_c, ctrl_close};

    /// `STATUS_CONTROL_C_EXIT`, the status of a console process ended by Ctrl-C.
    const CONTROL_C_EXIT: i32 = 0xC000_013A_u32 as i32;

    let mut interrupt = ctrl_c()?;
    let mut interrupt_break = ctrl_break()?;
    let mut close = ctrl_close()?;
    tokio::select! {
        _ = interrupt.recv() => {}
        _ = interrupt_break.recv() => {}
        _ = close.recv() => {}
    }
    Ok(CONTROL_C_EXIT)
}
