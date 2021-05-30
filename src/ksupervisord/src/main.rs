#![feature(never_type)]

extern crate libc;

use std::{
    ffi::{
        CStr,
        CString,
    },
    io,
    mem,
    os::unix::prelude::*,
    process,
    ptr,
    sync::mpsc::{
        self,
        Receiver,
        Sender,
    },
    thread,
    time,
};

use anyhow::{
    bail,
    Context,
    Result,
};
use async_pidfd::PidFd;
use clap::Clap;
use libc::{
    c_char,
    pid_t,
    sigset_t,
    SFD_CLOEXEC,
    SIGINT,
    SIGKILL,
    SIGTERM,
    SIG_BLOCK,
};
use polling::{
    Event,
    Poller,
};
use xdg::BaseDirectories;

#[derive(Clap)]
#[clap(version = "0.1.0", author = "Danilo Spinella <oss@danyspin97.org>")]
struct Opts {
    #[clap(long)]
    configdir: Option<String>,
}

// io::Error includes all system errors
// https://stackoverflow.com/questions/42772307/how-do-i-handle-errors-from-libc-functions-in-an-idiomatic-rust-manner
macro_rules! try_syscall {
    ($ret:expr, $msg:tt) => {
        if $ret == -1 {
            bail!("{}: {}", $msg, io::Error::last_os_error());
        } else {
            $ret
        }
    };
}

fn spawn_svc(configdir: &CStr) -> Result<pid_t, io::Error> {
    let pid = unsafe { libc::fork() };
    if pid == -1 {
        return Err(io::Error::last_os_error());
    }
    if pid == 0 {
        unsafe {
            let exe = CString::new("ksvc").unwrap();
            let config_file = configdir;
            let ret = libc::execlp(
                exe.as_ptr(),
                exe.as_ptr(),
                config_file.as_ptr(),
                ptr::null() as *const libc::c_char,
            );
            if ret == -1 {
                eprintln!("there has been an error while executing ksvc");
                process::exit(1);
            }
        }
    }

    Ok(pid)
}

fn spawn_and_supervise_svc(configdir: &CStr) -> PidFd {
    let pid = loop {
        if let Ok(pid) = spawn_svc(&configdir) {
            break pid;
        } else {
            eprintln!("unable to spawn ksvc");
            thread::sleep(time::Duration::from_secs(5));
        }
    };
    loop {
        if let Ok(pidfd) = PidFd::from_pid(pid) {
            break pidfd;
        } else {
            eprintln!("unable to open pidfd");
            thread::sleep(time::Duration::from_millis(500));
        }
    }
}

pub fn main() -> Result<()> {
    let opts = Opts::try_parse()?;

    let uid = unsafe { libc::getuid() };
    let configdir = if let Some(configdir) = opts.configdir {
        configdir
    } else if uid == 0 {
        "/etc/kansei".to_string()
    } else {
        let xdg = BaseDirectories::with_prefix("kansei")
            .context("unable to initialize XDG BaseDirectories")?;
        xdg.get_config_home()
            .to_str()
            .with_context(|| format!("unable to convert {:?} to String", xdg.get_config_home()))?
            .to_string()
    };

    let mut sigset: sigset_t = unsafe { mem::zeroed() };
    unsafe {
        try_syscall!(
            libc::sigemptyset(&mut sigset as *mut sigset_t),
            "unable to create empty signal set"
        );
        try_syscall!(
            libc::sigaddset(&mut sigset as *mut sigset_t, SIGINT),
            "unable to add signal SIGINT to signal set"
        );
        try_syscall!(
            libc::sigaddset(&mut sigset as *mut sigset_t, SIGTERM),
            "unable to add signal SIGTERM to signal set"
        );

        try_syscall!(
            libc::sigprocmask(SIG_BLOCK, &sigset as *const sigset_t, ptr::null_mut()),
            "unable to mask signals"
        );
    }

    let signalfd = try_syscall!(
        unsafe { libc::signalfd(-1, &sigset as *const sigset_t, SFD_CLOEXEC) },
        "unable to open signalfd"
    );

    let configdir = CString::new(configdir).context("unable to create c string for configdir")?;
    // If this operation fails at startup, just make the program fails
    let mut pidfd = PidFd::from_pid(spawn_svc(&configdir).context("unable to spawn ksvc")?)
        .context("unable to open pidfd")?;
    let poller = Poller::new()?;
    const PIDFD: usize = 0;
    const SIGNALFD: usize = 1;
    poller.add(&pidfd, Event::readable(PIDFD))?;
    poller.add(signalfd as i32, Event::readable(SIGNALFD))?;

    let mut events = Vec::new();
    // 10 events are too many, but it's still better to allocate more now at startup
    events.reserve(10);
    // From now on don't allocate, this process might be running as PID 1
    // generally, we don't want this process to panic because it's supervising ksvc
    'run: loop {
        events.clear();
        poller.wait(&mut events, None)?;
        for ev in &events {
            // The process has died, restart it
            if ev.key == PIDFD {
                // The possible errors shouldn't likely happen
                // If they happen, we can't ignore them, the process has already exited anyway
                let _ = pidfd.wait();
                pidfd = spawn_and_supervise_svc(&configdir);
                'modify_poller: loop {
                    // Use Poller::add because the pidfd is different, hence Poller::modify won't
                    // work
                    if poller.add(&pidfd, Event::readable(PIDFD)).is_ok() {
                        break 'modify_poller;
                    }
                }
            } else if ev.key == SIGNALFD {
                // We got a signal telling us to stop ksupervisord
                // Send a signal to the process we are supervisioning
                let pidfd_raw = pidfd.as_raw_fd();
                let _ = unsafe {
                    libc::syscall(
                        libc::SYS_pidfd_send_signal,
                        pidfd_raw,
                        SIGTERM,
                        ptr::null_mut() as *mut c_char,
                        0,
                    )
                };
                let (sender, receiver): (Sender<Result<(), !>>, Receiver<Result<(), !>>) =
                    mpsc::channel();
                let _ = thread::spawn(move || {
                    let _ = pidfd.wait();
                    // We don't care if the main thread receive it or not
                    let _ = sender.send(Ok(()));
                });

                // Use SIGKILL if the process hasn't exited after a certain time
                if !match receiver.recv_timeout(time::Duration::from_secs(3)) {
                    Ok(Ok(_)) => true,            // The process has exited
                    Ok(Err(_)) => unreachable!(), // The Error is type !
                    Err(_) => false,              // wait hasn't returned yet
                } {
                    let _ = unsafe {
                        libc::syscall(
                            libc::SYS_pidfd_send_signal,
                            pidfd_raw,
                            SIGKILL,
                            ptr::null_mut() as *mut c_char,
                            0,
                        )
                    };
                }
                break 'run;
            }
        }
    }

    Ok(())
}
