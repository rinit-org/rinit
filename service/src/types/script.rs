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
    #[serde(flatten, default)]
    pub config: Option<ScriptConfig>,
    #[serde(default = "Script::default_timeout")]
    pub timeout: Option<u32>,
    #[serde(default = "Script::default_timeout_kill")]
    pub timeout_kill: Option<u32>,
    #[serde(default = "Script::default_max_deaths")]
    pub max_deaths: Option<u8>,
    #[serde(default = "Script::default_down_signal")]
    pub down_signal: Option<i32>,
    #[serde(default = "Script::default_autostart")]
    pub autostart: Option<bool>,
    pub user: Option<String>,
    pub group: Option<String>,
    pub notify: Option<u8>,
}

impl Script {
    pub const DEFAULT_TIMEOUT: u32 = 3000;
    pub const DEFAULT_TIMEOUT_KILL: u32 = 3000;
    pub const DEFAULT_MAX_DEATHS: u8 = 3;
    pub const DEFAULT_DOWN_SIGNAL: i32 = libc::SIGTERM;

    const fn default_timeout() -> Option<u32> {
        Some(Self::DEFAULT_TIMEOUT)
    }

    const fn default_timeout_kill() -> Option<u32> {
        Some(Self::DEFAULT_TIMEOUT_KILL)
    }

    const fn default_max_deaths() -> Option<u8> {
        Some(Self::DEFAULT_MAX_DEATHS)
    }

    const fn default_down_signal() -> Option<i32> {
        Some(Self::DEFAULT_DOWN_SIGNAL)
    }

    const fn default_autostart() -> Option<bool> {
        Some(true)
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
            config: Some(ScriptConfig::new()),
            timeout: Self::default_timeout(),
            timeout_kill: Self::default_timeout_kill(),
            max_deaths: Self::default_max_deaths(),
            down_signal: Self::default_down_signal(),
            autostart: Self::default_autostart(),
            user: None,
            group: None,
            notify: None,
        }
    }

    // This function returns a new Script without setting the defaults and leaving
    // the fields to None. Used in parser tests
    pub fn new_empty(
        prefix: ScriptPrefix,
        execute: String,
    ) -> Self {
        Self {
            prefix,
            execute,
            config: None,
            timeout: None,
            timeout_kill: None,
            max_deaths: None,
            down_signal: None,
            autostart: None,
            user: None,
            group: None,
            notify: None,
        }
    }

    pub fn get_maximum_time(&self) -> u32 {
        // unwrap here is safe because these values are either set by Self::new() or by
        // serde during deserialization
        (self.timeout.unwrap() + self.timeout_kill.unwrap()) * self.max_deaths.unwrap() as u32
    }
}
