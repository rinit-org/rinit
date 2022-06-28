use std::process::Stdio;

use anyhow::{
    Context,
    Result,
};
use nix::{
    sys::signal::{
        SigSet,
        Signal,
    },
    unistd::{
        Group,
        User,
    },
};
use rinit_service::types::{
    Script,
    ScriptPrefix,
};
use tokio::process::{
    Child,
    Command,
};

pub async fn exec_script(script: &Script) -> Result<Child> {
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
    let mut cmd = Command::new(&exe);
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
        cmd.pre_exec(move || -> Result<(), std::io::Error> {
            let mut mask = SigSet::empty();
            mask.add(Signal::SIGINT);
            mask.add(Signal::SIGTERM);
            mask.thread_unblock().map_err(|err| err.into())
        })
    };
    let child = cmd.spawn().context("unable to spawn script")?;

    Ok(child)
}
