use std::convert::TryFrom;

use libc::SIGINT;
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
    #[serde(flatten)]
    pub config: Option<ScriptConfig>,
    pub timeout: Option<u32>,
    pub timeout_kill: Option<u32>,
    pub max_deaths: Option<u8>,
    pub down_signal: Option<i32>,
    pub autostart: Option<bool>,
    pub user: Option<String>,
    pub group: Option<String>,
    pub notify: Option<u8>,
}

impl Script {
    pub const DEFAULT_TIMEOUT: u32 = 3000;
    pub const DEFAULT_TIMEOUT_KILL: u32 = 3000;
    pub const DEFAULT_MAX_DEATHS: u8 = 3;

    pub fn new(
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

    pub fn set_defaults(&mut self) {
        self.config.get_or_insert_default();
        self.timeout.get_or_insert(Self::DEFAULT_TIMEOUT);
        self.timeout_kill.get_or_insert(Self::DEFAULT_TIMEOUT_KILL);
        self.max_deaths.get_or_insert(Self::DEFAULT_MAX_DEATHS);
        self.down_signal.get_or_insert(SIGINT);
        self.autostart.get_or_insert(true);
    }

    pub fn get_maximum_time(&self) -> u32 {
        (self.timeout.unwrap() + self.timeout_kill.unwrap()) * self.max_deaths.unwrap() as u32
    }
}
