use std::{
    process::ExitStatus,
    time::Duration,
};

use anyhow::{
    Context,
    Result,
};
use flexi_logger::writers::FileLogWriterHandle;
use rinit_ipc::Request;
use rinit_service::types::Longrun;
use tokio::{
    process::Child,
    select,
    sync::{
        mpsc,
        oneshot::{
            self,
            Sender,
        },
        watch,
    },
    task::{
        self,
        JoinHandle,
    },
    time::timeout,
};
use tracing::{
    error,
    instrument::WithSubscriber,
    warn,
};

use crate::supervision::{
    exec_script,
    kill_process,
    log_output,
    run_short_lived_script,
};

struct RunningScript {
    child: Child,
    logger: JoinHandle<Result<(), anyhow::Error>>,
    logger_stop: Sender<()>,
}

pub struct Supervisor {
    running_script: Option<RunningScript>,
    terminate: watch::Receiver<()>,
    longrun: Longrun,
    // Store the fds of the logger so that they will stay open
    _fw_handle: FileLogWriterHandle,
}

enum ScriptResult {
    Exited(ExitStatus),
    Running(RunningScript),
    Terminated,
}

impl Supervisor {
    /// Return Some(Self) if the script started successfully
    /// None if the script failed during startup
    /// The wrapping Result is for system errors
    pub fn new(
        longrun: Longrun,
        terminate: watch::Receiver<()>,
        fw_handle: FileLogWriterHandle,
    ) -> Self {
        Self {
            longrun,
            running_script: None,
            terminate,
            _fw_handle: fw_handle,
        }
    }

    pub async fn start(&mut self) -> Result<bool> {
        let mut time_tried = 0;
        Ok(loop {
            let script_res = self.start_process().await?;

            match script_res {
                ScriptResult::Exited(status) => {
                    // TODO: Proper logging
                    warn!("process exited with {status}");
                    time_tried += 1;
                    if let Some(finish_script) = &self.longrun.finish {
                        if let Err(err) =
                            run_short_lived_script(finish_script, &self.longrun.environment).await
                        {
                            error!("{err}");
                        }
                    }
                    if time_tried == self.longrun.run.max_deaths {
                        break false;
                    }
                }
                ScriptResult::Running(running_script) => {
                    self.running_script = Some(running_script);
                    break true;
                }
                ScriptResult::Terminated => break false,
            }
        })
    }

    async fn start_process(&mut self) -> Result<ScriptResult> {
        let script = &self.longrun.run;
        let script_timeout = Duration::from_millis(script.timeout as u64);

        let mut child = exec_script(script, &self.longrun.environment)
            .await
            .context("unable to execute script")?;
        let (tx, rx) = oneshot::channel();
        // let (fw_handle, subscriber) = self.logger_subscriber();
        let logger = task::spawn_local(
            log_output(
                child.stdout.take().unwrap(),
                child.stderr.take().unwrap(),
                rx,
            )
            .with_current_subscriber(),
        );
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
                    ScriptResult::Running(RunningScript {child, logger, logger_stop: tx})
                }
            }
            _ = self.terminate.changed() => {
                kill_process(&mut child, script.down_signal, script.timeout_kill).await?;
                if !tx.is_closed() {
                    tx.send(()).unwrap();
                }
                logger.await??;
                ScriptResult::Terminated
            }
        })
    }

    pub async fn supervise(
        &mut self,
        send: mpsc::Sender<Request>,
    ) -> Result<()> {
        debug_assert!(self.running_script.is_some());
        loop {
            // This is never empty. Move out the value so that we can use logger and
            // logger_stop
            let mut running_script = self.running_script.take().unwrap();
            let res = select! {
                exit_status = running_script.child.wait() => {
                    ScriptResult::Exited(exit_status.context("unable to wait on child process")?)
                }
                _ = self.terminate.changed() => {
                    ScriptResult::Terminated
                }
            };
            match res {
                ScriptResult::Terminated => {
                    // stop running
                    kill_process(
                        &mut running_script.child,
                        self.longrun.run.down_signal,
                        self.longrun.run.timeout_kill,
                    )
                    .await?;
                }
                ScriptResult::Exited(status) => warn!("process exited with {status}"),
                ScriptResult::Running(_) => unreachable!(),
            }
            if let Err(err) = send
                .send(Request::UpdateServiceStatus(
                    self.longrun.name.to_owned(),
                    rinit_service::service_state::IdleServiceState::Down,
                ))
                .await
            {
                error!("Could not notify the main thread: {err}");
            };
            if !running_script.logger_stop.is_closed() {
                if let Err(_err) = running_script.logger_stop.send(()) {
                    warn!("logger was not working properly");
                }
            }
            running_script.logger.await??;
            if let ScriptResult::Terminated = res {
                break;
            }
            match self.start_process().await? {
                ScriptResult::Exited(_) | ScriptResult::Terminated => break,
                ScriptResult::Running(running_script) => {
                    if let Err(err) = send
                        .send(Request::UpdateServiceStatus(
                            self.longrun.name.to_owned(),
                            rinit_service::service_state::IdleServiceState::Up,
                        ))
                        .await
                    {
                        error!("Could not notify the main thread: {err}");
                    }
                    self.running_script = Some(running_script);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use flexi_logger::{
        writers::FileLogWriter,
        FileSpec,
    };
    use rinit_service::types::{
        Script,
        ScriptEnvironment,
        ScriptPrefix,
        ServiceOptions,
    };
    use tokio::join;

    use super::*;

    macro_rules! spawn_local {
        ($fn:expr) => {
            let local_set = task::LocalSet::new();
            local_set.run_until($fn).await;
        };
    }

    macro_rules! new_supervisor {
        ($supervisor:tt, $tx:tt, $longrun:tt) => {
            let ($tx, rx) = watch::channel(());
            let (_file_writer, fw_handle) = FileLogWriter::builder(FileSpec::default())
                .try_build_with_handle()
                .unwrap();
            let mut $supervisor = Supervisor::new($longrun, rx, fw_handle);
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
        new_supervisor!(supervisor, _tx, longrun);
        spawn_local!(async move {
            assert!(supervisor.start().await.unwrap());
        });
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
        new_supervisor!(supervisor, _tx, longrun);
        spawn_local!(async move {
            assert!(!supervisor.start().await.unwrap());
        });
    }

    #[tokio::test]
    async fn test_supervise_terminate() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 1".to_string());
        script.timeout = 1;
        let longrun = Longrun {
            name: "test".to_string(),
            run: script,
            finish: None,
            options: ServiceOptions::new(),
            environment: ScriptEnvironment::new(),
        };
        new_supervisor!(supervisor, tx, longrun);
        spawn_local!(async move {
            assert!(supervisor.start().await.unwrap());
            let (send, _) = mpsc::channel(1);
            let (res1, _res2) = join! {
                timeout(Duration::from_millis(5), supervisor.supervise(send)),
                async {
                    tx.send(()).unwrap()
                },
            };
            res1.unwrap().unwrap();
        });
    }
}
