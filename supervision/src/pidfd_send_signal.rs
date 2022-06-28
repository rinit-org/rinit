use std::{
    io,
    os::unix::prelude::RawFd,
    ptr,
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
