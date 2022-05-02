use std::{
    io,
    os::unix::prelude::RawFd,
    ptr,
};

use crate::syscall_result;

pub fn pidfd_send_signal(
    pidfd: RawFd,
    signal: i32,
) -> io::Result<()> {
    unsafe {
        syscall_result(libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd,
            signal,
            ptr::null_mut() as *mut libc::c_char,
            0,
        ))?;
    }

    Ok(())
}
