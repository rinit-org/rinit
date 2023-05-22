use std::{
    process::ExitStatus,
    time::Duration,
};

use anyhow::{
    Context,
    Result,
};
use rinit_service::types::{
    Script,
    ScriptEnvironment,
};
use tokio::{
    select,
    sync::oneshot,
    task,
    time::timeout,
};
use tracing::{
    instrument::WithSubscriber,
    warn,
};

use crate::supervision::{
    exec_script,
    kill_process,
    log_output,
    signal_wait::WaitFn,
};

#[derive(Debug, PartialEq, Eq)]
enum ScriptResult {
    Exited(ExitStatus),
    SignalReceived,
    TimedOut,
}

pub async fn run_short_lived_script<F>(
    script: &Script,
    env: &ScriptEnvironment,
    mut wait: F,
) -> Result<bool>
where
    F: FnMut() -> WaitFn,
{
    let script_timeout = Duration::from_millis(script.timeout as u64);

    let mut time_tried = 0;
    let success = loop {
        let mut child = exec_script(script, env)
            .await
            .context("unable to execute script")?;
        let (tx, rx) = oneshot::channel();
        // TODO
        let logger = task::spawn(
            log_output(
                child.stdout.take().unwrap(),
                child.stderr.take().unwrap(),
                rx,
            )
            .with_current_subscriber(),
        );
        let script_res = select! {
            timeout_res = timeout(script_timeout, child.wait()) => {
                if let Ok(exit_status) = timeout_res {
                    ScriptResult::Exited(match exit_status.context("unable to call wait on child") {
                        Ok(exit_status) => exit_status,
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
            // The process exited on its own within timeout
            ScriptResult::Exited(exit_status) => {
                // We want the process to exit successfully to consider it "up"
                if exit_status.success() {
                    break true;
                }
            }
            // The supervisor received a signal while waiting and interuppted the wait
            ScriptResult::SignalReceived => {
                // Kill the process before exiting
                kill_process(&mut child, script.down_signal, script.timeout_kill).await?;
                break false;
            }
            // The script didn't exit within timeout
            ScriptResult::TimedOut => {
                // Kill it and try again
                kill_process(&mut child, script.down_signal, script.timeout_kill).await?;
            }
        }

        if !tx.is_closed() {
            // Why do we need to close the pipes manually? The process has either exited
            // or has been killed, the pipes should have been already closed
            // Add this as workaround
            tx.send(()).unwrap();
        }
        // TODO
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
    use std::path::Path;

    use nix::sys::signal::Signal;
    use rinit_service::types::ScriptPrefix;
    use tokio::{
        fs::remove_file,
        time::sleep,
    };

    use super::*;

    macro_rules! wait {
        ($time:literal) => {
            || {
                Box::pin(tokio::spawn(async {
                    sleep(Duration::from_secs($time)).await;
                    Signal::SIGUSR1
                }))
            }
        };
    }

    #[tokio::test]
    async fn test_run_script_success() {
        let script = Script::new(ScriptPrefix::Bash, "exit 0".to_string());
        assert!(
            run_short_lived_script(&script, &ScriptEnvironment::default(), wait!(100))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_run_script_failure() {
        let script = Script::new(ScriptPrefix::Bash, "exit 1".to_string());
        assert!(
            !run_short_lived_script(&script, &ScriptEnvironment::default(), wait!(100))
                .await
                .unwrap()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_run_script_timeout() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 15".to_string());
        script.timeout = 10;
        assert!(
            !run_short_lived_script(&script, &ScriptEnvironment::default(), wait!(100))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_run_script_force_kill() {
        let mut script = Script::new(ScriptPrefix::Path, "sleep 100".to_string());
        // Make it timeout immediately
        script.timeout = 1;
        // Set it to a low value, we know that down_signal won't stop it
        script.timeout_kill = 1;
        // 10 is SIGUSR1. Send a signal that won't terminate the program
        script.down_signal = 10;
        script.max_deaths = 1;
        assert!(
            !run_short_lived_script(&script, &ScriptEnvironment::default(), wait!(100))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_run_script_side_effects() {
        let filename = "test_run_script_side_effects";
        let script = Script::new(ScriptPrefix::Bash, format!("touch {filename}"));
        assert!(
            run_short_lived_script(&script, &ScriptEnvironment::default(), wait!(100))
                .await
                .unwrap()
        );
        assert!(Path::new(filename).exists());
        // cleanup
        remove_file(filename).await.unwrap();
    }

    #[tokio::test]
    async fn test_run_script_env() {
        let filename = "test_run_script_env";
        let script = Script::new(ScriptPrefix::Bash, "touch ${filename}".to_string());
        let mut env = ScriptEnvironment::new();
        env.add("filename", filename.to_string());
        assert!(
            run_short_lived_script(&script, &env, wait!(100))
                .await
                .unwrap()
        );
        assert!(Path::new(filename).exists());
        // cleanup
        remove_file(filename).await.unwrap();
    }

    #[tokio::test]
    async fn receive_signal_while_starting_prefix_path() {
        // Spawn another bash shell and listen for SIGTERM signals there
        // We want to be sure that bash is properly sending SIGTERM to all its children
        // (which is still bash in this case)
        let execute = "sleep 100".to_string();
        let mut script = Script::new(ScriptPrefix::Path, execute);
        script.timeout = 100000;
        // Wait 50 milliseconds to give time for the file to be created
        assert!(
            !run_short_lived_script(&script, &ScriptEnvironment::default(), wait!(1))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn receive_signal_while_starting_prefix_bash() {
        // Spawn another bash shell and listen for SIGTERM signals there
        // We want to be sure that bash is properly sending SIGTERM to all its children
        // (which is still bash in this case)
        let execute = "sleep 100".to_string();
        let mut script = Script::new(ScriptPrefix::Bash, execute);
        script.timeout = 100000;
        // Wait 50 milliseconds to give time for the file to be created
        assert!(
            !run_short_lived_script(&script, &ScriptEnvironment::default(), wait!(1))
                .await
                .unwrap()
        );
    }
}
