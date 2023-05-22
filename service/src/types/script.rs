use std::convert::TryFrom;

use serde::{
    Deserialize,
    Serialize,
};
use serde_with::skip_serializing_none;
use snafu::Snafu;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum ScriptPrefix {
    Bash,
    Path,
    Sh,
}

#[derive(Snafu, Debug)]
pub struct InvalidScriptPrefixError {
    prefix: String,
}

impl TryFrom<String> for ScriptPrefix {
    type Error = InvalidScriptPrefixError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(match value.as_str() {
            "bash" => ScriptPrefix::Bash,
            "path" => ScriptPrefix::Path,
            "sh" => ScriptPrefix::Sh,
            _ => {
                InvalidScriptPrefixSnafu {
                    prefix: value.to_owned(),
                }
                .fail()?
            }
        })
    }
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Script {
    /// How the script will executed, i.e. by passing the program to execvp, by
    /// calling bash, etc..
    pub prefix: ScriptPrefix,
    /// The program/script to execute
    pub execute: String,
    #[serde(
        default = "Script::default_timeout",
        skip_serializing_if = "Script::is_default_timeout"
    )]
    /// How long will the supervisor wait to consider this service up?
    /// Short lived scripts have to exit within timeout milliseconds
    /// Long lived scripts have to live at timeout milliseconds
    pub timeout: u32,
    #[serde(
        default = "Script::default_timeout_kill",
        skip_serializing_if = "Script::is_default_timeout_kill"
    )]
    /// The time to wait until the script has exited after down_signal has been
    /// sent, in milliseconds
    pub timeout_kill: u32,
    #[serde(
        default = "Script::default_max_deaths",
        skip_serializing_if = "Script::is_default_max_deaths"
    )]
    /// How many times can this script dies before it is considered "down"
    pub max_deaths: u8,
    #[serde(
        default = "Script::default_down_signal",
        skip_serializing_if = "Script::is_default_down_signal"
    )]
    /// The signal to send when we want to stop/close a script/process
    pub down_signal: i32,
    pub user: Option<String>,
    pub group: Option<String>,
    pub notify: Option<u8>,
}

impl Script {
    pub const DEFAULT_TIMEOUT: u32 = 3000;
    pub const DEFAULT_TIMEOUT_KILL: u32 = 3000;
    pub const DEFAULT_MAX_DEATHS: u8 = 3;
    // SIGHUP is the only signal that is handled by shells and that is forwarded to
    // children as well. Sending SIGTERM would only kill the shell and leave the
    // children runnning
    pub const DEFAULT_DOWN_SIGNAL: i32 = libc::SIGHUP;

    const fn default_timeout() -> u32 {
        Self::DEFAULT_TIMEOUT
    }

    fn is_default_timeout(timeout: &u32) -> bool {
        *timeout == Self::DEFAULT_TIMEOUT
    }

    const fn default_timeout_kill() -> u32 {
        Self::DEFAULT_TIMEOUT_KILL
    }

    fn is_default_timeout_kill(timeout_kill: &u32) -> bool {
        *timeout_kill == Self::DEFAULT_TIMEOUT_KILL
    }

    const fn default_max_deaths() -> u8 {
        Self::DEFAULT_MAX_DEATHS
    }

    fn is_default_max_deaths(max_deaths: &u8) -> bool {
        *max_deaths == Self::DEFAULT_MAX_DEATHS
    }

    const fn default_down_signal() -> i32 {
        Self::DEFAULT_DOWN_SIGNAL
    }

    const fn is_default_down_signal(signal: &i32) -> bool {
        *signal == Self::DEFAULT_DOWN_SIGNAL
    }

    // This function always set the default values instead of leaving None
    // Use it everywhere the script will be read and executed
    pub fn new(
        prefix: ScriptPrefix,
        execute: String,
    ) -> Self {
        Self {
            prefix,
            execute,
            timeout: Self::default_timeout(),
            timeout_kill: Self::default_timeout_kill(),
            max_deaths: Self::default_max_deaths(),
            down_signal: Self::default_down_signal(),
            user: None,
            group: None,
            notify: None,
        }
    }

    /// get the maximum time that this service might take before being
    /// considered "up"
    pub fn get_maximum_time(&self) -> u32 {
        // If it has failed self.max_deaths times, we don't need to wait until it gets
        // killed
        (self.timeout + self.timeout_kill) * self.max_deaths as u32 - self.timeout_kill
    }
}
