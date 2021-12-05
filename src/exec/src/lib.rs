pub mod exec_script;
pub mod pidfd_send_signal;

use std::io;

pub use exec_script::exec_script;
pub use pidfd_send_signal::pidfd_send_signal;

fn syscall_result(ret: libc::c_long) -> io::Result<libc::c_long> {
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}
