use std::io;
use std::process::{ExitStatus, Stdio};

#[cfg(windows)]
use process_wrap::tokio::JobObject;
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::process::{ChildStderr, ChildStdout};

pub(super) struct ChildProcess {
    child: Box<dyn ChildWrapper>,
}

impl ChildProcess {
    pub(super) fn spawn(program: &str, arguments: &[String]) -> io::Result<ChildProcess> {
        let mut command = CommandWrap::with_new(program, |command| {
            command
                .args(arguments)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        });
        command.wrap(KillOnDrop);
        #[cfg(unix)]
        command.wrap(ProcessGroup::leader());
        #[cfg(windows)]
        command.wrap(JobObject);

        let child = command.spawn()?;
        Ok(ChildProcess { child })
    }

    pub(super) fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout().take()
    }

    pub(super) fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr().take()
    }

    pub(super) async fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait().await
    }

    pub(super) async fn kill(&mut self) -> io::Result<()> {
        Box::into_pin(self.child.kill()).await
    }
}
