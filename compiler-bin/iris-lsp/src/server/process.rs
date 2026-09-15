use std::io;
use std::path::Path;
use std::process::{Output, Stdio};

#[cfg(windows)]
use process_wrap::tokio::JobObject;
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

struct Process {
    child: Box<dyn ChildWrapper>,
    retired: bool,
}

impl Drop for Process {
    fn drop(&mut self) {
        if !self.retired {
            let _ = self.child.start_kill();
        }
    }
}

pub(super) async fn run(
    root: &Path,
    program: &str,
    arguments: &[String],
    cancellation: &CancellationToken,
) -> io::Result<Output> {
    if cancellation.is_cancelled() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    let mut command = CommandWrap::with_new(program, |command| {
        command
            .args(arguments)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    });
    command.wrap(KillOnDrop);
    #[cfg(unix)]
    command.wrap(ProcessGroup::leader());
    #[cfg(windows)]
    command.wrap(JobObject);

    let mut process = Process { child: command.spawn()?, retired: false };
    let mut stdout = process.child.stdout().take().expect("piped stdout missing");
    let mut stderr = process.child.stderr().take().expect("piped stderr missing");
    let pipes = async move {
        let mut output = Vec::new();
        let mut errors = Vec::new();
        tokio::try_join!(stdout.read_to_end(&mut output), stderr.read_to_end(&mut errors))?;
        Ok::<_, io::Error>((output, errors))
    };
    tokio::pin!(pipes);

    // Wrapped wait may own a blocking group/job waiter. Only the native
    // Tokio leader wait is cancellation-safe; preserve the wrapper itself
    // for tree termination and one uninterrupted final retirement below.
    // SAFETY: waiting for the native leader does not mutate process-group or
    // job state, and the wrapper wait still observes Tokio's cached status.
    let leader =
        unsafe { process.child.try_inner_child_mut().expect("native Tokio child missing") };
    let mut pipe_result = None;
    let result = tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(io::ErrorKind::Interrupted.into()),
        result = &mut pipes => {
            match result {
                Ok(counts) => {
                    pipe_result = Some(Ok(counts));
                    tokio::select! {
                        biased;
                        _ = cancellation.cancelled() => Err(io::ErrorKind::Interrupted.into()),
                        result = leader.wait() => result,
                    }
                }
                Err(error) => {
                    pipe_result = Some(Err(io::Error::new(error.kind(), error.to_string())));
                    Err(error)
                }
            }
        }
        result = leader.wait() => result,
    };

    // Leader exit does not imply pipe EOF: descendants may still hold the
    // descriptors. Terminate the owned tree before joining either reader.
    let termination = process.child.start_kill();
    let retirement = process.child.wait().await;
    process.retired = retirement.is_ok();
    let pipe_result = match pipe_result {
        Some(result) => result,
        None => pipes.await,
    };
    let status = result?;
    retirement?;
    let (output, errors) = pipe_result?;
    if let Err(error) = termination {
        #[cfg(unix)]
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(Output { status, stdout: output, stderr: errors });
        }
        return Err(error);
    }
    Ok(Output { status, stdout: output, stderr: errors })
}
