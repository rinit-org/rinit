use std::{
    env,
    os::unix::prelude::AsRawFd,
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
};
use tokio::{
    fs,
    io::unix::AsyncFd,
    select,
    signal::unix::{
        signal,
        SignalKind,
    },
    sync::mpsc::{
        self,
        Receiver,
    },
    time::timeout,
};

const SIGKILL: i32 = 9;

#[derive(Debug, PartialEq)]
enum ScriptResult {
    Exited(ExitStatus),
    SignalReceived,
    TimedOut,
}

async fn run_script(
    script: &Script,
    rx: &mut Receiver<()>,
) -> Result<bool> {
    let script_timeout = Duration::from_millis(script.timeout as u64);

    let mut time_tried = 0;
    let success = loop {
        let child = exec_script(&script)
            .await
            .context("unable to execute script")?;
        let pidfd = AsyncFd::new(
            PidFd::from_pid(child.id().unwrap() as i32)
                .context("unable to create PidFd from child pid")?,
        )
        .context("unable to create AsyncFd from PidFd")?;
        let script_res = select! {
            timeout_res = timeout(script_timeout, pidfd.readable()) => {
                if let Ok(_) = timeout_res {
                    ScriptResult::Exited(pidfd.get_ref().wait().context("unable to call waitid on child process")?.status())
                } else {
                    ScriptResult::TimedOut
                }
            }
            _ = rx.recv() => ScriptResult::SignalReceived
        };
        let success = match script_res {
            ScriptResult::Exited(exit_status) => exit_status.success(),
            ScriptResult::SignalReceived => false,
            ScriptResult::TimedOut => false,
        };
        if success {
            break true;
        }

        match script_res {
            ScriptResult::Exited(_) => {}
            ScriptResult::SignalReceived | ScriptResult::TimedOut => {
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
            }
        }
        if script_res == ScriptResult::SignalReceived {
            break false;
        }

        time_tried += 1;
        if time_tried == script.max_deaths {
            break false;
        }
    };

    Ok(success)
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args();
    args.next();
    let script: Script = bincode::deserialize(&fs::read(args.next().unwrap()).await?)?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    let (tx, mut rx) = mpsc::channel(1);
    let success = select! {
        success = run_script(&script, &mut rx) => success?,
        _ = sigint.recv() => { tx.send(()).await.unwrap(); false },
        _ = sigterm.recv() => { tx.send(()).await.unwrap(); false },
    };

    println!("success: {}", success);

    //TODO: notify svc

    Ok(())
}

#[cfg(test)]
mod tests {
    use kansei_core::types::ScriptPrefix;

    use super::*;

    #[tokio::test]
    async fn test_run_script_success() {
        let script = Script::new(ScriptPrefix::Bash, "exit 0".to_string());
        let (_tx, mut rx) = mpsc::channel(1);
        assert!(run_script(&script, &mut rx).await.unwrap());
    }

    #[tokio::test]
    async fn test_run_script_failure() {
        let script = Script::new(ScriptPrefix::Bash, "exit 1".to_string());
        let (_tx, mut rx) = mpsc::channel(1);
        assert!(!run_script(&script, &mut rx).await.unwrap());
    }

    #[tokio::test]
    async fn test_run_script_timeout() {
        let mut script = Script::new(ScriptPrefix::Bash, "sleep 15".to_string());
        script.timeout = 10;
        let (_tx, mut rx) = mpsc::channel(1);
        assert!(!run_script(&script, &mut rx).await.unwrap());
    }

    #[tokio::test]
    async fn test_run_script_force_kill() {
        let mut script = Script::new(ScriptPrefix::Path, "sleep 100".to_string());
        script.timeout = 10;
        script.timeout_kill = 10;
        script.down_signal = 0;
        script.max_deaths = 1;
        let (_tx, mut rx) = mpsc::channel(1);
        assert!(!run_script(&script, &mut rx).await.unwrap());
    }

    #[tokio::test]
    async fn test_run_script_signal_received() {
        let mut script = Script::new(ScriptPrefix::Path, "sleep 100".to_string());
        script.timeout = 100000;
        let (tx, mut rx) = mpsc::channel(1);
        tx.send(()).await.unwrap();
        assert!(!run_script(&script, &mut rx).await.unwrap());
    }
}
