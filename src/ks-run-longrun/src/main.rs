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
use kansei_core::types::Script;
use kansei_exec::{
    exec_script,
    pidfd_send_signal,
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
) -> Result<bool>
where
    F: FnMut() -> Pin<Box<dyn Future<Output = Result<(), JoinError>> + Unpin>>,
{
    todo!()
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args();
    args.next();
    let script: Script = bincode::deserialize(&fs::read(args.next().unwrap()).await?)?;
    let mut pidfd_opt = start_script(&script, signal_wait()).await?;

    // TODO: notify SVC

    while let Some(pidfd) = &pidfd_opt {
        let res = supervise(pidfd, &script, signal_wait()).await?;

        if res {
            // If the process has died, run finish script,
            // notify the SVC and run into the next loop cycle
            //to run start_script again
            let finish_res = todo!();
            pidfd_opt = start_script(&script, signal_wait()).await?;

            // TODO: notify SVC
        } else {
            // Received signal => stop running
            break;
        }
    }
    // pidfd_opt == None => Received signal => stop running

    Ok(())
}
