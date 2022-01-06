pub mod exec_script;
pub mod kill_process;
pub mod pidfd_send_signal;
pub mod run_short_lived_script;
pub mod signal_wait;

#[macro_use]
extern crate lazy_static;

use std::io;

pub use exec_script::exec_script;
pub use kill_process::kill_process;
pub use pidfd_send_signal::pidfd_send_signal;
pub use run_short_lived_script::run_short_lived_script;
pub use signal_wait::signal_wait;

fn syscall_result(ret: libc::c_long) -> io::Result<libc::c_long> {
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}
