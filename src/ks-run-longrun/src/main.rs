#![feature(async_closure)]

use std::{
    env,
    future::Future,
    os::unix::prelude::AsRawFd,
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
    signal_wait,
};
use tokio::{
    fs,
    io::unix::AsyncFd,
    select,
    task::JoinError,
    time::timeout,
};

const SIGKILL: i32 = 9;

#[derive(Debug, PartialEq)]
enum ScriptResult {
    Exited(ExitStatus),
    Running,
    SignalReceived,
}

async fn start_script<F>(
    script: &Script,
    mut wait: F,
) -> Result<Option<AsyncFd<PidFd>>>
where
    F: FnMut() -> Pin<Box<dyn Future<Output = Result<(), JoinError>> + Unpin>>,
{
    let script_timeout = Duration::from_millis(script.timeout as u64);

    let mut time_tried = 0;
    Ok(loop {
        let child = exec_script(script)
            .await
            .context("unable to execute script")?;
        let pidfd = AsyncFd::new(
            PidFd::from_pid(child.id().unwrap() as i32)
                .context("unable to create PidFd from child pid")?,
        )
        .context("unable to create AsyncFd from PidFd")?;
        let script_res = select! {
            timeout_res = timeout(script_timeout, pidfd.readable()) => {
                if timeout_res.is_ok() {
                    ScriptResult::Exited(pidfd.get_ref().wait().context("unable to call waitid on child process")?.status())
                } else {
                    ScriptResult::Running
                }
            }
            _ = wait() => ScriptResult::SignalReceived
        };

        match script_res {
            ScriptResult::Running => break Some(pidfd),
            ScriptResult::Exited(_) => {
                time_tried += 1;
                if time_tried == script.max_deaths {
                    break None;
                }
            }
            ScriptResult::SignalReceived => {
                pidfd_send_signal(pidfd.as_raw_fd(), script.down_signal)
                    .with_context(|| format!("unable to send signal {:?}", script.down_signal))?;
                let timeout_res = timeout(
                    Duration::from_millis(script.timeout_kill as u64),
                    pidfd.readable(),
                )
                .await;
                if timeout_res.is_err() {
                    pidfd_send_signal(pidfd.as_raw_fd(), SIGKILL)
                        .context("unable to send signal SIGKILL")?;
                }
                pidfd.get_ref().wait().context("unable to call waitid")?;
                break None;
            }
        }
    })
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
    let longrun: Longrun = bincode::deserialize(&fs::read(args.next().unwrap()).await?)?;
    let mut pidfd_opt = start_script(&longrun.run, signal_wait()).await?;

    // TODO: notify SVC

    while let Some(pidfd) = &pidfd_opt {
        let res = supervise(pidfd, &longrun.run, signal_wait()).await?;

        match res {
            ScriptResult::Exited(_) => {
                // If the process has died, run finish script,
                // notify the SVC and run into the next loop cycle
                //to run start_script again
                if let Some(finish_script) = &longrun.finish {
                    let _ = run_short_lived_script(finish_script, signal_wait());
                }
                pidfd_opt = start_script(&longrun.run, signal_wait()).await?;

                // TODO: notify SVC
            }
            ScriptResult::SignalReceived => {
                // stop running
                break;
            }
            ScriptResult::Running => unreachable!(),
        }
    }
    // pidfd_opt == None => Received signal => stop running

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
    async fn test_start_script() {
        // sleep for 10ms
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 0.001".to_string());
        // wait for 1ms
        script.timeout = 1;
        let pidfd = start_script(&script, wait!(1000)).await.unwrap();
        assert!(pidfd.is_some());
    }

    #[tokio::test]
    async fn test_start_script_failure() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 0".to_string());
        script.timeout = 5;
        let pidfd = start_script(&script, wait!(1000)).await.unwrap();
        assert!(pidfd.is_none());
    }

    #[tokio::test]
    async fn test_supervise() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 0.1".to_string());
        script.timeout = 1;
        let pidfd = start_script(&script, wait!(1000)).await.unwrap().unwrap();
        assert!(matches!(
            supervise(&pidfd, &script, wait!(1000)).await.unwrap(),
            ScriptResult::Exited(..)
        ));
    }

    #[tokio::test]
    async fn test_supervise_signal() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 1".to_string());
        script.timeout = 1;
        let pidfd = start_script(&script, wait!(1000)).await.unwrap().unwrap();
        assert!(matches!(
            supervise(&pidfd, &script, wait!(1)).await.unwrap(),
            ScriptResult::SignalReceived
        ));
    }
}
