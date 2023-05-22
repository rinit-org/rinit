use std::time::Duration;

use anyhow::{
    Context,
    Result,
};
use nix::{
    sys::signal::{
        kill,
        Signal,
    },
    unistd::Pid,
};
use tokio::{
    process::Child,
    time::timeout,
};
use tracing::warn;

pub async fn kill_process(
    child: &mut Child,
    down_signal: i32,
    timeout_kill: u32,
) -> Result<()> {
    let child_id = child.id().unwrap() as i32;
    let child_pid = Pid::from_raw(child_id);
    kill(
        child_pid,
        // Safe, down_signal is always parsed from Signal
        Signal::try_from(down_signal).unwrap(),
    )
    .with_context(|| format!("unable to send signal {:?}", down_signal))?;
    let timeout_res = timeout(Duration::from_millis(timeout_kill as u64), child.wait()).await;
    if let Ok(exit_status) = timeout_res {
        exit_status.context("unable to call wait")?;
    } else {
        warn!(
            "the process didn't exit after signal {} and waiting {}ms. Sending SIGKILL",
            // This is always valid as it's parsed from the name
            Signal::try_from(down_signal).unwrap(),
            timeout_kill
        );
        kill(child_pid, Signal::SIGKILL).context("unable to send signal SIGKILL")?;
    }

    // The process might have spawned other processes, if it didn't cleanup it's a
    // bug. Kill them with SIGKILL. This isn't a lot of overhead, kill_process
    // shouldn't be called in normal circumstances, and
    let res = kill(Pid::from_raw(-child_id), Signal::SIGKILL);
    match res {
        Ok(_) => warn!("The were lingering children of the process. Killing them with SIGKILL."),
        Err(errno) => {
            if !matches!(errno, nix::errno::Errno::ESRCH) {
                res.context("unable to send signal SIGKILL to process group")?
            }
        }
    }

    Ok(())
}
