use std::{
    future::Future,
    pin::Pin,
    process::ExitStatus,
    time::Duration,
};

use anyhow::{
    Context,
    Result,
};
use async_pidfd::PidFd;
use rinit_service::types::Script;
use tokio::{
    io::unix::AsyncFd,
    select,
    sync::oneshot,
    task::{
        self,
        JoinError,
    },
    time::timeout,
};
use tracing::warn;

use crate::{
    exec_script,
    kill_process,
    log_output,
};

#[derive(Debug, PartialEq, Eq)]
enum ScriptResult {
    Exited(ExitStatus),
    SignalReceived,
    TimedOut,
}

type WaitFn = Pin<Box<dyn Future<Output = Result<(), JoinError>> + Unpin>>;

pub async fn run_short_lived_script<F>(
    script: &Script,
    mut wait: F,
) -> Result<bool>
where
    F: FnMut() -> WaitFn,
{
    let script_timeout = Duration::from_millis(script.timeout as u64);

    let mut time_tried = 0;
    let success = loop {
        let mut child = exec_script(script)
            .await
            .context("unable to execute script")?;
        let (tx, rx) = oneshot::channel();
        let logger = task::spawn(log_output(
            child.stdout.take().unwrap(),
            child.stderr.take().unwrap(),
            rx,
        ));
        let pidfd = AsyncFd::new(
            PidFd::from_pid(child.id().unwrap() as i32)
                .context("unable to create PidFd from child pid")?,
        )
        .context("unable to create AsyncFd from PidFd")?;
        let script_res = select! {
            timeout_res = timeout(script_timeout, pidfd.readable()) => {
                if timeout_res.is_ok() {
                    ScriptResult::Exited(match pidfd.get_ref().wait().context("unable to call waitid on child process") {
                        Ok(wait) => wait.status(),
                        Err(err) => {
                            warn!("{err}");
                            time_tried += 1;
                            if time_tried == script.max_deaths {
                                break false;
                            }
                            continue
                        },
                    })
                } else {
                    ScriptResult::TimedOut
                }
            }
            _ = wait() => ScriptResult::SignalReceived
        };

        match script_res {
            ScriptResult::Exited(exit_status) => {
                if exit_status.success() {
                    break true;
                }
            }
            ScriptResult::SignalReceived => {
                kill_process(&pidfd, script.down_signal, script.timeout_kill).await?;
                break false;
            }
            ScriptResult::TimedOut => {
                kill_process(&pidfd, script.down_signal, script.timeout_kill).await?;
            }
        }

        if !tx.is_closed() {
            // Why do we need to close the pipes manually? The process has either exited
            // or has been killed, the pipes should have been already closed
            // Add this as workaround
            tx.send(()).unwrap();
        }
        logger.await??;

        time_tried += 1;
        if time_tried == script.max_deaths {
            break false;
        }
    };

    Ok(success)
}

#[cfg(test)]
mod tests {
    use rinit_service::types::ScriptPrefix;
    use tokio::time::sleep;

    use super::*;

    macro_rules! wait {
        ($time:literal) => {
            || {
                Box::pin(tokio::spawn(async {
                    sleep(Duration::from_secs($time)).await;
                }))
            }
        };
    }

    #[tokio::test]
    async fn test_run_script_success() {
        let script = Script::new(ScriptPrefix::Bash, "exit 0".to_string());
        assert!(run_short_lived_script(&script, wait!(100)).await.unwrap());
    }

    #[tokio::test]
    async fn test_run_script_failure() {
        let script = Script::new(ScriptPrefix::Bash, "exit 1".to_string());
        assert!(!run_short_lived_script(&script, wait!(100)).await.unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_run_script_timeout() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 15".to_string());
        script.timeout = 10;
        assert!(!run_short_lived_script(&script, wait!(100)).await.unwrap());
    }

    #[tokio::test]
    async fn test_run_script_force_kill() {
        let mut script = Script::new(ScriptPrefix::Path, "sleep 100".to_string());
        script.timeout = 10;
        script.timeout_kill = 10;
        script.down_signal = 0;
        script.max_deaths = 1;
        assert!(!run_short_lived_script(&script, wait!(100)).await.unwrap());
    }

    #[tokio::test]
    async fn test_run_script_signal_received() {
        let mut script = Script::new(ScriptPrefix::Path, "sleep 100".to_string());
        script.timeout = 100000;
        assert!(!run_short_lived_script(&script, wait!(0)).await.unwrap());
    }
}
