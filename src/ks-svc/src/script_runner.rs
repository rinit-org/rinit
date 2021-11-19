use std::time::Duration;

use anyhow::{
    bail,
    Context,
    Result,
};
use async_std::future;
use kansei_core::types::{
    Script,
    ScriptConfig,
};
use libc;
use nix::{
    sys::{
        signal::Signal,
        wait::{
            waitpid,
            WaitPidFlag,
            WaitStatus,
        },
    },
    unistd::{
        fork,
        ForkResult,
        Pid,
    },
};

use crate::exec_args::ExecArgs;

pub enum ScriptResult {
    Exited(u8),
    KilledBySignal(Signal),
    TimedOut,
}

pub struct ScriptRunner<'a> {
    script: &'a Script,
}

impl<'a> ScriptRunner<'a> {
    pub async fn run(script: &'a Script) -> Result<bool> {
        let mut time_tried = 0;
        Ok(loop {
            let res = Self::new(script).run_impl().await;
            if res.is_ok() {
                break true;
            } else {
                time_tried += 1;
                if time_tried == script.max_deaths {
                    break false;
                }
            }
        })
    }

    pub fn new(script: &'a Script) -> Self {
        Self { script }
    }

    async fn run_impl(&self) -> Result<ScriptResult> {
        let timeout = Duration::from_millis(self.script.timeout.into());
        let exe_args = ExecArgs::new(
            &self.script.prefix,
            &self.script.execute,
            ScriptConfig::new(),
        )
        .context("unable to generate exec arguments")?;
        Ok(match unsafe { fork()? } {
            ForkResult::Parent { child } => {
                match future::timeout(timeout, self.check_exit(child)).await {
                    Ok(res) => res?,
                    Err(_) => {
                        if future::timeout(
                            Duration::from_millis(self.script.timeout_kill.into()),
                            self.kill_gracefully(child),
                        )
                        .await
                        .is_err()
                        {
                            self.kill()
                        }
                        ScriptResult::TimedOut
                    }
                }
            }
            ForkResult::Child => {
                let mut p_args: Vec<_> = exe_args.args.iter().map(|arg| arg.as_ptr()).collect();
                p_args.push(std::ptr::null());
                let mut p_env: Vec<_> = exe_args.env.iter().map(|entry| entry.as_ptr()).collect();
                p_env.push(std::ptr::null());
                unsafe {
                    libc::execvpe(exe_args.exe.as_ptr(), p_args.as_ptr(), p_env.as_ptr());
                }
                unsafe { libc::_exit(1) }
            }
        })
    }

    async fn check_exit(
        &self,
        pid: Pid,
    ) -> Result<ScriptResult> {
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

    async fn kill_gracefully(
        &self,
        child: Pid,
    ) {
    }

    fn kill(&self) {
        todo!()
    }
}
