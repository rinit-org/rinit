#![feature(never_type)]

extern crate libc;

use std::{
    os::unix::prelude::*,
    process::Command,
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
    Context,
    Result,
};
use async_pidfd::PidFd;
use clap::Clap;
use libc::{
    c_char,
    SIGKILL,
};
use nix::sys::{
    signal::{
        SigSet,
        SIGINT,
        SIGTERM,
    },
    signalfd::{
        SfdFlags,
        SignalFd,
    },
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

fn spawn_svc(configdir: &str) -> PidFd {
    let child = loop {
        if let Ok(child) = Command::new("ksvc").args([configdir]).spawn() {
            break child;
        } else {
            eprintln!("unable to spawn ksvc");
            thread::sleep(time::Duration::from_secs(5));
        }
    };
    loop {
        if let Ok(pidfd) = PidFd::from_pid(child.id() as libc::pid_t) {
            break pidfd;
        } else {
            eprintln!("unable to open pidfd");
            thread::sleep(time::Duration::from_millis(500));
        }
    }
}

pub fn main() -> Result<()> {
    let opts = Opts::parse();

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

    let mut mask = SigSet::empty();
    mask.add(SIGINT);
    mask.add(SIGTERM);
    mask.thread_block().context("unable to mask signals")?;

    let sfd = SignalFd::with_flags(&mask, SfdFlags::SFD_CLOEXEC)
        .context("unable to initialize signalfd")?;

    // If this operation fails at startup, just make the program fails
    let mut pidfd = spawn_svc(&configdir);
    let poller = Poller::new()?;
    const PIDFD: usize = 0;
    const SIGNALFD: usize = 1;
    poller.add(&pidfd, Event::readable(PIDFD))?;
    poller.add(sfd.as_raw_fd(), Event::readable(SIGNALFD))?;

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
                pidfd = spawn_svc(&configdir);
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
                        libc::SIGTERM,
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
