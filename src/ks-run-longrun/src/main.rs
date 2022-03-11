#![feature(async_closure)]

use std::{
    env,
    future::Future,
    os::unix::prelude::AsRawFd,
    path::Path,
    pin::Pin,
    process::ExitStatus,
    time::Duration,
};

use anyhow::{
    Context,
    Result,
};
use async_pidfd::PidFd;
use kansei_core::types::{
    Longrun,
    Script,
};
use kansei_exec::{
    exec_script,
    pidfd_send_signal,
    run_short_lived_script,
    signal_wait::signal_wait_fun,
};
use kansei_message::Message;
use tokio::{
    fs,
    io::unix::AsyncFd,
    select,
    task::JoinError,
    time::timeout,
};

const SIGKILL: i32 = 9;

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
            stop_process(&pidfd, script.down_signal, script.timeout_kill as u64).await?;
            ScriptResult::SignalReceived
        }
    })
}

async fn stop_process(
    pidfd: &AsyncFd<PidFd>,
    down_signal: i32,
    timeout_kill: u64,
) -> Result<()> {
    pidfd_send_signal(pidfd.as_raw_fd(), down_signal)
        .with_context(|| format!("unable to send signal {:?}", down_signal))?;
    let timeout_res = timeout(Duration::from_millis(timeout_kill), pidfd.readable()).await;
    if timeout_res.is_err() {
        pidfd_send_signal(pidfd.as_raw_fd(), SIGKILL).context("unable to send signal SIGKILL")?;
    }
    pidfd.get_ref().wait().context("unable to call waitid")?;

    Ok(())
}

async fn supervise<F>(
    pidfd: &AsyncFd<PidFd>,
    script: &Script,
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

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args();
    args.next();
    let longrun: Longrun = serde_json::from_slice(
        &mut fs::read(Path::new(&args.next().unwrap()).join("service")).await?,
    )?;
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
                let message = Message::ServiceIsUp(true, longrun.name.clone());
                message.send().await.context("unable to notify svc")?;
                let res = supervise(&pidfd, &longrun.run, signal_wait_fun()).await?;
                match res {
                    ScriptResult::Exited(_) => {}
                    ScriptResult::SignalReceived => {
                        // stop running
                        stop_process(
                            &pidfd,
                            longrun.run.down_signal,
                            longrun.run.timeout_kill as u64,
                        )
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

    let message = Message::ServiceIsUp(false, longrun.name.clone());
    message.send().await.context("unable to notify svc")?;

    Ok(())
}

#[cfg(test)]
mod test {
    use kansei_core::types::ScriptPrefix;
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
                supervise(&pidfd, &script, wait!(1000)).await.unwrap(),
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
                supervise(&pidfd, &script, wait!(1)).await.unwrap(),
                ScriptResult::SignalReceived
            ));
        }
    }
}
