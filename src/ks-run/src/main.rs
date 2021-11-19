use std::{
    io::{
        self,
        Read,
    },
    os::unix::{
        prelude::{
            AsRawFd,
            RawFd,
        },
        process::CommandExt,
    },
    process::Command,
    sync::atomic::{
        AtomicBool,
        Ordering,
    },
    time::Duration,
};

use anyhow::{
    bail,
    ensure,
    Context,
    Result,
};
use async_std::future;
use kansei_core::types::{
    Script,
    ScriptConfig,
    ScriptPrefix,
};
use libc::{
    self,
    STDERR_FILENO,
    STDIN_FILENO,
    STDOUT_FILENO,
};
use nix::{
    fcntl::OFlag,
    sys::{
        signal::{
            SigSet,
            Signal,
        },
        signalfd::{
            SfdFlags,
            SignalFd,
        },
        wait::{
            waitpid,
            WaitPidFlag,
            WaitStatus,
        },
    },
    unistd::{
        close,
        dup2,
        fork,
        pipe2,
        ForkResult,
        Pid,
    },
};
use polling::{
    Event,
    Poller,
};

pub enum ScriptResult {
    Exited(u8),
    KilledBySignal(Signal),
    TimedOut,
}

pub async fn run<'a>(
    script: &Script,
    notify: RawFd,
    should_run: &AtomicBool,
) -> Result<bool> {
    let mut time_tried = 0;
    Ok(loop {
        if should_run.load(Ordering::Relaxed) {
            break false;
        }
        if run_impl(script).await.is_ok() {
            break true;
        } else {
            time_tried += 1;
            if time_tried == script.max_deaths {
                break false;
            }
        }
    })
}
async fn run_impl(script: &Script) -> Result<ScriptResult> {
    let timeout = Duration::from_millis(script.timeout.into());
    let p1 = pipe2(OFlag::O_CLOEXEC)?;
    let p2 = pipe2(OFlag::O_CLOEXEC)?;
    Ok(match unsafe { fork()? } {
        ForkResult::Parent { child } => {
            match future::timeout(timeout, check_exit(child)).await {
                Ok(res) => res?,
                Err(_) => {
                    if future::timeout(
                        Duration::from_millis(script.timeout_kill.into()),
                        kill_gracefully(child),
                    )
                    .await
                    .is_err()
                    {
                        kill()
                    }
                    ScriptResult::TimedOut
                }
            }
        }
        ForkResult::Child => {
            let mut cmd = Command::new(match &script.prefix {
                ScriptPrefix::Bash => "bash",
                ScriptPrefix::Path => {
                    script
                        .execute
                        .split_whitespace()
                        .next()
                        .filter(|word| word.chars().all(char::is_alphabetic))
                        .unwrap_or("")
                }
                ScriptPrefix::Sh => "sh",
            });
            // TODO: Use a proper splitting function
            cmd.args(script.execute.split_whitespace().skip(1));
            if let Some(user) = &script.user {
                cmd.uid(user_to_uid(user));
            }
            if let Some(group) = &script.group {
                cmd.gid(group_to_gid(group));
            }
            unsafe {
                cmd.pre_exec(move || -> Result<(), std::io::Error> {
                    // Close fds
                    close(STDIN_FILENO)
                            .and_then(|_| close(STDOUT_FILENO))
                            .and_then(|_| close(STDERR_FILENO))
                            // Pipe the stdout and stderr of the program
                            // into ks-log
                            .and_then(|_| dup2(p1.0, STDOUT_FILENO).map(|_| ()))
                            .and_then(|_| dup2(p2.0, STDERR_FILENO).map(|_| ()))
                            .map_err(|err| err.into())
                })
            };
            unsafe {
                cmd.pre_exec(move || -> Result<(), std::io::Error> {
                    let mut mask = SigSet::empty();
                    mask.add(Signal::SIGINT);
                    mask.add(Signal::SIGTERM);
                    mask.thread_unblock().map_err(|err| err.into())
                })
            };
            cmd.exec();
            unsafe { libc::_exit(1) }
        }
    })
}
async fn check_exit(pid: Pid) -> Result<ScriptResult> {
    let mut flags = WaitPidFlag::empty();
    flags.insert(WaitPidFlag::WEXITED);
    Ok(match waitpid(pid, Some(flags))? {
        WaitStatus::Exited(_, status) => ScriptResult::Exited(status.try_into()?),
        WaitStatus::Signaled(_, signal, _) => ScriptResult::KilledBySignal(signal),
        WaitStatus::Stopped(..)
        | WaitStatus::PtraceEvent(..)
        | WaitStatus::PtraceSyscall(_)
        | WaitStatus::Continued(_)
        | WaitStatus::StillAlive => unreachable!(),
    })
}
async fn kill_gracefully(child: Pid) {}

fn kill() {
    todo!()
}

fn group_to_gid(group: &str) -> u32 {
    todo!()
}

fn user_to_uid(user: &str) -> u32 {
    todo!()
}

#[async_std::main]
async fn main() -> Result<()> {
    let mut buf = Vec::new();
    let mut stdin = io::stdin();
    stdin.read_to_end(&mut buf)?;
    let script = bincode::deserialize(&buf)?;
    let pipe = pipe2(OFlag::O_CLOEXEC)?;
    let should_run = AtomicBool::new(true);
    let run_future = run(&script, pipe.0, &should_run);

    let mut mask = SigSet::empty();
    mask.add(Signal::SIGINT);
    mask.add(Signal::SIGTERM);
    mask.thread_block().context("unable to mask signals")?;

    let sfd = SignalFd::with_flags(&mask, SfdFlags::SFD_CLOEXEC)
        .context("unable to initialize signalfd")?;
    let poller = Poller::new()?;
    const RUN_SCRIPT: usize = 0;
    const SIGNALFD: usize = 1;
    poller.add(pipe.1, Event::readable(RUN_SCRIPT))?;
    poller.add(sfd.as_raw_fd(), Event::readable(SIGNALFD))?;

    let mut events = Vec::new();
    events.reserve(10);

    'run: loop {
        events.clear();
        poller.wait(&mut events, None)?;
        for ev in &events {
            // The process has finished running
            if ev.key == RUN_SCRIPT {
                ensure!(run_future.await?, "failed!");
                break 'run;
            } else if ev.key == SIGNALFD {
                should_run.store(false, Ordering::Relaxed);
                run_future.await?;
                break 'run;
            }
        }
    }

    Ok(())
}
