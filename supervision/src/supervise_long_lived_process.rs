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
use rinit_ipc::{
    AsyncConnection,
    Request,
};
use rinit_service::types::{
    Longrun,
    Script,
};
use tokio::{
    io::unix::AsyncFd,
    select,
    task::JoinError,
    time::timeout,
};

use crate::{
    exec_script,
    kill_process,
    run_short_lived_script,
    signal_wait::signal_wait_fun,
};

enum ScriptResult {
    Exited(ExitStatus),
    Running(AsyncFd<PidFd>),
    SignalReceived,
}

async fn start_process<F>(
    script: &Script,
    mut wait: F,
) -> Result<ScriptResult>
where
    F: FnMut() -> Pin<Box<dyn Future<Output = Result<(), JoinError>> + Unpin>>,
{
    let script_timeout = Duration::from_millis(script.timeout as u64);

    let child = exec_script(script)
        .await
        .context("unable to execute script")?;
    let pidfd = AsyncFd::new(
        PidFd::from_pid(child.id().unwrap() as i32)
            .context("unable to create PidFd from child pid")?,
    )
    .context("unable to create AsyncFd from PidFd")?;
    Ok(select! {
        timeout_res = timeout(script_timeout, pidfd.readable()) => {
            if timeout_res.is_ok() {
                ScriptResult::Exited(pidfd.get_ref().wait().context("unable to call waitid on child process")?.status())
            } else {
                ScriptResult::Running(pidfd)
            }
        }
        _ = wait() => {
            kill_process(&pidfd, script.down_signal, script.timeout_kill).await?;
            ScriptResult::SignalReceived
        }
    })
}

async fn supervise<F>(
    pidfd: &AsyncFd<PidFd>,
    mut wait: F,
) -> Result<ScriptResult>
where
    F: FnMut() -> Pin<Box<dyn Future<Output = Result<(), JoinError>> + Unpin>>,
{
    Ok(select! {
        _ = pidfd.readable() => {
            ScriptResult::Exited(pidfd.get_ref().wait().context("unable to call waitid on child process")?.status())
        }
        _ = wait() => ScriptResult::SignalReceived
    })
}

pub async fn supervise_long_lived_process(service: &str) -> Result<()> {
    let longrun: Longrun = serde_json::from_str(service)?;
    let mut conn = AsyncConnection::new_host_address().await?;
    let mut time_tried = 0;
    loop {
        let script_res = start_process(&longrun.run, signal_wait_fun()).await?;

        match script_res {
            ScriptResult::Exited(_) => {
                time_tried += 1;
                if time_tried == longrun.run.max_deaths {
                    break;
                }
                if let Some(finish_script) = &longrun.finish {
                    let _ = run_short_lived_script(finish_script, signal_wait_fun()).await;
                }
            }
            ScriptResult::Running(pidfd) => {
                time_tried = 0;
                let request = Request::ServiceIsUp(longrun.name.clone(), true);
                // TODO: handle this
                conn.send_request(request).await??;
                let res = supervise(&pidfd, signal_wait_fun()).await?;
                match res {
                    ScriptResult::Exited(_) => {}
                    ScriptResult::SignalReceived => {
                        // stop running
                        kill_process(&pidfd, longrun.run.down_signal, longrun.run.timeout_kill)
                            .await?;
                        if let Some(finish_script) = &longrun.finish {
                            let _ = run_short_lived_script(finish_script, signal_wait_fun()).await;
                        }
                        break;
                    }
                    ScriptResult::Running(_) => unreachable!(),
                }
            }
            ScriptResult::SignalReceived => break,
        }
    }

    let request = Request::ServiceIsUp(longrun.name.clone(), false);
    // TODO: log this
    let _ = conn
        .send_request(request)
        .await
        .context("error while communicating with svc")?
        .context("the request failed")?;

    Ok(())
}

#[cfg(test)]
mod test {
    use rinit_service::types::ScriptPrefix;
    use tokio::time::sleep;

    use super::*;

    macro_rules! wait {
        ($time:literal) => {
            || {
                Box::pin(tokio::spawn(async {
                    sleep(Duration::from_millis($time)).await;
                }))
            }
        };
    }

    #[tokio::test]
    async fn test_start_process() {
        // sleep for 100ms
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 0.01".to_string());
        // wait for 1ms
        script.timeout = 1;
        assert!(matches!(
            start_process(&script, wait!(1000)).await.unwrap(),
            ScriptResult::Running(..)
        ));
    }

    #[tokio::test]
    async fn test_start_process_failure() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 0".to_string());
        script.timeout = 5;
        assert!(matches!(
            start_process(&script, wait!(1000)).await.unwrap(),
            ScriptResult::Exited(..)
        ));
    }

    #[tokio::test]
    async fn test_supervise() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 0.1".to_string());
        script.timeout = 1;
        let pidfd = start_process(&script, wait!(1000)).await.unwrap();
        assert!(matches!(pidfd, ScriptResult::Running(..)));
        if let ScriptResult::Running(pidfd) = pidfd {
            assert!(matches!(
                supervise(&pidfd, wait!(1000)).await.unwrap(),
                ScriptResult::Exited(..)
            ));
        }
    }

    #[tokio::test]
    async fn test_supervise_signal() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 1".to_string());
        script.timeout = 1;
        let pidfd = start_process(&script, wait!(1000)).await.unwrap();
        assert!(matches!(pidfd, ScriptResult::Running(..)));
        if let ScriptResult::Running(pidfd) = pidfd {
            assert!(matches!(
                supervise(&pidfd, wait!(1)).await.unwrap(),
                ScriptResult::SignalReceived
            ));
        }
    }
}
