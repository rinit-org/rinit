use std::convert::TryFrom;

use serde::{
    Deserialize,
    Serialize,
};
use serde_with::skip_serializing_none;
use snafu::Snafu;

use super::script_config::ScriptConfig;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
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
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Script {
    pub prefix: ScriptPrefix,
    pub execute: String,
    #[serde(flatten, default, skip_serializing_if = "ScriptConfig::is_empty")]
    pub config: ScriptConfig,
    #[serde(
        default = "Script::default_timeout",
        skip_serializing_if = "Script::is_default_timeout"
    )]
    pub timeout: u32,
    #[serde(
        default = "Script::default_timeout_kill",
        skip_serializing_if = "Script::is_default_timeout_kill"
    )]
    pub timeout_kill: u32,
    #[serde(
        default = "Script::default_max_deaths",
        skip_serializing_if = "Script::is_default_max_deaths"
    )]
    pub max_deaths: u8,
    #[serde(
        default = "Script::default_down_signal",
        skip_serializing_if = "Script::is_default_down_signal"
    )]
    pub down_signal: i32,
    pub user: Option<String>,
    pub group: Option<String>,
    pub notify: Option<u8>,
}

impl Script {
    pub const DEFAULT_TIMEOUT: u32 = 3000;
    pub const DEFAULT_TIMEOUT_KILL: u32 = 3000;
    pub const DEFAULT_MAX_DEATHS: u8 = 3;
    pub const DEFAULT_DOWN_SIGNAL: i32 = libc::SIGTERM;

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
            config: ScriptConfig::new(),
            timeout: Self::default_timeout(),
            timeout_kill: Self::default_timeout_kill(),
            max_deaths: Self::default_max_deaths(),
            down_signal: Self::default_down_signal(),
            user: None,
            group: None,
            notify: None,
        }
    }

    pub fn get_maximum_time(&self) -> u32 {
        (self.timeout + self.timeout_kill) * self.max_deaths as u32
    }
}
