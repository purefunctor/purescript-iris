use std::io;
use std::path::Path;
use std::process::{Output, Stdio};

use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

struct ProcessTree {
    child: Box<dyn ChildWrapper>,
    retired: bool,
}

impl ProcessTree {
    fn terminate(&mut self) -> io::Result<()> {
        match self.child.start_kill() {
            #[cfg(unix)]
            Err(error) if error.raw_os_error() == Some(libc::ESRCH) => Ok(()),
            result => result,
        }
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        if !self.retired {
            let _ = self.terminate();
        }
    }
}

pub(super) async fn output(
    root: &Path,
    program: &str,
    arguments: &[String],
    cancellation: &CancellationToken,
) -> io::Result<Output> {
    if cancellation.is_cancelled() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    let mut command = Command::new(program);
    command.args(arguments).current_dir(root).stdin(Stdio::null());
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut command = CommandWrap::from(command);
    command.wrap(KillOnDrop);
    #[cfg(unix)]
    command.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    command.wrap(process_wrap::tokio::JobObject);

    let mut process = ProcessTree { child: command.spawn()?, retired: false };
    let mut stdout = process.child.stdout().take().expect("source command stdout is piped");
    let mut stderr = process.child.stderr().take().expect("source command stderr is piped");
    let mut stdout_bytes = vec![];
    let mut stderr_bytes = vec![];

    // Wait only on the leader in the cancellable section. Wrapped waits may own blocking
    // descendant-reaping tasks, which must be awaited rather than dropped by select.
    let result = tokio::select! {
        _ = cancellation.cancelled() => Err(io::Error::from(io::ErrorKind::Interrupted)),
        result = async {
            tokio::try_join!(
                stdout.read_to_end(&mut stdout_bytes),
                stderr.read_to_end(&mut stderr_bytes),
                async {
                    // Safety: this module installs only KillOnDrop and a group/job wrapper.
                    // Waiting caches the leader status in Tokio; the full wrapper wait below
                    // still performs group/job cleanup using that same cached status.
                    let child = unsafe { process.child.try_inner_child_mut() }
                        .expect("owned command has a native child");
                    let status = child.wait().await?;
                    process.terminate()?;
                    Ok::<_, io::Error>(status)
                },
            )
        } => result.map(|(_, _, status)| status),
    };

    let termination = process.terminate();
    let retirement = process.child.wait().await;
    process.retired = retirement.is_ok();
    let status = result?;
    termination?;
    retirement?;
    Ok(Output { status, stdout: stdout_bytes, stderr: stderr_bytes })
}
