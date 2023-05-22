use std::{
    process::ExitStatus,
    time::Duration,
};

use anyhow::{
    Context,
    Result,
};
use rinit_ipc::{
    AsyncConnection,
    Request,
};
use rinit_service::types::Longrun;
use tokio::{
    process::Child,
    select,
    sync::oneshot::{
        self,
        Sender,
    },
    task::{
        self,
        JoinHandle,
    },
    time::timeout,
};
use tracing::{
    info,
    warn,
};

use crate::supervision::{
    exec_script,
    kill_process,
    log_output,
    run_short_lived_script,
    signal_wait::{
        signal_wait_fun,
        WaitFn,
    },
};

struct RunningScript {
    child: Child,
    logger: JoinHandle<Result<(), anyhow::Error>>,
    logger_stop: Sender<()>,
}

enum ScriptResult {
    Exited(ExitStatus),
    Running(RunningScript),
    SignalReceived,
}

async fn start_process(
    longrun: &Longrun,
    wait: WaitFn,
) -> Result<ScriptResult> {
    let script = &longrun.run;
    let script_timeout = Duration::from_millis(script.timeout as u64);

    let mut child = exec_script(script, &longrun.environment)
        .await
        .context("unable to execute script")?;
    let (tx, rx) = oneshot::channel();
    let logger = task::spawn(log_output(
        child.stdout.take().unwrap(),
        child.stderr.take().unwrap(),
        rx,
    ));
    Ok(select! {
        timeout_res = timeout(script_timeout, child.wait()) => {
            if let Ok(exit_status) = timeout_res {
                let status = exit_status.context("unable to call wait on child")?;
                if !tx.is_closed() {
                    tx.send(()).unwrap();
                }
                logger.await??;
                ScriptResult::Exited(status)
            } else {
                ScriptResult::Running(RunningScript { child, logger, logger_stop: tx } )
            }
        }
        signal = wait => {
            info!("received signal {}", signal?.as_str());
            kill_process(child, script.down_signal, script.timeout_kill).await?;
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
    F: FnMut() -> WaitFn,
{
    let mut time_tried = 0;
    Ok(loop {
        let script_res = start_process(longrun, wait()).await?;

        match script_res {
            ScriptResult::Exited(status) => {
                warn!("process exited with {status}");
                time_tried += 1;
                if let Some(finish_script) = &longrun.finish {
                    let _ = run_short_lived_script(
                        finish_script,
                        &longrun.environment,
                        signal_wait_fun(),
                    )
                    .await;
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

async fn supervise<F>(
    child: &mut Child,
    mut wait: F,
) -> Result<ScriptResult>
where
    F: FnMut() -> WaitFn,
{
    Ok(select! {
        exit_status = child.wait() => {
            ScriptResult::Exited(exit_status.context("unable to wait on child process")?)
        }
        signal = wait() => {
            info!("received signal {}", signal?.as_str());
            ScriptResult::SignalReceived
        }
    })
}

pub async fn supervise_long_lived_process(longrun: &Longrun) -> Result<()> {
    let mut conn = AsyncConnection::new_host_address().await?;
    while let Some(mut running_script) = try_start_process(longrun, signal_wait_fun()).await? {
        let request = Request::ServiceIsUp(longrun.name.clone(), true);
        // TODO: handle this
        conn.send_request(request).await??;
        let res = supervise(&mut running_script.child, signal_wait_fun()).await;
        let res = match res {
            Ok(res) => {
                match res {
                    ScriptResult::SignalReceived => {
                        // stop running
                        kill_process(
                            running_script.child,
                            longrun.run.down_signal,
                            longrun.run.timeout_kill,
                        )
                        .await?;
                    }
                    ScriptResult::Exited(status) => warn!("process exited with {status}"),
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
        if let Some(ScriptResult::SignalReceived) = res {
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
    use nix::sys::signal::Signal;
    use rinit_service::types::{
        Script,
        ScriptEnvironment,
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
                    Signal::SIGUSR1
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
            environment: ScriptEnvironment::new(),
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
        script.timeout = 50;
        let longrun = Longrun {
            name: "test".to_string(),
            run: script,
            finish: None,
            options: ServiceOptions::new(),
            environment: ScriptEnvironment::new(),
        };
        assert!(matches!(
            try_start_process(&longrun, wait!(1000)).await.unwrap(),
            None
        ));
    }

    #[tokio::test]
    async fn test_supervise() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 0.3".to_string());
        script.timeout = 1;
        let longrun = Longrun {
            name: "test".to_string(),
            run: script,
            finish: None,
            options: ServiceOptions::new(),
            environment: ScriptEnvironment::new(),
        };
        let res = try_start_process(&longrun, wait!(1000)).await.unwrap();
        let RunningScript {
            mut child,
            logger,
            logger_stop,
        } = res.unwrap();
        logger_stop.send(()).unwrap();
        logger.await.unwrap().unwrap();
        assert!(matches!(
            supervise(&mut child, wait!(1000)).await.unwrap(),
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
            environment: ScriptEnvironment::new(),
        };
        let res = try_start_process(&longrun, wait!(1000)).await.unwrap();
        assert!(res.is_some());
        if let Some(mut running_script) = res {
            assert!(matches!(
                supervise(&mut running_script.child, wait!(1))
                    .await
                    .unwrap(),
                ScriptResult::SignalReceived
            ));
        }
    }
}
