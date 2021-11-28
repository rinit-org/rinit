use std::{
    os::{
        linux::process::{
            ChildExt,
            CommandExt as LinuxCommandExt,
            PidFd,
        },
        unix::process::CommandExt,
    },
    process::Command,
};

use anyhow::{
    Context,
    Result,
};
use kansei_core::types::{
    Script,
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
    sys::signal::{
        SigSet,
        Signal,
    },
    unistd::{
        close,
        dup2,
        pipe2,
        Group,
        User,
    },
};

pub async fn exec_script(script: &Script) -> Result<PidFd> {
    let p1 = pipe2(OFlag::O_CLOEXEC)?;
    let p2 = pipe2(OFlag::O_CLOEXEC)?;
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
        cmd.uid(
            User::from_name(user)
                .with_context(|| format!("unable to get UID for user {}", user))?
                .with_context(|| format!("unable to find UID for user {}", user))?
                .uid
                .as_raw(),
        );
    }
    if let Some(group) = &script.group {
        cmd.gid(
            Group::from_name(group)
                .with_context(|| format!("unable to get GID for group {}", group))?
                .with_context(|| format!("unable to find GID for group {}", group))?
                .gid
                .as_raw(),
        );
    }
    let mut child = unsafe {
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
        .pre_exec(move || -> Result<(), std::io::Error> {
            let mut mask = SigSet::empty();
            mask.add(Signal::SIGINT);
            mask.add(Signal::SIGTERM);
            mask.thread_unblock().map_err(|err| err.into())
        })
    }
    .create_pidfd(true)
    .spawn()
    .context("unable to spawn script")?;

    Ok(child.take_pidfd().context("unable to take pidfd")?)
}
