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
    Service,
};
use tokio::{
    io::unix::AsyncFd,
    select,
    sync::oneshot::{
        self,
        Sender,
    },
    task::{
        self,
        JoinError,
        JoinHandle,
    },
    time::timeout,
};
use tracing::warn;

use crate::{
    exec_script,
    kill_process,
    log_output,
    run_short_lived_script,
    signal_wait::signal_wait_fun,
};

struct RunningScript {
    pidfd: AsyncFd<PidFd>,
    logger: JoinHandle<Result<(), anyhow::Error>>,
    logger_stop: Sender<()>,
}

enum ScriptResult {
    Exited(ExitStatus),
    Running(RunningScript),
    SignalReceived,
}

async fn start_process(
    script: &Script,
    wait: WaitFn,
) -> Result<ScriptResult> {
    let script_timeout = Duration::from_millis(script.timeout as u64);

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
    Ok(select! {
        timeout_res = timeout(script_timeout, pidfd.readable()) => {
            if timeout_res.is_ok() {
                let status = pidfd.get_ref().wait().context("unable to call waitid on child process")?.status();
                if !tx.is_closed() {
                    tx.send(()).unwrap();
                }
                logger.await??;
                ScriptResult::Exited(status)
            } else {
                ScriptResult::Running(RunningScript { pidfd, logger, logger_stop: tx } )
            }
        }
        _ = wait => {
            kill_process(&pidfd, script.down_signal, script.timeout_kill).await?;
            if !tx.is_closed() {
                tx.send(()).unwrap();
            }
            logger.await??;
            ScriptResult::SignalReceived
        }
    })
}

async fn try_start_process<F>(
    longrun: &Longrun,
    mut wait: F,
) -> Result<Option<RunningScript>>
where
    F: FnMut() -> Pin<Box<dyn Future<Output = Result<(), JoinError>> + Unpin>>,
{
    let mut time_tried = 0;
    Ok(loop {
        let script_res = start_process(&longrun.run, wait()).await?;

        match script_res {
            ScriptResult::Exited(status) => {
                warn!("process exited with status: {status}");
                time_tried += 1;
                if let Some(finish_script) = &longrun.finish {
                    let _ = run_short_lived_script(finish_script, signal_wait_fun()).await;
                }
                if time_tried == longrun.run.max_deaths {
                    break None;
                }
            }
            ScriptResult::Running(running_script) => break Some(running_script),
            ScriptResult::SignalReceived => break None,
        }
    })
}
type WaitFn = Pin<Box<dyn Future<Output = Result<(), JoinError>> + Unpin>>;

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

pub async fn supervise_long_lived_process(service: Service) -> Result<()> {
    let longrun = match service {
        Service::Longrun(longrun) => longrun,
        _ => unreachable!(),
    };
    let mut conn = AsyncConnection::new_host_address().await?;
    while let Some(running_script) = try_start_process(&longrun, signal_wait_fun()).await? {
        let request = Request::ServiceIsUp(longrun.name.clone(), true);
        // TODO: handle this
        conn.send_request(request).await??;
        let res = supervise(&running_script.pidfd, signal_wait_fun()).await;
        let res = match res {
            Ok(res) => {
                match res {
                    ScriptResult::SignalReceived => {
                        // stop running
                        kill_process(
                            &running_script.pidfd,
                            longrun.run.down_signal,
                            longrun.run.timeout_kill,
                        )
                        .await?;
                    }
                    ScriptResult::Exited(status) => warn!("process exited with status: {status}"),
                    ScriptResult::Running(_) => unreachable!(),
                }
                Some(res)
            }
            Err(err) => {
                warn!("{err}");
                None
            }
        };
        // Notify that it stopped
        let request = Request::ServiceIsUp(longrun.name.clone(), false);
        // TODO: handle this
        conn.send_request(request).await??;
        if !running_script.logger_stop.is_closed() {
            running_script.logger_stop.send(()).unwrap();
        }
        running_script.logger.await??;
        if let Some(res) = res && matches!(res, ScriptResult::SignalReceived) {
            break;
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
    use rinit_service::types::{
        ScriptPrefix,
        ServiceOptions,
    };
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
        let longrun = Longrun {
            name: "test".to_string(),
            run: script,
            finish: None,
            options: ServiceOptions::new(),
        };
        assert!(
            try_start_process(&longrun, wait!(1000))
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_start_process_failure() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 0".to_string());
        script.timeout = 5;
        let longrun = Longrun {
            name: "test".to_string(),
            run: script,
            finish: None,
            options: ServiceOptions::new(),
        };
        assert!(matches!(
            try_start_process(&longrun, wait!(1000)).await.unwrap(),
            None
        ));
    }

    #[tokio::test]
    async fn test_supervise() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 0.1".to_string());
        script.timeout = 1;
        let longrun = Longrun {
            name: "test".to_string(),
            run: script,
            finish: None,
            options: ServiceOptions::new(),
        };
        let res = try_start_process(&longrun, wait!(1000)).await.unwrap();
        let RunningScript {
            pidfd,
            logger,
            logger_stop,
        } = res.unwrap();
        logger_stop.send(()).unwrap();
        logger.await.unwrap().unwrap();
        assert!(matches!(
            supervise(&pidfd, wait!(1000)).await.unwrap(),
            ScriptResult::Exited(..)
        ));
    }

    #[tokio::test]
    async fn test_supervise_signal() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 1".to_string());
        script.timeout = 1;
        let longrun = Longrun {
            name: "test".to_string(),
            run: script,
            finish: None,
            options: ServiceOptions::new(),
        };
        let res = try_start_process(&longrun, wait!(1000)).await.unwrap();
        assert!(res.is_some());
        if let Some(running_script) = res {
            assert!(matches!(
                supervise(&running_script.pidfd, wait!(1)).await.unwrap(),
                ScriptResult::SignalReceived
            ));
        }
    }
}
