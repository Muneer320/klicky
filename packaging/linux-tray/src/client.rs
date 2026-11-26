use anyhow::{bail, Context, Result};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

pub struct Client {
    pub binary: PathBuf,
}

// The CLI may launch curl. Cancelling a refresh or quitting the helper must not
// leave that child behind or keep waiting on a descendant's inherited pipe.
struct ProcessGroup {
    pid: libc::pid_t,
    armed: bool,
}

impl ProcessGroup {
    fn kill(&mut self) {
        if self.armed {
            // The CLI leader remains unreaped until after this kill, so its
            // process-group ID cannot be recycled for an unrelated process.
            unsafe { libc::kill(-self.pid, libc::SIGKILL) };
            self.armed = false;
        }
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        self.kill();
    }
}

impl Client {
    pub async fn invoke(&self, args: &[String], limit: Duration) -> Result<String> {
        let mut child = Command::new(&self.binary)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("Cannot run {}", self.binary.display()))?;
        let pid = child.id().context("CLI process has no PID")? as libc::pid_t;
        let mut group = ProcessGroup { pid, armed: true };
        let stdout = child.stdout.take().context("CLI stdout is unavailable")?;
        let stderr = child.stderr.take().context("CLI stderr is unavailable")?;
        let operation = async {
            let (_, out, err) = tokio::try_join!(
                wait_unreaped(pid),
                bounded_read(stdout),
                bounded_read(stderr)
            )?;
            group.kill();
            let status = child.wait().await?;
            let out = String::from_utf8(out).context("CLI output is not UTF-8")?;
            if !status.success() {
                let err = String::from_utf8_lossy(&err);
                bail!("CLI failed ({}): {}", status, err.trim());
            }
            Ok(out)
        };
        timeout(limit, operation)
            .await
            .context("CLI timed out; refresh to confirm the current state")?
    }
}

async fn wait_unreaped(pid: libc::pid_t) -> std::io::Result<()> {
    loop {
        // WNOWAIT observes exit without releasing the PID. On cancellation the
        // group guard runs before Child drops and reaps the leader.
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error);
            }
        } else if unsafe { info.assume_init().si_pid() } == pid {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn bounded_read(stream: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    const MAX: u64 = 64 * 1024;
    let mut bytes = Vec::new();
    stream.take(MAX + 1).read_to_end(&mut bytes).await?;
    if bytes.len() > MAX as usize {
        return Err(std::io::Error::other("CLI output exceeded 64 KiB"));
    }
    Ok(bytes)
}
