use std::io;
use std::process::{Command, ExitStatus, Stdio};

use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::process::{ChildStderr, ChildStdout};

/// Owns a command and every descendant that remains in its process tree.
pub(super) struct ProcessTree {
    child: Box<dyn ChildWrapper>,
}

impl ProcessTree {
    pub(super) fn spawn(command: Command) -> io::Result<ProcessTree> {
        let mut command = tokio::process::Command::from(command);
        command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut command = CommandWrap::from(command);
        command.wrap(KillOnDrop);
        #[cfg(unix)]
        command.wrap(process_wrap::tokio::ProcessGroup::leader());
        #[cfg(windows)]
        command.wrap(process_wrap::tokio::JobObject);
        let child = command.spawn()?;
        Ok(ProcessTree { child })
    }

    pub(super) fn take_standard_output(&mut self) -> Option<ChildStdout> {
        self.child.stdout().take()
    }

    pub(super) fn take_standard_error(&mut self) -> Option<ChildStderr> {
        self.child.stderr().take()
    }

    pub(super) async fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait().await
    }

    pub(super) async fn terminate(&mut self) -> io::Result<()> {
        Box::into_pin(self.child.kill()).await
    }
}
