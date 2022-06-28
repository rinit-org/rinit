use std::{
    io,
    os::unix::prelude::{
        AsRawFd,
        RawFd,
    },
    ptr,
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

pub fn pidfd_send_signal(
    pidfd: RawFd,
    signal: i32,
) -> io::Result<()> {
    unsafe {
        let ret = libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd,
            signal,
            ptr::null_mut() as *mut libc::c_char,
            0,
        );
        if ret == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(ret)
        }
    }?;

    Ok(())
}

pub async fn kill_process(
    pidfd: &AsyncFd<PidFd>,
    down_signal: i32,
    timeout_kill: u32,
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
