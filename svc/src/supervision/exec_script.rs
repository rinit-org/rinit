use std::{
    collections::HashMap,
    env,
    os::fd::RawFd,
    process::Stdio,
};

use anyhow::{
    Context,
    Result,
};
use nix::{
    sys::signal::{
        SigSet,
        SigmaskHow,
    },
    unistd::{
        close,
        dup2,
        Group,
        Pid,
        User,
    },
};
use rinit_service::types::{
    Script,
    ScriptEnvironment,
    ScriptPrefix,
};
use tokio::{
    io::{
        unix::AsyncFd,
        Interest,
    },
    process::{
        Child,
        Command,
    },
};
use tracing::{
    error,
    warn,
};

pub async fn exec_script(
    script: &Script,
    env: &ScriptEnvironment,
) -> Result<(Child, Option<AsyncFd<i32>>)> {
    let (exe, args) = match &script.prefix {
        ScriptPrefix::Bash => ("bash", vec!["-c", &script.execute]),
        ScriptPrefix::Path => {
            let mut split = script.execute.split_whitespace().peekable();
            (
                split
                    .next()
                    .filter(|word| word.chars().all(char::is_alphabetic))
                    .unwrap_or(""),
                split.collect(),
            )
        }
        ScriptPrefix::Sh => ("sh", vec!["-c", &script.execute]),
    };
    let mut cmd = Command::new(exe);
    // TODO: Use a proper splitting function
    cmd.args(args);
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
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    unsafe {
        cmd.pre_exec(move || {
            let mask = SigSet::empty();
            if let Err(err) = mask.thread_swap_mask(SigmaskHow::SIG_SETMASK) {
                warn!("failed to unblock signals: {:#?}", err);
            }
            // create a new process group
            if let Err(err) = nix::unistd::setpgid(Pid::from_raw(0), Pid::from_raw(0)) {
                warn!("failed to create new process group: {:#?}", err);
            }
            Ok(())
        })
    };

    let mut pipe = None;
    if let Some(notify) = &script.notify {
        let res = nix::unistd::pipe();
        match res {
            Ok((read, write)) => unsafe {
                pipe = Some((read, write));
                let notify: RawFd = (*notify).into();
                cmd.pre_exec(move || {
                    close(read)?;
                    dup2(write, notify)?;
                    close(write)?;
                    Ok(())
                });
            },
            Err(err) => error!("Could not setup a pipe for readiness: {err}"),
        }
    }

    let merged_env: HashMap<String, String> = env::vars()
        .chain(env.contents.clone().into_iter())
        .collect();
    cmd.envs(merged_env);
    let child = cmd.spawn().context("unable to spawn script")?;
    Ok((
        child,
        pipe.and_then(|(read, write)| {
            if let Err(err) = close(write) {
                error!("could not close pipe: {err}");
            }
            match AsyncFd::with_interest(read, Interest::READABLE) {
                Ok(notify) => Some(notify),
                Err(err) => {
                    error!("{err}");
                    None
                }
            }
        }),
    ))
}
