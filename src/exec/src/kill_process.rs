use std::{
    os::unix::prelude::AsRawFd,
    time::Duration,
};

use anyhow::{
    Context,
    Result,
};
use async_pidfd::PidFd;
use libc::SIGKILL;
use tokio::{
    io::unix::AsyncFd,
    time::timeout,
};

use crate::pidfd_send_signal;

pub async fn kill_process(
    pidfd: AsyncFd<PidFd>,
    timeout_kill: u32,
    down_signal: i32,
) -> Result<()> {
    pidfd_send_signal(pidfd.as_raw_fd(), down_signal)
        .with_context(|| format!("unable to send signal {:?}", down_signal))?;
    let timeout_res = timeout(Duration::from_millis(timeout_kill as u64), pidfd.readable()).await;
    if timeout_res.is_err() {
        pidfd_send_signal(pidfd.as_raw_fd(), SIGKILL).context("unable to send signal SIGKILL")?;
    }
    pidfd.get_ref().wait().context("unable to call waitid")?;

    Ok(())
}
