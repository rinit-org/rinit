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
};

#[derive(Debug, PartialEq, Eq)]
enum ScriptResult {
    Exited(ExitStatus),
    TimedOut,
}

pub async fn run_short_lived_script(
    script: &Script,
    env: &ScriptEnvironment,
) -> Result<bool> {
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
        let timeout_res = timeout(script_timeout, child.wait()).await;
        let script_res = if let Ok(exit_status) = timeout_res {
            ScriptResult::Exited(match exit_status.context("unable to call wait on child") {
                Ok(exit_status) => exit_status,
                Err(err) => {
                    warn!("{err}");
                    time_tried += 1;
                    if time_tried == script.max_deaths {
                        break false;
                    }
                    continue;
                }
            })
        } else {
            ScriptResult::TimedOut
        };

        match script_res {
            // The process exited on its own within timeout
            ScriptResult::Exited(exit_status) => {
                // We want the process to exit successfully to consider it "up"
                if exit_status.success() {
                    break true;
                }
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

    use rinit_service::types::ScriptPrefix;
    use tokio::fs::remove_file;

    use super::*;

    #[tokio::test]
    async fn test_run_script_success() {
        let script = Script::new(ScriptPrefix::Bash, "exit 0".to_string());
        assert!(
            run_short_lived_script(&script, &ScriptEnvironment::default())
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_run_script_failure() {
        let script = Script::new(ScriptPrefix::Bash, "exit 1".to_string());
        assert!(
            !run_short_lived_script(&script, &ScriptEnvironment::default())
                .await
                .unwrap()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_run_script_timeout() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 15".to_string());
        script.timeout = 10;
        assert!(
            !run_short_lived_script(&script, &ScriptEnvironment::default())
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
            !run_short_lived_script(&script, &ScriptEnvironment::default())
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_run_script_side_effects() {
        let filename = "test_run_script_side_effects";
        let script = Script::new(ScriptPrefix::Bash, format!("touch {filename}"));
        assert!(
            run_short_lived_script(&script, &ScriptEnvironment::default())
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
        assert!(run_short_lived_script(&script, &env).await.unwrap());
        assert!(Path::new(filename).exists());
        // cleanup
        remove_file(filename).await.unwrap();
    }
}
